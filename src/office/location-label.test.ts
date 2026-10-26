import { describe, expect, it, vi } from "vitest";

let android = false;
vi.mock("../lib/mobile", () => ({ isAndroid: () => android }));

import { documentLocationLabel } from "./location-label";

describe("documentLocationLabel", () => {
  const incoming = "/data/user/0/io.github.ozanstn1.pdfswissarmyknife/cache/incoming/3394/omni-test.docx";

  it("shows only the file name on Android", () => {
    android = true;
    expect(documentLocationLabel(incoming)).toBe("omni-test.docx");
  });

  it("keeps the full path on desktop and null for an unsaved document", () => {
    android = false;
    expect(documentLocationLabel("C:\\Docs\\report.docx")).toBe("C:\\Docs\\report.docx");
    expect(documentLocationLabel(null)).toBeNull();
    expect(documentLocationLabel("")).toBeNull();
  });
});
