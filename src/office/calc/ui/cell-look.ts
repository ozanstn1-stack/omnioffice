import type { SpreadsheetTable } from "../../../lib/office-types";
import type { CellPosition } from "../grid-types";

/** A structured table with its parsed range, as CalcEditor keeps them. */
export interface PlacedTable {
  table: SpreadsheetTable;
  parts: { start: CellPosition; end: CellPosition };
}

/** The light fill of every other body row in a banded table. */
export const BAND_FILL = "#EFF6FF";

/**
 * What a structured table contributes to one cell (3.3h): which table owns
 * it, whether it is the header row, and the header or banding fill. The
 * cell's own formatting still wins over it; the totals row is never banded.
 */
export function tableLook(
  tables: readonly PlacedTable[],
  row: number,
  col: number,
): { entry: PlacedTable | undefined; fill: string | undefined; header: boolean } {
  const entry = tables.find(
    ({ parts }) => row >= parts.start.row && row <= parts.end.row && col >= parts.start.col && col <= parts.end.col,
  );
  if (!entry) return { entry, fill: undefined, header: false };
  const { table, parts } = entry;
  const header = table.hasHeaders && row === parts.start.row;
  if (header) return { entry, fill: table.headerFill ?? undefined, header };
  const totalsRow = table.hasTotals && row === parts.end.row;
  if (totalsRow || !table.bandedRows) return { entry, fill: undefined, header };
  const bodyStart = parts.start.row + (table.hasHeaders ? 1 : 0);
  return { entry, fill: (row - bodyStart) % 2 === 1 ? BAND_FILL : undefined, header };
}
