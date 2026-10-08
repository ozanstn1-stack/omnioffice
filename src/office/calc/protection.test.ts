import { describe, expect, it } from "vitest";
import { newSheet } from "../../lib/office-types";
import { isSheetProtected } from "./protection";

describe("isSheetProtected", () => {
  it("is false for a plain sheet", () => {
    expect(isSheetProtected(newSheet("S"))).toBe(false);
  });

  it("is true when the legacy hash is present, even without the full record", () => {
    expect(isSheetProtected({ ...newSheet("S"), sheetProtection: "ABCD" })).toBe(true);
    expect(isSheetProtected({ ...newSheet("S"), sheetProtection: "  " })).toBe(false);
  });

  it("follows the enabled flag of the full protection record", () => {
    const record = {
      enabled: true,
      passwordHash: null,
      algorithmName: "",
      hashValue: "",
      saltValue: "",
      spinCount: 0,
      options: [],
    };
    expect(isSheetProtected({ ...newSheet("S"), protection: record })).toBe(true);
    expect(isSheetProtected({ ...newSheet("S"), protection: { ...record, enabled: false } })).toBe(false);
  });
});
