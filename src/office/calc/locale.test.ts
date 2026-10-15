import { afterEach, describe, expect, it } from "vitest";
import { newWorkbook, emptyCell, type Workbook } from "../../lib/office-types";
import { applyCellEdit, cellEditText, parseInputValue } from "./cells";
import {
  getCalcLocale,
  lowerLocale,
  parseLocalizedDateParts,
  parseLocalizedNumber,
  properLocale,
  resolveCalcLocale,
  setCalcLocale,
  upperLocale,
} from "./locale";
import { formatNumber } from "./numberFormat";

function book(): Workbook {
  return newWorkbook("Locale");
}

afterEach(() => {
  setCalcLocale("en");
});

describe("locale selection", () => {
  it("resolves auto against the interface language and honours an explicit choice", () => {
    expect(resolveCalcLocale("auto", "tr")).toBe("tr");
    expect(resolveCalcLocale("auto", "en")).toBe("en");
    expect(resolveCalcLocale("en", "tr")).toBe("en");
    expect(resolveCalcLocale("tr", "en")).toBe("tr");
    expect(resolveCalcLocale(undefined, "tr")).toBe("tr");
  });

  it("keeps the active locale between calls", () => {
    setCalcLocale("tr");
    expect(getCalcLocale()).toBe("tr");
    setCalcLocale("en");
    expect(getCalcLocale()).toBe("en");
  });
});

describe("Turkish case rules", () => {
  const cases: [string, string, string][] = [
    // input, tr upper, tr lower
    ["işık", "İŞIK", "işık"],
    ["IŞIK", "IŞIK", "ışık"],
    ["İstanbul", "İSTANBUL", "istanbul"],
    ["güneş ğ ş ç ö ü", "GÜNEŞ Ğ Ş Ç Ö Ü", "güneş ğ ş ç ö ü"],
  ];
  for (const [input, upper, lower] of cases) {
    it(`maps "${input}" like tr-TR`, () => {
      expect(upperLocale(input, "tr")).toBe(upper);
      expect(lowerLocale(input, "tr")).toBe(lower);
    });
  }

  it("keeps the default rules for English", () => {
    expect(upperLocale("işık", "en")).toBe("IŞIK");
    expect(lowerLocale("IŞIK", "en")).toBe("işik");
  });

  it("title-cases words per locale", () => {
    expect(properLocale("şişli güneş iğdır ı", "tr")).toBe("Şişli Güneş İğdır I");
    expect(properLocale("ada lovelace", "en")).toBe("Ada Lovelace");
  });
});

describe("localized number input", () => {
  const tr: [string, number][] = [
    ["1.234,56", 1234.56],
    ["1.234", 1234],
    ["1,5", 1.5],
    ["₺1.250", 1250],
    ["1.250 TL", 1250],
    ["%18", 0.18],
    ["18%", 0.18],
    ["-25,50", -25.5],
    ["(1.234,56)", -1234.56],
    ["1 250,00", 1250],
  ];
  for (const [input, expected] of tr) {
    it(`reads "${input}" as ${expected} in tr`, () => {
      expect(parseLocalizedNumber(input, "tr")).toBe(expected);
    });
  }

  const en: [string, number][] = [
    ["1,234.56", 1234.56],
    ["1,234", 1234],
    ["1.5", 1.5],
    ["$1,250", 1250],
    ["1.234", 1.234],
  ];
  for (const [input, expected] of en) {
    it(`reads "${input}" as ${expected} in en`, () => {
      expect(parseLocalizedNumber(input, "en")).toBe(expected);
    });
  }

  it("rejects text that is not a number", () => {
    expect(parseLocalizedNumber("12abc", "tr")).toBeNull();
    expect(parseLocalizedNumber("hello", "en")).toBeNull();
    expect(parseLocalizedNumber("", "tr")).toBeNull();
  });

  it("drives typed input through the active locale", () => {
    setCalcLocale("tr");
    expect(parseInputValue("1.234")).toEqual({ kind: "number", value: 1234 });
    expect(parseInputValue("1.234,56")).toEqual({ kind: "number", value: 1234.56 });
    setCalcLocale("en");
    expect(parseInputValue("1.234")).toEqual({ kind: "number", value: 1.234 });
    expect(parseInputValue("1,234")).toEqual({ kind: "number", value: 1234 });
  });

  it("round-trips a number through the editor text in both locales", () => {
    // Editing a cell used to show the canonical "1.234", which Turkish reads as
    // 1234: the value silently grew by a thousand on every edit.
    const cell = { ...emptyCell(), value: { kind: "number" as const, value: 1.234 } };
    setCalcLocale("tr");
    expect(cellEditText(cell)).toBe("1,234");
    expect(parseInputValue(cellEditText(cell))).toEqual({ kind: "number", value: 1.234 });
    setCalcLocale("en");
    expect(cellEditText(cell)).toBe("1.234");
    expect(parseInputValue(cellEditText(cell))).toEqual({ kind: "number", value: 1.234 });
  });
});

describe("localized dates", () => {
  it("reads the Turkish shape and the ISO shape", () => {
    expect(parseLocalizedDateParts("11.10.2026", "tr")).toEqual({
      year: 2026,
      month: 10,
      day: 11,
      numberFormat: "DD.MM.YYYY",
    });
    expect(parseLocalizedDateParts("2026-10-11", "tr")?.day).toBe(11);
    expect(parseLocalizedDateParts("2026-10-11", "en")?.month).toBe(10);
  });

  it("reads the English shape only for English", () => {
    expect(parseLocalizedDateParts("10/11/2026", "en")).toEqual({
      year: 2026,
      month: 10,
      day: 11,
      numberFormat: "MM/DD/YYYY",
    });
    expect(parseLocalizedDateParts("10/11/2026", "tr")).toBeNull();
  });

  it("rejects impossible dates", () => {
    expect(parseLocalizedDateParts("31.02.2026", "tr")).toBeNull();
    expect(parseLocalizedDateParts("2026-13-01", "tr")).toBeNull();
    expect(parseLocalizedDateParts("hello", "tr")).toBeNull();
  });

  it("stores a typed date as a serial with its format", () => {
    setCalcLocale("tr");
    const workbook = applyCellEdit(book(), 0, 0, 0, "11.10.2026");
    const cell = workbook.sheets[0].cells.A1;
    expect(cell.value.kind).toBe("number");
    expect(cell.style.numberFormat).toBe("DD.MM.YYYY");
    expect(formatNumber((cell.value as { kind: "number"; value: number }).value, "DD.MM.YYYY")).toBe("11.10.2026");
  });
});

describe("localized number formatting", () => {
  it("writes the separators of the active locale", () => {
    setCalcLocale("en");
    expect(formatNumber(1234.56, "#,##0.00")).toBe("1,234.56");
    setCalcLocale("tr");
    expect(formatNumber(1234.56, "#,##0.00")).toBe("1.234,56");
    expect(formatNumber(1234.56, "#,##0")).toBe("1.235");
    expect(formatNumber(1250, "₺#,##0.00")).toBe("₺1.250,00");
  });
});
