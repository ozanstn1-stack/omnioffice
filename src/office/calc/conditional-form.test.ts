import { describe, expect, it } from "vitest";
import { defaultRuleForm, defaultRuleRange, isValidRange, ruleFromForm, type RuleForm } from "./conditional-form";

const form = (patch: Partial<RuleForm>): RuleForm => ({ ...defaultRuleForm("A1:A9"), ...patch });
const rule = (patch: Partial<RuleForm>) => {
  const result = ruleFromForm(form(patch));
  if ("error" in result) throw new Error(result.error);
  return result.rule;
};
const error = (patch: Partial<RuleForm>) => {
  const result = ruleFromForm(form(patch));
  return "error" in result ? result.error : null;
};

describe("range", () => {
  it("starts on the selected block, or on the used range for a single cell", () => {
    expect(defaultRuleRange({ start: { row: 1, col: 0 }, end: { row: 3, col: 2 } }, "A1:Z9")).toBe("A2:C4");
    expect(defaultRuleRange({ start: { row: 4, col: 4 }, end: { row: 4, col: 4 } }, "A1:Z9")).toBe("A1:Z9");
  });

  it("accepts cells, blocks and lists of areas", () => {
    expect(isValidRange("A1")).toBe(true);
    expect(isValidRange("$A$1:B9")).toBe(true);
    expect(isValidRange("A1:A3 C1:C3")).toBe(true);
    expect(isValidRange("")).toBe(false);
    expect(isValidRange("A1:")).toBe(false);
    expect(isValidRange("hello")).toBe(false);
    expect(isValidRange("A1:A3 nope")).toBe(false);
  });

  it("is upper-cased and refused when unreadable", () => {
    expect(rule({ range: " b2:c9 " }).range).toBe("B2:C9");
    expect(error({ range: "nope" })).toBe("calc.cfBadRange");
  });
});

describe("highlight rules", () => {
  it("greater and less keep one number; between keeps two", () => {
    expect(rule({ type: "greater", first: " 20 " })).toMatchObject({
      kind: "greater",
      values: ["20"],
      fill: "#FEE2E2",
    });
    expect(rule({ type: "between", first: "1", second: "5" }).values).toEqual(["1", "5"]);
    expect(error({ type: "greater", first: "abc" })).toBe("calc.cfBadNumber");
    expect(error({ type: "between", first: "1", second: "" })).toBe("calc.cfBadNumber");
  });

  it("equal and text contains need a value but not a number", () => {
    expect(rule({ type: "equal", first: "Done" }).values).toEqual(["Done"]);
    expect(rule({ type: "textContains", first: "err" }).values).toEqual(["err"]);
    expect(error({ type: "equal", first: " " })).toBe("calc.cfBadValue");
  });

  it("duplicate needs no value", () => {
    expect(rule({ type: "duplicate", first: "" })).toMatchObject({ kind: "duplicate", values: [], topN: null });
  });

  it("top and bottom store a whole count", () => {
    expect(rule({ type: "top", first: "3" })).toMatchObject({ kind: "top", topN: 3, values: [] });
    expect(rule({ type: "bottom", first: "2.9" })).toMatchObject({ kind: "bottom", topN: 2 });
    expect(error({ type: "top", first: "0" })).toBe("calc.cfBadNumber");
    expect(error({ type: "bottom", first: "x" })).toBe("calc.cfBadNumber");
  });

  it("a custom formula is stored without the leading =", () => {
    expect(rule({ type: "expression", formula: " = $B1>100 " })).toMatchObject({
      kind: "expression",
      formula: "$B1>100",
      values: [],
    });
    expect(error({ type: "expression", formula: " = " })).toBe("calc.cfBadFormula");
  });

  it("stores the font colour, bold and italic only when they are on", () => {
    const plain = rule({ type: "greater" });
    expect(plain.color).toBeNull();
    expect("bold" in plain).toBe(false);
    expect("italic" in plain).toBe(false);
    expect(rule({ type: "greater", useFontColor: true, fontColor: "#112233", bold: true, italic: true })).toMatchObject(
      { color: "#112233", bold: true, italic: true },
    );
  });
});

describe("colour scales", () => {
  it("makes three stops with their colours, or two without the middle one", () => {
    const three = rule({ type: "colorScale" });
    expect(three.thresholds).toEqual([
      { kind: "min", value: "", color: "#F8696B" },
      { kind: "percentile", value: "50", color: "#FFEB84" },
      { kind: "max", value: "", color: "#63BE7B" },
    ]);
    expect(three.fill).toBeNull();
    const two = rule({ type: "colorScale", colors: 2 });
    expect(two.thresholds?.map((stop) => stop.kind)).toEqual(["min", "max"]);
  });

  it("checks the values the stops use", () => {
    const stops = defaultRuleForm("A1").stops;
    const withMid = (mid: Partial<(typeof stops)[1]>): Partial<RuleForm> => ({
      type: "colorScale",
      stops: [stops[0], { ...stops[1], ...mid }, stops[2]],
    });
    expect(error(withMid({ kind: "num", value: "abc" }))).toBe("calc.cfBadNumber");
    expect(error(withMid({ kind: "percent", value: "120" }))).toBe("calc.cfBadPercent");
    expect(error(withMid({ kind: "percentile", value: "-1" }))).toBe("calc.cfBadPercent");
    expect(error(withMid({ kind: "formula", value: "" }))).toBe("calc.cfBadFormula");
    expect(rule(withMid({ kind: "formula", value: "=AVERAGE(A1:A9)" })).thresholds?.[1]).toMatchObject({
      kind: "formula",
      value: "=AVERAGE(A1:A9)",
    });
    // A middle stop that is not used is not checked when only two colours are asked for.
    expect(error({ ...withMid({ kind: "num", value: "abc" }), colors: 2 })).toBeNull();
  });
});

describe("data bars", () => {
  it("keeps the bar colour as the fill and shows the value by default", () => {
    const bar = rule({ type: "dataBar", barColor: "#112233" });
    expect(bar).toMatchObject({ kind: "dataBar", fill: "#112233", values: [] });
    expect("hideValue" in bar).toBe(false);
    expect("thresholds" in bar).toBe(false);
    expect(rule({ type: "dataBar", showValue: false }).hideValue).toBe(true);
  });
});

describe("icon sets", () => {
  it("stores the set, the bounds of the icons and the options that are on", () => {
    const icons = rule({ type: "iconSet", iconSet: "3Arrows" });
    expect(icons).toMatchObject({ kind: "iconSet", iconSet: "3Arrows" });
    expect(icons.thresholds).toEqual([
      { kind: "percent", value: "0" },
      { kind: "percent", value: "33" },
      { kind: "percent", value: "67" },
    ]);
    expect("reverseIcons" in icons).toBe(false);
    expect("hideValue" in icons).toBe(false);
    expect(rule({ type: "iconSet", reverseIcons: true, showValue: false })).toMatchObject({
      reverseIcons: true,
      hideValue: true,
    });
  });

  it("checks the bounds", () => {
    const icons = defaultRuleForm("A1").icons;
    expect(error({ type: "iconSet", icons: [{ ...icons[0], kind: "num", value: "" }, icons[1]] })).toBe(
      "calc.cfBadNumber",
    );
    expect(error({ type: "iconSet", icons: [icons[0], { ...icons[1], value: "101" }] })).toBe("calc.cfBadPercent");
    expect(
      rule({
        type: "iconSet",
        icons: [
          { ...icons[0], kind: "num", value: "10" },
          { ...icons[1], kind: "num", value: "20" },
        ],
      }).thresholds,
    ).toEqual([
      { kind: "percent", value: "0" },
      { kind: "num", value: "10" },
      { kind: "num", value: "20" },
    ]);
  });
});
