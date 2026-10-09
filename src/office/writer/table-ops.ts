/**
 * Table geometry and merge/split operations for the Writer.
 *
 * A row's `cells` only contains cells that *start* in that row; a cell with
 * colspan/rowspan covers grid positions that have no entry of their own. Every
 * helper below works on that grid view, so merging never has to guess where a
 * rowspan from an earlier row occupies space.
 */
import { defaultParaProps, type Block, type TableCell, type TableData } from "../../lib/office-types";
import { emptyRun } from "./runs";

/** A rectangular selection of grid positions, inclusive on every side. */
export interface CellRange {
  row0: number;
  col0: number;
  row1: number;
  col1: number;
}

/** Where the cell covering a grid position starts. */
export interface CellOrigin {
  row: number;
  cell: number;
}

export interface CellRef extends CellOrigin {
  data: TableCell;
}

function key(row: number, col: number): string {
  return `${row}:${col}`;
}

/**
 * Grid start column of every cell, per row. A rowspan from an earlier row
 * pushes later cells to the right, which is what keeps columns aligned.
 */
export function cellStartColumns(table: TableData): number[][] {
  const occupied = new Set<string>();
  return table.rows.map((row, rowIndex) => {
    let col = 0;
    return row.cells.map((cell) => {
      while (occupied.has(key(rowIndex, col))) col += 1;
      const start = col;
      for (let rowStep = 0; rowStep < Math.max(1, cell.rowspan); rowStep += 1) {
        for (let colStep = 0; colStep < Math.max(1, cell.colspan); colStep += 1) {
          occupied.add(key(rowIndex + rowStep, start + colStep));
        }
      }
      col += Math.max(1, cell.colspan);
      return start;
    });
  });
}

/** Maps every covered grid position to the cell that covers it. */
function originMap(table: TableData): Map<string, CellRef> {
  const map = new Map<string, CellRef>();
  const starts = cellStartColumns(table);
  table.rows.forEach((row, rowIndex) => {
    row.cells.forEach((cell, cellIndex) => {
      const start = starts[rowIndex][cellIndex];
      for (let rowStep = 0; rowStep < Math.max(1, cell.rowspan); rowStep += 1) {
        for (let colStep = 0; colStep < Math.max(1, cell.colspan); colStep += 1) {
          const position = key(rowIndex + rowStep, start + colStep);
          if (!map.has(position)) map.set(position, { row: rowIndex, cell: cellIndex, data: cell });
        }
      }
    });
  });
  return map;
}

/** Number of grid columns in the widest row, at least 1. */
export function gridWidth(table: TableData): number {
  const starts = cellStartColumns(table);
  let width = 1;
  table.rows.forEach((row, rowIndex) => {
    row.cells.forEach((cell, cellIndex) => {
      width = Math.max(width, starts[rowIndex][cellIndex] + Math.max(1, cell.colspan));
    });
  });
  return width;
}

/** The cell covering a grid position (whether at its origin or inside its span). */
export function cellAt(table: TableData, row: number, col: number): CellRef | null {
  if (row < 0 || col < 0 || row >= table.rows.length) return null;
  return originMap(table).get(key(row, col)) ?? null;
}

/** The origin of the cell covering a position, or null when uncovered. */
export function coveredBy(table: TableData, row: number, col: number): CellOrigin | null {
  const found = originMap(table).get(key(row, col));
  return found ? { row: found.row, cell: found.cell } : null;
}

function normalize(range: CellRange): CellRange {
  return {
    row0: Math.min(range.row0, range.row1),
    col0: Math.min(range.col0, range.col1),
    row1: Math.max(range.row0, range.row1),
    col1: Math.max(range.col0, range.col1),
  };
}

/**
 * True when the selection covers a proper rectangle: at least two grid cells
 * and no cell crossing the selection boundary.
 */
export function isRectangular(table: TableData, range: CellRange): boolean {
  const rect = normalize(range);
  if (rect.row0 < 0 || rect.col0 < 0 || rect.row1 >= table.rows.length) return false;
  if (rect.col1 >= gridWidth(table)) return false;
  if (rect.row0 === rect.row1 && rect.col0 === rect.col1) return false;
  const origins = originMap(table);
  const starts = cellStartColumns(table);
  const seen = new Set<string>();
  for (let row = rect.row0; row <= rect.row1; row += 1) {
    for (let col = rect.col0; col <= rect.col1; col += 1) {
      const origin = origins.get(key(row, col));
      if (!origin) return false;
      const originCol = starts[origin.row][origin.cell];
      // The origin must sit inside the selection, or the merged cell would
      // swallow (or lose) the part of it that sticks out.
      if (origin.row < rect.row0 || origin.row > rect.row1) return false;
      if (originCol < rect.col0 || originCol > rect.col1) return false;
      seen.add(key(origin.row, originCol));
    }
  }
  return seen.size >= 2;
}

/** An empty paragraph used as a separator between merged cell contents. */
function separatorBlock(): Block {
  return { type: "paragraph", props: { ...defaultParaProps(), spaceAfterPt: 0 }, runs: [emptyRun()] };
}

/**
 * Merges the cells in a rectangular selection into the top-left one.
 *
 * Invalid selections (a single cell, a non-rectangle, out of range) return the
 * table unchanged, so the UI can offer the command unconditionally.
 */
export function mergeCells(table: TableData, range: CellRange): TableData {
  if (!isRectangular(table, range)) return table;
  const rect = normalize(range);
  const starts = cellStartColumns(table);
  const origins = originMap(table);
  const covered = new Map<string, TableCell>();
  for (let row = rect.row0; row <= rect.row1; row += 1) {
    for (let col = rect.col0; col <= rect.col1; col += 1) {
      const origin = origins.get(key(row, col));
      if (!origin) continue;
      const originCol = starts[origin.row][origin.cell];
      covered.set(key(origin.row, originCol), origin.data);
    }
  }
  const mergedBlocks: Block[] = [];
  for (const cell of covered.values()) {
    if (mergedBlocks.length > 0) mergedBlocks.push(separatorBlock());
    mergedBlocks.push(...(cell.blocks.length > 0 ? cell.blocks : [separatorBlock()]));
  }
  const anchor = origins.get(key(rect.row0, rect.col0));
  if (!anchor) return table;
  const anchorCol = starts[anchor.row][anchor.cell];
  const width = rect.col1 - rect.col0 + 1;
  const height = rect.row1 - rect.row0 + 1;
  const merged: TableCell = { ...anchor.data, blocks: mergedBlocks, colspan: width, rowspan: height };
  const rows = table.rows.map((row, rowIndex) => {
    if (rowIndex < rect.row0 || rowIndex > rect.row1) return row;
    const cells: TableCell[] = [];
    row.cells.forEach((cell, cellIndex) => {
      const start = starts[rowIndex][cellIndex];
      if (start < rect.col0 || start > rect.col1) {
        cells.push(cell);
        return;
      }
      if (rowIndex === anchor.row && start === anchorCol) cells.push(merged);
      // Every other selected cell is swallowed by the merged one.
    });
    return { ...row, cells };
  });
  const columnWidthsPt = [...table.columnWidthsPt];
  const needed = Math.max(gridWidth(table), rect.col1 + 1);
  if (columnWidthsPt.length < needed) {
    const average = columnWidthsPt.length
      ? columnWidthsPt.reduce((sum, value) => sum + value, 0) / columnWidthsPt.length
      : 64;
    while (columnWidthsPt.length < needed) columnWidthsPt.push(average);
  }
  return { ...table, rows, columnWidthsPt };
}

function emptyCell(): TableCell {
  return { blocks: [], colspan: 1, rowspan: 1, background: null, align: "left", valign: "top", widthPt: null };
}

/**
 * Splits a merged cell back into a 1x1 origin plus empty covered cells.
 * Splitting a plain cell (or an out-of-range index) is a no-op.
 */
export function splitCell(table: TableData, row: number, cell: number): TableData {
  const target = table.rows[row]?.cells[cell];
  if (!target) return table;
  const columnSpan = Math.max(1, target.colspan);
  const rowSpan = Math.max(1, target.rowspan);
  if (columnSpan === 1 && rowSpan === 1) return table;
  const starts = cellStartColumns(table);
  const startCol = starts[row][cell];
  const rows = table.rows.map((current, rowIndex) => {
    if (rowIndex < row || rowIndex > row + rowSpan - 1) return current;
    const entries: Array<{ col: number; cell: TableCell }> = current.cells.map((entry, cellIndex) => ({
      col: starts[rowIndex][cellIndex],
      cell: entry,
    }));
    for (let col = startCol; col < startCol + columnSpan; col += 1) {
      if (rowIndex === row && col === startCol) continue;
      entries.push({ col, cell: emptyCell() });
    }
    entries.sort((left, right) => left.col - right.col);
    const cells = entries.map((entry) =>
      rowIndex === row && entry.col === startCol ? { ...target, colspan: 1, rowspan: 1 } : entry.cell,
    );
    return { ...current, cells };
  });
  return { ...table, rows };
}
