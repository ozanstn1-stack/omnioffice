/**
 * Paragraph tab stop mathematics for the Writer ruler.
 *
 * The model stores custom stops as `ParaProps.tabs`; the ruler is only a view
 * over that list. Keeping add/move/remove and the "next stop" lookup pure means
 * the ruler and the renderer agree on tab positions without a browser.
 */
import type { TabStop } from "../../lib/office-types";

/** Word's default: half an inch between implicit stops. */
export const DEFAULT_TAB_STEP_PT = 36;

/** Two stops closer than this are treated as the same position. */
const SAME_POSITION_PT = 0.5;

function sorted(tabs: TabStop[]): TabStop[] {
  return [...tabs].sort((left, right) => left.posPt - right.posPt);
}

/** Adds (or re-aligns) a stop; the list stays sorted and de-duplicated. */
export function addTabStop(tabs: TabStop[] | undefined, posPt: number, align = "left"): TabStop[] {
  const position = Math.max(0, posPt);
  const kept = (tabs ?? []).filter((stop) => Math.abs(stop.posPt - position) >= SAME_POSITION_PT);
  return sorted([...kept, { posPt: position, align }]);
}

/** Moves the stop at `index` to `posPt`. Out-of-range indices are a no-op. */
export function moveTabStop(tabs: TabStop[] | undefined, index: number, posPt: number): TabStop[] {
  const list = tabs ?? [];
  const stop = list[index];
  if (!stop) return list;
  const position = Math.max(0, posPt);
  const kept = list.filter(
    (candidate, candidateIndex) => candidateIndex !== index && Math.abs(candidate.posPt - position) >= SAME_POSITION_PT,
  );
  return sorted([...kept, { ...stop, posPt: position }]);
}

/** Removes the stop at `index`. Out-of-range indices are a no-op. */
export function removeTabStop(tabs: TabStop[] | undefined, index: number): TabStop[] {
  const list = tabs ?? [];
  if (!list[index]) return list;
  return list.filter((_, position) => position !== index);
}

/**
 * The next tab position after `posPt`: the first custom stop strictly to the
 * right, or the next implicit multiple of `defaultStep` when there is none.
 */
export function nextTabStop(tabs: TabStop[] | undefined, posPt: number, defaultStep = DEFAULT_TAB_STEP_PT): number {
  const step = defaultStep > 0 ? defaultStep : DEFAULT_TAB_STEP_PT;
  for (const stop of sorted(tabs ?? [])) {
    if (stop.posPt > posPt + SAME_POSITION_PT) return stop.posPt;
  }
  return (Math.floor(posPt / step) + 1) * step;
}
