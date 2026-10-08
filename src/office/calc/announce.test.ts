import { describe, expect, it } from "vitest";
import { makeTranslate } from "../../lib/i18n";
import { cellAnnouncement } from "./announce";

const en = makeTranslate("en");
const tr = makeTranslate("tr");

describe("cellAnnouncement", () => {
  it("names the cell and its value", () => {
    expect(cellAnnouncement(en, { address: "B7", range: null, display: "42", formula: null })).toBe("Cell B7: 42");
  });

  it("says when the cell is empty", () => {
    expect(cellAnnouncement(en, { address: "A1", range: null, display: "", formula: null })).toBe("Cell A1: empty");
  });

  it("adds the formula of a formula cell", () => {
    expect(cellAnnouncement(en, { address: "C3", range: null, display: "6", formula: "=A1*B1" })).toBe(
      "Cell C3: 6, formula =A1*B1",
    );
  });

  it("names the selected range and the active cell inside it", () => {
    expect(cellAnnouncement(en, { address: "B2", range: "A1:C3", display: "x", formula: null })).toBe(
      "A1:C3 selected. Active cell B2: x",
    );
  });

  it("is translated", () => {
    expect(cellAnnouncement(tr, { address: "B7", range: null, display: "", formula: null })).toBe("Hücre B7: boş");
  });
});
