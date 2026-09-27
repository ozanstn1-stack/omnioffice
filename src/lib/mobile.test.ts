import { describe, expect, it } from "vitest";
import { OFFICE_EXTENSIONS, isAndroid, mimeForName, publishedUri } from "./mobile";

describe("mobile helpers", () => {
  it("is a no-op on desktop", () => {
    expect(isAndroid()).toBe(false);
  });

  it("exports the office whitelist used by the Android shell and the pickers", () => {
    for (const extension of [
      "docx",
      "odt",
      "rtf",
      "txt",
      "md",
      "html",
      "xlsx",
      "ods",
      "csv",
      "tsv",
      "pptx",
      "odp",
      "osed",
      "ospr",
      "osdt",
      "oswk",
    ]) {
      expect(OFFICE_EXTENSIONS).toContain(extension);
    }
    // PDF and images are routed to their own tools, not the office workspace.
    expect(OFFICE_EXTENSIONS).not.toContain("pdf");
    expect(OFFICE_EXTENSIONS).not.toContain("png");
  });

  it("keeps the existing PDF and image MIME mapping", () => {
    expect(mimeForName("doc.pdf")).toBe("application/pdf");
    expect(mimeForName("photo.jpeg")).toBe("image/jpeg");
    expect(mimeForName("scan.tiff")).toBe("image/tiff");
    // Office formats keep the generic fallback: the SAF pickers infer the
    // precise type from the extension and the manifest declares it for intents.
    expect(mimeForName("report.docx")).toBe("application/octet-stream");
    expect(mimeForName("unit.oswk")).toBe("application/octet-stream");
  });

  it("has no published URI before a document is exported", () => {
    expect(publishedUri("C:/output/report.pdf")).toBeUndefined();
  });
});
