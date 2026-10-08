/**
 * What a screen reader is told when the active cell of the grid changes: the
 * address, the value shown and, for a formula cell, the formula. The grid's
 * cells are virtualised, so this text is the reliable source of the position.
 */
import type { Translate } from "../../lib/i18n";

export function cellAnnouncement(
  t: Translate,
  cell: {
    /** The active cell, e.g. "B7". */
    address: string;
    /** The selected range ("A1:C3") when several cells are selected, else null. */
    range: string | null;
    /** The text the cell shows; empty for an empty cell. */
    display: string;
    formula: string | null;
    /** The text of the active cell's note, which a keyboard user cannot hover to read. */
    note?: string | null;
  },
): string {
  const value = cell.display === "" ? t("calc.announceEmpty") : cell.display;
  const withNote = (text: string) => (cell.note ? `${text}. ${t("calc.announceNote", { note: cell.note })}` : text);
  if (cell.range) return withNote(t("calc.announceRange", { range: cell.range, cell: cell.address, value }));
  return withNote(
    cell.formula
      ? t("calc.announceCellFormula", { cell: cell.address, value, formula: cell.formula })
      : t("calc.announceCell", { cell: cell.address, value }),
  );
}
