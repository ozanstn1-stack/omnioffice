/**
 * Hiding and unhiding rows and columns.
 *
 * A hidden row or column is one whose size is 0 in the sheet's sparse size
 * table: `rowHeights` for rows (where a filter already hides rows this way and
 * `row-layout.ts` honours it) and `colWidths` for columns. Showing it again
 * drops the entry, which restores the default size; the model has nowhere to
 * remember a custom size under the 0. These functions work on one table and
 * return a new one (or null when nothing changes), so rows and columns share
 * them.
 */

type Sizes = Readonly<Record<string, number>>;

export function isHiddenIndex(sizes: Sizes, index: number): boolean {
  return sizes[String(index)] === 0;
}

/**
 * Hides `start..end` (either order, clipped to `total` indexes). Returns null
 * when nothing would change, and also when it would hide every index: a sheet
 * with no visible row or column leaves nothing to select or to unhide.
 */
export function hideRange(sizes: Sizes, start: number, end: number, total: number): Record<string, number> | null {
  const first = Math.max(0, Math.min(start, end));
  const last = Math.min(total - 1, Math.max(start, end));
  if (last - first + 1 >= total) return null;
  const next: Record<string, number> = { ...sizes };
  let changed = false;
  for (let index = first; index <= last; index += 1) {
    if (next[String(index)] === 0) continue;
    next[String(index)] = 0;
    changed = true;
  }
  if (!changed) return null;
  let hidden = 0;
  for (const key of Object.keys(next)) {
    const index = Number(key);
    if (next[key] === 0 && index >= 0 && index < total) hidden += 1;
  }
  return hidden >= total ? null : next;
}

/**
 * The hidden indexes "Unhide" brings back for a selected range: those inside
 * it, plus the hidden run directly before and after it. Hidden headers cannot
 * be clicked, so selecting the neighbours is how a user reaches them.
 */
export function revealableIndexes(sizes: Sizes, start: number, end: number, total: number): number[] {
  const first = Math.max(0, Math.min(start, end));
  const last = Math.min(total - 1, Math.max(start, end));
  let from = first;
  while (from > 0 && isHiddenIndex(sizes, from - 1)) from -= 1;
  let to = last;
  while (to < total - 1 && isHiddenIndex(sizes, to + 1)) to += 1;
  const out: number[] = [];
  // Walk the table's own keys, not the range: a whole-sheet selection spans a
  // million rows but only a few of them are hidden.
  for (const key of Object.keys(sizes)) {
    const index = Number(key);
    if (sizes[key] === 0 && Number.isInteger(index) && index >= from && index <= to) out.push(index);
  }
  return out.sort((a, b) => a - b);
}

/** Shows the hidden indexes around a range again; null when none are hidden. */
export function unhideRange(sizes: Sizes, start: number, end: number, total: number): Record<string, number> | null {
  const reveal = revealableIndexes(sizes, start, end, total);
  if (reveal.length === 0) return null;
  const next: Record<string, number> = { ...sizes };
  for (const index of reveal) delete next[String(index)];
  return next;
}

/**
 * Moves `step` visible indexes from `index` (negative goes back), hopping over
 * hidden runs and stopping at the first or last visible index.
 */
export function stepVisible(sizes: Sizes, index: number, step: number, total: number): number {
  const direction = step < 0 ? -1 : 1;
  let current = index;
  for (let left = Math.abs(Math.trunc(step)); left > 0; left -= 1) {
    let next = current + direction;
    while (next >= 0 && next < total && isHiddenIndex(sizes, next)) next += direction;
    if (next < 0 || next >= total) break;
    current = next;
  }
  return current;
}

/** The first or last index that is not hidden (the edge the Home / End keys go to). */
export function edgeVisible(sizes: Sizes, total: number, edge: "first" | "last"): number {
  const index = edge === "first" ? 0 : Math.max(0, total - 1);
  return isHiddenIndex(sizes, index) ? stepVisible(sizes, index, edge === "first" ? 1 : -1, total) : index;
}

/**
 * Re-keys a size table after an index was inserted (`delta` 1: everything at
 * or after `at` moves down by one) or deleted (`delta` -1: its entry goes and
 * everything after it moves up). Returns the same table when nothing moves.
 */
export function shiftSizes(sizes: Sizes, at: number, delta: 1 | -1): Record<string, number> {
  const keys = Object.keys(sizes);
  if (!keys.some((key) => Number(key) >= at)) return sizes as Record<string, number>;
  const next: Record<string, number> = {};
  for (const key of keys) {
    const index = Number(key);
    if (!Number.isInteger(index) || index < at) next[key] = sizes[key];
    else if (delta === 1) next[String(index + 1)] = sizes[key];
    else if (index > at) next[String(index - 1)] = sizes[key];
  }
  return next;
}
