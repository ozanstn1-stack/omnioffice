/** Positions and selections on the Calc grid (0-based rows and columns). */

export interface CellPosition {
  row: number;
  col: number;
}

export interface GridSelection {
  anchor: CellPosition;
  focus: CellPosition;
}
