/**
 * Conditional-format and data-validation rule evaluation, shared by the grid
 * render and the rule dialogs.
 */
import type { CondRule } from "../../lib/office-types";
import { addressesInRange, type Scalar } from "./formula";

/** The highlight one rule paints on one cell, or null when it does not match. */
export function ruleFill(
  rule: CondRule,
  value: Scalar,
  number: number,
  counts: Map<string, number>,
  threshold: number,
): string | null {
  const [first, second] = rule.values;
  switch (rule.kind) {
    case "greater":
      return Number.isFinite(number) && number > Number(first) ? (rule.fill ?? "#FEE2E2") : null;
    case "less":
      return Number.isFinite(number) && number < Number(first) ? (rule.fill ?? "#FEE2E2") : null;
    case "between":
      return Number.isFinite(number) && number >= Number(first) && number <= Number(second)
        ? (rule.fill ?? "#FEF3C7")
        : null;
    case "equal":
      return String(value) === String(first) ? (rule.fill ?? "#DBEAFE") : null;
    case "textContains":
      return String(value).toLowerCase().includes(String(first).toLowerCase()) ? (rule.fill ?? "#E0E7FF") : null;
    case "duplicate":
      return String(value ?? "") !== "" && (counts.get(String(value ?? "")) ?? 0) > 1 ? (rule.fill ?? "#FECACA") : null;
    case "top":
      return Number.isFinite(number) && number >= threshold ? (rule.fill ?? "#BBF7D0") : null;
    default:
      return null;
  }
}

/** `items` are the resolved choices of a list rule (see `listValidationItems`). */
export function isValid(
  rule: { kind: string; values: string[]; min: number | null; max: number | null },
  value: Scalar,
  items: readonly string[] = rule.values,
): boolean {
  if (value === "" || value === undefined) return true;
  if (rule.kind === "list")
    return items.map((entry) => entry.trim().toLowerCase()).includes(String(value).trim().toLowerCase());
  const number = Number(value);
  if (!Number.isFinite(number)) return rule.kind !== "number";
  if (rule.kind === "number") {
    if (rule.min !== null && number < rule.min) return false;
    if (rule.max !== null && number > rule.max) return false;
  }
  return true;
}

/**
 * Highlight colour per cell for every fill-style rule, evaluated once per
 * sheet/data change instead of per visible cell. The old per-cell
 * `conditionalFill` rescanned the rule range for every cell in the viewport,
 * which made a 5 000-cell rule quadratic on the render path.
 */
export function computeConditionalFills(
  rules: readonly CondRule[],
  computed: ReadonlyMap<string, Scalar>,
): Map<string, string> {
  const fills = new Map<string, string>();
  for (const rule of rules) {
    if (rule.kind === "dataBar" || fills.size >= 50_000) continue;
    const addresses = addressesInRange(rule.range, 5000);
    if (addresses.length === 0) continue;
    const values = addresses.map((address) => computed.get(address) ?? "");
    const numbers = values.map((value) => (typeof value === "number" ? value : Number(value)));
    const limit = Math.max(1, rule.topN ?? 10);
    const sorted = [...numbers.filter(Number.isFinite)].sort((a, b) => b - a);
    const threshold = sorted[Math.min(limit, sorted.length) - 1] ?? Number.POSITIVE_INFINITY;
    const counts = new Map<string, number>();
    if (rule.kind === "duplicate") {
      for (const value of values) {
        const key = String(value ?? "");
        if (key !== "") counts.set(key, (counts.get(key) ?? 0) + 1);
      }
    }
    addresses.forEach((address, index) => {
      // First matching rule wins, in the order the rules are listed.
      if (fills.has(address)) return;
      const fill = ruleFill(rule, values[index], numbers[index], counts, threshold);
      if (fill) fills.set(address, fill);
    });
  }
  return fills;
}

/**
 * One pass over the data-bar rules: the scale of a bar is relative to the
 * largest value in its range, which is how every spreadsheet draws them.
 */
export function computeDataBars(
  rules: readonly CondRule[],
  computed: ReadonlyMap<string, Scalar>,
): Map<string, { max: number; fill: string }> {
  const bars = new Map<string, { max: number; fill: string }>();
  for (const rule of rules) {
    if (rule.kind !== "dataBar") continue;
    const addresses = addressesInRange(rule.range, 5000);
    let max = 0;
    for (const address of addresses) {
      const value = Math.abs(Number(computed.get(address) ?? 0));
      if (Number.isFinite(value)) max = Math.max(max, value);
    }
    for (const address of addresses) bars.set(address, { max, fill: rule.fill ?? "#638EC6" });
  }
  return bars;
}
