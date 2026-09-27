/**
 * Large-workbook smoke tests.
 *
 * These are not micro-benchmarks: they fail if the engine stops being
 * dependency-driven (an edit that touches one cell must not recompute 100 000
 * formulas) or if a computation takes absurdly long. The time budgets are
 * deliberately generous so a loaded CI machine does not turn them flaky.
 */
import { describe, expect, it } from "vitest";
import { defaultCellStyle, newSheet, newWorkbook, type Cell, type Workbook } from "../../lib/office-types";
import { applyCellEdit, computeWorkbookValues, lastComputeStats } from "./cells";

function numberCell(value: number): Cell {
  return { value: { kind: "number", value }, formula: null, style: defaultCellStyle(), comment: null, link: null };
}

function formulaCell(formula: string): Cell {
  return { value: { kind: "empty" }, formula, style: defaultCellStyle(), comment: null, link: null };
}

/** A workbook with `rows` values in column A and `formulas` doubles in B. */
function buildWorkbook(rows: number, formulas: number): Workbook {
  const sheet = newSheet("Perf");
  const cells: Record<string, Cell> = {};
  for (let row = 0; row < rows; row += 1) cells[`A${row + 1}`] = numberCell(row + 1);
  for (let row = 0; row < formulas; row += 1) cells[`B${row + 1}`] = formulaCell(`=A${row + 1}*2`);
  return { ...newWorkbook("Perf"), sheets: [{ ...sheet, cells, rowCount: rows + 10, colCount: 8 }] };
}

describe("large workbook smoke", () => {
  it("computes 10 000 cells, then recalculates a single dependent incrementally", () => {
    const workbook = buildWorkbook(10_000, 1_000);
    const fullStart = Date.now();
    const values = computeWorkbookValues(workbook);
    const fullMs = Date.now() - fullStart;
    expect(values.get("Perf!B1")).toBe(2);
    expect(values.get("Perf!B1000")).toBe(2000);
    expect(lastComputeStats(workbook)?.mode).toBe("full");
    expect(fullMs).toBeLessThan(5_000);

    const edited = applyCellEdit(workbook, 0, 0, 0, "500");
    const editStart = Date.now();
    const next = computeWorkbookValues(edited);
    const editMs = Date.now() - editStart;
    expect(next.get("Perf!B1")).toBe(1000);
    expect(lastComputeStats(edited)).toEqual({ mode: "incremental", recomputed: 1 });
    expect(editMs).toBeLessThan(1_000);
  });

  it("computes 50 000 cells", () => {
    const workbook = buildWorkbook(50_000, 5_000);
    const start = Date.now();
    const values = computeWorkbookValues(workbook);
    const elapsed = Date.now() - start;
    expect(values.get("Perf!A50000")).toBe(50000);
    expect(values.get("Perf!B5000")).toBe(10000);
    expect(elapsed).toBeLessThan(10_000);
  });

  it("computes 100 000 cells and keeps an incremental edit cheap", () => {
    const workbook = buildWorkbook(100_000, 5_000);
    const start = Date.now();
    const values = computeWorkbookValues(workbook);
    const fullMs = Date.now() - start;
    expect(values.get("Perf!A100000")).toBe(100000);
    expect(fullMs).toBeLessThan(20_000);

    const edited = applyCellEdit(workbook, 0, 999, 0, "1");
    const editStart = Date.now();
    const next = computeWorkbookValues(edited);
    const editMs = Date.now() - editStart;
    expect(next.get("Perf!B1000")).toBe(2);
    expect(lastComputeStats(edited)).toEqual({ mode: "incremental", recomputed: 1 });
    expect(editMs).toBeLessThan(2_000);
  });

  it("walks a 1 000-cell dependency chain incrementally", () => {
    const sheet = newSheet("Chain");
    const cells: Record<string, Cell> = { A1: numberCell(1) };
    for (let row = 2; row <= 1_000; row += 1) cells[`A${row}`] = formulaCell(`=A${row - 1}+1`);
    const workbook: Workbook = { ...newWorkbook("Chain"), sheets: [{ ...sheet, cells }] };
    expect(computeWorkbookValues(workbook).get("Chain!A1000")).toBe(1000);

    const edited = applyCellEdit(workbook, 0, 0, 0, "11");
    const values = computeWorkbookValues(edited);
    expect(values.get("Chain!A1000")).toBe(1010);
    expect(lastComputeStats(edited)?.recomputed).toBe(999);
  });
});

/**
 * Dependency-chain workloads.
 *
 * The smoke tests above prove the engine does not fall over at size; these
 * pin the shape that matters for real workbooks: long dependency chains where
 * every cell feeds the next. A full recalculation must stay under a generous
 * wall clock, and a single edit must recompute only the downstream chain.
 *
 * The heavy 500 000-cell case is gated behind OSAK_PERF_HEAVY=1 because it is
 * minutes of CPU on a loaded machine:
 *   PowerShell: $env:OSAK_PERF_HEAVY="1"; npx vitest run src/office/calc/perf.test.ts
 */
function chainWorkbook(rows: number): Workbook {
  const sheet = newSheet("Chain");
  const cells: Record<string, Cell> = { A1: numberCell(1) };
  for (let row = 2; row <= rows; row += 1) cells[`A${row}`] = formulaCell(`=A${row - 1}+1`);
  return { ...newWorkbook("Chain"), sheets: [{ ...sheet, cells, rowCount: rows + 10, colCount: 8 }] };
}

const heavyIt = process.env.OSAK_PERF_HEAVY === "1" ? it : it.skip;

describe("dependency-chain performance", () => {
  it("recalculates a 20 000-cell dependency chain, then follows one edit", () => {
    const rows = 20_000;
    const workbook = chainWorkbook(rows);

    const fullStart = Date.now();
    const values = computeWorkbookValues(workbook);
    const fullMs = Date.now() - fullStart;
    expect(values.get(`Chain!A${rows}`)).toBe(rows);
    expect(lastComputeStats(workbook)?.mode).toBe("full");
    expect(fullMs).toBeLessThan(10_000);

    const edited = applyCellEdit(workbook, 0, 0, 0, "11");
    const editStart = Date.now();
    const next = computeWorkbookValues(edited);
    const editMs = Date.now() - editStart;
    expect(next.get(`Chain!A${rows}`)).toBe(rows + 10);
    // Only the downstream chain is recomputed, not the whole sheet.
    expect(lastComputeStats(edited)).toEqual({ mode: "incremental", recomputed: rows - 1 });
    expect(editMs).toBeLessThan(5_000);
  }, 60_000);

  it("keeps a 100 000-cell dependency chain under 15 seconds", () => {
    const rows = 100_000;
    const workbook = chainWorkbook(rows);
    const started = Date.now();
    const values = computeWorkbookValues(workbook);
    const elapsed = Date.now() - started;
    expect(values.get(`Chain!A${rows}`)).toBe(rows);
    expect(elapsed).toBeLessThan(15_000);
  }, 60_000);

  heavyIt(
    "heavy: recalculates a 500 000-cell dependency chain",
    () => {
      const rows = 500_000;
      const workbook = chainWorkbook(rows);
      const started = Date.now();
      const values = computeWorkbookValues(workbook);
      const elapsed = Date.now() - started;
      process.stderr.write(`calc heavy: 500 000-cell chain took ${elapsed}ms\n`);
      expect(values.get(`Chain!A${rows}`)).toBe(rows);
      expect(elapsed).toBeLessThan(120_000);
    },
    300_000,
  );
});

