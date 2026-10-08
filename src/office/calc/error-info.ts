/**
 * What the grid says about an error value: the explanation shown as the cell's
 * tooltip and in the banner under the ribbon. The codes are the ones the formula
 * engine produces (`ERR` in scalars.ts); an unknown code has no explanation.
 */
import type { Translate } from "../../lib/i18n";

const EXPLANATIONS: Record<string, string> = {
  "#REF!": "calc.error_ref",
  "#VALUE!": "calc.error_value",
  "#NAME?": "calc.error_name",
  "#DIV/0!": "calc.error_div0",
  "#N/A": "calc.error_na",
  "#NUM!": "calc.error_num",
  "#SPILL!": "calc.error_spill",
  "#CALC!": "calc.error_calc",
};

/** The translation key explaining an error code, or null for one the grid does not know. */
export function errorExplanationKey(code: string): string | null {
  return EXPLANATIONS[code.toUpperCase()] ?? null;
}

/** `#CALC!: The array is empty.`, or null when the code is not a known error. */
export function errorTitle(t: Translate, code: string): string | null {
  const key = errorExplanationKey(code);
  return key ? `${code}: ${t(key)}` : null;
}
