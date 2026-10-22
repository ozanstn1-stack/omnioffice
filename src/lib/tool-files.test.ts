import { describe, expect, it } from "vitest";

import { acceptsFile, extensionsFor, pathEntries, tokenEntries } from "./tool-files";

describe("tool file helpers", () => {
  it("filters names by the tool category", () => {
    expect(acceptsFile("pdf")("a.pdf")).toBe(true);
    expect(acceptsFile("pdf")("a.docx")).toBe(false);
    expect(acceptsFile("image")("a.png")).toBe(true);
    expect(acceptsFile("document")("a.docx")).toBe(true);
    expect(acceptsFile("document")("a.pdf")).toBe(true);
    expect(acceptsFile("document")("a.exe")).toBe(false);
  });

  it("offers the expected dialog extensions", () => {
    expect(extensionsFor("pdf")).toEqual(["pdf"]);
    expect(extensionsFor("image")).toContain("png");
    const documents = extensionsFor("document");
    expect(documents).toContain("docx");
    expect(documents).toContain("pdf");
    expect(new Set(documents).size).toBe(documents.length);
  });

  it("maps tokens to selected files without exposing a path", () => {
    const entries = tokenEntries(
      [
        { token: "tok-a", name: "a.pdf" },
        { token: "tok-b", name: "b.pdf" },
      ],
      [10, null],
    );
    expect(entries).toEqual([
      { path: "tok-a", token: "tok-a", name: "a.pdf", sizeBytes: 10 },
      { path: "tok-b", token: "tok-b", name: "b.pdf", sizeBytes: 0 },
    ]);
    expect(pathEntries(["C:/docs/a.pdf"], [5])).toEqual([{ path: "C:/docs/a.pdf", name: "a.pdf", sizeBytes: 5 }]);
  });
});
