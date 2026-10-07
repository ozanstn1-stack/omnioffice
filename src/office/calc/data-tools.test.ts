/**
 * Data tools: text to columns, duplicate detection and list validation items.
 */
import { describe, expect, it } from "vitest";
import { FormulaError } from "./formula";
import { delimiterText, findDuplicateRows, listValidationItems, planTextToColumns, splitDelimited } from "./data-tools";

describe("splitDelimited", () => {
  it("splits on each built-in delimiter", () => {
    expect(splitDelimited("a,b,c", { delimiter: "comma" })).toEqual(["a", "b", "c"]);
    expect(splitDelimited("a;b;c", { delimiter: "semicolon" })).toEqual(["a", "b", "c"]);
    expect(splitDelimited("a\tb\tc", { delimiter: "tab" })).toEqual(["a", "b", "c"]);
    expect(splitDelimited("a b c", { delimiter: "space" })).toEqual(["a", "b", "c"]);
    // Only the chosen delimiter splits.
    expect(splitDelimited("a;b,c", { delimiter: "comma" })).toEqual(["a;b", "c"]);
  });

  it("splits on a custom delimiter of one or more characters", () => {
    expect(splitDelimited("a|b|c", { delimiter: "custom", custom: "|" })).toEqual(["a", "b", "c"]);
    expect(splitDelimited("a::b:c", { delimiter: "custom", custom: "::" })).toEqual(["a", "b:c"]);
    // An empty custom delimiter does not split at all.
    expect(splitDelimited("a,b", { delimiter: "custom", custom: "" })).toEqual(["a,b"]);
    expect(delimiterText({ delimiter: "custom" })).toBe("");
    expect(delimiterText({ delimiter: "tab" })).toBe("\t");
  });

  it("keeps empty pieces unless consecutive delimiters count as one", () => {
    expect(splitDelimited("a,,b,", { delimiter: "comma" })).toEqual(["a", "", "b", ""]);
    expect(splitDelimited("a,,b,", { delimiter: "comma", mergeConsecutive: true })).toEqual(["a", "b"]);
    expect(splitDelimited("  a   b ", { delimiter: "space", mergeConsecutive: true })).toEqual(["a", "b"]);
    expect(splitDelimited("", { delimiter: "comma" })).toEqual([""]);
    expect(splitDelimited("", { delimiter: "comma", mergeConsecutive: true })).toEqual([]);
  });

  it("reads quoted pieces that contain the delimiter", () => {
    expect(splitDelimited('a,"b,c",d', { delimiter: "comma" })).toEqual(["a", "b,c", "d"]);
    // A doubled quote inside a quoted piece is a literal quote.
    expect(splitDelimited('"say ""hi""",x', { delimiter: "comma" })).toEqual(['say "hi"', "x"]);
    // An explicitly quoted empty piece survives merging.
    expect(splitDelimited('a,"",b', { delimiter: "comma", mergeConsecutive: true })).toEqual(["a", "", "b"]);
    // Quotes in the middle of a piece and unterminated quotes are plain text.
    expect(splitDelimited('5" disk,x', { delimiter: "comma" })).toEqual(['5" disk', "x"]);
    expect(splitDelimited('"open,end', { delimiter: "comma" })).toEqual(['"open', "end"]);
    expect(splitDelimited('"a,b",c', { delimiter: "comma", quote: null })).toEqual(['"a', 'b"', "c"]);
  });
});

describe("planTextToColumns", () => {
  it("returns per-row pieces and the widest row", () => {
    const plan = planTextToColumns(["Ada,Lovelace,1815", "Alan,Turing", "Grace"], { delimiter: "comma" });
    expect(plan.width).toBe(3);
    expect(plan.rows).toEqual([["Ada", "Lovelace", "1815"], ["Alan", "Turing"], null]);
  });

  it("leaves rows whose split changes nothing", () => {
    const plan = planTextToColumns(["12.5", "", '"quoted"'], { delimiter: "comma" });
    expect(plan.rows).toEqual([null, null, ["quoted"]]);
    expect(plan.width).toBe(1);
  });
});

describe("findDuplicateRows", () => {
  const rows = [
    ["Name", "City", "Age"],
    ["Ada", "London", 36],
    ["ada", "LONDON", 36],
    ["Ada", "Paris", 36],
    ["Alan", "London", 41],
    ["Ada", "London", 37],
  ];

  it("compares every chosen column, case-insensitively, keeping the first row", () => {
    expect(findDuplicateRows(rows, { columns: [0, 1, 2], hasHeaders: true })).toEqual({
      keep: [0, 1, 3, 4, 5],
      removed: [2],
    });
  });

  it("compares only a subset of columns", () => {
    expect(findDuplicateRows(rows, { columns: [0], hasHeaders: true })).toEqual({
      keep: [0, 1, 4],
      removed: [2, 3, 5],
    });
    expect(findDuplicateRows(rows, { columns: [1], hasHeaders: true })).toEqual({
      keep: [0, 1, 3],
      removed: [2, 4, 5],
    });
  });

  it("treats the first row as data without headers", () => {
    const data = [["x"], ["X"], ["y"], ["x"]];
    expect(findDuplicateRows(data, { columns: [0], hasHeaders: false })).toEqual({ keep: [0, 2], removed: [1, 3] });
    // With headers the first row is never compared, so its twin stays.
    expect(findDuplicateRows(data, { columns: [0], hasHeaders: true })).toEqual({ keep: [0, 1, 2], removed: [3] });
  });

  it("keeps numbers and their text twins apart and treats blank rows as equal", () => {
    const data = [[1], ["1"], [""], [], [new FormulaError("#N/A")], [new FormulaError("#N/A")]];
    expect(findDuplicateRows(data, { columns: [0], hasHeaders: false })).toEqual({
      keep: [0, 1, 2, 4],
      removed: [3, 5],
    });
  });

  it("removes nothing when no column is chosen", () => {
    expect(findDuplicateRows([["a"], ["a"]], { columns: [], hasHeaders: false })).toEqual({
      keep: [0, 1],
      removed: [],
    });
  });
});

describe("listValidationItems", () => {
  const none = () => null;

  it("cleans an inline list", () => {
    expect(listValidationItems(["Open", " In progress ", "", "Done", "open"], none)).toEqual([
      "Open",
      "In progress",
      "Done",
    ]);
  });

  it("reads a single reference entry from cells", () => {
    const calls: Array<[string | null, string]> = [];
    const resolve = (sheet: string | null, range: string) => {
      calls.push([sheet, range]);
      return ["Red", 2, "", "Red", true];
    };
    expect(listValidationItems(["=$A$1:$A$5"], resolve)).toEqual(["Red", "2", "TRUE"]);
    expect(listValidationItems(["=Lists!B2:B9"], resolve)).toEqual(["Red", "2", "TRUE"]);
    expect(listValidationItems(["='My lists'!C1"], resolve)).toEqual(["Red", "2", "TRUE"]);
    expect(calls).toEqual([
      [null, "$A$1:$A$5"],
      ["Lists", "B2:B9"],
      ["My lists", "C1"],
    ]);
    // An unresolvable reference offers no choices; a non-reference is literal.
    expect(listValidationItems(["=A1:A3"], none)).toEqual([]);
    expect(listValidationItems(["=1+2"], none)).toEqual(["=1+2"]);
  });
});
