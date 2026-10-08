/**
 * Conditional formatting: colour scales, data bars, icon sets, highlight rules
 * and formula rules, evaluated over the calculated values of a sheet.
 */
import { describe, expect, it } from "vitest";
import { newSheet, newWorkbook, type CondRule, type CondThreshold, type Workbook } from "../../lib/office-types";
import { applyCellEdit, computeSheetValues, sheetFormulaEvaluator } from "./cells";
import {
  defaultIconPercents,
  iconCount,
  parseColor,
  percentileInc,
  prepareConditional,
  rangeAreas,
  scaleColor,
  toHex,
} from "./conditional";
import type { Scalar } from "./formula";

function rule(patch: Partial<CondRule> & Pick<CondRule, "kind" | "range">): CondRule {
  return { id: "r", values: [], fill: null, color: null, topN: null, stopIfTrue: false, ...patch };
}

const t = (kind: CondThreshold["kind"], value = "", color?: string): CondThreshold => ({ kind, value, color });

/** Values by address, with a formula evaluator that is never expected to run. */
function over(values: Record<string, Scalar>, rules: CondRule[]) {
  return prepareConditional(rules, {
    values: new Map(Object.entries(values)),
    evaluate: () => {
      throw new Error("no formula expected");
    },
  });
}

/** A workbook typed like a user would, so formula rules run on the real engine. */
function sheetOf(cells: Record<string, string>, rules: CondRule[]) {
  let workbook: Workbook = { ...newWorkbook("CF"), sheets: [newSheet("Sheet1")] };
  for (const [address, text] of Object.entries(cells)) {
    workbook = applyCellEdit(workbook, 0, Number(address.slice(1)) - 1, address.charCodeAt(0) - 65, text);
  }
  const sheet = workbook.sheets[0];
  return prepareConditional(rules, {
    values: computeSheetValues(workbook, sheet),
    evaluate: sheetFormulaEvaluator(workbook, sheet),
  });
}

describe("colours", () => {
  it("reads #RGB, #RRGGBB and OOXML #AARRGGBB, with or without the hash", () => {
    expect(parseColor("#ff8000")).toEqual([255, 128, 0]);
    expect(parseColor("ff8000")).toEqual([255, 128, 0]);
    expect(parseColor("#f80")).toEqual([255, 136, 0]);
    expect(parseColor("#80ff8000")).toEqual([255, 128, 0]);
    expect(parseColor("red")).toBeNull();
    expect(parseColor(null)).toBeNull();
    expect(parseColor("#12345")).toBeNull();
  });

  it("writes lowercase hex and clamps each channel", () => {
    expect(toHex([255, 128, 0])).toBe("#ff8000");
    expect(toHex([300, -4, 15.4])).toBe("#ff000f");
  });

  it("blends linearly between stops and holds the end colours outside them", () => {
    const stops = [
      { pos: 0, rgb: [0, 0, 0] as [number, number, number] },
      { pos: 10, rgb: [100, 200, 50] as [number, number, number] },
      { pos: 30, rgb: [255, 255, 255] as [number, number, number] },
    ];
    expect(scaleColor(stops, -5)).toBe("#000000");
    expect(scaleColor(stops, 0)).toBe("#000000");
    expect(scaleColor(stops, 5)).toBe("#326419");
    expect(scaleColor(stops, 10)).toBe("#64c832");
    expect(scaleColor(stops, 20)).toBe("#b2e499");
    expect(scaleColor(stops, 30)).toBe("#ffffff");
    expect(scaleColor(stops, 99)).toBe("#ffffff");
  });
});

describe("percentiles", () => {
  it("follows PERCENTILE.INC", () => {
    const sorted = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    expect(percentileInc(sorted, 0)).toBe(1);
    expect(percentileInc(sorted, 0.5)).toBe(5.5);
    expect(percentileInc(sorted, 0.9)).toBeCloseTo(9.1, 10);
    expect(percentileInc(sorted, 1)).toBe(10);
    expect(percentileInc(sorted, 2)).toBe(10);
    expect(percentileInc([7], 0.3)).toBe(7);
    expect(percentileInc([], 0.5)).toBeNaN();
  });
});

describe("ranges", () => {
  it("reads one area, several areas and skips what it cannot read", () => {
    expect(rangeAreas("A1:B9")).toEqual([{ top: 0, left: 0, bottom: 8, right: 1 }]);
    expect(rangeAreas("$B$2:$A$1")).toEqual([{ top: 0, left: 0, bottom: 1, right: 1 }]);
    expect(rangeAreas("A1:A2 C3:C4")).toEqual([
      { top: 0, left: 0, bottom: 1, right: 0 },
      { top: 2, left: 2, bottom: 3, right: 2 },
    ]);
    expect(rangeAreas("A1:A2,C3")).toHaveLength(2);
    expect(rangeAreas("nonsense")).toEqual([]);
  });

  it("formats only the cells inside the rule's range, however large it is", () => {
    const prepared = over({ A1: 5, A2: 500000 }, [
      rule({ kind: "greater", range: "A1:A1048576", values: ["10"], fill: "#ff0000" }),
    ]);
    expect(prepared.formatAt(1, 0)).toEqual({ fill: "#ff0000" });
    expect(prepared.formatAt(0, 0)).toBeNull();
    expect(prepared.formatAt(1, 1)).toBeNull();
    expect(prepared.formatAt(1_000_000, 0)).toBeNull();
  });

  it("applies a rule over several areas", () => {
    const prepared = over({ A1: 50, C1: 50, E1: 50 }, [
      rule({ kind: "greater", range: "A1:A1 C1:C1", values: ["10"], fill: "#ff0000" }),
    ]);
    expect(prepared.formatAt(0, 0)).not.toBeNull();
    expect(prepared.formatAt(0, 2)).not.toBeNull();
    expect(prepared.formatAt(0, 4)).toBeNull();
  });

  it("has nothing to do without rules or with unreadable ones", () => {
    expect(over({}, []).empty).toBe(true);
    expect(over({ A1: 1 }, [rule({ kind: "greater", range: "??", values: ["0"] })]).empty).toBe(true);
    expect(over({ A1: 1 }, [rule({ kind: "mystery", range: "A1:A9" })]).empty).toBe(true);
  });
});

describe("colour scales", () => {
  const values = { A1: 0, A2: 50, A3: 100, A4: "text", A5: true };

  it("blends a two-colour scale between the lowest and the highest value", () => {
    const prepared = over(values, [
      rule({
        kind: "colorScale",
        range: "A1:A5",
        thresholds: [t("min", "", "#000000"), t("max", "", "#ffffff")],
      }),
    ]);
    expect(prepared.formatAt(0, 0)).toEqual({ fill: "#000000" });
    expect(prepared.formatAt(1, 0)).toEqual({ fill: "#808080" });
    expect(prepared.formatAt(2, 0)).toEqual({ fill: "#ffffff" });
    // Text and booleans are not part of the scale.
    expect(prepared.formatAt(3, 0)).toBeNull();
    expect(prepared.formatAt(4, 0)).toBeNull();
  });

  it("puts the middle colour at a three-colour scale's midpoint, by percentile, percent or number", () => {
    const base = { A1: 1, A2: 2, A3: 3, A4: 4, A5: 100 };
    const colours = [t("min", "", "#ff0000"), t("percentile", "50", "#ffff00"), t("max", "", "#00ff00")];
    const byPercentile = over(base, [rule({ kind: "colorScale", range: "A1:A5", thresholds: colours })]);
    // The median is 3, so the cell holding 3 has the middle colour exactly.
    expect(byPercentile.formatAt(2, 0)).toEqual({ fill: "#ffff00" });
    expect(byPercentile.formatAt(0, 0)).toEqual({ fill: "#ff0000" });
    expect(byPercentile.formatAt(4, 0)).toEqual({ fill: "#00ff00" });

    const byPercent = over(base, [
      rule({
        kind: "colorScale",
        range: "A1:A5",
        thresholds: [t("min", "", "#ff0000"), t("percent", "50", "#ffff00"), t("max", "", "#00ff00")],
      }),
    ]);
    // 50 percent of 1..100 is 50.5: a value of 4 is still close to the low colour.
    expect(byPercent.formatAt(3, 0)?.fill).toMatch(/^#f[0-9a-f]{3}00$/);

    const byNumber = over(base, [
      rule({
        kind: "colorScale",
        range: "A1:A5",
        thresholds: [t("num", "0", "#ff0000"), t("num", "4", "#ffff00"), t("num", "8", "#00ff00")],
      }),
    ]);
    expect(byNumber.formatAt(3, 0)).toEqual({ fill: "#ffff00" });
    expect(byNumber.formatAt(4, 0)).toEqual({ fill: "#00ff00" });
  });

  it("takes a formula threshold from the sheet", () => {
    const prepared = sheetOf({ A1: "0", A2: "5", A3: "10", C1: "10" }, [
      rule({
        kind: "colorScale",
        range: "A1:A3",
        thresholds: [t("num", "0", "#000000"), t("formula", "=C1*2", "#ffffff")],
      }),
    ]);
    expect(prepared.formatAt(1, 0)).toEqual({ fill: "#404040" });
    expect(prepared.formatAt(2, 0)).toEqual({ fill: "#808080" });
  });

  it("falls back to the default stop colours when a stop has none, and drops a scale it cannot resolve", () => {
    const withDefaults = over(values, [rule({ kind: "colorScale", range: "A1:A3", thresholds: [t("min"), t("max")] })]);
    expect(withDefaults.formatAt(0, 0)).toEqual({ fill: "#f8696b" });
    expect(withDefaults.formatAt(2, 0)).toEqual({ fill: "#63be7b" });
    expect(over(values, [rule({ kind: "colorScale", range: "A1:A3", thresholds: [t("min")] })]).empty).toBe(true);
    expect(
      over(values, [rule({ kind: "colorScale", range: "A1:A3", thresholds: [t("num", "x"), t("max")] })]).empty,
    ).toBe(true);
  });

  it("sorts stops given out of order", () => {
    const prepared = over({ A1: 0, A2: 10 }, [
      rule({
        kind: "colorScale",
        range: "A1:A2",
        thresholds: [t("num", "10", "#ffffff"), t("num", "0", "#000000")],
      }),
    ]);
    expect(prepared.formatAt(0, 0)).toEqual({ fill: "#000000" });
    expect(prepared.formatAt(1, 0)).toEqual({ fill: "#ffffff" });
  });
});

describe("data bars", () => {
  it("scales each bar to the highest value from zero, in the bar colour", () => {
    const prepared = over({ A1: 50, A2: 10, A3: 25, A4: "n/a" }, [
      rule({ kind: "dataBar", range: "A1:A4", fill: "#638ec6" }),
    ]);
    expect(prepared.formatAt(0, 0)).toEqual({ bar: { fraction: 1, color: "#638ec6" } });
    expect(prepared.formatAt(1, 0)?.bar?.fraction).toBeCloseTo(0.2, 10);
    expect(prepared.formatAt(2, 0)?.bar?.fraction).toBeCloseTo(0.5, 10);
    expect(prepared.formatAt(3, 0)).toBeNull();
  });

  it("uses the default colour and reads min/max thresholds as automatic ends", () => {
    const prepared = over({ A1: 40, A2: 20 }, [
      rule({ kind: "dataBar", range: "A1:A2", thresholds: [t("min"), t("max")] }),
    ]);
    expect(prepared.formatAt(0, 0)?.bar).toEqual({ fraction: 1, color: "#638EC6" });
    expect(prepared.formatAt(1, 0)?.bar?.fraction).toBeCloseTo(0.5, 10);
  });

  it("honours number, percent and percentile ends and clamps outside them", () => {
    const values = { A1: 0, A2: 50, A3: 100, A4: 200 };
    const fixed = over(values, [
      rule({ kind: "dataBar", range: "A1:A4", thresholds: [t("num", "50"), t("num", "150")] }),
    ]);
    expect(fixed.formatAt(0, 0)?.bar?.fraction).toBe(0);
    expect(fixed.formatAt(1, 0)?.bar?.fraction).toBe(0);
    expect(fixed.formatAt(2, 0)?.bar?.fraction).toBeCloseTo(0.5, 10);
    expect(fixed.formatAt(3, 0)?.bar?.fraction).toBe(1);

    const percent = over(values, [
      rule({ kind: "dataBar", range: "A1:A4", thresholds: [t("percent", "25"), t("percent", "75")] }),
    ]);
    expect(percent.formatAt(2, 0)?.bar?.fraction).toBeCloseTo(0.5, 10);

    const percentile = over(values, [
      rule({ kind: "dataBar", range: "A1:A4", thresholds: [t("percentile", "0"), t("percentile", "100")] }),
    ]);
    expect(percentile.formatAt(3, 0)?.bar?.fraction).toBe(1);
    expect(percentile.formatAt(0, 0)?.bar?.fraction).toBe(0);
  });

  it("starts the bar at the lowest value when the range has negatives", () => {
    const prepared = over({ A1: -10, A2: 0, A3: 10 }, [rule({ kind: "dataBar", range: "A1:A3" })]);
    expect(prepared.formatAt(0, 0)?.bar?.fraction).toBe(0);
    expect(prepared.formatAt(1, 0)?.bar?.fraction).toBeCloseTo(0.5, 10);
    expect(prepared.formatAt(2, 0)?.bar?.fraction).toBe(1);
  });

  it("draws no bar when every value is zero, and can hide the value", () => {
    expect(over({ A1: 0, A2: 0 }, [rule({ kind: "dataBar", range: "A1:A2" })]).formatAt(0, 0)?.bar?.fraction).toBe(0);
    const hidden = over({ A1: 4 }, [rule({ kind: "dataBar", range: "A1:A1", hideValue: true })]);
    expect(hidden.formatAt(0, 0)?.hideValue).toBe(true);
    expect(over({ A1: 4 }, [rule({ kind: "dataBar", range: "A1:A1" })]).formatAt(0, 0)?.hideValue).toBeUndefined();
  });
});

describe("icon sets", () => {
  const nine = { A1: 1, A2: 2, A3: 3, A4: 4, A5: 5, A6: 6, A7: 7, A8: 8, A9: 9 };
  const tierOf = (prepared: ReturnType<typeof over>, row: number) => prepared.formatAt(row, 0)?.icon;

  it("counts the icons of a set from its name and spaces the default thresholds evenly", () => {
    expect(iconCount("3Arrows")).toBe(3);
    expect(iconCount("4Rating")).toBe(4);
    expect(iconCount("5Quarters")).toBe(5);
    expect(iconCount("Funky")).toBe(3);
    expect(iconCount(null)).toBe(3);
    expect(defaultIconPercents(3)).toEqual([0, 33, 67]);
    expect(defaultIconPercents(4)).toEqual([0, 25, 50, 75]);
    expect(defaultIconPercents(5)).toEqual([0, 20, 40, 60, 80]);
  });

  it("splits the range into thirds by default: lowest tier first", () => {
    const prepared = over(nine, [rule({ kind: "iconSet", range: "A1:A9", iconSet: "3Arrows" })]);
    // Percent 33 of 1..9 is 3.64 and percent 67 is 6.36.
    expect([0, 1, 2].map((row) => tierOf(prepared, row)?.tier)).toEqual([0, 0, 0]);
    expect([3, 4, 5].map((row) => tierOf(prepared, row)?.tier)).toEqual([1, 1, 1]);
    expect([6, 7, 8].map((row) => tierOf(prepared, row)?.tier)).toEqual([2, 2, 2]);
    expect(tierOf(prepared, 0)).toEqual({ set: "3Arrows", tier: 0, rank: 0, count: 3 });
  });

  it("flips the tiers when the icons are reversed", () => {
    const prepared = over(nine, [rule({ kind: "iconSet", range: "A1:A9", iconSet: "3Flags", reverseIcons: true })]);
    expect(tierOf(prepared, 0)).toMatchObject({ tier: 2, rank: 0 });
    expect(tierOf(prepared, 8)).toMatchObject({ tier: 0, rank: 2 });
  });

  it("honours thresholds by number, percent and percentile; the first one is always the lowest tier", () => {
    const byNumber = over(nine, [
      rule({
        kind: "iconSet",
        range: "A1:A9",
        iconSet: "3TrafficLights1",
        thresholds: [t("num", "100"), t("num", "4"), t("num", "8")],
      }),
    ]);
    expect(tierOf(byNumber, 0)?.tier).toBe(0);
    expect(tierOf(byNumber, 2)?.tier).toBe(0);
    expect(tierOf(byNumber, 3)?.tier).toBe(1);
    expect(tierOf(byNumber, 6)?.tier).toBe(1);
    expect(tierOf(byNumber, 7)?.tier).toBe(2);

    const byPercentile = over(nine, [
      rule({
        kind: "iconSet",
        range: "A1:A9",
        thresholds: [t("percent", "0"), t("percentile", "50"), t("percentile", "90")],
      }),
    ]);
    // The median is 5 and the 90th percentile 8.2.
    expect(tierOf(byPercentile, 3)?.tier).toBe(0);
    expect(tierOf(byPercentile, 4)?.tier).toBe(1);
    expect(tierOf(byPercentile, 7)?.tier).toBe(1);
    expect(tierOf(byPercentile, 8)?.tier).toBe(2);
  });

  it("gives a set of four or five icons that many tiers", () => {
    const prepared = over({ A1: 0, A2: 25, A3: 50, A4: 75, A5: 100 }, [
      rule({ kind: "iconSet", range: "A1:A5", iconSet: "5Arrows" }),
    ]);
    expect([0, 1, 2, 3, 4].map((row) => tierOf(prepared, row)?.tier)).toEqual([0, 1, 2, 3, 4]);
    expect(tierOf(prepared, 0)?.count).toBe(5);
  });

  it("skips text and blanks, and can hide the value", () => {
    const prepared = over({ A1: 1, A2: "x", A3: 9 }, [rule({ kind: "iconSet", range: "A1:A4", hideValue: true })]);
    expect(prepared.formatAt(1, 0)).toBeNull();
    expect(prepared.formatAt(3, 0)).toBeNull();
    expect(prepared.formatAt(2, 0)?.hideValue).toBe(true);
  });
});

describe("highlight rules", () => {
  const values = { A1: 5, A2: 15, A3: 25, B1: "Apple pie", B2: "banana", B3: "apple" };

  it("greater, less and between use the rule's fill and compare numbers", () => {
    const prepared = over(values, [
      rule({ kind: "greater", range: "A1:A3", values: ["10"], fill: "#111111" }),
      rule({ kind: "between", range: "A1:A3", values: ["0", "10"], fill: "#222222" }),
    ]);
    expect(prepared.formatAt(0, 0)).toEqual({ fill: "#222222" });
    expect(prepared.formatAt(1, 0)).toEqual({ fill: "#111111" });
    const less = over(values, [rule({ kind: "less", range: "A1:A3", values: ["10"] })]);
    expect(less.formatAt(0, 0)).toEqual({ fill: "#FEE2E2" });
    expect(less.formatAt(1, 0)).toBeNull();
  });

  it("treats a blank cell inside the range as zero, like Excel", () => {
    const prepared = over({ A1: 5 }, [rule({ kind: "less", range: "A1:A3", values: ["1"] })]);
    expect(prepared.formatAt(2, 0)).not.toBeNull();
    expect(prepared.formatAt(0, 0)).toBeNull();
  });

  it("equal compares text and numbers; text contains ignores case", () => {
    const equal = over(values, [rule({ kind: "equal", range: "A1:B3", values: ["15"] })]);
    expect(equal.formatAt(1, 0)).not.toBeNull();
    expect(equal.formatAt(0, 0)).toBeNull();
    const decimal = over({ A1: 15 }, [rule({ kind: "equal", range: "A1:A1", values: ["15.0"] })]);
    expect(decimal.formatAt(0, 0)).not.toBeNull();
    const text = over(values, [rule({ kind: "textContains", range: "B1:B3", values: ["APPLE"] })]);
    expect(text.formatAt(0, 1)).not.toBeNull();
    expect(text.formatAt(1, 1)).toBeNull();
    expect(text.formatAt(2, 1)).not.toBeNull();
  });

  it("marks every value that occurs more than once", () => {
    const prepared = over({ A1: "x", A2: "y", A3: "x", A4: 3, A5: 3 }, [rule({ kind: "duplicate", range: "A1:A6" })]);
    expect([0, 1, 2, 3, 4, 5].map((row) => prepared.formatAt(row, 0) !== null)).toEqual([
      true,
      false,
      true,
      true,
      true,
      false,
    ]);
  });

  it("top and bottom N take the N highest or lowest numbers, ties included", () => {
    const data = { A1: 10, A2: 40, A3: 30, A4: 40, A5: 20, A6: "text" };
    const top = over(data, [rule({ kind: "top", range: "A1:A6", topN: 2 })]);
    expect([0, 1, 2, 3, 4, 5].map((row) => top.formatAt(row, 0) !== null)).toEqual([
      false,
      true,
      false,
      true,
      false,
      false,
    ]);
    const bottom = over(data, [rule({ kind: "bottom", range: "A1:A6", topN: 2 })]);
    expect([0, 1, 2, 3, 4, 5].map((row) => bottom.formatAt(row, 0) !== null)).toEqual([
      true,
      false,
      false,
      false,
      true,
      false,
    ]);
    // N larger than the data covers everything numeric; no number at all means no rule.
    expect(over(data, [rule({ kind: "top", range: "A1:A6", topN: 99 })]).formatAt(0, 0)).not.toBeNull();
    expect(over({ A1: "a" }, [rule({ kind: "top", range: "A1:A1" })]).empty).toBe(true);
  });

  it("carries the font colour, bold and italic, and only defaults the fill when nothing is styled", () => {
    const styled = over(values, [
      rule({ kind: "greater", range: "A1:A3", values: ["10"], color: "#cc0000", bold: true, italic: true }),
    ]);
    expect(styled.formatAt(1, 0)).toEqual({ color: "#cc0000", bold: true, italic: true });
    const filled = over(values, [
      rule({ kind: "greater", range: "A1:A3", values: ["10"], fill: "#00ff00", bold: true }),
    ]);
    expect(filled.formatAt(1, 0)).toEqual({ fill: "#00ff00", bold: true });
  });
});

describe("formula rules", () => {
  const cells = { A1: "10", A2: "200", A3: "30", B1: "x", B2: "y", B3: "x" };

  it("evaluates the formula relative to the first cell of the range", () => {
    const prepared = sheetOf(cells, [
      rule({ kind: "expression", range: "A1:B3", formula: "$A1>20", fill: "#ffcccc", bold: true }),
    ]);
    // Row 2 and row 3 are the rows where column A is above 20; the column anchor makes B follow.
    expect(prepared.formatAt(0, 0)).toBeNull();
    expect(prepared.formatAt(1, 0)).toEqual({ fill: "#ffcccc", bold: true });
    expect(prepared.formatAt(1, 1)).toEqual({ fill: "#ffcccc", bold: true });
    expect(prepared.formatAt(2, 1)).toEqual({ fill: "#ffcccc", bold: true });
    expect(prepared.formatAt(0, 1)).toBeNull();
  });

  it("moves relative references in both directions and keeps absolute ones", () => {
    const prepared = sheetOf({ A1: "1", B1: "2", A2: "2", B2: "2", C1: "2" }, [
      rule({ kind: "expression", range: "A1:B2", formula: "A1=$C$1", fill: "#00ff00" }),
    ]);
    expect(prepared.formatAt(0, 0)).toBeNull();
    expect(prepared.formatAt(0, 1)).not.toBeNull();
    expect(prepared.formatAt(1, 0)).not.toBeNull();
    expect(prepared.formatAt(1, 1)).not.toBeNull();
  });

  it("accepts a leading = and a function call, and reads other cells of the sheet", () => {
    const prepared = sheetOf(cells, [
      rule({ kind: "expression", range: "B1:B3", formula: "=COUNTIF($B$1:$B$3,B1)>1", fill: "#aaaaaa" }),
    ]);
    expect(prepared.formatAt(0, 1)).not.toBeNull();
    expect(prepared.formatAt(1, 1)).toBeNull();
    expect(prepared.formatAt(2, 1)).not.toBeNull();
  });

  it("is false for an error, text or zero result, and for an empty formula", () => {
    const base = { A1: "1" };
    for (const formula of ["1/0", '"yes"', "0", "NOSUCH(1)"]) {
      expect(sheetOf(base, [rule({ kind: "expression", range: "A1:A1", formula })]).formatAt(0, 0), formula).toBeNull();
    }
    expect(sheetOf(base, [rule({ kind: "expression", range: "A1:A1", formula: "  " })]).empty).toBe(true);
    expect(sheetOf(base, [rule({ kind: "expression", range: "A1:A1", formula: "2" })]).formatAt(0, 0)).not.toBeNull();
  });

  it("evaluates a cell once, however often the grid asks", () => {
    let calls = 0;
    const prepared = prepareConditional([rule({ kind: "expression", range: "A1:A9", formula: "A1>0", fill: "#fff" })], {
      values: new Map(),
      evaluate: () => {
        calls += 1;
        return true;
      },
    });
    prepared.formatAt(3, 0);
    prepared.formatAt(3, 0);
    prepared.formatAt(3, 0);
    expect(calls).toBe(1);
    // Cells never asked about are never evaluated.
    expect(calls).toBeLessThan(9);
  });

  it("hands the evaluator the shifted formula and the cell's own position", () => {
    const seen: Array<[string, number, number]> = [];
    const prepared = prepareConditional(
      [rule({ kind: "expression", range: "C3:D9", formula: "$A3+B$1", fill: "#fff" })],
      {
        values: new Map(),
        evaluate: (formula, row, col) => {
          seen.push([formula, row, col]);
          return true;
        },
      },
    );
    prepared.formatAt(4, 3);
    expect(seen).toEqual([["=$A5+C$1", 4, 3]]);
  });
});

describe("combining rules", () => {
  it("lets an earlier rule keep what it set while a later one adds the rest", () => {
    const prepared = over({ A1: 100 }, [
      rule({ kind: "greater", range: "A1:A1", values: ["10"], fill: "#ff0000" }),
      rule({ kind: "dataBar", range: "A1:A1", fill: "#0000ff" }),
      rule({ kind: "greater", range: "A1:A1", values: ["50"], fill: "#00ff00", italic: true }),
    ]);
    expect(prepared.formatAt(0, 0)).toEqual({
      fill: "#ff0000",
      bar: { fraction: 1, color: "#0000ff" },
      italic: true,
    });
  });

  it("stops at a matching rule marked stop-if-true, and goes on past one that does not match", () => {
    const stopping = over({ A1: 100, A2: 1 }, [
      rule({ kind: "greater", range: "A1:A2", values: ["10"], fill: "#ff0000", stopIfTrue: true }),
      rule({ kind: "greater", range: "A1:A2", values: ["0"], bold: true }),
    ]);
    expect(stopping.formatAt(0, 0)).toEqual({ fill: "#ff0000" });
    // A2 does not match the first rule, so the second one still applies.
    expect(stopping.formatAt(1, 0)).toEqual({ bold: true });
  });

  it("returns the same answer, from the cache, when asked twice", () => {
    const prepared = over({ A1: 100 }, [rule({ kind: "greater", range: "A1:A1", values: ["10"], fill: "#ff0000" })]);
    expect(prepared.formatAt(0, 0)).toBe(prepared.formatAt(0, 0));
    expect(prepared.formatAt(5, 5)).toBeNull();
    expect(prepared.formatAt(5, 5)).toBeNull();
  });
});
