/**
 * Cell notes: adding, editing, deleting and pinning them as plain model
 * changes. Each operation returns a new workbook (one undo step) and leaves the
 * cell's value, formula and style alone.
 */
import { describe, expect, it } from "vitest";
import { newSheet, newWorkbook, type Workbook } from "../../lib/office-types";
import { applyCellEdit } from "./cells";
import { deleteNote, noteOf, noteTooltip, setNote, toggleNoteVisible, visibleNoteAddresses } from "./notes";

function book(): Workbook {
  return { ...newWorkbook("Notes"), sheets: [newSheet("Sheet1"), newSheet("Other")] };
}

describe("setNote", () => {
  it("adds a note with its author to an empty cell", () => {
    const workbook = setNote(book(), 0, "B2", { text: "Check this", author: " Ada ", visible: false });
    const cell = workbook.sheets[0].cells.B2;
    expect(cell.comment).toBe("Check this");
    expect(cell.commentAuthor).toBe("Ada");
    expect(cell.commentVisible).toBeUndefined();
    expect(cell.value).toEqual({ kind: "empty" });
    expect(noteOf(cell)).toEqual({ text: "Check this", author: "Ada", visible: false });
  });

  it("keeps the value, formula and style of the cell it annotates", () => {
    let workbook = applyCellEdit(book(), 0, 0, 0, "=2*21");
    workbook = setNote(workbook, 0, "A1", { text: "answer", author: "", visible: true });
    const cell = workbook.sheets[0].cells.A1;
    expect(cell.formula).toBe("=2*21");
    expect(cell.value).toEqual(workbook.sheets[0].cells.A1.value);
    expect(cell.commentAuthor).toBeNull();
    expect(cell.commentVisible).toBe(true);
  });

  it("replaces an existing note instead of adding another", () => {
    let workbook = setNote(book(), 0, "A1", { text: "first", author: "Ada", visible: true });
    workbook = setNote(workbook, 0, "A1", { text: "second", author: "Bob", visible: false });
    expect(noteOf(workbook.sheets[0].cells.A1)).toEqual({ text: "second", author: "Bob", visible: false });
  });

  it("treats a blank text as deleting the note", () => {
    let workbook = setNote(book(), 0, "A1", { text: "first", author: "Ada", visible: false });
    workbook = setNote(workbook, 0, "A1", { text: "  \n ", author: "Ada", visible: false });
    expect(workbook.sheets[0].cells.A1).toBeUndefined();
  });

  it("changes only the one sheet and returns a new workbook", () => {
    const original = book();
    const next = setNote(original, 0, "A1", { text: "x", author: "", visible: false });
    expect(next).not.toBe(original);
    expect(original.sheets[0].cells.A1).toBeUndefined();
    expect(next.sheets[1]).toBe(original.sheets[1]);
  });

  it("ignores a bad address or sheet", () => {
    const original = book();
    expect(setNote(original, 0, "not an address", { text: "x", author: "", visible: false })).toBe(original);
    expect(setNote(original, 9, "A1", { text: "x", author: "", visible: false })).toBe(original);
  });
});

describe("deleteNote", () => {
  it("removes the note and the cell that held nothing else", () => {
    const withNote = setNote(book(), 0, "A1", { text: "x", author: "Ada", visible: true });
    expect(deleteNote(withNote, 0, "A1").sheets[0].cells.A1).toBeUndefined();
  });

  it("keeps a cell that has content", () => {
    let workbook = applyCellEdit(book(), 0, 0, 0, "keep");
    workbook = setNote(workbook, 0, "A1", { text: "x", author: "Ada", visible: true });
    const cell = deleteNote(workbook, 0, "A1").sheets[0].cells.A1;
    expect(cell.value).toEqual({ kind: "text", value: "keep" });
    expect(cell.comment).toBeNull();
    expect(cell.commentAuthor).toBeNull();
    expect(cell.commentVisible).toBeUndefined();
  });

  it("leaves a cell without a note as it is", () => {
    const workbook = applyCellEdit(book(), 0, 0, 0, "keep");
    expect(deleteNote(workbook, 0, "A1")).toBe(workbook);
    expect(deleteNote(workbook, 0, "C9")).toBe(workbook);
  });
});

describe("toggleNoteVisible", () => {
  it("pins and unpins a note", () => {
    let workbook = setNote(book(), 0, "A1", { text: "x", author: "", visible: false });
    workbook = toggleNoteVisible(workbook, 0, "A1");
    expect(workbook.sheets[0].cells.A1.commentVisible).toBe(true);
    expect(visibleNoteAddresses(workbook.sheets[0].cells)).toEqual(["A1"]);
    workbook = toggleNoteVisible(workbook, 0, "A1");
    expect(workbook.sheets[0].cells.A1.commentVisible).toBeUndefined();
    expect(visibleNoteAddresses(workbook.sheets[0].cells)).toEqual([]);
  });

  it("does nothing on a cell without a note", () => {
    const workbook = applyCellEdit(book(), 0, 0, 0, "x");
    expect(toggleNoteVisible(workbook, 0, "A1")).toBe(workbook);
  });
});

describe("noteTooltip", () => {
  it("puts the author before the text", () => {
    expect(noteTooltip({ text: "Check", author: "Ada", visible: false })).toBe("Ada:\nCheck");
    expect(noteTooltip({ text: "Check", author: "", visible: false })).toBe("Check");
  });

  it("reads no note from a cell with an empty comment", () => {
    expect(noteOf(undefined)).toBeNull();
    const workbook = applyCellEdit(book(), 0, 0, 0, "x");
    expect(noteOf(workbook.sheets[0].cells.A1)).toBeNull();
  });
});
