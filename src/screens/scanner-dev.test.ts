import { describe, expect, it } from "vitest";

import { devScanOutputPath } from "./scanner-dev";

describe("scanner development output path", () => {
  it("puts the pdf next to the first page on a POSIX path", () => {
    expect(devScanOutputPath("/tmp/e2e/scan-photo.png")).toBe("/tmp/e2e/scan-photo_scan.pdf");
  });

  it("keeps the Windows separator", () => {
    expect(devScanOutputPath("C:\\work\\scan-photo.png")).toBe("C:\\work\\scan-photo_scan.pdf");
  });

  it("drops only the last extension", () => {
    expect(devScanOutputPath("/tmp/page.v2.jpeg")).toBe("/tmp/page.v2_scan.pdf");
  });

  it("still names a file that has no extension", () => {
    expect(devScanOutputPath("/tmp/scan-photo")).toBe("/tmp/scan-photo_scan.pdf");
  });
});
