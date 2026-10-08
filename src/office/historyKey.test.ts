import { describe, expect, it } from "vitest";
import { historyKeyFor } from "./historyKey";

// The backend only accepts [A-Za-z0-9_-] keys of at most 64 characters.
const VALID_KEY = /^[A-Za-z0-9_-]{1,64}$/;

describe("historyKeyFor", () => {
  it("keys an unsaved document by its tab id", () => {
    const id = "3f2b8c1e-6a52-4d0e-9c43-0a1b2c3d4e5f";
    expect(historyKeyFor({ id, path: null })).toBe(id);
  });

  it("keys a saved document by its path, not by the tab id", () => {
    const first = historyKeyFor({ id: "tab-one", path: "C:\\Docs\\report.docx" });
    const reopened = historyKeyFor({ id: "tab-two", path: "C:\\Docs\\report.docx" });
    expect(reopened).toBe(first);
    expect(first).not.toBe("tab-one");
    expect(first).toMatch(VALID_KEY);
  });

  it("gives different files different keys", () => {
    const a = historyKeyFor({ id: "t", path: "/home/me/a.docx" });
    const b = historyKeyFor({ id: "t", path: "/home/me/b.docx" });
    const c = historyKeyFor({ id: "t", path: "/home/you/a.docx" });
    expect(new Set([a, b, c]).size).toBe(3);
    for (const key of [a, b, c]) expect(key).toMatch(VALID_KEY);
  });

  it("treats Windows path spellings of one file alike", () => {
    const backslashes = historyKeyFor({ id: "t", path: "C:\\Users\\Ada\\Notes.docx" });
    expect(historyKeyFor({ id: "t", path: "C:/Users/Ada/Notes.docx" })).toBe(backslashes);
    expect(historyKeyFor({ id: "t", path: "c:\\users\\ada\\notes.DOCX" })).toBe(backslashes);
  });

  it("keeps case significant on POSIX paths", () => {
    const lower = historyKeyFor({ id: "t", path: "/home/ada/notes.docx" });
    const upper = historyKeyFor({ id: "t", path: "/home/ada/Notes.docx" });
    expect(upper).not.toBe(lower);
  });

  it("copes with long and non-ASCII paths", () => {
    const long = `/data/${"çalışma/".repeat(60)}rapor.xlsx`;
    expect(historyKeyFor({ id: "t", path: long })).toMatch(VALID_KEY);
    expect(historyKeyFor({ id: "t", path: "/home/ada/Sunum ı.pptx" })).toMatch(VALID_KEY);
  });

  it("falls back to the tab id for an empty path", () => {
    expect(historyKeyFor({ id: "tab-x", path: "" })).toBe("tab-x");
  });
});
