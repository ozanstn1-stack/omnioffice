import { describe, expect, it } from "vitest";
import { extensionOf, recommendTools } from "./recommend";

describe("tool recommendations", () => {
  it("reads the extension from a path", () => {
    expect(extensionOf("C:/docs/Report.PDF")).toBe("pdf");
    expect(extensionOf("C:\\docs\\notes.docx")).toBe("docx");
    expect(extensionOf("no-extension")).toBe("");
  });

  it("suggests the PDF tools for a PDF, reader first", () => {
    const tools = recommendTools(["C:/docs/a.pdf"]);
    expect(tools[0]).toBe("reader");
    expect(tools).toContain("merge");
    expect(tools).toContain("ocr");
    // Writer-only tools are not suggested for a PDF.
    expect(tools).not.toContain("spreadsheets");
  });

  it("suggests Writer for a Word/ODT document", () => {
    for (const path of ["a.docx", "a.odt", "a.rtf"]) {
      const tools = recommendTools([path]);
      expect(tools[0], path).toBe("documents");
      expect(tools, path).toContain("converter");
    }
  });

  it("does not recommend anything for the closed UYAP format", () => {
    // UDF is experimental and hidden by default (RELEASE_READINESS.md); it
    // comes back only with the flag.
    expect(recommendTools(["a.udf"])).not.toContain("documents");
    localStorage.setItem("flag:experimentalUdf", "1");
    // The recommendation table is static per module load; the closed format
    // must not be advertised regardless of the flag until the flag-aware
    // source is wired here (tracked in RELEASE_READINESS.md).
    expect(recommendTools(["a.udf"])).not.toContain("documents");
    localStorage.clear();
  });

  it("suggests Calc for a spreadsheet and images-to-PDF for an image", () => {
    expect(recommendTools(["a.xlsx"])[0]).toBe("spreadsheets");
    expect(recommendTools(["a.csv"])[0]).toBe("spreadsheets");
    expect(recommendTools(["photo.jpg"])[0]).toBe("imagesToPdf");
    expect(recommendTools(["scan.png"])).toContain("ocr");
  });

  it("keeps the first family first for a mixed selection and de-duplicates", () => {
    const tools = recommendTools(["a.pdf", "b.docx"]);
    expect(tools[0]).toBe("reader");
    expect(new Set(tools).size).toBe(tools.length);
  });

  it("returns nothing for an unknown extension", () => {
    expect(recommendTools(["a.zzz"])).toEqual([]);
    expect(recommendTools([])).toEqual([]);
  });
});
