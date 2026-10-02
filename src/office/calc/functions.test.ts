/**
 * Tests for the extended formula library.
 *
 * Every function added to the engine gets a case here, including the error
 * paths: a function that quietly returns a wrong number is worse than one that
 * reports `#VALUE!`, so the failure modes are pinned down explicitly.
 */
import { describe, expect, it } from "vitest";
import { emptyCell, newSheet, newWorkbook, type Workbook } from "../../lib/office-types";
import { applyCellEdit, computeWorkbookValues } from "./cells";
import { builtinFunctionCount } from "./functions";
import {
  ERR,
  addressesInRange,
  evaluateFormula,
  evaluateToMatrix,
  formatNumber,
  functionNames,
  isError,
  parseAddress,
  parseRange,
  registerFunction,
  suggestFunctions,
  type FormulaContext,
  type Scalar,
} from "./formula";

/** Context backed by a flat address -> value map. */
function context(values: Record<string, Scalar> = {}): FormulaContext {
  return {
    getValue: (_sheet, address) => (address in values ? values[address] : ""),
    sheetNames: ["Sheet1", "Data"],
    currentSheet: "Sheet1",
  };
}

/** Evaluates and returns the value, failing loudly on an error result. */
function n(formula: string, values: Record<string, Scalar> = {}): number {
  const result = evaluateFormula(formula, context(values));
  if (isError(result)) throw new Error(`${formula} returned ${result.code}`);
  if (typeof result !== "number") throw new Error(`${formula} returned ${String(result)}`);
  return result;
}

/** Evaluates and returns the value as text. */
function s(formula: string, values: Record<string, Scalar> = {}): string {
  const result = evaluateFormula(formula, context(values));
  if (isError(result)) return result.code;
  return String(result);
}

/** The error code a formula produced, or "" when it succeeded. */
function code(formula: string, values: Record<string, Scalar> = {}): string {
  const result = evaluateFormula(formula, context(values));
  return isError(result) ? result.code : "";
}

/** Error code stored in a computed map, or "" when the cell is clean. */
function codeOf(values: Map<string, Scalar>, key: string): string {
  const value = values.get(key);
  return isError(value) ? value.code : "";
}

describe("library wiring", () => {
  it("registers a large function set", () => {
    expect(builtinFunctionCount()).toBeGreaterThan(160);
  });

  it("registers every function the suite documents", () => {
    const required = [
      "FILTER",
      "SORT",
      "SORTBY",
      "UNIQUE",
      "SEQUENCE",
      "LET",
      "SUMIFS",
      "COUNTIFS",
      "AVERAGEIFS",
      "MAXIFS",
      "MINIFS",
      "TEXTBEFORE",
      "TEXTAFTER",
      "TEXTSPLIT",
      "FIND",
      "SEARCH",
      "REPLACE",
      "EXACT",
      "EDATE",
      "EOMONTH",
      "NETWORKDAYS",
      "WORKDAY",
      "DAYS",
      "DATEDIF",
      "WEEKDAY",
      "WEEKNUM",
      "ISOWEEKNUM",
      "PMT",
      "PV",
      "FV",
      "NPER",
      "RATE",
      "IRR",
      "XIRR",
      "NPV",
      "XNPV",
      "MIRR",
      "SLN",
      "SYD",
      "DB",
      "DDB",
      "EFFECT",
      "NOMINAL",
      "CUMIPMT",
      "CUMPRINC",
      "XLOOKUP",
      "XMATCH",
      "LOOKUP",
      "CHOOSE",
      "ROWS",
      "COLUMNS",
      "TRANSPOSE",
      "SWITCH",
      "IFNA",
      "ISERR",
      "ISNA",
    ];
    const names = new Set(functionNames());
    for (const name of required) expect(names.has(name), `${name} is missing`).toBe(true);
  });

  it("suggests functions by prefix", () => {
    expect(suggestFunctions("textb")).toContain("TEXTBEFORE");
    expect(suggestFunctions("SU", 5).every((name) => name.startsWith("SU"))).toBe(true);
  });

  it("lets callers register their own function", () => {
    registerFunction(
      "DOUBLEX",
      (args) => {
        const value = args[0]?.[0]?.[0];
        return typeof value === "number" ? value * 2 : ERR.value();
      },
      1,
      1,
    );
    expect(n("=DOUBLEX(21)")).toBe(42);
  });
});

describe("inline array literals", () => {
  it("parses a single row", () => {
    expect(evaluateToMatrix("={1,2,3}", context())).toEqual([[1, 2, 3]]);
  });

  it("uses a semicolon for a new row", () => {
    expect(evaluateToMatrix("={1,2;3,4}", context())).toEqual([
      [1, 2],
      [3, 4],
    ]);
  });

  it("parses strings and booleans inside a literal", () => {
    expect(evaluateToMatrix('={"a",TRUE}', context())).toEqual([["a", true]]);
  });

  it("pads a ragged literal to a rectangle", () => {
    expect(evaluateToMatrix("={1,2,3;4}", context())).toEqual([
      [1, 2, 3],
      [4, "", ""],
    ]);
  });

  it("an empty literal is a #VALUE!", () => {
    expect(code("={}")).toBe("#VALUE!");
  });

  it("separator-only literals are #VALUE!, not ambiguous values", () => {
    // `{,}` / `{;}` used to parse into a 1x1 error matrix; they must report a
    // plain #VALUE! so the user sees a malformed literal.
    expect(code("={,}")).toBe("#VALUE!");
    expect(code("={;}")).toBe("#VALUE!");
    expect(code("={,;}")).toBe("#VALUE!");
  });

  it("feeds straight into an aggregate", () => {
    expect(n("=SUM({1,2;3,4})")).toBe(10);
  });

  it("an omitted argument is an empty string, not a syntax error", () => {
    expect(evaluateToMatrix("=UNIQUE({1;2;1},,TRUE)", context())).toEqual([[2]]);
  });
});

describe("dynamic arrays", () => {
  it("SEQUENCE builds a column of numbers", () => {
    expect(evaluateToMatrix("=SEQUENCE(3)", context())).toEqual([[1], [2], [3]]);
  });

  it("SEQUENCE builds a grid with a start and step", () => {
    expect(evaluateToMatrix("=SEQUENCE(2,3,10,5)", context())).toEqual([
      [10, 15, 20],
      [25, 30, 35],
    ]);
  });

  it("SEQUENCE rejects a non-positive size", () => {
    expect(code("=SEQUENCE(0)")).toBe("#VALUE!");
  });

  it("UNIQUE removes duplicate rows", () => {
    expect(evaluateToMatrix('=UNIQUE({1,"a";2,"b";1,"a"})', context())).toEqual([
      [1, "a"],
      [2, "b"],
    ]);
  });

  it("UNIQUE with exactly_once drops repeated values", () => {
    expect(evaluateToMatrix("=UNIQUE({1;2;1;3},,TRUE)", context())).toEqual([[2], [3]]);
  });

  it("UNIQUE can compare by column", () => {
    // Columns 1 and 2 are identical, so the second is dropped.
    expect(evaluateToMatrix('=UNIQUE({1,1;"a","a"},TRUE)', context())).toEqual([[1], ["a"]]);
  });

  it("SORT orders ascending by default", () => {
    expect(evaluateToMatrix("=SORT({3;1;2})", context())).toEqual([[1], [2], [3]]);
  });

  it("SORT honours a descending order", () => {
    expect(evaluateToMatrix("=SORT({1;3;2},1,-1)", context())).toEqual([[3], [2], [1]]);
  });

  it("SORT puts blanks last in both directions", () => {
    expect(evaluateToMatrix('=SORT({1;"";2},1,-1)', context())).toEqual([[2], [1], [""]]);
  });

  it("SORT sorts by a chosen column", () => {
    expect(evaluateToMatrix('=SORT({"b",1;"a",2})', context())).toEqual([
      ["a", 2],
      ["b", 1],
    ]);
  });

  it("SORTBY orders by a separate key", () => {
    expect(evaluateToMatrix('=SORTBY({"a";"b";"c"},{3;1;2})', context())).toEqual([["b"], ["c"], ["a"]]);
  });

  it("SORTBY supports a descending key", () => {
    expect(evaluateToMatrix('=SORTBY({"a";"b";"c"},{1;2;3},-1)', context())).toEqual([["c"], ["b"], ["a"]]);
  });

  it("SORTBY rejects a key of the wrong length", () => {
    expect(code("=SORTBY({1;2;3},{1;2})")).toBe("#VALUE!");
  });

  it("FILTER keeps the rows a boolean array selects", () => {
    expect(evaluateToMatrix("=FILTER({1;2;3;4},{TRUE;FALSE;TRUE;FALSE})", context())).toEqual([[1], [3]]);
  });

  it("FILTER with no matches returns the if_empty argument", () => {
    expect(s('=FILTER({1;2},{FALSE;FALSE},"none")')).toBe("none");
    expect(code("=FILTER({1;2},{FALSE;FALSE})")).toBe("#N/A");
  });

  it("FILTER rejects an include that is not a boolean array", () => {
    expect(code('=FILTER({"a",1;"b",2},">1")')).toBe("#VALUE!");
  });

  it("TRANSPOSE flips a matrix", () => {
    expect(evaluateToMatrix("=TRANSPOSE({1,2;3,4})", context())).toEqual([
      [1, 3],
      [2, 4],
    ]);
  });

  it("a spilled array collapses to its first cell in a single cell", () => {
    expect(n("=SEQUENCE(3,3,7)")).toBe(7);
  });
});

describe("LET", () => {
  it("binds a name and uses it", () => {
    expect(n("=LET(x,5,x*2)")).toBe(10);
  });

  it("binds several names in order", () => {
    expect(n("=LET(a,2,b,3,a*b)")).toBe(6);
  });

  it("a later binding can use an earlier one", () => {
    expect(n("=LET(a,2,b,a*5,b+1)")).toBe(11);
  });

  it("works with a range", () => {
    expect(n("=LET(r,A1:A3,SUM(r))", { A1: 1, A2: 2, A3: 3 })).toBe(6);
  });

  it("rejects an even argument count", () => {
    expect(code("=LET(x,1)")).toBe("#VALUE!");
    expect(code("=LET(x,1,x,2)")).toBe("#VALUE!");
  });

  it("rejects a non-name first argument", () => {
    expect(code("=LET(1,2,1)")).toBe("#NAME?");
  });

  it("does not leak the binding into another formula", () => {
    expect(code("=x")).toBe("#NAME?");
  });

  it("a self-referential binding terminates as an unbound name, not a hang", () => {
    // Regression guard: `x` is not yet bound while its own value is evaluated,
    // so it must resolve to #NAME? (Excel semantics) instead of recursing.
    expect(code("=LET(x,x+1,x)")).toBe("#NAME?");
    expect(code("=LET(x,LET(x,x,x),x)")).toBe("#NAME?");
  });

  it("a defined name that references itself is a circular reference", () => {
    const ctx = context();
    ctx.names = { SELF: { definition: "=SELF+1" } };
    const result = evaluateFormula("=SELF", ctx);
    expect(isError(result)).toBe(true);
    expect((result as { code: string }).code).toBe("#REF!");
  });
});

describe("conditional aggregation", () => {
  const data = { A1: 10, A2: 20, A3: 30, B1: "x", B2: "y", B3: "x" };

  it("SUMIFS adds matching values", () => {
    expect(n('=SUMIFS(A1:A3,B1:B3,"x")', data)).toBe(40);
  });

  it("SUMIFS accepts a comparison criterion", () => {
    expect(n('=SUMIFS(A1:A3,A1:A3,">15")', data)).toBe(50);
  });

  it("COUNTIFS counts matching rows", () => {
    expect(n('=COUNTIFS(B1:B3,"x")', data)).toBe(2);
  });

  it("COUNTIFS combines criteria", () => {
    expect(n('=COUNTIFS(B1:B3,"x",A1:A3,">15")', data)).toBe(1);
  });

  it("AVERAGEIFS averages matching values", () => {
    expect(n('=AVERAGEIFS(A1:A3,B1:B3,"x")', data)).toBe(20);
  });

  it("AVERAGEIFS with no match divides by zero", () => {
    expect(code('=AVERAGEIFS(A1:A3,B1:B3,"zzz")', data)).toBe("#DIV/0!");
  });

  it("MAXIFS and MINIFS pick the extremes", () => {
    expect(n('=MAXIFS(A1:A3,B1:B3,"x")', data)).toBe(30);
    expect(n('=MINIFS(A1:A3,B1:B3,"x")', data)).toBe(10);
  });

  it("COUNTIF and SUMIF keep working", () => {
    expect(n('=COUNTIF(A1:A3,">15")', data)).toBe(2);
    expect(n('=SUMIF(A1:A3,">15")', data)).toBe(50);
    expect(n('=AVERAGEIF(A1:A3,">15")', data)).toBe(25);
  });

  it("COUNT skips an error inside the range but SUM propagates it", () => {
    const withError = { A1: 10, A2: ERR.div(), A3: 30 };
    expect(n("=COUNT(A1:A3)", withError)).toBe(2);
    expect(code("=SUM(A1:A3)", withError)).toBe("#DIV/0!");
  });

  it("COUNTIF does not fail on an error in the range", () => {
    expect(n('=COUNTIF(A1:A3,">5")', { A1: 10, A2: ERR.div(), A3: 30 })).toBe(2);
  });

  it("SUMPRODUCT multiplies element-wise", () => {
    expect(n("=SUMPRODUCT(A1:A3,A1:A3)", { A1: 2, A2: 3, A3: 4 })).toBe(29);
  });

  it("SUMPRODUCT counts a boolean mask via the -- idiom", () => {
    // Regression: the unary operator collapsed an array operand to its first
    // cell, so `--(A1:A3>1)` was always 0.
    const values = { A1: 1, A2: 5, A3: 9 };
    expect(n("=SUMPRODUCT(--(A1:A3>1))", values)).toBe(2);
    expect(n("=SUMPRODUCT((A1:A3>1)*1)", values)).toBe(2);
  });

  it("COUNTIF and SUMIF support * and ? wildcards", () => {
    const text = { A1: "apple", A2: "apricot", A3: "banana" };
    expect(n('=COUNTIF(A1:A3,"ap*")', text)).toBe(2);
    expect(n('=COUNTIF(A1:A3,"?????")', text)).toBe(1);
    expect(n('=COUNTIF(A1:A3,"<>ap*")', text)).toBe(1);
    expect(n('=SUMIF(A1:A3,"banana",A1:A3)', text)).toBe(0);
  });

  it("FILTER treats a blank mask cell as false and broadcasts a single boolean", () => {
    const values = { A1: "x", A2: "y", A3: "z", B1: true, B2: "", B3: true };
    // A blank mask cell must be FALSE (the old code treated "" as TRUE).
    expect(evaluateToMatrix("=FILTER(A1:A3,B1:B3)", context(values))).toEqual([["x"], ["z"]]);
    // A single boolean broadcasts over every data row.
    expect(evaluateToMatrix("=FILTER(A1:A3,B1)", context({ A1: "x", A2: "y", A3: "z", B1: true }))).toEqual([
      ["x"],
      ["y"],
      ["z"],
    ]);
    // A single FALSE yields the if_empty fallback.
    expect(s('=FILTER(A1:A3,B1,"none")', { A1: "x", A2: "y", A3: "z", B1: false })).toBe("none");
  });
});

describe("text functions", () => {
  it("TEXTBEFORE takes the part in front of a delimiter", () => {
    expect(s('=TEXTBEFORE("a-b-c","-")')).toBe("a");
  });

  it("TEXTBEFORE honours the instance number", () => {
    expect(s('=TEXTBEFORE("a-b-c","-",2)')).toBe("a-b");
  });

  it("TEXTAFTER takes the part behind a delimiter", () => {
    expect(s('=TEXTAFTER("report.pdf",".")')).toBe("pdf");
  });

  it("a negative instance counts from the end", () => {
    expect(s('=TEXTAFTER("a.b.c",".",-1)')).toBe("c");
    expect(s('=TEXTBEFORE("a.b.c",".",-1)')).toBe("a.b");
  });

  it("a missing delimiter is #N/A and instance 0 is #VALUE!", () => {
    expect(code('=TEXTAFTER("abc","-")')).toBe("#N/A");
    expect(code('=TEXTAFTER("a-b","-",0)')).toBe("#VALUE!");
  });

  it("TEXTSPLIT splits into columns and pads short rows", () => {
    expect(evaluateToMatrix('=TEXTSPLIT("a,b,c",",")', context())).toEqual([["a", "b", "c"]]);
  });

  it("TEXTSPLIT handles a row delimiter too", () => {
    expect(evaluateToMatrix('=TEXTSPLIT("a,1; b,2",",",";")', context())).toEqual([
      ["a", "1"],
      [" b", "2"],
    ]);
  });

  it("NUMBERVALUE handles locale separators", () => {
    // Regression: the decimal separator was stripped as if it were a group
    // separator, so `NUMBERVALUE("3.5",".")` returned 0.
    expect(n('=NUMBERVALUE("1,234.5")')).toBe(1234.5);
    expect(n('=NUMBERVALUE("3.5",".")')).toBe(3.5);
    expect(n('=NUMBERVALUE("1.234,5",",",".")')).toBe(1234.5);
  });

  it("FIND is case sensitive and 1-based", () => {
    expect(n('=FIND("B","aBc")')).toBe(2);
    expect(code('=FIND("b","aBc")')).toBe("#VALUE!");
    expect(code('=FIND("z","abc")')).toBe("#VALUE!");
  });

  it("SEARCH ignores case", () => {
    expect(n('=SEARCH("b","aBc")')).toBe(2);
  });

  it("REPLACE substitutes by position and length", () => {
    expect(s('=REPLACE("abcdef",2,3,"XY")')).toBe("aXYef");
  });

  it("EXACT compares case sensitively", () => {
    expect(evaluateFormula('=EXACT("a","A")', context())).toBe(false);
    expect(evaluateFormula('=EXACT("a","a")', context())).toBe(true);
  });

  it("CHAR and CODE round-trip", () => {
    expect(s("=CHAR(65)")).toBe("A");
    expect(n('=CODE("A")')).toBe(65);
  });

  it("VALUE parses text, including a trailing percent", () => {
    expect(n('=VALUE("1234.56")')).toBe(1234.56);
    expect(n('=VALUE("12%")')).toBe(0.12);
    expect(code('=VALUE("nope")')).toBe("#VALUE!");
  });

  it("TEXTJOIN joins and can keep blanks", () => {
    expect(s('=TEXTJOIN(", ",TRUE,"a","","b")')).toBe("a, b");
    expect(s('=TEXTJOIN(",",FALSE,"a","","b")')).toBe("a,,b");
  });

  it("SUBSTITUTE can replace one instance only", () => {
    expect(s('=SUBSTITUTE("a-a-a","-","+",2)')).toBe("a-a+a");
  });

  it("LEFT, RIGHT and MID clamp sensibly", () => {
    expect(s('=LEFT("abcdef",3)')).toBe("abc");
    expect(s('=RIGHT("abcdef",0)')).toBe("");
    expect(code('=MID("abcdef",0,2)')).toBe("#VALUE!");
    expect(s('=MID("abcdef",2,3)')).toBe("bcd");
  });

  it("REPT is bounded so a huge count cannot exhaust memory", () => {
    expect(s('=REPT("ab",3)')).toBe("ababab");
    expect(s('=REPT("x",1E9)').length).toBeLessThanOrEqual(32_767);
  });
});

describe("date and time functions", () => {
  // 45292 = 2024-01-01 (Monday); 45322 = 2024-01-31; 45296 = 2024-01-05.
  it("EDATE shifts by whole months", () => {
    expect(n("=YEAR(EDATE(45292,1))")).toBe(2024);
    expect(n("=MONTH(EDATE(45292,1))")).toBe(2);
  });

  it("EDATE clamps the day to the end of a shorter month", () => {
    expect(n("=DAY(EDATE(45322,1))")).toBe(29);
  });

  it("EDATE with a negative offset goes back", () => {
    expect(n("=MONTH(EDATE(45292,-1))")).toBe(12);
    expect(n("=YEAR(EDATE(45292,-1))")).toBe(2023);
  });

  it("EOMONTH returns the last day of the month", () => {
    expect(n("=EOMONTH(45322,0)")).toBe(45322);
    expect(n("=DAY(EOMONTH(45322,0))")).toBe(31);
    expect(n("=DAY(EOMONTH(45322,1))")).toBe(29);
  });

  it("NETWORKDAYS counts weekdays", () => {
    expect(n("=NETWORKDAYS(45292,45296)")).toBe(5);
  });

  it("NETWORKDAYS excludes weekends in the middle", () => {
    expect(n("=NETWORKDAYS(45292,45299)")).toBe(6);
  });

  it("NETWORKDAYS subtracts listed holidays", () => {
    expect(n("=NETWORKDAYS(45292,45296,45293)")).toBe(4);
  });

  it("NETWORKDAYS works backwards", () => {
    expect(n("=NETWORKDAYS(45296,45292)")).toBe(-5);
  });

  it("WORKDAY steps over the weekend", () => {
    // Friday + 1 working day is the following Monday.
    expect(n("=DAY(WORKDAY(45296,1))")).toBe(8);
    expect(n("=WEEKDAY(WORKDAY(45296,1))")).toBe(1);
  });

  it("WORKDAY steps backwards", () => {
    expect(n("=DAY(WORKDAY(45292,-1))")).toBe(29);
  });

  it("DAYS returns the signed distance", () => {
    expect(n("=DAYS(45299,45292)")).toBe(7);
    expect(n("=DAYS(45292,45299)")).toBe(-7);
  });

  it("DATEDIF measures Y, M and D", () => {
    expect(n('=DATEDIF(45292,45658,"Y")')).toBe(1);
    expect(n('=DATEDIF(45292,45658,"M")')).toBe(12);
    expect(n('=DATEDIF(45292,45299,"D")')).toBe(7);
  });

  it("DATEDIF rejects an unknown unit", () => {
    expect(code('=DATEDIF(45292,45299,"Q")')).toBe("#NUM!");
  });

  it("WEEKDAY supports the three numbering schemes", () => {
    expect(n("=WEEKDAY(45292,1)")).toBe(1);
    expect(n("=WEEKDAY(45292,2)")).toBe(7);
    expect(n("=WEEKDAY(45292,3)")).toBe(6);
  });

  it("ISOWEEKNUM follows the ISO rule", () => {
    expect(n("=ISOWEEKNUM(45292)")).toBe(1);
  });

  it("WEEKNUM counts the week containing January 1 as week 1", () => {
    // 45292 is Monday 2024-01-01, so every scheme starts that week at 1.
    expect(n("=WEEKNUM(45292)")).toBe(1);
    expect(n("=WEEKNUM(45292,2)")).toBe(1);
    // Sunday 2024-01-07 closes week 1 under a Monday start, but opens week 2
    // under a Sunday start.
    expect(n("=WEEKNUM(45298,2)")).toBe(1);
    expect(n("=WEEKNUM(45298,1)")).toBe(2);
    // Monday 2024-01-08 starts week 2 under both.
    expect(n("=WEEKNUM(45299,2)")).toBe(2);
    expect(n("=WEEKNUM(45299,1)")).toBe(2);
  });

  it("WEEKNUM matches Excel on a Friday 1 January", () => {
    // 44197 is Friday 2021-01-01; 44199 is the Sunday that starts week 2.
    expect(n("=WEEKNUM(44197,1)")).toBe(1);
    expect(n("=WEEKNUM(44197,2)")).toBe(1);
    expect(n("=WEEKNUM(44199,1)")).toBe(2);
  });

  it("WEEKNUM supports the alternate start days and the ISO scheme", () => {
    // 45322 is 2024-01-31, a Wednesday; type 13 starts the week on Wednesday,
    // so Jan 31 begins the sixth week (Jan 3, 10, 17, 24 are the earlier ones).
    expect(n("=WEEKNUM(45322,13)")).toBe(6);
    expect(n("=WEEKNUM(45292,21)")).toBe(1);
    expect(n("=ISOWEEKNUM(45292)")).toBe(n("=WEEKNUM(45292,21)"));
  });

  it("WEEKNUM rejects an unknown scheme instead of guessing", () => {
    expect(code("=WEEKNUM(45292,99)")).toBe("#NUM!");
  });

  it("DATEVALUE parses an ISO string", () => {
    expect(n('=DATEVALUE("2024-01-01")')).toBe(45292);
  });

  it("an unparseable date is an error, not a wrong number", () => {
    expect(code('=DATEVALUE("not a date")')).toBe("#VALUE!");
  });

  it("TIME builds a day fraction and wraps past midnight", () => {
    expect(n("=TIME(12,0,0)")).toBe(0.5);
    expect(n("=TIME(6,30,0)")).toBeCloseTo(6.5 / 24, 10);
    // 25:00 wraps to 01:00 of the next day, as in Excel.
    expect(n("=TIME(25,0,0)")).toBeCloseTo(1 / 24, 10);
  });

  it("TIME rejects negative components", () => {
    expect(code("=TIME(0,-1,0)")).toBe("#NUM!");
    expect(code("=TIME(-1,0,0)")).toBe("#NUM!");
  });
});

describe("financial functions", () => {
  it("PMT matches the standard annuity result", () => {
    // 100000 at 10%/year for 10 years, paid monthly.
    expect(n("=PMT(0.1/12,120,100000)")).toBeCloseTo(-1321.5074, 3);
  });

  it("PMT with a zero rate divides the principal", () => {
    expect(n("=PMT(0,10,-1000)")).toBe(100);
  });

  it("FV compounds the payments", () => {
    expect(n("=FV(0.05,10,-100,0)")).toBeCloseTo(1257.789, 2);
  });

  it("PV discounts the payments", () => {
    expect(n("=PV(0.05,10,-1000,0)")).toBeCloseTo(7721.735, 2);
  });

  it("PMT, PV and FV are mutually consistent", () => {
    const payment = n("=PMT(0.06,60,-10000)");
    expect(n(`=FV(0.06,60,${payment},-10000)`)).toBeCloseTo(0, 4);
  });

  it("NPER solves for the number of periods", () => {
    expect(n("=NPER(0.06,-1000,10000)")).toBeCloseTo(15.725, 2);
  });

  it("RATE recovers the rate from a known annuity", () => {
    expect(n("=RATE(60,-212.47,10000)")).toBeCloseTo(0.008333, 5);
  });

  it("RATE reports a failure instead of a wrong rate", () => {
    expect(code("=RATE(0,100,100)")).toBe("#NUM!");
  });

  it("NPV discounts the first value by one period", () => {
    expect(n("=NPV(0.1,-1000,500,500,500)")).toBeCloseTo(221.296, 2);
  });

  it("IRR finds the internal rate of return", () => {
    expect(n("=IRR({-1000,300,400,500})")).toBeCloseTo(0.08896, 4);
  });

  it("IRR needs a sign change", () => {
    expect(code("=IRR({100,200,300})")).toBe("#NUM!");
  });

  it("XNPV discounts by the actual day count", () => {
    const daily = 0.1;
    // 45658 - 45292 = 366 days, and XNPV divides by 365.
    const expected = -1000 + 1100 / (1 + daily) ** (366 / 365);
    expect(n(`=XNPV(${daily},{-1000,1100},{45292,45658})`)).toBeCloseTo(expected, 4);
  });

  it("XIRR matches XNPV at the same rate", () => {
    const rate = n("=XIRR({-1000,1100},{45292,45658})");
    expect(n(`=XNPV(${rate},{-1000,1100},{45292,45658})`)).toBeCloseTo(0, 3);
  });

  it("MIRR accounts for both financing and reinvestment", () => {
    expect(n("=MIRR({-120000,39000,30000,21000,37000,46000},0.1,0.12)")).toBeCloseTo(0.1261, 3);
  });

  it("CUMIPMT sums the interest of a period range", () => {
    // Microsoft's reference case: 125000 at 9%/year over 30 years.
    expect(n("=CUMIPMT(0.09/12,360,125000,13,24,0)")).toBeCloseTo(-11135.23, 2);
    expect(n("=CUMIPMT(0.09/12,360,125000,1,1,0)")).toBeCloseTo(-937.5, 2);
  });

  it("CUMPRINC sums the principal of a period range", () => {
    expect(n("=CUMPRINC(0.09/12,360,125000,13,24,0)")).toBeCloseTo(-934.1071, 3);
    expect(n("=CUMPRINC(0.09/12,360,125000,1,1,0)")).toBeCloseTo(-68.27827, 4);
  });

  it("CUMIPMT and CUMPRINC add up to the payments over the same range", () => {
    const interest = n("=CUMIPMT(0.05/12,120,50000,1,120,0)");
    const principal = n("=CUMPRINC(0.05/12,120,50000,1,120,0)");
    const payment = n("=PMT(0.05/12,120,50000)");
    expect(interest + principal).toBeCloseTo(payment * 120, 3);
  });

  it("CUMIPMT validates its arguments", () => {
    expect(code("=CUMIPMT(0.05/12,120,50000,0,10,0)")).toBe("#NUM!");
    expect(code("=CUMIPMT(0.05/12,120,50000,10,5,0)")).toBe("#NUM!");
    expect(code("=CUMIPMT(0.05/12,120,50000,1,121,0)")).toBe("#NUM!");
    expect(code("=CUMIPMT(0.05/12,120,50000,1,10,2)")).toBe("#NUM!");
    expect(code("=CUMIPMT(-0.01,120,50000,1,10,0)")).toBe("#NUM!");
  });

  it("SLN spreads the cost evenly", () => {
    expect(n("=SLN(30000,7500,10)")).toBe(2250);
  });

  it("SLN with a zero life divides by zero", () => {
    expect(code("=SLN(1000,0,0)")).toBe("#DIV/0!");
  });

  it("SYD accelerates the depreciation", () => {
    expect(n("=SYD(30000,7500,10,1)")).toBeCloseTo(4090.909, 2);
  });

  it("DDB declines faster than SYD", () => {
    expect(n("=DDB(30000,7500,10,1)")).toBeGreaterThan(n("=SYD(30000,7500,10,1)"));
    expect(n("=DDB(30000,7500,10,1)")).toBeCloseTo(4500, 6);
  });

  it("DDB never depreciates below the salvage value", () => {
    let total = 0;
    for (let period = 1; period <= 10; period += 1) total += n(`=DDB(30000,7500,10,${period})`);
    expect(total).toBeLessThanOrEqual(22500.01);
    expect(total).toBeGreaterThan(20000);
  });

  it("DB follows the declining balance schedule", () => {
    expect(n("=DB(1000000,100000,6,1,7)")).toBeCloseTo(186083.33, 0);
  });

  it("EFFECT and NOMINAL are inverses", () => {
    const effective = n("=EFFECT(0.12,12)");
    expect(n(`=NOMINAL(${effective},12)`)).toBeCloseTo(0.12, 8);
  });
});

describe("lookup functions", () => {
  const data = { A1: "a", A2: "b", A3: "c", B1: 10, B2: 20, B3: 30 };

  it("MATCH finds an exact value", () => {
    expect(n('=MATCH("b",A1:A3,0)', data)).toBe(2);
    expect(code('=MATCH("z",A1:A3,0)', data)).toBe("#N/A");
  });

  it("MATCH works on a horizontal range", () => {
    expect(n('=MATCH("c",A1:C1,0)', { A1: "a", B1: "b", C1: "c" })).toBe(3);
  });

  it("MATCH with type 1 finds the largest value not above the lookup", () => {
    expect(n("=MATCH(15,B1:B3,1)", data)).toBe(1);
    expect(n("=MATCH(35,B1:B3,1)", data)).toBe(3);
  });

  it("MATCH approximate returns the position in the original (unsorted) range", () => {
    // Regression: the old implementation sorted a copy, so an unsorted range
    // returned an index that pointed at a different cell.
    const unsorted = { A1: 10, A2: 5, A3: 20 };
    expect(n("=MATCH(7,A1:A3,1)", unsorted)).toBe(2);
    expect(n("=MATCH(5,A1:A3,1)", unsorted)).toBe(2);
    expect(code("=MATCH(3,A1:A3,1)", unsorted)).toBe("#N/A");
  });

  it("HLOOKUP supports and honours the approximate flag", () => {
    const table = { A1: 5, B1: 10, C1: 20, A2: "a", B2: "b", C2: "c" };
    // Approximate: largest header <= 7 is 5 (column A) -> row 2 = "a".
    expect(s("=HLOOKUP(7,A1:C2,2,TRUE)", table)).toBe("a");
    expect(code("=HLOOKUP(7,A1:C2,2,FALSE)", table)).toBe("#N/A");
  });

  it("XMATCH supports the search modes", () => {
    expect(n('=XMATCH("b",A1:A3)', data)).toBe(2);
    expect(n("=XMATCH(25,B1:B3,-1)", data)).toBe(2);
    expect(n("=XMATCH(25,B1:B3,1)", data)).toBe(3);
  });

  it("XLOOKUP returns the matching value", () => {
    expect(n('=XLOOKUP("b",A1:A3,B1:B3)', data)).toBe(20);
  });

  it("XLOOKUP uses the fallback when nothing matches", () => {
    expect(s('=XLOOKUP("z",A1:A3,B1:B3,"none")', data)).toBe("none");
    expect(code('=XLOOKUP("z",A1:A3,B1:B3)', data)).toBe("#N/A");
  });

  it("XLOOKUP approximate modes pick the nearest value", () => {
    // Regression: mode 2 had a dead branch and mode -2 never matched.
    const unsorted = { A1: 10, A2: 5, A3: 20, B1: "a", B2: "b", B3: "c" };
    // mode 2: smallest value >= 15 -> 20 -> "c".
    expect(s('=XLOOKUP(15,A1:A3,B1:B3,"none",2)', unsorted)).toBe("c");
    // mode -2: largest value <= 15 -> 10 -> "a".
    expect(s('=XLOOKUP(15,A1:A3,B1:B3,"none",-2)', unsorted)).toBe("a");
    // mode 2: smallest >= 5 -> 5 -> "b".
    expect(s('=XLOOKUP(5,A1:A3,B1:B3,"none",2)', unsorted)).toBe("b");
  });

  it("XLOOKUP approximate positions follow the original order", () => {
    // The two-array form must not sort: the chosen result is the one paired
    // with the selected lookup cell, not the result at a sorted position.
    const data2 = { A1: 30, A2: 10, A3: 20, B1: "thirty", B2: "ten", B3: "twenty" };
    // mode 2: smallest lookup >= 25 -> 30 (A1) -> "thirty".
    expect(s('=XLOOKUP(25,A1:A3,B1:B3,"none",2)', data2)).toBe("thirty");
  });

  it("VLOOKUP finds a column", () => {
    expect(n('=VLOOKUP("b",A1:B3,2,FALSE)', data)).toBe(20);
  });

  it("VLOOKUP approximates by default", () => {
    const table = { A1: 10, B1: "ten", A2: 20, B2: "twenty" };
    expect(s("=VLOOKUP(15,A1:B3,2)", table)).toBe("ten");
  });

  it("HLOOKUP finds a row", () => {
    const table = { A1: "a", B1: "b", A2: 1, B2: 2 };
    expect(n('=HLOOKUP("b",A1:B2,2,FALSE)', table)).toBe(2);
  });

  it("LOOKUP finds the last value not above the needle", () => {
    const table = { A1: 10, A2: 20, A3: 30, B1: "a", B2: "b", B3: "c" };
    expect(s("=LOOKUP(15,A1:A3,B1:B3)", table)).toBe("a");
  });

  it("INDEX reads a cell and a row", () => {
    expect(n("=INDEX(A1:B3,2,2)", data)).toBe(20);
    expect(s("=INDEX(A1:A3,3)", data)).toBe("c");
  });

  it("INDEX out of range is #REF!", () => {
    expect(code("=INDEX(A1:A3,9)", data)).toBe("#REF!");
  });

  it("CHOOSE picks by index", () => {
    expect(s('=CHOOSE(2,"a","b","c")')).toBe("b");
    expect(code('=CHOOSE(9,"a","b")')).toBe("#VALUE!");
  });

  it("ROWS and COLUMNS describe a range", () => {
    expect(n("=ROWS(A1:A3)")).toBe(3);
    expect(n("=COLUMNS(A1:C1)")).toBe(3);
  });
});

describe("math functions", () => {
  it("MOD follows the sign of the divisor", () => {
    expect(n("=MOD(-3,2)")).toBe(1);
    expect(n("=MOD(3,-2)")).toBe(-1);
  });

  it("MOD by zero is #DIV/0!", () => {
    expect(code("=MOD(3,0)")).toBe("#DIV/0!");
  });

  it("SQRT rejects a negative number", () => {
    expect(n("=SQRT(9)")).toBe(3);
    expect(code("=SQRT(-1)")).toBe("#NUM!");
  });

  it("ROUND avoids the classic 2.675 float error", () => {
    expect(n("=ROUND(2.675,2)")).toBe(2.68);
  });

  it("ROUNDUP and ROUNDDOWN move away from and toward zero", () => {
    expect(n("=ROUNDUP(1.001,2)")).toBe(1.01);
    expect(n("=ROUNDDOWN(1.009,2)")).toBe(1);
    expect(n("=ROUNDDOWN(-1.009,2)")).toBe(-1);
  });

  it("CEILING and FLOOR respect the significance", () => {
    expect(n("=CEILING(4.2,1)")).toBe(5);
    expect(n("=FLOOR(4.8,1)")).toBe(4);
    expect(n("=CEILING(4.2,0.5)")).toBe(4.5);
  });

  it("GCD and LCM work", () => {
    expect(n("=GCD(12,18)")).toBe(6);
    expect(n("=LCM(4,6)")).toBe(12);
  });

  it("TRUNC drops the fraction", () => {
    expect(n("=TRUNC(-1.9)")).toBe(-1);
    expect(n("=TRUNC(1.9)")).toBe(1);
  });

  it("QUOTIENT truncates toward zero", () => {
    expect(n("=QUOTIENT(7,2)")).toBe(3);
    expect(n("=QUOTIENT(-7,2)")).toBe(-3);
  });

  it("ISEVEN and ISODD", () => {
    expect(evaluateFormula("=ISEVEN(4)", context())).toBe(true);
    expect(evaluateFormula("=ISODD(4)", context())).toBe(false);
  });

  it("SUMXMY2 and friends", () => {
    expect(n("=SUMXMY2({1,2,3},{1,1,1})")).toBe(5);
    expect(n("=SUMX2MY2({2,3},{1,1})")).toBe(11);
  });
});

describe("error handling contract", () => {
  it("an unknown function is #NAME?", () => {
    expect(code("=NOPE(1)")).toBe("#NAME?");
  });

  it("an unknown name is #NAME?", () => {
    expect(code("=UNKNOWN_NAME")).toBe("#NAME?");
  });

  it("the wrong argument count is #VALUE!", () => {
    expect(code("=LEFT()")).toBe("#VALUE!");
  });

  it("a runaway range is refused rather than allocated", () => {
    expect(code("=SUM(A1:AMJ1048576)")).toBe("#REF!");
  });

  it("an unknown sheet is #REF!", () => {
    expect(code("=SUM(Nope!A1:A2)")).toBe("#REF!");
  });

  it("division by zero is #DIV/0!", () => {
    expect(code("=1/0")).toBe("#DIV/0!");
  });

  it("text in arithmetic is #VALUE!", () => {
    expect(code('="a"+1')).toBe("#VALUE!");
  });

  it("a truncated formula is #VALUE! rather than a partial result", () => {
    expect(code("=1+")).toBe("#VALUE!");
  });

  it("trailing junk after a complete formula is refused", () => {
    expect(code("=1+1 2")).toBe("#VALUE!");
  });

  it("SWITCH falls through to its default", () => {
    expect(s('=SWITCH(2,1,"one",2,"two","other")')).toBe("two");
    expect(s('=SWITCH(9,1,"one","other")')).toBe("other");
  });

  it("XOR is odd parity", () => {
    expect(evaluateFormula("=XOR(TRUE,TRUE,TRUE)", context())).toBe(true);
    expect(evaluateFormula("=XOR(TRUE,TRUE)", context())).toBe(false);
  });
});

describe("named ranges", () => {
  function book(): Workbook {
    return { ...newWorkbook("Named"), sheets: [newSheet("Sheet1"), newSheet("Data")], names: [] };
  }

  it("resolves a workbook-level name to a range", () => {
    let wb = book();
    wb = applyCellEdit(wb, 1, 0, 0, "10");
    wb = applyCellEdit(wb, 1, 1, 0, "20");
    wb = { ...wb, names: [{ name: "Prices", definition: "Data!A1:A2", sheet: null }] };
    wb = applyCellEdit(wb, 0, 0, 0, "=SUM(Prices)");
    expect(computeWorkbookValues(wb).get("Sheet1!A1")).toBe(30);
  });

  it("resolves a name to a constant", () => {
    let wb: Workbook = { ...book(), names: [{ name: "VAT_RATE", definition: "0.2", sheet: null }] };
    wb = applyCellEdit(wb, 0, 0, 0, "=100*VAT_RATE");
    expect(computeWorkbookValues(wb).get("Sheet1!A1")).toBe(20);
  });

  it("a sheet-scoped name is visible on its own sheet", () => {
    let wb: Workbook = { ...book(), names: [{ name: "Local", definition: "7", sheet: "Data" }] };
    wb = applyCellEdit(wb, 1, 0, 0, "=Local+1");
    expect(computeWorkbookValues(wb).get("Data!A1")).toBe(8);
  });

  it("a sheet-scoped name is hidden from other sheets", () => {
    let wb: Workbook = { ...book(), names: [{ name: "Local", definition: "7", sheet: "Data" }] };
    wb = applyCellEdit(wb, 0, 0, 0, "=Local");
    expect(codeOf(computeWorkbookValues(wb), "Sheet1!A1")).toBe("#NAME?");
  });

  it("a sheet-scoped name shadows a workbook-level one", () => {
    let wb: Workbook = book();
    wb = {
      ...wb,
      names: [
        { name: "RATE", definition: "1", sheet: null },
        { name: "RATE", definition: "2", sheet: "Data" },
      ],
    };
    wb = applyCellEdit(wb, 0, 0, 0, "=RATE");
    wb = applyCellEdit(wb, 1, 0, 0, "=RATE");
    const values = computeWorkbookValues(wb);
    expect(values.get("Sheet1!A1")).toBe(1);
    expect(values.get("Data!A1")).toBe(2);
  });

  it("name matching is case-insensitive", () => {
    let wb: Workbook = { ...book(), names: [{ name: "Vat", definition: "0.2", sheet: null }] };
    wb = applyCellEdit(wb, 0, 0, 0, "=vat*10");
    expect(computeWorkbookValues(wb).get("Sheet1!A1")).toBe(2);
  });

  it("a name referring to itself is a circular reference", () => {
    let wb: Workbook = { ...book(), names: [{ name: "Loop", definition: "Loop+1", sheet: null }] };
    wb = applyCellEdit(wb, 0, 0, 0, "=Loop");
    expect(codeOf(computeWorkbookValues(wb), "Sheet1!A1")).toBe("#REF!");
  });

  it("a name can hold a formula", () => {
    let wb: Workbook = applyCellEdit(book(), 0, 0, 0, "4");
    wb = { ...wb, names: [{ name: "DOUBLED", definition: "A1*2", sheet: null }] };
    wb = applyCellEdit(wb, 0, 1, 0, "=DOUBLED");
    expect(computeWorkbookValues(wb).get("Sheet1!A2")).toBe(8);
  });
});

describe("address helpers", () => {
  it("parses and formats", () => {
    expect(parseAddress("$C$7")).toEqual({ row: 6, col: 2 });
    expect(parseAddress("A0")).toBeNull();
    expect(parseAddress("AAAA1")).toBeNull();
  });

  it("normalises a reversed range", () => {
    expect(parseRange("C3:A1")).toEqual({ start: { row: 0, col: 0 }, end: { row: 2, col: 2 } });
  });

  it("addressesInRange respects its cap", () => {
    expect(addressesInRange("A1:Z100", 10)).toHaveLength(10);
  });
});

describe("number formatting", () => {
  it("renders General, percent, grouping and negatives", () => {
    expect(formatNumber(1234.5, "General")).toBe("1234.5");
    expect(formatNumber(0.25, "0%")).toBe("25%");
    expect(formatNumber(1234567, "#,##0")).toBe("1,234,567");
    expect(formatNumber(-12, "#,##0.00;(#,##0.00)")).toBe("(12.00)");
  });

  it("renders a date pattern", () => {
    expect(formatNumber(45292, "yyyy-mm-dd")).toBe("2024-01-01");
  });
});

describe("workbook integration", () => {
  it("a formula chain recalculates when an upstream cell changes", () => {
    let wb: Workbook = { ...newWorkbook("Chain"), sheets: [newSheet("Sheet1")] };
    wb = applyCellEdit(wb, 0, 0, 0, "2");
    wb = applyCellEdit(wb, 0, 1, 0, "=A1^2");
    wb = applyCellEdit(wb, 0, 2, 0, "=A2+1");
    expect(computeWorkbookValues(wb).get("Sheet1!A3")).toBe(5);
    wb = applyCellEdit(wb, 0, 0, 0, "3");
    expect(computeWorkbookValues(wb).get("Sheet1!A3")).toBe(10);
  });

  it("keeps a number format when the text is cleared", () => {
    let wb: Workbook = { ...newWorkbook("Fmt"), sheets: [newSheet("Sheet1")] };
    const styled = emptyCell();
    styled.style = { ...styled.style, numberFormat: "#,##0.00" };
    wb = {
      ...wb,
      sheets: [{ ...wb.sheets[0], cells: { A1: { ...styled, value: { kind: "number" as const, value: 5 } } } }],
    };
    expect(wb.sheets[0].cells.A1).toBeDefined();
    wb = applyCellEdit(wb, 0, 0, 0, "");
    expect(wb.sheets[0].cells.A1?.value).toEqual({ kind: "empty" });
    expect(wb.sheets[0].cells.A1?.style.numberFormat).toBe("#,##0.00");
  });
});

describe("extended statistics and covariance", () => {
  it("VAR.S, VAR.P, STDEV.S and STDEV.P agree with the sample/population split", () => {
    expect(n("=VAR.S({1,2,3,4})")).toBeCloseTo(5 / 3, 10);
    expect(n("=VAR.P({1,2,3,4})")).toBeCloseTo(1.25, 10);
    expect(n("=VAR({1,2,3,4})")).toBeCloseTo(5 / 3, 10);
    expect(n("=VARP({1,2,3,4})")).toBeCloseTo(1.25, 10);
    expect(n("=STDEV.S({1,2,3,4})")).toBeCloseTo(Math.sqrt(5 / 3), 10);
    expect(n("=STDEV.P({1,2,3,4})")).toBeCloseTo(Math.sqrt(1.25), 10);
  });

  it("VAR.S needs at least two values", () => {
    expect(code("=VAR.S({5})")).toBe("#DIV/0!");
  });

  it("PERCENTILE interpolates between neighbours", () => {
    expect(n("=PERCENTILE({1,2,3,4,5},0)")).toBe(1);
    expect(n("=PERCENTILE({1,2,3,4,5},1)")).toBe(5);
    expect(n("=PERCENTILE({1,2,3,4,5},0.5)")).toBe(3);
    expect(n("=PERCENTILE({1,2,3,4,5},0.25)")).toBe(2);
    expect(n("=PERCENTILE.INC({1,2,3,4,5},0.75)")).toBe(4);
  });

  it("PERCENTILE rejects k outside 0..1", () => {
    expect(code("=PERCENTILE({1,2,3},1.5)")).toBe("#NUM!");
  });

  it("QUARTILE maps 0..4 onto the percentile scale", () => {
    expect(n("=QUARTILE({1,2,3,4,5},0)")).toBe(1);
    expect(n("=QUARTILE({1,2,3,4,5},2)")).toBe(3);
    expect(n("=QUARTILE({1,2,3,4,5},4)")).toBe(5);
    expect(n("=QUARTILE.INC({1,2,3,4,5},1)")).toBe(2);
    expect(code("=QUARTILE({1,2,3},9)")).toBe("#NUM!");
  });

  it("CORREL is 1 for a perfectly linear pair and -1 for an inverse one", () => {
    expect(n("=CORREL({1,2,3},{2,4,6})")).toBeCloseTo(1, 10);
    expect(n("=CORREL({1,2,3},{6,4,2})")).toBeCloseTo(-1, 10);
    expect(code("=CORREL({1},{2})")).toBe("#DIV/0!");
  });

  it("COVARIANCE.P and COVARIANCE.S differ by the divisor", () => {
    expect(n("=COVARIANCE.P({1,2,3},{2,4,6})")).toBeCloseTo((2 / 3) * 2, 10);
    expect(n("=COVARIANCE.S({1,2,3},{2,4,6})")).toBeCloseTo(2, 10);
    expect(n("=COVAR({1,2,3},{2,4,6})")).toBe(n("=COVARIANCE.P({1,2,3},{2,4,6})"));
  });

  it("array comparisons broadcast, so FILTER can read a boolean range", () => {
    const data = context({ A1: 1, A2: 5, A3: 2 });
    expect(evaluateToMatrix("=FILTER(A1:A3,A1:A3>1)", data)).toEqual([[5], [2]]);
  });
});
