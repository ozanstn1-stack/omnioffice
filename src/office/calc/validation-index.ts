/**
 * Cell -> data-validation rule lookup for the grid.
 *
 * A rule's range is parsed once into numeric bounds, so asking "which rule
 * covers this cell?" is a few integer comparisons per rule instead of expanding
 * the range into address strings for every visible cell on every render. There
 * is no cap on the range size: a cell is covered wherever it sits.
 */
import { parseRange } from "./addresses";

export interface RangeBounds {
  top: number;
  left: number;
  bottom: number;
  right: number;
}

/** Bounds of every area in a range; XLSX separates several areas with spaces. */
export function parseAreas(range: string): RangeBounds[] {
  const areas: RangeBounds[] = [];
  for (const area of range.split(/\s+/)) {
    const parts = area ? parseRange(area) : null;
    if (parts) {
      areas.push({ top: parts.start.row, left: parts.start.col, bottom: parts.end.row, right: parts.end.col });
    }
  }
  return areas;
}

export function boundsContain(bounds: RangeBounds, row: number, col: number): boolean {
  return row >= bounds.top && row <= bounds.bottom && col >= bounds.left && col <= bounds.right;
}

export interface ValidationLookup<T> {
  /** The first rule, in list order, whose range covers the cell. */
  find(row: number, col: number): T | undefined;
}

const lookups = new WeakMap<readonly unknown[], ValidationLookup<never>>();

/**
 * Builds (or reuses) the lookup for a rule list. It is memoized on the array
 * itself: the sheet model is immutable, so the same array is the same rules and
 * an edit that replaces `sheet.validations` gets a fresh lookup.
 */
export function validationLookup<T extends { range: string }>(rules: readonly T[]): ValidationLookup<T> {
  const cached = lookups.get(rules);
  if (cached) return cached as ValidationLookup<T>;
  const entries = rules.map((rule) => ({ rule, areas: parseAreas(rule.range) }));
  const lookup: ValidationLookup<T> = {
    find(row, col) {
      for (const entry of entries) {
        for (const area of entry.areas) {
          if (boundsContain(area, row, col)) return entry.rule;
        }
      }
      return undefined;
    },
  };
  lookups.set(rules, lookup as ValidationLookup<never>);
  return lookup;
}
