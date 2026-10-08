import { describe, expect, it } from "vitest";
import { newSheet, type Sheet } from "../../lib/office-types";
import { deleteColumn, deleteRow, insertColumn, insertRow } from "./structure";

/** Runs a structure edit against a sheet, the way the editor's `updateSheet` does. */
function run(sheet: Sheet, edit: (update: (mutate: (sheet: Sheet) => Sheet) => void) => void): Sheet {
  let result = sheet;
  edit((mutate) => {
    result = mutate(result);
  });
  return result;
}

describe("row and column edits keep hidden and resized rows and columns attached to their data", () => {
  const sheet: Sheet = {
    ...newSheet("S"),
    rowHeights: { "1": 0, "4": 48 },
    colWidths: { "0": 120, "2": 0 },
  };

  it("moves row heights down when a row is inserted above them", () => {
    const next = run(sheet, (update) => insertRow(sheet, 2, update));
    expect(next.rowHeights).toEqual({ "1": 0, "5": 48 });
    expect(next.rowCount).toBe(sheet.rowCount + 1);
  });

  it("moves row heights up when a row above them is deleted, and drops the deleted row's own", () => {
    const next = run(sheet, (update) => deleteRow(sheet, 1, update));
    expect(next.rowHeights).toEqual({ "3": 48 });
  });

  it("moves column widths, hidden columns included, when a column is inserted", () => {
    const next = run(sheet, (update) => insertColumn(sheet, 1, update));
    expect(next.colWidths).toEqual({ "0": 120, "3": 0 });
  });

  it("moves column widths left when a column before them is deleted", () => {
    const next = run(sheet, (update) => deleteColumn(sheet, 0, update));
    expect(next.colWidths).toEqual({ "1": 0 });
  });

  it("leaves the tables alone when the edit is after every entry", () => {
    const next = run(sheet, (update) => insertRow(sheet, 50, update));
    expect(next.rowHeights).toBe(sheet.rowHeights);
  });
});
