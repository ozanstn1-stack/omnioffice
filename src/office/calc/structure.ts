/** Row, column and merge edits the Calc ribbon and header menus perform. */
import type { Sheet } from "../../lib/office-types";
import { formatAddress, parseAddress } from "./formula";
import type { GridSelection } from "./grid-types";
import { shiftSizes } from "./visibility";

type UpdateSheet = (mutate: (sheet: Sheet) => Sheet) => void;

export function insertRow(_sheet: Sheet, row: number, updateSheet: UpdateSheet) {
  updateSheet((current) => {
    const cells: Sheet["cells"] = {};
    for (const [address, cell] of Object.entries(current.cells)) {
      const position = parseAddress(address);
      if (!position) continue;
      cells[formatAddress(position.row >= row ? position.row + 1 : position.row, position.col)] = cell;
    }
    return { ...current, cells, rowHeights: shiftSizes(current.rowHeights, row, 1), rowCount: current.rowCount + 1 };
  });
}

export function deleteRow(_sheet: Sheet, row: number, updateSheet: UpdateSheet) {
  updateSheet((current) => {
    const cells: Sheet["cells"] = {};
    for (const [address, cell] of Object.entries(current.cells)) {
      const position = parseAddress(address);
      if (!position || position.row === row) continue;
      cells[formatAddress(position.row > row ? position.row - 1 : position.row, position.col)] = cell;
    }
    return {
      ...current,
      cells,
      rowHeights: shiftSizes(current.rowHeights, row, -1),
      rowCount: Math.max(10, current.rowCount - 1),
    };
  });
}

export function insertColumn(_sheet: Sheet, col: number, updateSheet: UpdateSheet) {
  updateSheet((current) => {
    const cells: Sheet["cells"] = {};
    for (const [address, cell] of Object.entries(current.cells)) {
      const position = parseAddress(address);
      if (!position) continue;
      cells[formatAddress(position.row, position.col >= col ? position.col + 1 : position.col)] = cell;
    }
    return { ...current, cells, colWidths: shiftSizes(current.colWidths, col, 1), colCount: current.colCount + 1 };
  });
}

export function deleteColumn(_sheet: Sheet, col: number, updateSheet: UpdateSheet) {
  updateSheet((current) => {
    const cells: Sheet["cells"] = {};
    for (const [address, cell] of Object.entries(current.cells)) {
      const position = parseAddress(address);
      if (!position || position.col === col) continue;
      cells[formatAddress(position.row, position.col > col ? position.col - 1 : position.col)] = cell;
    }
    return {
      ...current,
      cells,
      colWidths: shiftSizes(current.colWidths, col, -1),
      colCount: Math.max(5, current.colCount - 1),
    };
  });
}

export function toggleMerge(_sheet: Sheet, selection: GridSelection, updateSheet: UpdateSheet) {
  const start = formatAddress(
    Math.min(selection.anchor.row, selection.focus.row),
    Math.min(selection.anchor.col, selection.focus.col),
  );
  const end = formatAddress(
    Math.max(selection.anchor.row, selection.focus.row),
    Math.max(selection.anchor.col, selection.focus.col),
  );
  updateSheet((current) => {
    const existing = current.merges.findIndex((merge) => merge.start === start && merge.end === end);
    if (existing >= 0) return { ...current, merges: current.merges.filter((_, index) => index !== existing) };
    return { ...current, merges: [...current.merges, { start, end }] };
  });
}
