/**
 * Conditional formatting: what each rule of a sheet does to a cell.
 *
 * `prepareConditional` reads the rules once per data change: it indexes the
 * calculated values, takes the statistics a rule needs (lowest and highest
 * value, percentiles, duplicate counts) over its range and resolves its
 * thresholds. After that `formatAt(row, col)` is a handful of comparisons per
 * rule, so the grid asks only for the cells it draws, whatever the size of the
 * range; an expression rule evaluates its formula for those cells only, once
 * each.
 *
 * Rules apply in list order. A cell collects the properties of every rule that
 * matches it, an earlier rule winning when two set the same property (a fill,
 * a font colour, a bar, an icon), and `stopIfTrue` ends the walk.
 */
import type { CondRule, CondThreshold } from "../../lib/office-types";
import { shiftFormulaRows } from "./cells";
import { formatAddress, isError, parseAddress, parseRange, type Scalar } from "./formula";
import { shiftFormulaColumns } from "./grid-math";

/** What the rules add to one cell. */
export interface CellFormat {
  /** Background: a highlight fill or a colour-scale colour. */
  fill?: string;
  /** Font colour. */
  color?: string;
  bold?: boolean;
  italic?: boolean;
  /** A data bar: `fraction` (0..1) of the cell width. */
  bar?: { fraction: number; color: string };
  /**
   * An icon: `tier` 0 is the first of the set's `count` icons (already reversed for a reversed
   * set); `rank` 0 is the lowest tier of values, whatever icon it gets.
   */
  icon?: { set: string; tier: number; rank: number; count: number };
  /** Show only the bar or icon, not the cell value. */
  hideValue?: boolean;
}

export interface ConditionalContext {
  /** The calculated values of the sheet by address. */
  values: ReadonlyMap<string, Scalar>;
  /** Evaluates a formula (with its `=`) as if it were typed in the cell at `row`, `col` (0-based). */
  evaluate: (formula: string, row: number, col: number) => Scalar;
}

export interface PreparedConditional {
  /** The combined format of a cell, or null when no rule touches it. */
  formatAt: (row: number, col: number) => CellFormat | null;
  /** True when there is nothing to look up (no rule has a usable range). */
  empty: boolean;
}

// ---------------------------------------------------------------------------
// Numbers and colours
// ---------------------------------------------------------------------------

/** The value of a number cell; null for text, booleans, errors and blanks. */
function numberOf(value: Scalar): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

/** The number a comparison rule sees: text that reads as a number counts, a blank is 0. */
function looseNumber(value: Scalar): number {
  return typeof value === "number" ? value : Number(value);
}

/** PERCENTILE.INC over an ascending list; `p` is a fraction. */
export function percentileInc(sorted: readonly number[], p: number): number {
  if (sorted.length === 0) return Number.NaN;
  const rank = Math.min(1, Math.max(0, p)) * (sorted.length - 1);
  const below = Math.floor(rank);
  const above = Math.ceil(rank);
  return sorted[below] + (sorted[above] - sorted[below]) * (rank - below);
}

type Rgb = [number, number, number];

export function parseColor(color: string | null | undefined): Rgb | null {
  const hex = /^#?([0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/i.exec((color ?? "").trim())?.[1];
  if (!hex) return null;
  // A leading alpha pair (#AARRGGBB, as OOXML writes it) is dropped.
  const body = hex.length === 8 ? hex.slice(2) : hex.length === 3 ? [...hex].map((c) => c + c).join("") : hex;
  return [0, 2, 4].map((at) => parseInt(body.slice(at, at + 2), 16)) as Rgb;
}

export function toHex(rgb: Rgb): string {
  return `#${rgb
    .map((part) =>
      Math.round(Math.min(255, Math.max(0, part)))
        .toString(16)
        .padStart(2, "0"),
    )
    .join("")}`;
}

/** The colour at `value` on a scale of stops (ascending positions), blending linearly in RGB. */
export function scaleColor(stops: ReadonlyArray<{ pos: number; rgb: Rgb }>, value: number): string {
  const first = stops[0];
  const last = stops[stops.length - 1];
  if (value <= first.pos) return toHex(first.rgb);
  if (value >= last.pos) return toHex(last.rgb);
  let at = 1;
  while (value > stops[at].pos) at += 1;
  const a = stops[at - 1];
  const b = stops[at];
  const t = (value - a.pos) / (b.pos - a.pos || 1);
  return toHex(a.rgb.map((part, index) => part + (b.rgb[index] - part) * t) as Rgb);
}

/** Colours a stop falls back to when the rule gives none or an unreadable one (Excel's red-yellow-green). */
const DEFAULT_STOPS: Record<number, Rgb[]> = {
  2: [
    [0xf8, 0x69, 0x6b],
    [0x63, 0xbe, 0x7b],
  ],
  3: [
    [0xf8, 0x69, 0x6b],
    [0xff, 0xeb, 0x84],
    [0x63, 0xbe, 0x7b],
  ],
};

export const DEFAULT_BAR_COLOR = "#638EC6";

// ---------------------------------------------------------------------------
// Icon sets
// ---------------------------------------------------------------------------

/** The icon sets the editor offers; files can name others, which draw as coloured dots. */
export const ICON_SETS = ["3Arrows", "3TrafficLights1", "3Flags"] as const;

/** How many icons a set has: the digit its OOXML name starts with. */
export function iconCount(set: string | null | undefined): number {
  const count = Number(/^(\d)/.exec(set ?? "")?.[1]);
  return count >= 3 && count <= 5 ? count : 3;
}

/** The default lower bound of each icon, as a percentage: 0, 33, 67 for three icons. */
export function defaultIconPercents(count: number): number[] {
  return Array.from({ length: count }, (_unused, index) => Math.round((100 * index) / count));
}

// ---------------------------------------------------------------------------
// Rules
// ---------------------------------------------------------------------------

interface Area {
  top: number;
  left: number;
  bottom: number;
  right: number;
}

/** The areas of a range such as `A1:B9` or `A1:A9 C1:C9` (OOXML lists areas with spaces); unreadable parts are skipped. */
export function rangeAreas(range: string): Area[] {
  const areas: Area[] = [];
  for (const part of range.split(/[\s,]+/)) {
    const parts = part ? parseRange(part) : null;
    if (parts) areas.push({ top: parts.start.row, left: parts.start.col, bottom: parts.end.row, right: parts.end.col });
  }
  return areas;
}

const inArea = (area: Area, row: number, col: number) =>
  row >= area.top && row <= area.bottom && col >= area.left && col <= area.right;

interface Entry {
  row: number;
  col: number;
  value: Scalar;
}

function indexValues(values: ReadonlyMap<string, Scalar>): Entry[] {
  const entries: Entry[] = [];
  for (const [address, value] of values) {
    const position = parseAddress(address);
    if (position) entries.push({ row: position.row, col: position.col, value });
  }
  return entries;
}

interface Stats {
  min: number;
  max: number;
  /** The numbers of the range, ascending. */
  sorted: number[];
}

function statsOf(entries: readonly Entry[]): Stats | null {
  const sorted = entries
    .map((entry) => numberOf(entry.value))
    .filter((value): value is number => value !== null)
    .sort((a, b) => a - b);
  return sorted.length === 0 ? null : { min: sorted[0], max: sorted[sorted.length - 1], sorted };
}

/** The number a threshold stands for within a range, or null when it cannot be read. */
function thresholdValue(threshold: CondThreshold, stats: Stats, formula: (source: string) => Scalar): number | null {
  const amount = Number(threshold.value);
  const given = threshold.value.trim() !== "" && Number.isFinite(amount);
  switch (threshold.kind) {
    case "min":
      return stats.min;
    case "max":
      return stats.max;
    case "num":
      return given ? amount : null;
    case "percent":
      return given ? stats.min + ((stats.max - stats.min) * amount) / 100 : null;
    case "percentile":
      return given ? percentileInc(stats.sorted, amount / 100) : null;
    case "formula": {
      const result = formula(threshold.value);
      return typeof result === "number" && Number.isFinite(result) ? result : null;
    }
    default:
      return null;
  }
}

/** One rule ready to be asked about cells. */
interface Compiled {
  rule: CondRule;
  areas: Area[];
  /** What the rule adds to a cell holding `value`, or null when the rule does not apply to it. */
  apply: (value: Scalar, row: number, col: number) => CellFormat | null;
}

const DEFAULT_FILLS: Record<string, string> = {
  greater: "#FEE2E2",
  less: "#FEE2E2",
  between: "#FEF3C7",
  equal: "#DBEAFE",
  textContains: "#E0E7FF",
  duplicate: "#FECACA",
  top: "#BBF7D0",
  bottom: "#BBF7D0",
  expression: "#FEF3C7",
};

/** The look of a matching highlight rule: its fill (a kind-specific default when it styles nothing else), font colour, bold and italic. */
function highlightLook(rule: CondRule): CellFormat {
  const look: CellFormat = {};
  if (rule.fill) look.fill = rule.fill;
  if (rule.color) look.color = rule.color;
  if (rule.bold) look.bold = true;
  if (rule.italic) look.italic = true;
  if (Object.keys(look).length === 0) look.fill = DEFAULT_FILLS[rule.kind];
  return look;
}

/** Shifts the relative references of a rule formula from the first cell of its range to `row`, `col`. */
function shiftedFormula(source: string, dRow: number, dCol: number): string {
  return shiftFormulaColumns(shiftFormulaRows(source, dRow), dCol) ?? source;
}

function compile(rule: CondRule, context: ConditionalContext, entries: () => Entry[]): Compiled | null {
  const areas = rangeAreas(rule.range);
  if (areas.length === 0) return null;
  const origin = areas[0];
  const here = (source: string) =>
    context.evaluate(source.startsWith("=") ? source : `=${source}`, origin.top, origin.left);
  const inRange = () => entries().filter((entry) => areas.some((area) => inArea(area, entry.row, entry.col)));
  const [first, second] = rule.values ?? [];

  switch (rule.kind) {
    case "greater":
    case "less":
    case "between":
    case "equal":
    case "textContains": {
      const look = highlightLook(rule);
      const a = Number(first);
      const b = Number(second);
      const matches = (value: Scalar): boolean => {
        const number = looseNumber(value);
        switch (rule.kind) {
          case "greater":
            return Number.isFinite(number) && number > a;
          case "less":
            return Number.isFinite(number) && number < a;
          case "between":
            return Number.isFinite(number) && number >= a && number <= b;
          case "equal":
            return (
              String(value) === String(first) ||
              (typeof value === "number" && String(first ?? "").trim() !== "" && value === a)
            );
          default:
            return String(value)
              .toLowerCase()
              .includes(String(first ?? "").toLowerCase());
        }
      };
      return { rule, areas, apply: (value) => (matches(value) ? look : null) };
    }
    case "duplicate": {
      const look = highlightLook(rule);
      const counts = new Map<string, number>();
      for (const entry of inRange()) {
        const key = String(entry.value ?? "");
        if (key !== "") counts.set(key, (counts.get(key) ?? 0) + 1);
      }
      return {
        rule,
        areas,
        apply: (value) => {
          const key = String(value ?? "");
          return key !== "" && (counts.get(key) ?? 0) > 1 ? look : null;
        },
      };
    }
    case "top":
    case "bottom": {
      const look = highlightLook(rule);
      const stats = statsOf(inRange());
      if (!stats) return null;
      const limit = Math.max(1, rule.topN ?? 10);
      const ordered = rule.kind === "top" ? [...stats.sorted].reverse() : stats.sorted;
      const edge = ordered[Math.min(limit, ordered.length) - 1];
      return {
        rule,
        areas,
        apply: (value) => {
          const number = numberOf(value);
          if (number === null) return null;
          return (rule.kind === "top" ? number >= edge : number <= edge) ? look : null;
        },
      };
    }
    case "expression": {
      const source = (rule.formula ?? "").trim().replace(/^=/, "");
      if (source === "") return null;
      const look = highlightLook(rule);
      const cache = new Map<number, boolean>();
      return {
        rule,
        areas,
        apply: (_value, row, col) => {
          const key = row * 16_384 + col;
          let hit = cache.get(key);
          if (hit === undefined) {
            const result = context.evaluate(
              `=${shiftedFormula(source, row - origin.top, col - origin.left)}`,
              row,
              col,
            );
            hit = !isError(result) && (result === true || (typeof result === "number" && result !== 0));
            cache.set(key, hit);
          }
          return hit ? look : null;
        },
      };
    }
    case "colorScale": {
      const stats = statsOf(inRange());
      const given = (rule.thresholds ?? []).slice(0, 3);
      if (!stats || given.length < 2) return null;
      const fallback = DEFAULT_STOPS[given.length];
      const stops: Array<{ pos: number; rgb: Rgb }> = [];
      for (const [index, threshold] of given.entries()) {
        const pos = thresholdValue(threshold, stats, here);
        if (pos === null) return null;
        stops.push({ pos, rgb: parseColor(threshold.color) ?? fallback[index] });
      }
      stops.sort((a, b) => a.pos - b.pos);
      return {
        rule,
        areas,
        apply: (value) => {
          const number = numberOf(value);
          return number === null ? null : { fill: scaleColor(stops, number) };
        },
      };
    }
    case "dataBar": {
      const stats = statsOf(inRange());
      if (!stats) return null;
      const [lowThreshold, highThreshold] = rule.thresholds ?? [];
      // The ends are automatic unless a threshold fixes them: from zero (or the lowest negative
      // value) to the highest value, which is how Excel scales a bar that says only "min" and "max".
      const end = (threshold: CondThreshold | undefined, automatic: number) =>
        threshold && threshold.kind !== "min" && threshold.kind !== "max"
          ? (thresholdValue(threshold, stats, here) ?? automatic)
          : automatic;
      const low = end(lowThreshold, Math.min(0, stats.min));
      const high = end(highThreshold, Math.max(0, stats.max));
      const color = rule.fill ?? DEFAULT_BAR_COLOR;
      return {
        rule,
        areas,
        apply: (value) => {
          const number = numberOf(value);
          if (number === null) return null;
          const fraction = high > low ? Math.min(1, Math.max(0, (number - low) / (high - low))) : 0;
          return { bar: { fraction, color }, ...(rule.hideValue ? { hideValue: true } : {}) };
        },
      };
    }
    case "iconSet": {
      const stats = statsOf(inRange());
      if (!stats) return null;
      const set = rule.iconSet ?? "3TrafficLights1";
      const count = iconCount(set);
      const percents = defaultIconPercents(count);
      const bounds = Array.from({ length: count }, (_unused, index) => {
        const threshold = rule.thresholds?.[index] ?? { kind: "percent" as const, value: String(percents[index]) };
        return index === 0 ? -Infinity : (thresholdValue(threshold, stats, here) ?? Infinity);
      });
      return {
        rule,
        areas,
        apply: (value) => {
          const number = numberOf(value);
          if (number === null) return null;
          let tier = 0;
          for (let index = 1; index < count; index += 1) if (number >= bounds[index]) tier = index;
          return {
            icon: { set, tier: rule.reverseIcons ? count - 1 - tier : tier, rank: tier, count },
            ...(rule.hideValue ? { hideValue: true } : {}),
          };
        },
      };
    }
    default:
      return null;
  }
}

const NO_RULES: PreparedConditional = { formatAt: () => null, empty: true };

export function prepareConditional(rules: readonly CondRule[], context: ConditionalContext): PreparedConditional {
  if (rules.length === 0) return NO_RULES;
  let indexed: Entry[] | null = null;
  const entries = () => (indexed ??= indexValues(context.values));
  const compiled = rules
    .map((rule) => compile(rule, context, entries))
    .filter((candidate): candidate is Compiled => candidate !== null);
  if (compiled.length === 0) return NO_RULES;

  const cache = new Map<number, CellFormat | null>();
  const formatAt = (row: number, col: number): CellFormat | null => {
    const key = row * 16_384 + col;
    const known = cache.get(key);
    if (known !== undefined) return known;
    const value = context.values.get(formatAddress(row, col)) ?? "";
    let combined: CellFormat | null = null;
    for (const entry of compiled) {
      if (!entry.areas.some((area) => inArea(area, row, col))) continue;
      const found = entry.apply(value, row, col);
      if (!found) continue;
      combined = { ...found, ...(combined ?? {}) };
      // `combined` keys win over `found`: an earlier rule keeps what it set.
      if (entry.rule.stopIfTrue) break;
    }
    cache.set(key, combined);
    return combined;
  };
  return { formatAt, empty: false };
}
