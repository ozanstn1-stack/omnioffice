/**
 * Pure cell-model operations for the Calc engine.
 *
 * Everything here is a plain function over the `Workbook` model so it can be
 * unit tested without mounting the editor or talking to Tauri. `CalcEditor`
 * is the only consumer; keeping the logic out of the component is what makes
 * the commit path (inline editor, formula bar, paste, fill) testable at all.
 */
import {
  defaultCellStyle,
  emptyCell,
  type Cell,
  type CellStyle,
  type CellValue,
  type Sheet,
  type Workbook,
} from "../../lib/office-types";
import {
  ERR,
  FormulaError,
  MAX_COLS,
  MAX_ROWS,
  addressesInRange,
  asScalar,
  collectReferences,
  evaluateFormulaResult,
  formatAddress,
  formatNumber,
  isError,
  parseAddress,
  rangeSize,
  type CellMatrix,
  type Scalar,
} from "./formula";
import { structuredReferenceRanges } from "./structured";

export function cellValueToScalar(value: CellValue): Scalar {
  switch (value.kind) {
    case "number":
      return value.value;
    case "text":
      return value.value;
    case "bool":
      return value.value;
    case "error":
      return new FormulaError(value.value);
    default:
      return "";
  }
}

export function scalarToCellValue(value: Scalar): CellValue {
  if (isError(value)) return { kind: "error", value: value.code };
  if (typeof value === "number") return { kind: "number", value };
  if (typeof value === "boolean") return { kind: "bool", value };
  return { kind: "text", value: String(value) };
}

/** Parses what a user typed into a cell into a typed value. */
export function parseInputValue(text: string): CellValue {
  const trimmed = text.trim();
  if (trimmed === "") return { kind: "empty" };
  if (/^-?\d+([.,]\d+)?$/.test(trimmed)) {
    const numeric = Number(trimmed.replace(",", "."));
    if (Number.isFinite(numeric)) return { kind: "number", value: numeric };
  }
  if (/^(true|false)$/i.test(trimmed)) return { kind: "bool", value: trimmed.toLowerCase() === "true" };
  if (/^#(REF|VALUE|NAME|DIV\/0|N\/A|NUM)/i.test(trimmed)) return { kind: "error", value: trimmed.toUpperCase() };
  return { kind: "text", value: text };
}

export function isDefaultCellStyle(style: CellStyle): boolean {
  const base = defaultCellStyle();
  return (
    !style.font &&
    style.sizePt === base.sizePt &&
    !style.bold &&
    !style.italic &&
    !style.underline &&
    !style.strike &&
    !style.color &&
    !style.fill &&
    style.align === base.align &&
    style.valign === base.valign &&
    !style.wrap &&
    !style.rotation &&
    !style.borders.top &&
    !style.borders.right &&
    !style.borders.bottom &&
    !style.borders.left &&
    style.numberFormat === base.numberFormat
  );
}

/** True when a cell carries no value, formula, comment or formatting. */
export function isBlankCell(cell: Cell): boolean {
  return cell.value.kind === "empty" && !cell.formula && !cell.comment && isDefaultCellStyle(cell.style);
}

function replaceSheet(workbook: Workbook, index: number, sheet: Sheet): Workbook {
  return { ...workbook, sheets: workbook.sheets.map((candidate, at) => (at === index ? sheet : candidate)) };
}

/**
 * Applies a typed value to one cell and returns the resulting workbook.
 *
 * A formula is evaluated against the workbook that *already contains the new
 * formula text*, so the cached value persisted into `.oswk`/XLSX/ODS on save is
 * never one edit behind - the bug that made chained commits write stale
 * numbers to disk.
 */
export function applyCellEdit(workbook: Workbook, sheetIndex: number, row: number, col: number, rawValue: string): Workbook {
  const index = Math.min(Math.max(0, sheetIndex), workbook.sheets.length - 1);
  const target = workbook.sheets[index];
  if (!target) return workbook;
  const address = formatAddress(row, col);
  const current = target.cells[address] ?? emptyCell();
  const isFormula = rawValue.startsWith("=");
  const staged: Cell = isFormula
    ? { ...current, formula: rawValue, value: { kind: "empty" } }
    : { ...current, formula: null, value: parseInputValue(rawValue) };

  const cells = { ...target.cells };
  if (isBlankCell(staged)) delete cells[address];
  else cells[address] = staged;
  const stagedSheet: Sheet = {
    ...target,
    cells,
    rowCount: Math.max(target.rowCount, row + 51),
    colCount: Math.max(target.colCount, col + 6),
  };
  const stagedWorkbook = replaceSheet(workbook, index, stagedSheet);
  if (!isFormula) return markDirty(stagedWorkbook, workbook, target.name, [address]);

  // Marking the staged workbook as dirty lets the formula evaluate against a
  // cache that only recalculates the cells this edit can affect.
  const stagedDirty = markDirty(stagedWorkbook, workbook, target.name, [address]);
  const resolved: Cell = { ...staged, value: formulaResult(rawValue, stagedDirty, stagedSheet, row + 1) };
  const final = replaceSheet(stagedDirty, index, { ...stagedSheet, cells: { ...stagedSheet.cells, [address]: resolved } });
  return markDirty(final, workbook, target.name, [address]);
}

/** Applies a block of typed values starting at a cell (used by paste). */
export function applyCellEdits(workbook: Workbook, sheetIndex: number, start: { row: number; col: number }, values: string[][]): Workbook {
  let next = workbook;
  values.forEach((line, rowOffset) => {
    line.forEach((value, colOffset) => {
      next = applyCellEdit(next, sheetIndex, start.row + rowOffset, start.col + colOffset, value);
    });
  });
  return next;
}

/**
 * Shifts the row part of every relative reference in a formula by `delta`.
 * Absolute rows (`$1`) and non-references (bare numbers) are untouched. The
 * lookbehind stops the rewrite from biting into sheet names such as `Data1`,
 * while still allowing the `!` that precedes a sheet-qualified reference.
 */
export function shiftFormulaRows(formula: string | null, delta: number): string | null {
  if (!formula || delta === 0) return formula;
  return formula.replace(/(?<![A-Za-z0-9_$])(\$?)([A-Za-z]{1,3})(\$?)(\d{1,7})(?![A-Za-z0-9_(])/g, (match, dollarCol: string, letters: string, dollarRow: string, digits: string) => {
    if (parseAddress(`${letters}${digits}`) === null) return match;
    if (dollarRow) return match;
    const nextRow = Number(digits) + delta;
    if (nextRow < 1 || nextRow > 1_048_576) return match;
    return `${dollarCol}${letters}${nextRow}`;
  });
}

// ---------------------------------------------------------------------------
// Dependency graph and incremental recalculation
// ---------------------------------------------------------------------------

/**
 * A range with more cells than this makes every formula that reads it
 * "global": it is recalculated after any edit. Expanding a 100 000-cell range
 * into individual graph edges would cost more memory than the recalculation it
 * tries to avoid.
 */
const RANGE_EDGE_LIMIT = 4096;
/** Dynamic-array passes; two settle every spill whose extent is stable. */
const SPILL_PASS_LIMIT = 3;

interface ComputeEntry {
  /** Every cell value, sheet-qualified as `Sheet!A1`. */
  values: Map<string, Scalar>;
  /** Formula cell -> the cells it reads. */
  precedents: Map<string, Set<string>>;
  /** Cell -> the formulas that read it. */
  dependents: Map<string, Set<string>>;
  /** Formulas that have to recompute after any change (volatile/opaque refs). */
  globalDependents: Set<string>;
  /** Formula cell -> the target cells its dynamic array spills into. */
  spills: Map<string, string[]>;
  hasSpills: boolean;
}

const computeEntries = new WeakMap<Workbook, ComputeEntry>();
const DIRTY = Symbol("oswk.calc.dirty");

interface DirtyMeta {
  base: Workbook;
  sheet: string;
  addresses: string[];
}

/**
 * Marks a workbook as "one edit away from `base`", so the next value
 * computation can recalculate only the affected cells. The attribute is
 * non-enumerable, so serialising the model never sees it.
 */
function markDirty<T extends Workbook>(workbook: T, base: Workbook, sheet: string, addresses: string[]): T {
  Object.defineProperty(workbook, DIRTY, { value: { base, sheet, addresses }, enumerable: false, configurable: true });
  return workbook;
}

function keyOf(sheet: string, address: string): string {
  return `${sheet}!${address}`;
}

function splitKey(key: string): { sheet: string; address: string } {
  const at = key.lastIndexOf("!");
  return { sheet: key.slice(0, at), address: key.slice(at + 1) };
}

/** Sheet-scoped names shadow workbook-level ones, compared case-insensitively. */
function namesContext(workbook: Workbook): { names: Record<string, string>; scoped: Map<string, Record<string, string>> } {
  const names: Record<string, string> = {};
  const scoped = new Map<string, Record<string, string>>();
  for (const entry of workbook.names ?? []) {
    if (entry.sheet === null || entry.sheet === undefined) names[entry.name.toUpperCase()] = entry.definition;
    else {
      const bucket = scoped.get(entry.sheet) ?? {};
      bucket[entry.name.toUpperCase()] = entry.definition;
      scoped.set(entry.sheet, bucket);
    }
  }
  return { names, scoped };
}

function addEdges(entry: ComputeEntry, workbook: Workbook, sheetName: string, address: string, formula: string): void {
  const key = keyOf(sheetName, address);
  const refs = collectReferences(formula);
  if (!refs || refs.volatile) {
    entry.globalDependents.add(key);
    if (!refs) return;
  }
  const precedents = entry.precedents.get(key) ?? new Set<string>();
  const addReference = (sheet: string, target: string) => precedents.add(keyOf(sheet, target));
  const addRange = (sheet: string, range: string): boolean => {
    const size = rangeSize(range);
    if (!size || size.rows * size.cols > RANGE_EDGE_LIMIT) return false;
    for (const target of addressesInRange(range, RANGE_EDGE_LIMIT + 1)) addReference(sheet, target);
    return true;
  };
  const localNames = ((): Record<string, string> => {
    const context = namesContext(workbook);
    return { ...context.names, ...(context.scoped.get(sheetName) ?? {}) };
  })();
  for (const ref of refs.refs) addReference(ref.sheet ?? sheetName, ref.address);
  for (const range of refs.ranges) {
    if (!addRange(range.sheet ?? sheetName, range.range)) entry.globalDependents.add(key);
  }
  for (const name of refs.names) {
    const definition = localNames[name];
    const summary = definition ? collectReferences(definition) : null;
    if (!summary) {
      entry.globalDependents.add(key);
      continue;
    }
    if (summary.volatile) entry.globalDependents.add(key);
    for (const ref of summary.refs) addReference(ref.sheet ?? sheetName, ref.address);
    for (const range of summary.ranges) {
      if (!addRange(range.sheet ?? sheetName, range.range)) entry.globalDependents.add(key);
    }
  }
  // Structured references (`SUM(Sales[Amount])`) read the table's cells, so a
  // change to any body cell has to mark this formula dirty.
  const sheet = workbook.sheets.find((candidate) => candidate.name === sheetName);
  const position = parseAddress(address);
  for (const range of structuredReferenceRanges(formula, sheet?.tables, position ? position.row + 1 : undefined)) {
    if (!addRange(sheetName, range)) entry.globalDependents.add(key);
  }
  if (precedents.size > 0) {
    entry.precedents.set(key, precedents);
    for (const precedent of precedents) {
      const readers = entry.dependents.get(precedent) ?? new Set<string>();
      readers.add(key);
      entry.dependents.set(precedent, readers);
    }
  }
}

function removeEdges(entry: ComputeEntry, key: string): void {
  const precedents = entry.precedents.get(key);
  if (precedents) {
    for (const precedent of precedents) entry.dependents.get(precedent)?.delete(key);
    entry.precedents.delete(key);
  }
  entry.globalDependents.delete(key);
}

interface EvalScope {
  workbook: Workbook;
  entry: ComputeEntry;
  sheets: Map<string, Sheet>;
  sheetNames: string[];
  names: { names: Record<string, string>; scoped: Map<string, Record<string, string>> };
  resolving: Set<string>;
  /** Formula cells (re)computed in this pass. */
  computed: Set<string>;
  /** Full pass: every formula is recomputed, regardless of the cache. */
  full: boolean;
  /** Incremental pass: the formulas that must be recomputed. */
  dirty: Set<string>;
  /** Spill target key -> the source cell that owns it. */
  spillTargets: Map<string, string>;
}

/** True when a cell holds something a spill may not overwrite. */
function cellHasContent(cell: Cell | undefined): boolean {
  if (!cell) return false;
  return cell.formula !== null || cell.comment !== null || cell.value.kind !== "empty";
}

/**
 * Writes a dynamic array's extra cells into the value map.
 *
 * Returns false when the array cannot spill: a target cell already holds data
 * or another formula's spill, which Excel reports as `#SPILL!`.
 */
function planSpill(scope: EvalScope, key: string, sheetName: string, address: string, matrix: CellMatrix): boolean {
  const height = matrix.length;
  const width = Math.max(0, ...matrix.map((row) => row.length));
  if (height <= 1 && width <= 1) return true;
  const origin = parseAddress(address);
  if (!origin) return false;
  const sheet = scope.sheets.get(sheetName);
  const plan: Array<{ key: string; value: Scalar }> = [];
  for (let row = 0; row < height; row += 1) {
    for (let col = 0; col < width; col += 1) {
      if (row === 0 && col === 0) continue;
      const targetRow = origin.row + row;
      const targetCol = origin.col + col;
      if (targetRow >= MAX_ROWS || targetCol >= MAX_COLS) return false;
      const targetAddress = formatAddress(targetRow, targetCol);
      const targetKey = keyOf(sheetName, targetAddress);
      const owner = scope.spillTargets.get(targetKey);
      if (owner !== undefined && owner !== key) return false;
      if (owner === undefined && cellHasContent(sheet?.cells[targetAddress])) return false;
      plan.push({ key: targetKey, value: matrix[row]?.[col] ?? "" });
    }
  }
  for (const item of plan) {
    scope.entry.values.set(item.key, item.value);
    scope.spillTargets.set(item.key, key);
  }
  return true;
}

function evaluateCell(scope: EvalScope, key: string): Scalar {
  if (scope.resolving.has(key)) return ERR.circular();
  if (scope.computed.has(key)) return scope.entry.values.get(key) ?? "";
  if (!scope.full && !scope.dirty.has(key)) return scope.entry.values.get(key) ?? "";
  const { sheet: sheetName, address } = splitKey(key);
  const sheet = scope.sheets.get(sheetName);
  if (!sheet) return ERR.ref();
  const cell = sheet.cells[address];
  if (!cell?.formula) {
    // A seeded spill value wins over the (empty) model cell it lives in.
    const seated = scope.entry.values.get(key);
    if (seated !== undefined && scope.full) {
      scope.computed.add(key);
      return seated;
    }
    const scalar = cellValueToScalar(cell?.value ?? { kind: "empty" });
    scope.entry.values.set(key, scalar);
    scope.computed.add(key);
    return scalar;
  }
  const position = parseAddress(address);
  scope.resolving.add(key);
  let result: Scalar | CellMatrix = evaluateFormulaResult(cell.formula, {
    getValue: (sheetName2, address2) => evaluateCell(scope, keyOf(sheetName2 ?? sheetName, address2)),
    sheetNames: scope.sheetNames,
    currentSheet: sheetName,
    names: { ...scope.names.names, ...(scope.names.scoped.get(sheetName) ?? {}) },
    tables: sheet.tables ?? [],
    currentRow: position ? position.row + 1 : undefined,
  });
  scope.resolving.delete(key);
  if (Array.isArray(result)) {
    const spilled = planSpill(scope, key, sheetName, address, result);
    result = spilled ? asScalar(result) : ERR.spill();
  }
  scope.entry.values.set(key, result);
  scope.computed.add(key);
  return result;
}

function sameOwners(left: Map<string, string>, right: Map<string, string>): boolean {
  if (left.size !== right.size) return false;
  for (const [key, value] of left) {
    if (right.get(key) !== value) return false;
  }
  return true;
}

function groupSpills(owners: Map<string, string>): Map<string, string[]> {
  const spills = new Map<string, string[]>();
  for (const [target, source] of owners) {
    const list = spills.get(source) ?? [];
    list.push(target);
    spills.set(source, list);
  }
  return spills;
}

function buildGraph(entry: ComputeEntry, workbook: Workbook): void {
  for (const sheet of workbook.sheets) {
    for (const [address, cell] of Object.entries(sheet.cells)) {
      if (cell.formula) addEdges(entry, workbook, sheet.name, address, cell.formula);
    }
  }
}

function computeFull(workbook: Workbook): Map<string, Scalar> {
  const entry: ComputeEntry = {
    values: new Map(),
    precedents: new Map(),
    dependents: new Map(),
    globalDependents: new Set(),
    spills: new Map(),
    hasSpills: false,
  };
  buildGraph(entry, workbook);
  const sheetNames = workbook.sheets.map((candidate) => candidate.name);
  const sheets = new Map(workbook.sheets.map((sheet) => [sheet.name, sheet]));
  const names = namesContext(workbook);
  let owners = new Map<string, string>();
  let seeded = new Map<string, Scalar>();
  for (let pass = 0; pass < SPILL_PASS_LIMIT; pass += 1) {
    const scope: EvalScope = {
      workbook,
      entry,
      sheets,
      sheetNames,
      names,
      resolving: new Set(),
      computed: new Set(),
      full: true,
      dirty: new Set(),
      spillTargets: owners,
    };
    entry.values = seeded;
    let recomputed = 0;
    for (const sheet of workbook.sheets) {
      for (const address of Object.keys(sheet.cells)) {
        if (sheet.cells[address]?.formula) recomputed += 1;
        evaluateCell(scope, keyOf(sheet.name, address));
      }
    }
    computeStats.set(workbook, { mode: "full", recomputed });
    const nextSeeded = new Map<string, Scalar>();
    for (const target of scope.spillTargets.keys()) nextSeeded.set(target, entry.values.get(target) ?? "");
    const settled = sameOwners(scope.spillTargets, owners);
    owners = scope.spillTargets;
    seeded = nextSeeded;
    if (settled) break;
    if (owners.size === 0) break;
  }
  entry.spills = groupSpills(owners);
  entry.hasSpills = owners.size > 0;
  computeEntries.set(workbook, entry);
  return entry.values;
}

/**
 * Recalculates only the cells an edit can affect, using the graph of the
 * previous computation. Falls back to a full pass when the workbook is new,
 * carries spill ranges, or when the dirty cell's sheet cannot be matched.
 */
function recomputeIncremental(workbook: Workbook, entry: ComputeEntry, meta: DirtyMeta): Map<string, Scalar> {
  const sheets = new Map(workbook.sheets.map((sheet) => [sheet.name, sheet]));
  const sheet = sheets.get(meta.sheet);
  if (!sheet) return computeFull(workbook);
  for (const address of meta.addresses) {
    const key = keyOf(meta.sheet, address);
    removeEdges(entry, key);
    const cell = sheet.cells[address];
    if (!cell) {
      entry.values.delete(key);
      continue;
    }
    if (cell.formula) {
      if (entry.values.get(key) === undefined) entry.values.set(key, "");
      addEdges(entry, workbook, meta.sheet, address, cell.formula);
    } else {
      entry.values.set(key, cellValueToScalar(cell.value));
    }
  }
  const dirty = new Set<string>();
  const stack = meta.addresses.map((address) => keyOf(meta.sheet, address));
  while (stack.length > 0) {
    const key = stack.pop()!;
    if (dirty.has(key)) continue;
    dirty.add(key);
    for (const dependent of entry.dependents.get(key) ?? []) stack.push(dependent);
  }
  for (const key of entry.globalDependents) dirty.add(key);
  const scope: EvalScope = {
    workbook,
    entry,
    sheets,
    sheetNames: workbook.sheets.map((candidate) => candidate.name),
    names: namesContext(workbook),
    resolving: new Set(),
    computed: new Set(),
    full: false,
    dirty,
    spillTargets: new Map(),
  };
  let recomputed = 0;
  for (const key of dirty) {
    if (entry.precedents.has(key) || entry.globalDependents.has(key)) {
      recomputed += 1;
      evaluateCell(scope, key);
    }
  }
  computeStats.set(workbook, { mode: "incremental", recomputed });
  computeEntries.set(workbook, entry);
  return entry.values;
}

/** Evaluates every formula cell of the workbook with cycle protection. */
export function computeWorkbookValues(workbook: Workbook): Map<string, Scalar> {
  const meta = (workbook as unknown as Record<symbol, DirtyMeta | undefined>)[DIRTY];
  const parent = meta ? computeEntries.get(meta.base) : undefined;
  if (meta && parent && !parent.hasSpills) {
    try {
      return recomputeIncremental(workbook, parent, meta);
    } catch {
      // A graph that cannot explain the edit falls back to a clean full pass.
    }
  }
  return computeFull(workbook);
}

export interface ComputeStats {
  mode: "full" | "incremental";
  /** Formula cells evaluated by that computation. */
  recomputed: number;
}

const computeStats = new WeakMap<Workbook, ComputeStats>();

/** How the last value computation ran, for tests and performance checks. */
export function lastComputeStats(workbook: Workbook): ComputeStats | null {
  return computeStats.get(workbook) ?? null;
}

/** Precedent/dependent sets of the last computation, for the auditing UI. */
export function dependencyGraph(workbook: Workbook): { precedents: Map<string, Set<string>>; dependents: Map<string, Set<string>> } {
  if (!computeEntries.has(workbook)) computeWorkbookValues(workbook);
  const entry = computeEntries.get(workbook);
  return { precedents: entry?.precedents ?? new Map(), dependents: entry?.dependents ?? new Map() };
}

/** The cells one dynamic-array formula spills into (empty for a scalar). */
export function spillTargetsOf(workbook: Workbook, sheetName: string, address: string): string[] {
  if (!computeEntries.has(workbook)) computeWorkbookValues(workbook);
  return computeEntries.get(workbook)?.spills.get(keyOf(sheetName, address)) ?? [];
}

/** Values of one sheet keyed by bare address (for display). */
export function computeSheetValues(workbook: Workbook, sheet: Sheet): Map<string, Scalar> {
  const prefixed = computeWorkbookValues(workbook);
  const out = new Map<string, Scalar>();
  const prefix = `${sheet.name}!`;
  for (const [key, value] of prefixed) {
    if (key.startsWith(prefix)) out.set(key.slice(prefix.length), value);
  }
  return out;
}

export function formulaResult(formula: string, workbook: Workbook, sheet: Sheet, currentRow?: number): CellValue {
  const values = computeWorkbookValues(workbook);
  const result = evaluateFormulaResult(formula, {
    getValue: (sheetName, address) => values.get(`${sheetName ?? sheet.name}!${address}`) ?? "",
    sheetNames: workbook.sheets.map((candidate) => candidate.name),
    currentSheet: sheet.name,
    tables: sheet.tables ?? [],
    currentRow,
  });
  // A dynamic-array formula keeps its first value in the source cell; the rest
  // of the matrix is spilled by the value pass, not stored in the model.
  return scalarToCellValue(asScalar(result));
}

export function formatCellDisplay(value: Scalar, style: CellStyle): string {
  if (isError(value)) return value.code;
  if (typeof value === "number") return formatNumber(value, style.numberFormat || "General");
  if (typeof value === "boolean") return value ? "TRUE" : "FALSE";
  return String(value ?? "");
}

export function usedRange(sheet: Sheet): string {
  let maxRow = 0;
  let maxCol = 0;
  for (const address of Object.keys(sheet.cells)) {
    const position = parseAddress(address);
    if (!position) continue;
    maxRow = Math.max(maxRow, position.row);
    maxCol = Math.max(maxCol, position.col);
  }
  return `A1:${formatAddress(Math.max(0, maxRow), Math.max(0, maxCol))}`;
}

export function uniqueSheetName(workbook: Workbook, base: string): string {
  let index = 1;
  while (workbook.sheets.some((sheet) => sheet.name === (index === 1 ? base : `${base}${index}`))) index += 1;
  return index === 1 ? base : `${base}${index}`;
}
