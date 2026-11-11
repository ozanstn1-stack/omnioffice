import { describe, expect, it } from "vitest";
import { applyCellEdit } from "./calc/cells";
import { exportModel } from "./export-model";
import { newWorkbook, type Workbook } from "../lib/office-types";

describe("the model a non-native save writes (4.2b)", () => {
  it("adds spill ranges to a spreadsheet only", async () => {
    const workbook = applyCellEdit(newWorkbook(), 0, 0, 0, "=SEQUENCE(2)");
    const saved = (await exportModel("calc", workbook)) as Workbook;
    expect(saved.sheets[0].cells.A1.spill).toBe("A1:A2");
    const document = { blocks: [] };
    expect(await exportModel("writer", document)).toBe(document);
  });
});
