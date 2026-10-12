import { describe, expect, it } from "vitest";
import { parseLocationPage, resolveCitationTarget } from "./citation-target";

describe("citation target resolution", () => {
  it("parses a page location", () => {
    expect(parseLocationPage("page 3")).toBe(3);
    expect(parseLocationPage("Page 12")).toBe(12);
    expect(parseLocationPage("paragraph 4")).toBe(0);
    expect(parseLocationPage(undefined)).toBe(0);
  });

  it("opens a cited PDF at the cited page instead of the current document", () => {
    const target = resolveCitationTarget("C:/vault/karar.pdf", 7, "page 7", "C:/current.pdf");
    expect(target).toEqual({ kind: "pdf", path: "C:/vault/karar.pdf", page: 7 });
  });

  it("falls back to the location's page when the page field is a pseudo-page", () => {
    // The vault chat context uses pseudo-pages (1..5); the real page is in the
    // location label.
    const target = resolveCitationTarget("C:/vault/karar.pdf", 0, "page 42", "C:/current.pdf");
    expect(target?.page).toBe(42);
  });

  it("sends an office document to the workspace", () => {
    const target = resolveCitationTarget("C:/vault/rapor.docx", 0, "paragraph 12", "C:/current.pdf");
    expect(target).toEqual({ kind: "office", path: "C:/vault/rapor.docx", page: 0 });
  });

  it("uses the current document when a citation has no path (old thread)", () => {
    const target = resolveCitationTarget(undefined, 2, "page 2", "C:/current.pdf");
    expect(target).toEqual({ kind: "pdf", path: "C:/current.pdf", page: 2 });
  });

  it("returns null when there is nothing to open", () => {
    expect(resolveCitationTarget(undefined, 1, undefined, undefined)).toBeNull();
    expect(resolveCitationTarget("", 1, "", "")).toBeNull();
  });
});
