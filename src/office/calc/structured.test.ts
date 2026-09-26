/**
 * Structured table reference tests.
 *
 * The engine translates `Sales[Amount]` into an ordinary A1 range before the
 * evaluator or the dependency graph sees it, so these tests cover the
 * translation layer and then the two integration points that matter:
 * `=SUM(Sales[Amount])` evaluates through the workbook engine, and editing a
 * body cell recalculates the formulas that read the table.
 */
import { describe, expect, it } from "vitest";
import { newSheet, newSpreadsheetTable, newWorkbook, type SpreadsheetTable, type Workbook } from "../../lib/office-types";
import { applyCellEdit, computeSheetValues, computeWorkbookValues, lastComputeStats } from "./cells";
import type { FormulaContext, Scalar } from "./formula";
import { evaluateFormula, isError } from "./formula";
import {
  collectStructuredReferences,
  columnNameOf,
  parseStructuredReference,
  resolveStructuredReference,
  structuredReferenceRanges,
  tableBodyRange,
  tableByName,
  tableColumnBodyRange,
  tableHeaderRange,
  tableTotalsRange,
} from "./structured";

function salesTable(overrides: Partial<SpreadsheetTable> = {}): SpreadsheetTable {
  return { ...newSpreadsheetTable("Sales", "A1:B4", ["Item", "Amount"]), ...overrides };
}

function totalsTable(): SpreadsheetTable {
  return { ...newSpreadsheetTable("Sales", "A1:B5", ["Item", "Amount"]), hasTotals: true };
}

function tableBook(tables: SpreadsheetTable[]): Workbook {
  return { ...newWorkbook("Tables"), sheets: [{ ...newSheet("Sheet1"), tables }] };
}

function context(tables: SpreadsheetTable[], values: Record<string, Scalar> = {}, currentRow?: number): FormulaContext {
  return {
    getValue: (_sheet, address) => (address in values ? values[address] : ""),
    sheetNames: ["Sheet1"],
    currentSheet: "Sheet1",
    tables,
    currentRow,
  };
}

describe("table helpers", () => {
  it("finds tables by name, ignoring case", () => {
    const sales = salesTable();
    expect(tableByName([sales], "Sales")).toBe(sales);
    expect(tableByName([sales], "sales")).toBe(sales);
    expect(tableByName([sales], "SALES")).toBe(sales);
    expect(tableByName([sales], "Nope")).toBeNull();
    expect(tableByName(undefined, "Sales")).toBeNull();
  });

  it("reads column names by index and returns null out of range", () => {
    const sales = salesTable();
    expect(columnNameOf(sales, 0)).toBe("Item");
    expect(columnNameOf(sales, 1)).toBe("Amount");
    expect(columnNameOf(sales, 2)).toBeNull();
    expect(columnNameOf(sales, -1)).toBeNull();
  });

  it("derives the body range from the header and totals flags", () => {
    expect(tableBodyRange(salesTable())).toEqual({ start: "A2", end: "B4" });
    expect(tableBodyRange(salesTable({ hasHeaders: false }))).toEqual({ start: "A1", end: "B4" });
    expect(tableBodyRange(totalsTable())).toEqual({ start: "A2", end: "B4" });
    // A table that is only a header and a totals row has no data body.
    expect(tableBodyRange(salesTable({ hasHeaders: true, hasTotals: true, range: "A1:B2" }))).toBeNull();
  });

  it("derives the header and totals ranges", () => {
    expect(tableHeaderRange(salesTable())).toEqual({ start: "A1", end: "B1" });
    expect(tableHeaderRange(salesTable({ hasHeaders: false }))).toBeNull();
    expect(tableTotalsRange(totalsTable())).toEqual({ start: "A5", end: "B5" });
    expect(tableTotalsRange(salesTable())).toBeNull();
  });

  it("slices one column down the body, case-insensitively", () => {
    expect(tableColumnBodyRange(salesTable(), "Amount")).toEqual({ start: "B2", end: "B4" });
    expect(tableColumnBodyRange(salesTable(), "amount")).toEqual({ start: "B2", end: "B4" });
    expect(tableColumnBodyRange(salesTable(), "Nope")).toBeNull();
  });
});

describe("parseStructuredReference", () => {
  const sales = salesTable();

  it("parses Table[Column] as a data reference", () => {
    expect(parseStructuredReference("Sales[Amount]", [sales])).toEqual({ table: sales, column: "Amount", kind: "data" });
  });

  it("parses the this-row forms with and without nested brackets", () => {
    expect(parseStructuredReference("Sales[@Amount]", [sales])).toEqual({ table: sales, column: "Amount", kind: "thisRow" });
    expect(parseStructuredReference("Sales[@[Amount]]", [sales])).toEqual({ table: sales, column: "Amount", kind: "thisRow" });
  });

  it("parses #All/#Headers/#Data/#Totals", () => {
    expect(parseStructuredReference("Sales[#All]", [sales])?.kind).toBe("all");
    expect(parseStructuredReference("Sales[#Headers]", [sales])?.kind).toBe("headers");
    expect(parseStructuredReference("Sales[#Data]", [sales])?.kind).toBe("data");
    expect(parseStructuredReference("Sales[#Totals]", [totalsTable()])?.kind).toBe("totals");
    expect(parseStructuredReference("Sales[#All]", [sales])?.column).toBeNull();
  });

  it("is case-insensitive for the table and the column", () => {
    const parsed = parseStructuredReference("sales[amount]", [sales]);
    expect(parsed?.table).toBe(sales);
    expect(parsed?.column).toBe("Amount");
    expect(parsed?.kind).toBe("data");
  });

  it("returns null when the table is not declared", () => {
    expect(parseStructuredReference("Nope[Amount]", [sales])).toBeNull();
    expect(parseStructuredReference("Nope[Amount]", undefined)).toBeNull();
    expect(parseStructuredReference("Amount", [sales])).toBeNull();
    expect(parseStructuredReference("Sales[Amount", [sales])).toBeNull();
  });
});

describe("resolveStructuredReference", () => {
  const sales = salesTable();
  const totals = totalsTable();

  it("resolves whole-part ranges", () => {
    expect(resolveStructuredReference("Sales[#All]", [sales])).toBe("A1:B4");
    expect(resolveStructuredReference("Sales[#Headers]", [sales])).toBe("A1:B1");
    expect(resolveStructuredReference("Sales[#Data]", [sales])).toBe("A2:B4");
    expect(resolveStructuredReference("Sales[#Totals]", [totals])).toBe("A5:B5");
    expect(resolveStructuredReference("Sales[Amount]", [sales])).toBe("B2:B4");
  });

  it("resolves a column inside a header, totals or all specifier", () => {
    expect(resolveStructuredReference("Sales[[#Headers],[Amount]]", [sales])).toBe("B1");
    expect(resolveStructuredReference("Sales[[#Totals],[Amount]]", [totals])).toBe("B5");
    expect(resolveStructuredReference("Sales[[#All],[Amount]]", [totals])).toBe("B1:B5");
  });

  it("resolves a this-row column to the formula's own row", () => {
    expect(resolveStructuredReference("Sales[@Amount]", [sales], 2)).toBe("B2");
    expect(resolveStructuredReference("Sales[@[Amount]]", [sales], 3)).toBe("B3");
    // Without a row to anchor to, or outside the table, the reference is unresolved.
    expect(resolveStructuredReference("Sales[@Amount]", [sales])).toBeNull();
    expect(resolveStructuredReference("Sales[@Amount]", [sales], 9)).toBeNull();
  });

  it("returns null for unknown columns and missing parts", () => {
    expect(resolveStructuredReference("Sales[Nope]", [sales])).toBeNull();
    expect(resolveStructuredReference("Sales[#Totals]", [sales])).toBeNull();
    expect(resolveStructuredReference("Nope[Amount]", [sales])).toBeNull();
  });
});

describe("structuredReferenceRanges", () => {
  const sales = salesTable();

  it("collects every resolvable range in a formula", () => {
    expect(structuredReferenceRanges("=SUM(Sales[Amount])+Sales[@Amount]", [sales], 3)).toEqual(["B2:B4", "B3"]);
    expect(structuredReferenceRanges("=Sales[#Headers]", [sales])).toEqual(["A1:B1"]);
  });

  it("ignores unresolvable and quoted bracket text", () => {
    expect(structuredReferenceRanges("=SUM(Nope[Amount])", [sales])).toEqual([]);
    expect(structuredReferenceRanges('=CONCAT("Sales[Amount]",Sales[Item])', [sales])).toEqual(["A2:A4"]);
    expect(collectStructuredReferences('="Sales[Amount]"')).toEqual([]);
    expect(collectStructuredReferences("=Sales[@[Amount]]*2")).toEqual(["Sales[@[Amount]]"]);
  });
});

describe("structured references through the formula engine", () => {
  const sales = salesTable();

  it("evaluates =SUM(Sales[Amount]) and a this-row reference", () => {
    const ctx = context([sales], { A2: "Widget", B2: 10, B3: 20, B4: 30 });
    expect(evaluateFormula("=SUM(Sales[Amount])", ctx)).toBe(60);
    expect(evaluateFormula("=Sales[@Amount]", context([sales], { B2: 10 }, 2))).toBe(10);
  });

  it("reports #REF! for a declared table and #NAME? for an unknown one", () => {
    const failing = evaluateFormula("=SUM(Sales[Nope])", context([sales]));
    expect(isError(failing)).toBe(true);
    expect((failing as { code: string }).code).toBe("#REF!");
    const unknown = evaluateFormula("=SUM(Missing[Amount])", context([sales]));
    expect(isError(unknown)).toBe(true);
    expect((unknown as { code: string }).code).toBe("#NAME?");
  });
});

describe("structured references in the workbook engine", () => {
  it("evaluates =SUM(Sales[Amount]) through the real workbook engine", () => {
    let workbook = tableBook([salesTable()]);
    workbook = applyCellEdit(workbook, 0, 1, 1, "10");
    workbook = applyCellEdit(workbook, 0, 2, 1, "20");
    workbook = applyCellEdit(workbook, 0, 3, 1, "30");
    workbook = applyCellEdit(workbook, 0, 0, 3, "=SUM(Sales[Amount])");
    expect(workbook.sheets[0].cells.D1?.value).toEqual({ kind: "number", value: 60 });
    expect(computeSheetValues(workbook, workbook.sheets[0]).get("D1")).toBe(60);
  });

  it("evaluates =Sales[@Amount] at the formula's own row", () => {
    let workbook = tableBook([salesTable()]);
    workbook = applyCellEdit(workbook, 0, 1, 1, "10");
    workbook = applyCellEdit(workbook, 0, 2, 1, "20");
    workbook = applyCellEdit(workbook, 0, 1, 3, "=Sales[@Amount]");
    expect(workbook.sheets[0].cells.D2?.value).toEqual({ kind: "number", value: 10 });
    expect(computeSheetValues(workbook, workbook.sheets[0]).get("D2")).toBe(10);
  });

  it("never turns an unresolvable reference into a silent zero", () => {
    const unknownColumn = applyCellEdit(tableBook([salesTable()]), 0, 0, 3, "=SUM(Sales[Nope])");
    expect(unknownColumn.sheets[0].cells.D1?.value).toEqual({ kind: "error", value: "#REF!" });
    const unknownTable = applyCellEdit(tableBook([salesTable()]), 0, 0, 3, "=SUM(Missing[Amount])");
    expect(unknownTable.sheets[0].cells.D1?.value).toEqual({ kind: "error", value: "#NAME?" });
  });

  it("recalculates a dependent formula when a table body cell changes", () => {
    let workbook = tableBook([salesTable()]);
    workbook = applyCellEdit(workbook, 0, 1, 1, "10");
    workbook = applyCellEdit(workbook, 0, 2, 1, "20");
    workbook = applyCellEdit(workbook, 0, 3, 1, "30");
    workbook = applyCellEdit(workbook, 0, 0, 3, "=SUM(Sales[Amount])");
    computeSheetValues(workbook, workbook.sheets[0]);
    workbook = applyCellEdit(workbook, 0, 1, 1, "100");
    const values = computeWorkbookValues(workbook);
    expect(values.get("Sheet1!D1")).toBe(150);
    expect(lastComputeStats(workbook)?.mode).toBe("incremental");
  });

  it("recalculates a this-row formula when its own table cell changes", () => {
    let workbook = tableBook([salesTable()]);
    workbook = applyCellEdit(workbook, 0, 1, 1, "10");
    workbook = applyCellEdit(workbook, 0, 2, 1, "20");
    workbook = applyCellEdit(workbook, 0, 1, 3, "=Sales[@Amount]*2");
    computeSheetValues(workbook, workbook.sheets[0]);
    workbook = applyCellEdit(workbook, 0, 1, 1, "7");
    expect(computeWorkbookValues(workbook).get("Sheet1!D2")).toBe(14);
  });
});
