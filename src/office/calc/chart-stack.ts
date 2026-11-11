/**
 * Where each value of a cartesian chart starts and ends on the value axis
 * (4.1c). Side by side, every value runs from zero; stacked, a series sits on
 * the ones before it, positives on positives and negatives on negatives.
 */
export interface Span {
  from: number;
  to: number;
}

/** The kinds that can stack; a stacked line is drawn as a plain line. */
export const STACKABLE = new Set(["column", "bar", "area"]);

export function valueSpans(values: readonly (readonly number[])[], stacked: boolean): Span[][] {
  const above: number[] = [];
  const below: number[] = [];
  return values.map((row) =>
    row.map((raw, index) => {
      const value = Number.isFinite(raw) ? raw : 0;
      if (!stacked) return { from: 0, to: value };
      const stack = value < 0 ? below : above;
      const from = stack[index] ?? 0;
      stack[index] = from + value;
      return { from, to: from + value };
    }),
  );
}

/** The value axis range: zero is always on it, and it is never empty. */
export function spanRange(spans: readonly (readonly Span[])[]): { min: number; max: number } {
  const ends = spans.flat().flatMap((span) => [span.from, span.to]);
  return { min: Math.min(0, ...ends), max: Math.max(1, ...ends) };
}
