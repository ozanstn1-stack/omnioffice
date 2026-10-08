/**
 * Data-validation rule evaluation, shared by the grid render and the rule
 * dialogs. Conditional formats live in conditional.ts.
 */
import type { Scalar } from "./formula";

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
