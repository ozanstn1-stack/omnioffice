/**
 * Pure grid maths: touch zoom and the column shift of a fill-right. Kept out
 * of the component so jsdom can test them without pointer events.
 */
import { columnLabel, parseAddress } from "./formula";

/** Clamps a grid zoom factor so a pinch can never shrink the grid away. */
export function clampGridZoom(value: number): number {
  if (!Number.isFinite(value)) return 1;
  return Math.min(2, Math.max(0.6, Number(value.toFixed(3))));
}

/** The zoom a two-finger pinch asks for: `startZoom` scaled by the distance ratio. */
export function pinchGridZoom(startZoom: number, startDistance: number, distance: number): number {
  if (!Number.isFinite(distance) || !Number.isFinite(startDistance) || startDistance <= 0) return startZoom;
  return clampGridZoom(startZoom * (distance / startDistance));
}

/**
 * Column shift for a fill-right, mirroring `shiftFormulaRows` in cells.ts:
 * relative column references move, absolute (`$A`) and the row stay put.
 */
export function shiftFormulaColumns(formula: string | null, delta: number): string | null {
  if (!formula || delta === 0) return formula;
  return formula.replace(
    /(?<![A-Za-z0-9_$])(\$?)([A-Za-z]{1,3})(\$?)(\d{1,7})(?![A-Za-z0-9_(])/g,
    (match, dollarCol: string, letters: string, dollarRow: string, digits: string) => {
      const position = parseAddress(`${letters}${digits}`);
      if (position === null || dollarCol) return match;
      const next = position.col + delta;
      if (next < 0 || next >= 16_384) return match;
      const label = columnLabel(next);
      // Keep the case the author typed, like shiftFormulaRows keeps the letters.
      return `${letters === letters.toLowerCase() ? label.toLowerCase() : label}${dollarRow}${digits}`;
    },
  );
}
