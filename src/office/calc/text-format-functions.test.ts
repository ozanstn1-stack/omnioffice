/**
 * Text and information functions added or tightened for Excel parity: DOLLAR,
 * FIXED, ARRAYTOTEXT, NUMBERVALUE, TEXTBEFORE/TEXTAFTER with their optional
 * arguments, CLEAN, UNICHAR/UNICODE, N and T.
 *
 * Expected values are the examples Microsoft documents for each function; the
 * comment on a case names the example it comes from.
 */
import { describe, expect, it } from "vitest";
import { evaluateFormula, isError, lookupFunction, type FormulaContext, type Scalar } from "./formula";

function context(values: Record<string, Scalar> = {}): FormulaContext {
  return {
    getValue: (_sheet, address) => (address in values ? values[address] : ""),
    sheetNames: ["Sheet1"],
    currentSheet: "Sheet1",
  };
}

function value(formula: string, values: Record<string, Scalar> = {}): Scalar {
  return evaluateFormula(formula, context(values));
}

/** The result as text, or the error code when the formula fails. */
function s(formula: string, values: Record<string, Scalar> = {}): string {
  const result = value(formula, values);
  return isError(result) ? result.code : String(result);
}

function n(formula: string): number {
  const result = value(formula);
  if (isError(result)) throw new Error(`${formula} returned ${result.code}`);
  if (typeof result !== "number") throw new Error(`${formula} returned ${String(result)}`);
  return result;
}

describe("DOLLAR", () => {
  it("follows the Microsoft examples", () => {
    expect(s("=DOLLAR(1234.567,2)")).toBe("$1,234.57");
    expect(s("=DOLLAR(1234.567,-2)")).toBe("$1,200");
    expect(s("=DOLLAR(-1234.567,-2)")).toBe("($1,200)");
    expect(s("=DOLLAR(-0.123,4)")).toBe("($0.1230)");
    expect(s("=DOLLAR(99.888)")).toBe("$99.89");
  });

  it("rounds half away from zero on the decimal value, like Excel", () => {
    expect(s("=DOLLAR(2.675)")).toBe("$2.68");
    expect(s("=DOLLAR(1.005)")).toBe("$1.01");
    expect(s("=DOLLAR(0.5,0)")).toBe("$1");
    expect(s("=DOLLAR(-0.5,0)")).toBe("($1)");
    expect(s("=DOLLAR(999.995)")).toBe("$1,000.00");
    expect(s("=DOLLAR(1234.5675,3)")).toBe("$1,234.568");
  });

  it("groups thousands and handles large and small magnitudes", () => {
    expect(s("=DOLLAR(1234567.891,1)")).toBe("$1,234,567.9");
    expect(s("=DOLLAR(999)")).toBe("$999.00");
    expect(s("=DOLLAR(1000)")).toBe("$1,000.00");
    expect(s("=DOLLAR(0.000004,6)")).toBe("$0.000004");
    expect(s("=DOLLAR(123456789012,0)")).toBe("$123,456,789,012");
  });

  it("shows zero without a sign, even for a tiny negative that rounds to it", () => {
    expect(s("=DOLLAR(0)")).toBe("$0.00");
    expect(s("=DOLLAR(-0.001)")).toBe("$0.00");
    expect(s("=DOLLAR(-4,-1)")).toBe("$0");
  });

  it("coerces text and rejects what is not a number", () => {
    expect(s('=DOLLAR("12.5")')).toBe("$12.50");
    expect(s('=DOLLAR("x")')).toBe("#VALUE!");
    expect(s("=DOLLAR(#N/A)")).toBe("#N/A");
  });
});

describe("FIXED", () => {
  it("follows the Microsoft examples", () => {
    expect(s("=FIXED(1234.567,1)")).toBe("1,234.6");
    expect(s("=FIXED(1234.567,-1)")).toBe("1,230");
    expect(s("=FIXED(-1234.567,-1,TRUE)")).toBe("-1230");
    expect(s("=FIXED(44.332)")).toBe("44.33");
  });

  it("leaves out the thousands separators when no_commas is TRUE", () => {
    expect(s("=FIXED(1234567.891,2,TRUE)")).toBe("1234567.89");
    expect(s("=FIXED(1234567.891,2,FALSE)")).toBe("1,234,567.89");
    expect(s("=FIXED(1234567.891,2,1)")).toBe("1234567.89");
  });

  it("rounds half away from zero and carries into the next digit", () => {
    expect(s("=FIXED(2.5,0)")).toBe("3");
    expect(s("=FIXED(-2.5,0)")).toBe("-3");
    expect(s("=FIXED(999.999,2)")).toBe("1,000.00");
    expect(s("=FIXED(2.675,2)")).toBe("2.68");
    expect(s("=FIXED(1234.5678,-2)")).toBe("1,200");
  });

  it("pads with zeros and shows zero without a sign", () => {
    expect(s("=FIXED(5,3)")).toBe("5.000");
    expect(s("=FIXED(0.1,0)")).toBe("0");
    expect(s("=FIXED(-0.001,2)")).toBe("0.00");
    expect(s("=FIXED(-0.4,0)")).toBe("0");
  });

  it("rejects text and passes errors through", () => {
    expect(s('=FIXED("x")')).toBe("#VALUE!");
    expect(s("=FIXED(1,#DIV/0!)")).toBe("#DIV/0!");
  });
});

describe("ARRAYTOTEXT", () => {
  it("joins the values with commas in concise format", () => {
    expect(s("=ARRAYTOTEXT({1,2;3,4})")).toBe("1, 2, 3, 4");
    expect(s("=ARRAYTOTEXT({1,2;3,4},0)")).toBe("1, 2, 3, 4");
    expect(s('=ARRAYTOTEXT({"a",TRUE,#N/A})')).toBe("a, TRUE, #N/A");
  });

  it("keeps the shape and quotes text in strict format", () => {
    expect(s("=ARRAYTOTEXT({1,2;3,4},1)")).toBe("{1,2;3,4}");
    expect(s('=ARRAYTOTEXT({"a","b"},1)')).toBe('{"a","b"}');
    expect(s('=ARRAYTOTEXT({"say ""hi""",TRUE,#N/A},1)')).toBe('{"say ""hi""",TRUE,#N/A}');
  });

  it("reads ranges, with blank cells empty", () => {
    expect(s("=ARRAYTOTEXT(A1:B2)", { A1: 1, B1: "x", A2: 2.5 })).toBe("1, x, 2.5, ");
  });

  it("accepts only 0 and 1 as the format", () => {
    expect(s("=ARRAYTOTEXT({1},2)")).toBe("#VALUE!");
    expect(s('=ARRAYTOTEXT({1},"x")')).toBe("#VALUE!");
  });
});

describe("NUMBERVALUE", () => {
  it("follows the Microsoft examples", () => {
    expect(n('=NUMBERVALUE("2.500,27",",",".")')).toBe(2500.27);
    expect(n('=NUMBERVALUE("3.5%")')).toBe(0.035);
  });

  it("ignores spaces anywhere, even between digits", () => {
    expect(n('=NUMBERVALUE(" 3 000 ")')).toBe(3000);
    expect(n('=NUMBERVALUE("1 234,5",",")')).toBe(1234.5);
  });

  it("divides by 100 for each percent sign at the end", () => {
    expect(n('=NUMBERVALUE("9%%")')).toBeCloseTo(0.0009, 15);
    expect(n('=NUMBERVALUE("50 %")')).toBe(0.5);
    expect(s('=NUMBERVALUE("5%5")')).toBe("#VALUE!");
  });

  it("is 0 for empty text and uses only the first character of a separator", () => {
    expect(n('=NUMBERVALUE("")')).toBe(0);
    expect(n('=NUMBERVALUE("1;234,5",",;",";")')).toBe(1234.5);
  });

  it("is #VALUE! for a second decimal separator or a group separator after it", () => {
    expect(s('=NUMBERVALUE("1.2.3")')).toBe("#VALUE!");
    expect(s('=NUMBERVALUE("1.5,5")')).toBe("#VALUE!");
    expect(s('=NUMBERVALUE("1.5","," )')).toBe("#VALUE!");
  });

  it("is #VALUE! for text that is not a number", () => {
    for (const text of ["abc", "0x10", "Infinity", "1e", "--1", "1_000"]) {
      expect(s(`=NUMBERVALUE("${text}")`), text).toBe("#VALUE!");
    }
  });

  it("keeps the existing behaviour for plain locale cases", () => {
    expect(n('=NUMBERVALUE("1,234.5")')).toBe(1234.5);
    expect(n('=NUMBERVALUE("3.5",".")')).toBe(3.5);
    expect(n('=NUMBERVALUE("1.234,5",",",".")')).toBe(1234.5);
    expect(n('=NUMBERVALUE("-12")')).toBe(-12);
    expect(n('=NUMBERVALUE("1E3")')).toBe(1000);
  });
});

describe("TEXTBEFORE and TEXTAFTER", () => {
  const hood = '"Red riding hood\'s, red hood"';

  it("follow the Microsoft examples", () => {
    expect(s(`=TEXTBEFORE(${hood},"hood")`)).toBe("Red riding ");
    expect(s(`=TEXTBEFORE(${hood},"hood",2)`)).toBe("Red riding hood's, red ");
    expect(s(`=TEXTBEFORE(${hood},"hood",-2)`)).toBe("Red riding ");
    expect(s(`=TEXTAFTER(${hood},"hood")`)).toBe("'s, red hood");
    expect(s(`=TEXTAFTER(${hood},"hood",2)`)).toBe("");
    expect(s(`=TEXTAFTER(${hood},"hood",-2)`)).toBe("'s, red hood");
  });

  it("match_mode 1 ignores case", () => {
    expect(s(`=TEXTBEFORE(${hood},"HOOD")`)).toBe("#N/A");
    expect(s(`=TEXTBEFORE(${hood},"HOOD",,1)`)).toBe("Red riding ");
    expect(s(`=TEXTAFTER(${hood},"HOOD",,1)`)).toBe("'s, red hood");
    expect(s(`=TEXTAFTER(${hood},"HOOD",,0)`)).toBe("#N/A");
  });

  it("match_end 1 treats the end (or, counting backwards, the start) as a delimiter", () => {
    expect(s(`=TEXTBEFORE(${hood},"hood",3,,1)`)).toBe("Red riding hood's, red hood");
    expect(s(`=TEXTBEFORE(${hood},"hood",3)`)).toBe("#N/A");
    expect(s(`=TEXTAFTER(${hood},"hood",3,,1)`)).toBe("");
    expect(s('=TEXTBEFORE("a-b","-",-2,,1)')).toBe("");
    expect(s('=TEXTAFTER("a-b","-",-2,,1)')).toBe("a-b");
  });

  it("if_not_found replaces #N/A, including an empty string", () => {
    expect(s('=TEXTBEFORE("abc","x",,,,"none")')).toBe("none");
    expect(s('=TEXTAFTER("abc","x",,,,"none")')).toBe("none");
    expect(s('=TEXTBEFORE("abc","x",,,,"")')).toBe("");
    expect(s('=TEXTBEFORE("abc","b",,,,"none")')).toBe("a");
  });

  it("an instance past the last delimiter is #N/A; 0 or longer than the text is #VALUE!", () => {
    expect(s('=TEXTBEFORE("a-b-c","-",3)')).toBe("#N/A");
    expect(s('=TEXTAFTER("a-b-c","-",-3)')).toBe("#N/A");
    expect(s('=TEXTBEFORE("a-b","-",0)')).toBe("#VALUE!");
    expect(s('=TEXTAFTER("a-b","-",9)')).toBe("#VALUE!");
    expect(s('=TEXTAFTER("a-b","-",-9)')).toBe("#VALUE!");
  });

  it("an empty delimiter matches at the start (or the end for a negative instance)", () => {
    expect(s('=TEXTBEFORE("abc","")')).toBe("");
    expect(s('=TEXTAFTER("abc","")')).toBe("abc");
    expect(s('=TEXTBEFORE("abc","",-1)')).toBe("abc");
    expect(s('=TEXTAFTER("abc","",-1)')).toBe("");
  });

  it("accept a bad match mode only as 0 or 1", () => {
    expect(s('=TEXTBEFORE("a-b","-",,2)')).toBe("#VALUE!");
    expect(s('=TEXTBEFORE("a-b","-",,,2)')).toBe("#VALUE!");
  });

  it("keep working as before for the plain forms", () => {
    expect(s('=TEXTBEFORE("a-b-c","-")')).toBe("a");
    expect(s('=TEXTBEFORE("a-b-c","-",2)')).toBe("a-b");
    expect(s('=TEXTAFTER("report.pdf",".")')).toBe("pdf");
    expect(s('=TEXTAFTER("a.b.c",".",-1)')).toBe("c");
    expect(s('=TEXTBEFORE("a.b.c",".",-1)')).toBe("a.b");
  });
});

describe("CLEAN", () => {
  it("removes every control character 0-31, tab and line break included (Microsoft example)", () => {
    // Microsoft: =CLEAN(CHAR(9)&"Monthly report"&CHAR(10)) is "Monthly report".
    expect(s('=CLEAN(CHAR(9)&"Monthly report"&CHAR(10))')).toBe("Monthly report");
    expect(s('=CLEAN("a"&CHAR(13)&CHAR(10)&"b"&CHAR(1)&"c"&CHAR(31))')).toBe("abc");
  });

  it("keeps printable text, spaces and non-ASCII characters", () => {
    expect(s('=CLEAN("Héllo wörld ★")')).toBe("Héllo wörld ★");
  });
});

describe("UNICHAR and UNICODE", () => {
  it("UNICHAR follows the Microsoft examples", () => {
    expect(s("=UNICHAR(66)")).toBe("B");
    expect(s("=UNICHAR(32)")).toBe(" ");
    expect(s("=UNICHAR(9733)")).toBe("★");
    expect(s("=UNICHAR(128512)")).toBe("\u{1f600}");
  });

  it("UNICHAR is #VALUE! for 0 and past the last code point, #N/A for a surrogate half", () => {
    expect(s("=UNICHAR(0)")).toBe("#VALUE!");
    expect(s("=UNICHAR(-5)")).toBe("#VALUE!");
    expect(s("=UNICHAR(1114112)")).toBe("#VALUE!");
    expect(s("=UNICHAR(55296)")).toBe("#N/A");
    expect(s("=UNICHAR(57343)")).toBe("#N/A");
    expect(s("=UNICHAR(55295)")).toBe("퟿");
  });

  it("UNICODE returns the first code point", () => {
    expect(n('=UNICODE("B")')).toBe(66);
    expect(n('=UNICODE("★ star")')).toBe(9733);
    expect(n('=UNICODE("\u{1f600}")')).toBe(128512);
    expect(s('=UNICODE("")')).toBe("#VALUE!");
  });
});

describe("N and T", () => {
  const values = { A1: 7, A2: "7", A3: true, A4: "text" };

  it("N turns numbers and booleans into numbers and everything else into 0", () => {
    expect(n("=N(7)")).toBe(7);
    expect(n("=N(TRUE)")).toBe(1);
    expect(n("=N(FALSE)")).toBe(0);
    expect(value("=N(A1)", values)).toBe(7);
    expect(value("=N(A2)", values)).toBe(0);
    expect(value("=N(A3)", values)).toBe(1);
    expect(value("=N(A4)", values)).toBe(0);
    expect(value("=N(Z9)", values)).toBe(0);
  });

  it("N passes an error through", () => {
    expect(s("=N(#N/A)")).toBe("#N/A");
    expect(s("=N(1/0)")).toBe("#DIV/0!");
  });

  it("T keeps text and turns everything else into an empty string", () => {
    expect(s('=T("hello")')).toBe("hello");
    expect(s("=T(5)")).toBe("");
    expect(s("=T(TRUE)")).toBe("");
    expect(s("=T(A1)", values)).toBe("");
    expect(s("=T(A4)", values)).toBe("text");
    expect(s("=T(Z9)", values)).toBe("");
  });

  it("T passes an error through", () => {
    expect(s("=T(#N/A)")).toBe("#N/A");
    expect(s("=T(1/0)")).toBe("#DIV/0!");
  });
});

describe("registration", () => {
  it("lists the new functions with a signature for the picker", () => {
    for (const name of ["DOLLAR", "FIXED", "ARRAYTOTEXT"]) {
      const spec = lookupFunction(name);
      expect(spec, name).toBeDefined();
      expect(spec?.signature, name).toMatch(new RegExp(`^${name}\\(`));
      expect(spec?.category, name).toBe("Text");
    }
  });

  it("documents the optional arguments of TEXTBEFORE and TEXTAFTER", () => {
    for (const name of ["TEXTBEFORE", "TEXTAFTER"]) {
      expect(lookupFunction(name)?.signature, name).toContain("[if_not_found]");
    }
  });
});
