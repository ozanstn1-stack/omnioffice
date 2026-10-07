import type { Translate } from "./i18n";

/**
 * The office importers report a sheet with cells outside the imported area
 * (100,000 rows, 1,000 columns) as an English warning. This module recognises
 * it so the UI can show it translated. The wording must match
 * `import_limit_warning` in crates/officecore/src/xlsx.rs; a warning that does
 * not match is simply shown as written.
 */
const LIMIT_WARNING =
  /^Import limit: sheet "(.*)" has cells beyond row (\d+) or column (\d+); they were not imported\.$/s;

export interface ImportLimit {
  sheet: string;
  rows: number;
  cols: number;
}

/** Separates the budget notices from the ordinary import notes. */
export function splitImportWarnings(warnings: string[]): { limits: ImportLimit[]; notes: string[] } {
  const limits: ImportLimit[] = [];
  const notes: string[] = [];
  for (const warning of warnings) {
    const match = LIMIT_WARNING.exec(warning);
    if (match) limits.push({ sheet: match[1], rows: Number(match[2]), cols: Number(match[3]) });
    else notes.push(warning);
  }
  return { limits, notes };
}

/** The translated text of one budget notice. */
export function describeImportLimit(limit: ImportLimit, t: Translate): string {
  return t("office.importLimit", { sheet: limit.sheet, rows: limit.rows, cols: limit.cols });
}

/** The translated budget notices first, then the other notes unchanged. */
export function localizeImportWarnings(warnings: string[], t: Translate): string[] {
  const { limits, notes } = splitImportWarnings(warnings);
  return [...limits.map((limit) => describeImportLimit(limit, t)), ...notes];
}
