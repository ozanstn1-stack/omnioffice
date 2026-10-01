import { describe, expect, it } from "vitest";
import {
  ERR,
  addressesInRange,
  columnLabel,
  evaluateFormula,
  evaluateToMatrix,
  formatAddress,
  formatNumber,
  functionNames,
  isError,
  isFormula,
  parseAddress,
  parseRange,
  registerFunction,
  suggestFunctions,
  type FormulaContext,
  type Scalar,
} from "./formula";

/** Minimal context: a plain address -> scalar map plus the sheet names. */
function context(values: Record<string, Scalar>, sheetNames: string[] = ["Sheet1"]): FormulaContext {
  return {
    getValue: (sheet, address) => {
      const key = sheet ? `${sheet}!${address}` : address;
      return key in values ? values[key] : "";
    },
    sheetNames,
    currentSheet: sheetNames[0] ?? "",
  };
}

describe("addresses", () => {
  it("parses and formats A1 addresses in both directions", () => {
    expect(parseAddress("A1")).toEqual({ row: 0, col: 0 });
    expect(parseAddress("B3")).toEqual({ row: 2, col: 1 });
    expect(parseAddress("$C$7")).toEqual({ row: 6, col: 2 });
    expect(formatAddress(0, 0)).toBe("A1");
    expect(formatAddress(6, 2)).toBe("C7");
    expect(parseAddress("A1")).toEqual({ row: 0, col: 0 });
  });

  it("returns null for malformed addresses", () => {
    expect(parseAddress("")).toBeNull();
    expect(parseAddress("1A")).toBeNull();
    expect(parseAddress("hello")).toBeNull();
  });

  it("labels columns past Z", () => {
    expect(columnLabel(0)).toBe("A");
    expect(columnLabel(25)).toBe("Z");
    expect(columnLabel(26)).toBe("AA");
    expect(columnLabel(701)).toBe("ZZ");
  });

  it("parses ranges and normalises reversed bounds", () => {
    expect(parseRange("A1:B2")).toEqual({ start: { row: 0, col: 0 }, end: { row: 1, col: 1 } });
    expect(parseRange("B2:A1")).toEqual({ start: { row: 0, col: 0 }, end: { row: 1, col: 1 } });
    expect(parseRange("A1")).toEqual({ start: { row: 0, col: 0 }, end: { row: 0, col: 0 } });
    expect(parseRange("nonsense")).toBeNull();
  });

  it("enumerates a range row by row", () => {
    expect(addressesInRange("A1:B2")).toEqual(["A1", "B1", "A2", "B2"]);
    expect(addressesInRange("A1:A3")).toEqual(["A1", "A2", "A3"]);
  });

  it("refuses to enumerate a runaway range", () => {
    expect(addressesInRange("A1:Z100000", 10)).toHaveLength(10);
  });
});

describe("formatNumber", () => {
  it("applies thousands grouping and fixed decimals", () => {
    expect(formatNumber(1234.5, "General")).toBe("1234.5");
    expect(formatNumber(1234.5, "#,##0.00")).toBe("1,234.50");
    expect(formatNumber(1234.5, "#,##0")).toBe("1,235");
  });

  it("applies percentages", () => {
    expect(formatNumber(0.25, "0%")).toBe("25%");
  });

  it("applies currency prefixes", () => {
    expect(formatNumber(10, "$#,##0.00")).toContain("10");
    expect(formatNumber(10, "$#,##0.00")).toContain("$");
  });

  it("formats times with minutes, not months", () => {
    // Regression: the old replacement order wrote the month into `hh:mm`.
    expect(formatNumber(0.5, "hh:mm")).toBe("12:00");
    expect(formatNumber(0.25, "h:mm")).toBe("6:00");
    expect(formatNumber(0.5, "hh:mm:ss")).toBe("12:00:00");
    // `mm` before an hour token is still a month.
    expect(formatNumber(45306, "yyyy-mm-dd")).toBe("2024-01-15");
  });
});

describe("evaluateFormula", () => {
  it("evaluates arithmetic and precedence", () => {
    const ctx = context({});
    expect(evaluateFormula("=1+2*3", ctx)).toBe(7);
    expect(evaluateFormula("=(1+2)*3", ctx)).toBe(9);
    expect(evaluateFormula("=2^3", ctx)).toBe(8);
    expect(evaluateFormula("=-4+10", ctx)).toBe(6);
  });

  it("tolerates a missing leading equals sign", () => {
    expect(evaluateFormula("SUM(1,2)", context({}))).toBe(3);
  });

  it("reads single cells and ranges", () => {
    const ctx = context({ A1: 10, A2: 20, A3: 30, B1: "x" });
    expect(evaluateFormula("=A1", ctx)).toBe(10);
    expect(evaluateFormula("=SUM(A1:A3)", ctx)).toBe(60);
    expect(evaluateFormula("=AVERAGE(A1:A3)", ctx)).toBe(20);
    expect(evaluateFormula("=A1&B1", ctx)).toBe("10x");
  });

  it("ignores text inside numeric aggregates", () => {
    const ctx = context({ A1: 10, A2: "text", A3: 30 });
    expect(evaluateFormula("=SUM(A1:A3)", ctx)).toBe(40);
    expect(evaluateFormula("=COUNT(A1:A3)", ctx)).toBe(2);
    expect(evaluateFormula("=COUNTA(A1:A3)", ctx)).toBe(3);
  });

  it("reports division by zero instead of producing Infinity", () => {
    const result = evaluateFormula("=1/0", context({}));
    expect(isError(result)).toBe(true);
    expect((result as { code: string }).code).toBe("#DIV/0!");
  });

  it("reports an unknown function as #NAME?", () => {
    const result = evaluateFormula("=NOSUCHFUNC(1)", context({}));
    expect(isError(result)).toBe(true);
    expect((result as { code: string }).code).toBe("#NAME?");
  });

  it("reports a bad reference for an unknown sheet", () => {
    const result = evaluateFormula("=Nope!A1", context({}, ["Sheet1"]));
    expect(isError(result)).toBe(true);
    expect((result as { code: string }).code).toBe("#REF!");
  });

  it("supports quoted sheet names and cross-sheet ranges", () => {
    const ctx = context({ "Data!D2": 5, "Data!D3": 7, "Data!D4": 11 }, ["Sheet1", "Data"]);
    expect(evaluateFormula("=SUM(Data!D2:D4)", ctx)).toBe(23);
    expect(evaluateFormula("=SUM('Data'!D2:D4)", ctx)).toBe(23);
  });

  it("propagates errors through arithmetic", () => {
    const ctx = context({ A1: ERR.ref() });
    const result = evaluateFormula("=A1+1", ctx);
    expect(isError(result)).toBe(true);
    expect((result as { code: string }).code).toBe("#REF!");
  });

  it("supports IF/IFS/IFERROR", () => {
    const ctx = context({ A1: 5 });
    expect(evaluateFormula("=IF(A1>3,\"big\",\"small\")", ctx)).toBe("big");
    expect(evaluateFormula("=IF(A1>30,\"big\",\"small\")", ctx)).toBe("small");
    expect(evaluateFormula("=IFS(A1>10,\"a\",A1>3,\"b\")", ctx)).toBe("b");
    expect(evaluateFormula("=IFERROR(1/0,\"safe\")", ctx)).toBe("safe");
  });

  it("supports lookup functions", () => {
    const ctx = context({ A1: 1, A2: 2, A3: 3, B1: "a", B2: "b", B3: "c" });
    expect(evaluateFormula("=VLOOKUP(2,A1:B3,2,FALSE)", ctx)).toBe("b");
    expect(evaluateFormula("=MATCH(3,A1:A3,0)", ctx)).toBe(3);
    expect(evaluateFormula("=INDEX(A1:A3,2)", ctx)).toBe(2);
  });

  it("supports conditional aggregates with comparison criteria", () => {
    const ctx = context({ A1: 1, A2: 5, A3: 9, B1: 10, B2: 20, B3: 30 });
    expect(evaluateFormula("=SUMIF(A1:A3,\">2\",B1:B3)", ctx)).toBe(50);
    expect(evaluateFormula("=COUNTIF(A1:A3,\">=5\")", ctx)).toBe(2);
  });

  it("supports text functions", () => {
    const ctx = context({ A1: "Hello World" });
    expect(evaluateFormula("=LEFT(A1,5)", ctx)).toBe("Hello");
    expect(evaluateFormula("=RIGHT(A1,5)", ctx)).toBe("World");
    expect(evaluateFormula("=MID(A1,7,5)", ctx)).toBe("World");
    expect(evaluateFormula("=LEN(A1)", ctx)).toBe(11);
    expect(evaluateFormula("=UPPER(A1)", ctx)).toBe("HELLO WORLD");
    expect(evaluateFormula("=SUBSTITUTE(A1,\"World\",\"There\")", ctx)).toBe("Hello There");
  });

  it("supports date helpers on serial numbers", () => {
    // 2024-01-15 is serial 45306.
    expect(evaluateFormula("=YEAR(45306)", context({}))).toBe(2024);
    expect(evaluateFormula("=MONTH(45306)", context({}))).toBe(1);
    expect(evaluateFormula("=DAY(45306)", context({}))).toBe(15);
  });

  it("rejects calls with the wrong arity as #VALUE!", () => {
    const result = evaluateFormula("=LEFT()", context({}));
    expect(isError(result)).toBe(true);
    expect((result as { code: string }).code).toBe("#VALUE!");
  });

  it("returns an empty string for an empty formula", () => {
    expect(evaluateFormula("=", context({}))).toBe("");
  });

  it("guards against runaway ranges", () => {
    const ctx: FormulaContext = { ...context({}), maxRangeCells: 4 };
    const result = evaluateFormula("=SUM(A1:Z1000)", ctx);
    expect(isError(result)).toBe(true);
    expect((result as { code: string }).code).toBe("#REF!");
  });

  it("propagates a range failure instead of silently summing zero", () => {
    // Regression: `numbers()` skips error values, so a failed range used to be
    // swallowed by SUM and reported as 0 instead of #REF!.
    const ctx: FormulaContext = { ...context({}), maxRangeCells: 4 };
    for (const formula of ["=SUM(A1:Z1000)", "=AVERAGE(A1:Z1000)", "=MAX(A1:Z1000)", "=COUNT(A1:Z1000)"]) {
      const result = evaluateFormula(formula, ctx);
      expect(isError(result), `${formula} should propagate the range error`).toBe(true);
      expect((result as { code: string }).code).toBe("#REF!");
    }
  });

  it("propagates errors found inside a range from numeric aggregates", () => {
    // Excel semantics: SUM over a range containing an error yields that error.
    const ctx = context({ A1: 10, A2: ERR.div(), A3: 30 });
    const result = evaluateFormula("=SUM(A1:A3)", ctx);
    expect(isError(result)).toBe(true);
    expect((result as { code: string }).code).toBe("#DIV/0!");
  });
});

describe("reference forms", () => {
  const ctx = context(
    { A1: 5, A2: 9, B1: 7, "Sheet1!A1": 5, "My Sheet!A1": 11, "My Sheet!A2": 13 },
    ["Sheet1", "My Sheet"],
  );

  it("treats absolute, relative and mixed anchors as the same cell", () => {
    expect(evaluateFormula("=$A$1", ctx)).toBe(5);
    expect(evaluateFormula("=A$1", ctx)).toBe(5);
    expect(evaluateFormula("=$A1", ctx)).toBe(5);
    expect(evaluateFormula("=A1", ctx)).toBe(5);
  });

  it("reads ranges with absolute and mixed anchors", () => {
    expect(evaluateFormula("=SUM($A$1:$A$2)", ctx)).toBe(14);
    expect(evaluateFormula("=SUM(A$1:A$2)", ctx)).toBe(14);
    expect(evaluateFormula("=SUM($A1:$A2)", ctx)).toBe(14);
    expect(evaluateFormula("=SUM(A1:B1)", ctx)).toBe(12);
  });

  it("reads cross-sheet references, quoted or bare", () => {
    expect(evaluateFormula("=Sheet1!A1", ctx)).toBe(5);
    expect(evaluateFormula("='My Sheet'!A1", ctx)).toBe(11);
    expect(evaluateFormula("=SUM('My Sheet'!A1:A2)", ctx)).toBe(24);
  });

  it("reports an unknown quoted sheet as #REF!, not as a name", () => {
    const result = evaluateFormula("='Nope Sheet'!A1", ctx);
    expect(isError(result)).toBe(true);
    expect((result as { code: string }).code).toBe("#REF!");
  });

  it("is case-insensitive like Excel for references and sheet names", () => {
    expect(evaluateFormula("=a1", ctx)).toBe(5);
    expect(evaluateFormula("=sum(a1:a2)", ctx)).toBe(14);
    expect(evaluateFormula("=sheet1!a1", ctx)).toBe(5);
  });

  it("rejects a malformed reference instead of reading a neighbour", () => {
    for (const formula of ["=A0+1", "=SUM(A0:A3)"]) {
      const result = evaluateFormula(formula, ctx);
      expect(isError(result), `${formula} should be an error`).toBe(true);
      expect((result as { code: string }).code).toBe("#REF!");
    }
    // Four letters is not a cell reference at all, so it reads as a name -
    // which is exactly what Excel does with `=AAAA1`.
    const longName = evaluateFormula("=AAAA1+1", ctx);
    expect(isError(longName)).toBe(true);
    expect((longName as { code: string }).code).toBe("#NAME?");
  });
});

describe("error handling contract", () => {
  it("IFERROR still catches an error argument", () => {
    const ctx = context({ A1: ERR.value() });
    expect(evaluateFormula("=IFERROR(A1,\"safe\")", ctx)).toBe("safe");
    expect(evaluateFormula("=IFERROR(5,\"safe\")", ctx)).toBe(5);
  });

  it("IF does not leak the error of the branch it does not take", () => {
    const ctx = context({ A1: 1 });
    expect(evaluateFormula("=IF(A1>0,\"yes\",1/0)", ctx)).toBe("yes");
    // ...but the taken branch is still allowed to fail.
    const failing = evaluateFormula("=IF(A1>0,1/0,\"no\")", ctx);
    expect(isError(failing)).toBe(true);
  });

  it("IFS does not leak errors from later unmatched conditions", () => {
    const ctx = context({ A1: 1 });
    expect(evaluateFormula("=IFS(A1>10,\"big\",A1>0,\"small\")", ctx)).toBe("small");
  });

  it("ISERROR and friends inspect errors instead of propagating them", () => {
    const ctx = context({ A1: ERR.na() });
    expect(evaluateFormula("=ISERROR(A1)", ctx)).toBe(true);
    expect(evaluateFormula("=ISNA(A1)", ctx)).toBe(true);
    expect(evaluateFormula("=ISNA(1)", ctx)).toBe(false);
  });

  it("AND and OR propagate errors like Excel does", () => {
    const ctx = context({ A1: ERR.value() });
    expect(isError(evaluateFormula("=AND(A1,TRUE)", ctx))).toBe(true);
  });

  it("IFNA only catches #N/A and passes other errors through", () => {
    const div = context({ A1: ERR.div() });
    const passthrough = evaluateFormula("=IFNA(A1,\"fallback\")", div);
    expect(isError(passthrough)).toBe(true);
    expect((passthrough as { code: string }).code).toBe("#DIV/0!");
    const na = context({ A1: ERR.na() });
    expect(evaluateFormula("=IFNA(A1,\"fallback\")", na)).toBe("fallback");
  });
});

describe("array returning functions", () => {
  it("TRANSPOSE flips a matrix", () => {
    const ctx = context({ A1: 1, B1: 2, A2: 3, B2: 4 });
    // A single-cell consumer sees the top-left of the transposed matrix.
    expect(evaluateFormula("=TRANSPOSE(A1:B2)", ctx)).toBe(1);
  });

  it("SUMPRODUCT multiplies matching cells once each", () => {
    // Regression: the old implementation multiplied by the first matrix twice.
    const ctx = context({ A1: 2, A2: 3, B1: 4, B2: 5 });
    expect(evaluateFormula("=SUMPRODUCT(A1:A2,B1:B2)", ctx)).toBe(2 * 4 + 3 * 5);
    expect(evaluateFormula("=SUMPRODUCT(A1:A2)", ctx)).toBe(5);
  });

  it("broadcasts a scalar over the whole range", () => {
    // Regression: `=A1:A3*2` only multiplied the first row; the rest were
    // padded with empty cells that coerce to 0.
    const ctx = context({ A1: 1, A2: 2, A3: 3 });
    expect(evaluateToMatrix("=A1:A3*2", ctx)).toEqual([[2], [4], [6]]);
    expect(evaluateToMatrix("=10+A1:A3", ctx)).toEqual([[11], [12], [13]]);
    expect(evaluateFormula("=SUM(A1:A3*10)", ctx)).toBe(60);
    expect(evaluateToMatrix("=2*A1:A3", ctx)).toEqual([[2], [4], [6]]);
  });

  it("parses error literals and lets IFERROR catch them", () => {
    // Regression: `#` was an unknown character, so `=#N/A` became #VALUE!.
    const ctx = context({});
    expect(evaluateFormula("=#N/A", ctx)).toMatchObject({ code: "#N/A" });
    expect(evaluateFormula('=IFERROR(#N/A,"safe")', ctx)).toBe("safe");
    expect(evaluateFormula("=ISERROR(#N/A)", ctx)).toBe(true);
    expect(evaluateFormula("=IF(TRUE,1,#DIV/0!)", ctx)).toBe(1);
  });
});

describe("registry", () => {
  it("exposes a sorted function list", () => {
    const names = functionNames();
    expect(names).toContain("SUM");
    expect(names).toContain("VLOOKUP");
    expect([...names].sort()).toEqual(names);
  });

  it("accepts a newly registered function", () => {
    registerFunction("OSWKTESTDOUBLE", (args) => {
      const value = args[0]?.[0]?.[0];
      return typeof value === "number" ? value * 2 : ERR.value();
    }, 1, 1);
    expect(evaluateFormula("=OSWKTESTDOUBLE(21)", context({}))).toBe(42);
    expect(functionNames()).toContain("OSWKTESTDOUBLE");
  });

  it("suggests functions by prefix", () => {
    expect(suggestFunctions("SUM")).toEqual(expect.arrayContaining(["SUM", "SUMIF", "SUMIFS"]));
    expect(suggestFunctions("ZZZZ")).toEqual([]);
  });
});

describe("isFormula", () => {
  it("detects a leading equals sign with a body", () => {
    expect(isFormula("=A1")).toBe(true);
    expect(isFormula("=SUM(A1:A2)")).toBe(true);
    expect(isFormula("A1")).toBe(false);
    expect(isFormula("=")).toBe(false);
    expect(isFormula("")).toBe(false);
  });
});
