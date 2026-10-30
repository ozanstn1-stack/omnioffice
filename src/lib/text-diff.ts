/**
 * Word-level text diff shared by the office diff engines (C13).
 *
 * The pair is split into whitespace/non-whitespace tokens and matched with an
 * LCS; the common prefix and suffix are trimmed first so a long paragraph with
 * a small change keeps a small table. A pair above `MAX_LCS_CELLS` falls back
 * to "removed + added", which is still truthful and stays bounded.
 */
import type { DiffSegment } from "./office-diff-types";

/** Above this many tokens the LCS table is skipped. */
const MAX_LCS_CELLS = 250_000;

function tokenize(text: string): string[] {
  return text.match(/\s+|\S+/g) ?? [];
}

/** LCS match pairs; the common prefix and suffix are trimmed to keep the table small. */
function commonTokens(base: string[], current: string[]): [number, number][] {
  let prefix = 0;
  while (prefix < base.length && prefix < current.length && base[prefix] === current[prefix]) prefix += 1;
  let suffix = 0;
  while (
    suffix < base.length - prefix &&
    suffix < current.length - prefix &&
    base[base.length - 1 - suffix] === current[current.length - 1 - suffix]
  ) {
    suffix += 1;
  }
  const matches: [number, number][] = [];
  for (let index = 0; index < prefix; index += 1) matches.push([index, index]);
  const baseMid = base.slice(prefix, base.length - suffix);
  const currentMid = current.slice(prefix, current.length - suffix);
  if (baseMid.length > 0 && currentMid.length > 0 && baseMid.length * currentMid.length <= MAX_LCS_CELLS) {
    const rows = baseMid.length;
    const cols = currentMid.length;
    const table: number[][] = Array.from({ length: rows + 1 }, () => new Array<number>(cols + 1).fill(0));
    for (let i = rows - 1; i >= 0; i -= 1) {
      for (let j = cols - 1; j >= 0; j -= 1) {
        table[i][j] =
          baseMid[i] === currentMid[j] ? table[i + 1][j + 1] + 1 : Math.max(table[i + 1][j], table[i][j + 1]);
      }
    }
    let i = 0;
    let j = 0;
    while (i < rows && j < cols) {
      if (baseMid[i] === currentMid[j]) {
        matches.push([prefix + i, prefix + j]);
        i += 1;
        j += 1;
      } else if (table[i + 1][j] >= table[i][j + 1]) {
        i += 1;
      } else {
        j += 1;
      }
    }
  }
  for (let index = 0; index < suffix; index += 1) {
    matches.push([base.length - suffix + index, current.length - suffix + index]);
  }
  return matches;
}

function pushSegment(segments: DiffSegment[], kind: DiffSegment["kind"], text: string): void {
  if (text === "") return;
  const last = segments[segments.length - 1];
  if (last && last.kind === kind) last.text += text;
  else segments.push({ kind, text });
}

/** Word-level segments between the base and the current text. */
export function diffSegments(base: string, current: string): DiffSegment[] {
  if (base === current) return base ? [{ kind: "same", text: base }] : [];
  const baseTokens = tokenize(base);
  const currentTokens = tokenize(current);
  const matches = commonTokens(baseTokens, currentTokens);
  const segments: DiffSegment[] = [];
  let baseIndex = 0;
  let currentIndex = 0;
  for (const [baseMatch, currentMatch] of matches) {
    pushSegment(segments, "removed", baseTokens.slice(baseIndex, baseMatch).join(""));
    pushSegment(segments, "added", currentTokens.slice(currentIndex, currentMatch).join(""));
    pushSegment(segments, "same", baseTokens[baseMatch]);
    baseIndex = baseMatch + 1;
    currentIndex = currentMatch + 1;
  }
  pushSegment(segments, "removed", baseTokens.slice(baseIndex).join(""));
  pushSegment(segments, "added", currentTokens.slice(currentIndex).join(""));
  return segments;
}
