/**
 * Formula-auditing tests.
 *
 * `audit.ts` is the read side of the dependency graph: it answers "what does
 * this cell read", "what reads this cell", "which formulas are in a cycle" and
 * "which formulas point at something that is not there". The graph itself is
 * covered by `deps.test.ts`; these tests pin the auditing API on top of it.
 */
import { describe, expect, it } from "vitest";
import { newSheet, newSpreadsheetTable, newWorkbook, type Workbook } from "../../lib/office-types";
import { applyCellEdit } from "./cells";
import { findCircularReferences, invalidReferences, traceDependents, tracePrecedents } from "./audit";

function book(): Workbook {
  return { ...newWorkbook("Audit"), sheets: [newSheet("Sheet1")] };
}

function withData(rows: Array<[string, string]>, workbook: Workbook = book()): Workbook {
  let next = workbook;
  for (const [address, input] of rows) {
    const position = address.match(/^([A-Z]+)(\d+)$/)!;
    const column = position[1].split("").reduce((sum, letter) => sum * 26 + letter.charCodeAt(0) - 64, 0) - 1;
    next = applyCellEdit(next, 0, Number(position[2]) - 1, column, input);
  }
  return next;
}

function sorted(nodes: Array<{ sheet: string; address: string; depth: number }>): Array<[string, number]> {
  return nodes
    .map((node) => [`${node.sheet}!${node.address}`, node.depth] as [string, number])
    .sort((a, b) => a[0].localeCompare(b[0]));
}

describe("tracePrecedents", () => {
  it("walks ranges, chains and cross-sheet references", () => {
    let workbook = book();
    workbook = { ...workbook, sheets: [workbook.sheets[0], newSheet("Data")] };
    workbook = applyCellEdit(workbook, 1, 0, 0, "5");
    workbook = applyCellEdit(workbook, 0, 0, 0, "=Data!A1+1");
    workbook = applyCellEdit(workbook, 0, 0, 1, "=A1*2");
    workbook = applyCellEdit(workbook, 0, 2, 0, "=SUM(A1:B1)");

    // B1 depends on A1; A1 depends on Data!A1.
    expect(sorted(tracePrecedents(workbook, "Sheet1", "B1"))).toEqual([
      ["Data!A1", 2],
      ["Sheet1!A1", 1],
    ]);

    // A3 reads the A1:B1 range, so A1 and B1 are direct and Data!A1 is one deeper.
    expect(sorted(tracePrecedents(workbook, "Sheet1", "A3"))).toEqual([
      ["Data!A1", 2],
      ["Sheet1!A1", 1],
      ["Sheet1!B1", 1],
    ]);
  });

  it("returns nothing for a cell that is not a formula", () => {
    const workbook = withData([
      ["A1", "1"],
      ["A2", "2"],
    ]);
    expect(tracePrecedents(workbook, "Sheet1", "A1")).toEqual([]);
    expect(tracePrecedents(workbook, "Sheet1", "Z99")).toEqual([]);
  });

  it("does not loop forever on a cycle", () => {
    const workbook = withData([
      ["A1", "=A2"],
      ["A2", "=A1"],
    ]);
    // A2 is A1's only precedent, and the walk stops when it comes back.
    expect(sorted(tracePrecedents(workbook, "Sheet1", "A1"))).toEqual([["Sheet1!A2", 1]]);
  });
});

describe("traceDependents", () => {
  it("follows the chain in the other direction", () => {
    const workbook = withData([
      ["A1", "10"],
      ["A2", "20"],
      ["A3", "=A1+A2"],
      ["B1", "=A3*2"],
      ["C1", "=SUM(A1:A3)"],
    ]);
    expect(sorted(traceDependents(workbook, "Sheet1", "A1"))).toEqual([
      ["Sheet1!A3", 1],
      ["Sheet1!B1", 2],
      ["Sheet1!C1", 1],
    ]);
    expect(traceDependents(workbook, "Sheet1", "B1")).toEqual([]);
  });

  it("tracks cross-sheet dependents", () => {
    let workbook = book();
    workbook = { ...workbook, sheets: [workbook.sheets[0], newSheet("Data")] };
    workbook = applyCellEdit(workbook, 1, 0, 0, "5");
    workbook = applyCellEdit(workbook, 0, 0, 0, "=Data!A1*3");
    expect(sorted(traceDependents(workbook, "Data", "A1"))).toEqual([["Sheet1!A1", 1]]);
  });
});

describe("findCircularReferences", () => {
  it("reports nothing for a clean workbook", () => {
    const workbook = withData([
      ["A1", "1"],
      ["A2", "=A1+1"],
      ["A3", "=A2+1"],
    ]);
    expect(findCircularReferences(workbook)).toEqual([]);
  });

  it("reports a two-cell cycle once, with the start repeated", () => {
    const workbook = withData([
      ["A1", "=A2"],
      ["A2", "=A1"],
    ]);
    expect(findCircularReferences(workbook)).toEqual([["Sheet1!A1", "Sheet1!A2", "Sheet1!A1"]]);
  });

  it("reports a self-reference and a longer cycle", () => {
    const workbook = withData([
      ["A1", "=A1"],
      ["B1", "=B2"],
      ["B2", "=B3"],
      ["B3", "=B1"],
      ["C1", "=1+1"],
    ]);
    const cycles = findCircularReferences(workbook);
    expect(cycles).toHaveLength(2);
    expect(cycles).toContainEqual(["Sheet1!A1", "Sheet1!A1"]);
    expect(cycles).toContainEqual(["Sheet1!B1", "Sheet1!B2", "Sheet1!B3", "Sheet1!B1"]);
  });
});

describe("invalidReferences", () => {
  it("is empty for a valid workbook", () => {
    const workbook = withData([
      ["A1", "1"],
      ["A2", "=A1+1"],
      ["A3", "=SUM(A1:A2)"],
    ]);
    expect(invalidReferences(workbook)).toEqual([]);
  });

  it("flags references to a sheet that does not exist", () => {
    const workbook = withData([["A1", "=Missing!A1"]]);
    expect(invalidReferences(workbook)).toEqual([
      { sheet: "Sheet1", address: "A1", reference: "Missing!A1", reason: "missing-sheet" },
    ]);
  });

  it("flags formulas whose value is #REF!", () => {
    const workbook = withData([["A1", "=A0+1"]]);
    const issues = invalidReferences(workbook);
    expect(issues).toHaveLength(1);
    expect(issues[0].reason).toBe("error");
    expect(issues[0].address).toBe("A1");
    expect(issues[0].reference).toBe("=A0+1");
  });

  it("flags a structured reference to an unknown table column", () => {
    const table = newSpreadsheetTable("Sales", "A1:B4", ["Item", "Amount"]);
    let workbook: Workbook = { ...book(), sheets: [{ ...newSheet("Sheet1"), tables: [table] }] };
    workbook = applyCellEdit(workbook, 0, 0, 3, "=SUM(Sales[Nope])");
    const issues = invalidReferences(workbook);
    expect(issues).toHaveLength(1);
    expect(issues[0]).toMatchObject({ sheet: "Sheet1", address: "D1", reason: "error" });
  });

  it("does not flag a valid cross-sheet or structured reference", () => {
    const table = newSpreadsheetTable("Sales", "A1:B4", ["Item", "Amount"]);
    let workbook: Workbook = { ...book(), sheets: [{ ...newSheet("Sheet1"), tables: [table] }, newSheet("Data")] };
    workbook = applyCellEdit(workbook, 0, 0, 3, "=SUM(Sales[Amount])+Data!A1");
    expect(invalidReferences(workbook)).toEqual([]);
  });
});
