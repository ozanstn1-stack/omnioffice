/**
 * Pivot table computation.
 *
 * A pivot is stored as its definition (source range, row/column/value fields)
 * and the rendered grid is derived from it every time the workbook or the
 * definition changes, so a pivot can never go stale in the model. The same
 * definition drives the `.oswk` round trip, the editor overlay and the
 * materialised values the XLSX exporter writes.
 */
import { type Cell, type PivotTable, type Sheet, type Workbook } from "../../lib/office-types";
import { computeWorkbookValues } from "./cells";
import { asScalar, compareScalars, formatAddress, isError, parseRange, toNumber, toText, type Scalar } from "./formula";

export interface PivotGrid {
  /** Header rows followed by body rows; written at the pivot's anchor. */
  grid: Scalar[][];
  /** Leading body columns that carry row-field labels. */
  rowFieldCount: number;
}

/** The field names a source range offers, taken from its first row. */
export function pivotFields(workbook: Workbook, sourceSheet: string, source: string): string[] {
  const sheet = workbook.sheets.find((candidate) => candidate.name === sourceSheet);
  const parts = parseRange(source);
  if (!sheet || !parts) return [];
  const values = computeWorkbookValues(workbook);
  const fields: string[] = [];
  for (let col = parts.start.col; col <= parts.end.col; col += 1) {
    const value = values.get(`${sheet.name}!${formatAddress(parts.start.row, col)}`) ?? "";
    const label = toText(value).trim();
    fields.push(label === "" ? `Column ${col + 1}` : label);
  }
  return fields;
}

interface PivotRecord {
  row: string[];
  column: string[];
  values: Scalar[];
}

function distinctKeys(keys: string[][]): string[][] {
  const seen = new Map<string, string[]>();
  for (const key of keys) {
    const signature = key.map((value) => value.toUpperCase()).join("\u0000");
    if (!seen.has(signature)) seen.set(signature, key);
  }
  return [...seen.values()].sort((left, right) => {
    for (let index = 0; index < Math.max(left.length, right.length); index += 1) {
      const comparison = compareScalars(left[index] ?? "", right[index] ?? "");
      if (comparison !== 0) return comparison;
    }
    return 0;
  });
}

function aggregate(
  records: PivotRecord[],
  valueIndex: number,
  aggregation: PivotTable["values"][number]["aggregation"],
): Scalar {
  const raw = records.map((record) => record.values[valueIndex] ?? "");
  if (aggregation === "count") return raw.filter((value) => !isError(value) && toText(value) !== "").length;
  const numbers: number[] = [];
  for (const value of raw) {
    const number = toNumber(value);
    if (!isError(number) && toText(value) !== "") numbers.push(number);
  }
  if (numbers.length === 0) return "";
  switch (aggregation) {
    case "sum":
      return numbers.reduce((sum, value) => sum + value, 0);
    case "average":
      return numbers.reduce((sum, value) => sum + value, 0) / numbers.length;
    case "min":
      return Math.min(...numbers);
    case "max":
      return Math.max(...numbers);
    default:
      return "";
  }
}

/**
 * Computes the pivot grid, or null when the definition cannot be read (the
 * source range is gone, the fields do not exist, nothing is configured).
 */
export function computePivot(workbook: Workbook, pivot: PivotTable): PivotGrid | null {
  const sheet: Sheet | undefined = workbook.sheets.find((candidate) => candidate.name === pivot.sourceSheet);
  const parts = parseRange(pivot.source);
  if (!sheet || !parts) return null;
  if (pivot.values.length === 0 || (pivot.rows.length === 0 && pivot.columns.length === 0)) return null;
  const fields = pivotFields(workbook, pivot.sourceSheet, pivot.source);
  if (fields.length === 0) return null;
  const indexOf = (field: string) => fields.findIndex((candidate) => candidate.toUpperCase() === field.toUpperCase());
  const valueIndexes = pivot.values.map((entry) => indexOf(entry.field));
  const rowIndexes = pivot.rows.map(indexOf);
  const columnIndexes = pivot.columns.map(indexOf);
  if ([...valueIndexes, ...rowIndexes, ...columnIndexes].some((index) => index < 0)) return null;

  const values = computeWorkbookValues(workbook);
  const cellAt = (row: number, col: number): Scalar => {
    const address = formatAddress(row, col);
    const cell: Cell | undefined = sheet.cells[address];
    if (!cell) return "";
    return values.get(`${sheet.name}!${address}`) ?? "";
  };
  const fieldText = (row: number, col: number): string => toText(cellAt(row, col)).trim();

  const records: PivotRecord[] = [];
  for (let row = parts.start.row + 1; row <= parts.end.row; row += 1) {
    const record: PivotRecord = {
      row: rowIndexes.map((col) => fieldText(row, parts.start.col + col)),
      column: columnIndexes.map((col) => fieldText(row, parts.start.col + col)),
      values: valueIndexes.map((col) => cellAt(row, parts.start.col + col)),
    };
    // A completely blank source row is skipped; a blank row key becomes
    // "(blank)" so it still shows up in the grid like Excel's default.
    if (
      record.row.every((value) => value === "") &&
      record.column.every((value) => value === "") &&
      record.values.every((value) => toText(value) === "")
    ) {
      continue;
    }
    const keep = pivot.filters.every((filter) => {
      const index = indexOf(filter.field);
      if (index < 0) return true;
      if (filter.values.length === 0) return true;
      return filter.values.some(
        (value) => value.toUpperCase() === fieldText(row, parts.start.col + index).toUpperCase(),
      );
    });
    if (keep) records.push(record);
  }

  const rowKeys = distinctKeys(records.map((record) => record.row.map((value) => (value === "" ? "(blank)" : value))));
  const columnKeys =
    pivot.columns.length > 0
      ? distinctKeys(records.map((record) => record.column.map((value) => (value === "" ? "(blank)" : value))))
      : [[]];

  const valueCount = pivot.values.length;
  const header: Scalar[][] = [];
  const corner = pivot.rows.length > 0 ? pivot.rows.join(" / ") : "Pivot";
  if (pivot.columns.length > 0) {
    header.push([corner, ...columnKeys.flatMap((key) => Array.from({ length: valueCount }, () => key.join(" / ")))]);
    if (valueCount > 1) {
      header.push([
        "",
        ...columnKeys.flatMap(() => pivot.values.map((entry) => `${entry.field} (${entry.aggregation})`)),
      ]);
    }
  } else {
    header.push([corner, ...pivot.values.map((entry) => `${entry.field} (${entry.aggregation})`)]);
  }

  const body: Scalar[][] = [];
  for (const rowKey of rowKeys) {
    const line: Scalar[] = [...rowKey];
    for (const columnKey of columnKeys) {
      const matching = records.filter(
        (record) =>
          record.row.map((value) => (value === "" ? "(blank)" : value)).join("\u0000") === rowKey.join("\u0000") &&
          record.column.map((value) => (value === "" ? "(blank)" : value)).join("\u0000") === columnKey.join("\u0000"),
      );
      for (let valueIndex = 0; valueIndex < valueCount; valueIndex += 1) {
        line.push(matching.length === 0 ? "" : aggregate(matching, valueIndex, pivot.values[valueIndex].aggregation));
      }
    }
    body.push(line);
  }
  if (body.length === 0) return null;
  return { grid: [...header, ...body], rowFieldCount: pivot.rows.length };
}

/** The pivot grid as plain values, ready to be written into cells. */
export function pivotValues(workbook: Workbook, pivot: PivotTable): Scalar[][] {
  return computePivot(workbook, pivot)?.grid ?? [];
}

/** A sane default definition over a source range. */
export function defaultPivot(
  workbook: Workbook,
  sourceSheet: string,
  source: string,
  anchor: string,
): PivotTable | null {
  const fields = pivotFields(workbook, sourceSheet, source);
  if (fields.length < 2) return null;
  const parts = parseRange(source);
  const values = computeWorkbookValues(workbook);
  const sheet = workbook.sheets.find((candidate) => candidate.name === sourceSheet);
  const numeric = new Set<string>();
  if (parts && sheet) {
    fields.forEach((field, index) => {
      for (let row = parts.start.row + 1; row <= Math.min(parts.end.row, parts.start.row + 20); row += 1) {
        const raw = values.get(`${sheet.name}!${formatAddress(row, parts.start.col + index)}`) ?? "";
        const text = toText(raw);
        if (text !== "" && !isError(raw) && Number.isFinite(Number(text))) {
          numeric.add(field);
          break;
        }
      }
    });
  }
  // Measures usually sit at the end of a table; labels come first.
  const numericFields = fields.filter((field) => numeric.has(field));
  const valueField = numericFields[numericFields.length - 1] ?? fields[fields.length - 1];
  const rowField =
    fields.find((field) => field !== valueField && !numeric.has(field)) ??
    fields.find((field) => field !== valueField) ??
    fields[0];
  return {
    id: `pivot-${Math.random().toString(36).slice(2, 10)}`,
    name: `Pivot${1}`,
    sourceSheet,
    source,
    rows: [rowField],
    columns: [],
    values: [{ field: valueField, aggregation: "sum" }],
    filters: [],
    anchor,
  };
}

export { asScalar };
