/**
 * Sheet protection as the editor honours it.
 *
 * The model keeps the verifier Excel wrote (`protection`, plus the legacy
 * `sheetProtection` hash older documents carry) and never tries to open it, so
 * a protected sheet stays read-only for the operations that rewrite cells in
 * bulk: replace, sort, paste special, AutoSum, hiding rows and columns.
 */
import type { Sheet } from "../../lib/office-types";

export function isSheetProtected(sheet: Sheet): boolean {
  return sheet.protection?.enabled === true || (sheet.sheetProtection ?? "").trim() !== "";
}
