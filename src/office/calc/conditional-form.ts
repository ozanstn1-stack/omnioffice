/**
 * The conditional-format dialog's form and the rule it makes. Pure, so what the
 * dialog accepts and stores can be tested without rendering it.
 */
import type { CondRule, CondThreshold } from "../../lib/office-types";
import { formatAddress, parseRange, type RangeParts } from "./formula";

export const RULE_TYPES = [
  "greater",
  "less",
  "between",
  "equal",
  "textContains",
  "duplicate",
  "top",
  "bottom",
  "expression",
  "colorScale",
  "dataBar",
  "iconSet",
] as const;
export type RuleType = (typeof RULE_TYPES)[number];

/** Rules that paint a fill, a font colour, bold or italic. */
export const HIGHLIGHT_TYPES: readonly RuleType[] = [
  "greater",
  "less",
  "between",
  "equal",
  "textContains",
  "duplicate",
  "top",
  "bottom",
  "expression",
];

/** One stop of a colour scale or one bound of an icon set, as the dialog edits it. */
export interface StopForm {
  kind: CondThreshold["kind"];
  value: string;
  color: string;
}

export interface RuleForm {
  type: RuleType;
  range: string;
  first: string;
  second: string;
  formula: string;
  fill: string;
  useFontColor: boolean;
  fontColor: string;
  bold: boolean;
  italic: boolean;
  /** Two or three colours in a scale. */
  colors: 2 | 3;
  /** Lowest, middle and highest stop. */
  stops: [StopForm, StopForm, StopForm];
  barColor: string;
  showValue: boolean;
  iconSet: string;
  reverseIcons: boolean;
  /** The lower bound of the middle and the top icon. */
  icons: [StopForm, StopForm];
}

export function defaultRuleForm(range: string): RuleForm {
  return {
    type: "greater",
    range,
    first: "100",
    second: "0",
    formula: "",
    fill: "#FEE2E2",
    useFontColor: false,
    fontColor: "#9C0006",
    bold: false,
    italic: false,
    colors: 3,
    stops: [
      { kind: "min", value: "", color: "#F8696B" },
      { kind: "percentile", value: "50", color: "#FFEB84" },
      { kind: "max", value: "", color: "#63BE7B" },
    ],
    barColor: "#638EC6",
    showValue: true,
    iconSet: "3TrafficLights1",
    reverseIcons: false,
    icons: [
      { kind: "percent", value: "33", color: "" },
      { kind: "percent", value: "67", color: "" },
    ],
  };
}

/** A new rule starts on the selected block, or on everything in use (`used`) when one cell is selected. */
export function defaultRuleRange(selection: RangeParts, used: string): string {
  const { start, end } = selection;
  return start.row === end.row && start.col === end.col
    ? used
    : `${formatAddress(start.row, start.col)}:${formatAddress(end.row, end.col)}`;
}

export type RuleResult = { rule: Omit<CondRule, "id"> } | { error: string };

/** True when every part of the range reads as a cell or a block of cells. */
export function isValidRange(range: string): boolean {
  const parts = range.trim().split(/[\s,]+/);
  const shape = /^\$?[A-Z]{1,3}\$?\d+(:\$?[A-Z]{1,3}\$?\d+)?$/i;
  return parts[0] !== "" && parts.every((part) => shape.test(part) && parseRange(part) !== null);
}

/** The error (a translation key) for a threshold the rule cannot use, else null. */
function thresholdError(stop: StopForm): string | null {
  const value = stop.value.trim();
  if (stop.kind === "min" || stop.kind === "max") return null;
  if (stop.kind === "formula") return value === "" ? "calc.cfBadFormula" : null;
  const amount = Number(value);
  if (value === "" || !Number.isFinite(amount)) return "calc.cfBadNumber";
  if ((stop.kind === "percent" || stop.kind === "percentile") && (amount < 0 || amount > 100))
    return "calc.cfBadPercent";
  return null;
}

function thresholdOf(stop: StopForm, withColor: boolean): CondThreshold {
  const open = stop.kind === "min" || stop.kind === "max";
  return {
    kind: stop.kind,
    value: open ? "" : stop.value.trim(),
    ...(withColor ? { color: stop.color } : {}),
  };
}

/** The rule the form describes, or the translation key of what is wrong with it. */
export function ruleFromForm(form: RuleForm): RuleResult {
  const range = form.range.trim().toUpperCase();
  if (!isValidRange(range)) return { error: "calc.cfBadRange" };
  const base = { range, values: [] as string[], fill: null, color: null, topN: null, stopIfTrue: false };
  const first = form.first.trim();
  const second = form.second.trim();
  const isNumber = (text: string) => text !== "" && Number.isFinite(Number(text));

  switch (form.type) {
    case "greater":
    case "less":
    case "between":
    case "equal":
    case "textContains":
    case "duplicate":
    case "top":
    case "bottom":
    case "expression": {
      let values: string[] = [];
      let topN: number | null = null;
      let formula: string | null = null;
      if (form.type === "greater" || form.type === "less") {
        if (!isNumber(first)) return { error: "calc.cfBadNumber" };
        values = [first];
      } else if (form.type === "between") {
        if (!isNumber(first) || !isNumber(second)) return { error: "calc.cfBadNumber" };
        values = [first, second];
      } else if (form.type === "equal" || form.type === "textContains") {
        if (first === "") return { error: "calc.cfBadValue" };
        values = [first];
      } else if (form.type === "top" || form.type === "bottom") {
        const count = Math.floor(Number(first));
        if (!isNumber(first) || count < 1) return { error: "calc.cfBadNumber" };
        topN = count;
      } else if (form.type === "expression") {
        formula = form.formula.trim().replace(/^=/, "").trim();
        if (formula === "") return { error: "calc.cfBadFormula" };
      }
      return {
        rule: {
          ...base,
          kind: form.type,
          values,
          fill: form.fill,
          color: form.useFontColor ? form.fontColor : null,
          topN,
          ...(formula !== null ? { formula } : {}),
          ...(form.bold ? { bold: true } : {}),
          ...(form.italic ? { italic: true } : {}),
        },
      };
    }
    case "colorScale": {
      const stops = form.colors === 2 ? [form.stops[0], form.stops[2]] : form.stops;
      for (const stop of stops) {
        const error = thresholdError(stop);
        if (error) return { error };
      }
      return { rule: { ...base, kind: "colorScale", thresholds: stops.map((stop) => thresholdOf(stop, true)) } };
    }
    case "dataBar":
      return {
        rule: { ...base, kind: "dataBar", fill: form.barColor, ...(form.showValue ? {} : { hideValue: true }) },
      };
    case "iconSet": {
      for (const stop of form.icons) {
        const error = thresholdError(stop);
        if (error) return { error };
      }
      return {
        rule: {
          ...base,
          kind: "iconSet",
          iconSet: form.iconSet,
          thresholds: [{ kind: "percent", value: "0" }, ...form.icons.map((stop) => thresholdOf(stop, false))],
          ...(form.reverseIcons ? { reverseIcons: true } : {}),
          ...(form.showValue ? {} : { hideValue: true }),
        },
      };
    }
  }
}
