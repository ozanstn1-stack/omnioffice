/** Name helpers for structured tables and their columns. */
import type { SpreadsheetTable } from "../../lib/office-types";

/** Appends a number until the name is unique inside the sheet's tables. */
export function uniqueTableName(tables: readonly SpreadsheetTable[], base: string): string {
  let name = base;
  let index = 1;
  while (tables.some((table) => table.name.toLowerCase() === name.toLowerCase())) {
    index += 1;
    name = `${base}${index}`;
  }
  return name;
}

/** Appends a number until the column name is unique inside the table. */
export function uniqueColumnName(columns: readonly string[], base: string): string {
  let name = base;
  let index = 1;
  while (columns.some((column) => column.toLowerCase() === name.toLowerCase())) {
    index += 1;
    name = `${base}${index}`;
  }
  return name;
}
