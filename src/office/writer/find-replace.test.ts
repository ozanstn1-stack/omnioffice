/**
 * Find & replace works on the model, not on the rendered page, so the rules
 * that matter to users are locked down here: regular expressions with `$1`
 * groups, match case and whole word in every combination, invalid patterns
 * that report instead of throwing, run formatting that survives a replacement,
 * and the document order Find next / Replace walk in.
 */
import { describe, expect, it } from "vitest";
import { defaultParaProps, newTextDocument, type Block, type Run, type TextDocument } from "../../lib/office-types";
import { emptyRun, runsText } from "./runs";
import {
  compileSearch,
  documentMatches,
  expandReplacement,
  findInText,
  nextMatchIndex,
  replaceAllInDocument,
  replaceAllInRuns,
  replaceMatch,
  sameMatch,
  spliceRuns,
  type FindOptions,
} from "./find-replace";

function run(text: string, extra: Partial<Run> = {}): Run {
  return { ...emptyRun(text), ...extra };
}

function para(...runs: Run[]): Block {
  return { type: "paragraph", props: defaultParaProps(), runs };
}

function cellTable(texts: string[][]): Block {
  return {
    type: "table",
    table: {
      rows: texts.map((cells) => ({
        cells: cells.map((text) => ({
          blocks: [para(run(text))],
          colspan: 1,
          rowspan: 1,
          background: null,
          align: "left",
          valign: "top",
          widthPt: null,
        })),
        heightPt: null,
        header: false,
      })),
      columnWidthsPt: texts[0].map(() => 100),
      borders: true,
      borderColor: "#000000",
      align: "left",
    },
  };
}

function doc(blocks: Block[], header: Block[] = [], footer: Block[] = []): TextDocument {
  return { ...newTextDocument("Test"), blocks, header, footer };
}

const PLAIN: FindOptions = { matchCase: false, wholeWord: false, regex: false };

function pattern(query: string, options: Partial<FindOptions> = {}): RegExp {
  const compiled = compileSearch(query, { ...PLAIN, ...options });
  if (!compiled?.ok) throw new Error(`pattern did not compile: ${query}`);
  return compiled.pattern;
}

function texts(text: string, query: string, options: Partial<FindOptions> = {}): string[] {
  return findInText(text, pattern(query, options)).map((match) => match[0]);
}

function paragraphText(block: Block | undefined): string {
  return block?.type === "paragraph" ? runsText(block.runs) : "";
}

describe("compileSearch", () => {
  it("returns null for an empty query", () => {
    expect(compileSearch("", PLAIN)).toBeNull();
  });

  it("matches literal text without interpreting regex syntax", () => {
    expect(texts("Cost (net): 3.50 + tax", "(net)")).toEqual(["(net)"]);
    expect(texts("a.b axb", "a.b")).toEqual(["a.b"]);
  });

  it("ignores case unless match case is on", () => {
    expect(texts("Word word WORD", "word")).toEqual(["Word", "word", "WORD"]);
    expect(texts("Word word WORD", "word", { matchCase: true })).toEqual(["word"]);
  });

  it("keeps whole word Unicode aware, so Turkish words match", () => {
    expect(texts("çay çaydanlık çay", "çay", { wholeWord: true })).toEqual(["çay", "çay"]);
    expect(texts("cat concat cat_x cat", "cat", { wholeWord: true })).toEqual(["cat", "cat"]);
  });

  it("combines regex with match case and whole word", () => {
    expect(texts("Item1 item2 items3", "item\\d", { regex: true })).toEqual(["Item1", "item2"]);
    expect(texts("Item1 item2 items3", "item\\d", { regex: true, matchCase: true })).toEqual(["item2"]);
    expect(texts("cat category scatter cat", "cat\\w*", { regex: true, wholeWord: true })).toEqual([
      "cat",
      "category",
      "cat",
    ]);
    // An alternation stays inside the word boundaries.
    expect(texts("one bone two", "one|two", { regex: true, wholeWord: true })).toEqual(["one", "two"]);
  });

  it("reports an invalid regular expression instead of throwing", () => {
    for (const query of ["(", "[a-", "a)|(b", "*"]) {
      const compiled = compileSearch(query, { ...PLAIN, regex: true });
      expect(compiled?.ok, query).toBe(false);
      if (compiled && !compiled.ok) expect(compiled.error.length).toBeGreaterThan(0);
    }
    // The same text is fine as a literal search.
    expect(compileSearch("(", PLAIN)?.ok).toBe(true);
  });

  it("skips zero-length matches", () => {
    expect(texts("abc", "^", { regex: true })).toEqual([]);
    expect(texts("aa b aaa", "a*", { regex: true })).toEqual(["aa", "aaa"]);
  });
});

describe("expandReplacement", () => {
  const cases: Array<[string, string, string]> = [
    ["(\\d{4})-(\\d{2})-(\\d{2})", "2026-10-06", "$3.$2.$1"],
    ["(b)", "abc", "[$&|$`|$'|$$|$1|$2|$0]"],
    ["(?<word>b)", "abc", "<$<word>|$<missing>|$<word"],
    ["(a)(b)(c)(d)(e)(f)(g)(h)(i)(j)(k)", "abcdefghijk", "$11-$10-$1x-$01"],
    ["(a)", "a", "$10"],
    ["b", "abc", "$1$<x>"],
  ];

  it.each(cases)("expands /%s/ on %s like String#replace", (source, input, template) => {
    const regex = new RegExp(source, "u");
    const match = input.match(regex);
    if (!match) throw new Error("no match");
    const start = match.index ?? 0;
    const expected = input.replace(regex, template);
    const actual = input.slice(0, start) + expandReplacement(template, match) + input.slice(start + match[0].length);
    expect(actual).toBe(expected);
  });
});

describe("run formatting", () => {
  it("keeps formatting outside the match and gives the replacement the first matched run's format", () => {
    const runs = [run("Hello "), run("big", { bold: true }), run(" world", { italic: true })];
    const shape = (result: Run[]) => result.map((entry) => [entry.text, entry.bold, entry.italic]);
    expect(shape(spliceRuns(runs, 6, 9, "XY"))).toEqual([
      ["Hello ", false, false],
      ["XY", true, false],
      [" world", false, true],
    ]);
    // A match that starts in the plain run and ends inside the bold one.
    expect(shape(spliceRuns(runs, 4, 7, "XY"))).toEqual([
      ["HellXY", false, false],
      ["ig", true, false],
      [" world", false, true],
    ]);
  });

  it("matches across runs and keeps note anchors inside a replaced range", () => {
    const note = run("", { footnote: "n1" });
    const runs = [run("foo", { bold: true }), note, run("bar")];
    const result = replaceAllInRuns(runs, pattern("foobar"), "baz", false);
    expect(result.count).toBe(1);
    expect(runsText(result.runs)).toBe("baz");
    expect(result.runs[0]).toMatchObject({ text: "baz", bold: true });
    expect(result.runs.some((entry) => entry.footnote === "n1")).toBe(true);
  });

  it("replaces inside one run exactly like the old per-run Replace All", () => {
    const runs = [run("Date: ", { bold: true }), run("2026-10-06", { italic: true }), run(" end")];
    const result = replaceAllInRuns(runs, pattern("(\\d{4})-(\\d{2})-(\\d{2})", { regex: true }), "$3.$2.$1", true);
    expect(result.runs.map((entry) => [entry.text, entry.bold, entry.italic])).toEqual([
      ["Date: ", true, false],
      ["06.10.2026", false, true],
      [" end", false, false],
    ]);
  });

  it("treats the replacement literally outside regex mode", () => {
    const result = replaceAllInRuns([run("a-b")], pattern("-"), "$&$1", false);
    expect(runsText(result.runs)).toBe("a$&$1b");
  });

  it("anchors ^ and $ to the paragraph, not to each run", () => {
    const runs = [run("ab", { bold: true }), run("ab")];
    const result = replaceAllInRuns(runs, pattern("^ab", { regex: true }), "X", true);
    expect(runsText(result.runs)).toBe("Xab");
  });
});

describe("document matches and replacement", () => {
  const document = doc(
    [para(run("cat and cat")), cellTable([["a cat", "dog"]]), para(run("no match"))],
    [para(run("Header cat"))],
    [para(run("Footer cat"))],
  );

  it("finds matches in the body, table cells, header and footer in document order", () => {
    const matches = documentMatches(document, pattern("cat"));
    expect(matches.map((match) => [match.scope, match.path, match.start])).toEqual([
      ["body", [0], 0],
      ["body", [0], 8],
      ["body", [1, 0, 0, 0], 2],
      ["header", [0], 7],
      ["footer", [0], 7],
    ]);
  });

  it("stops counting at the limit", () => {
    expect(documentMatches(document, pattern("cat"), 2)).toHaveLength(2);
  });

  it("replaces everywhere and counts the replacements", () => {
    const result = replaceAllInDocument(document, pattern("cat"), "cow", false);
    expect(result.count).toBe(5);
    expect(paragraphText(result.document.blocks[0])).toBe("cow and cow");
    const table = result.document.blocks[1];
    expect(table.type === "table" ? paragraphText(table.table.rows[0].cells[0].blocks[0]) : "").toBe("a cow");
    expect(paragraphText(result.document.header[0])).toBe("Header cow");
    expect(paragraphText(result.document.footer[0])).toBe("Footer cow");
    // Blocks without a match are not rebuilt.
    expect(result.document.blocks[2]).toBe(document.blocks[2]);
  });

  it("leaves the document untouched when nothing matches", () => {
    const result = replaceAllInDocument(document, pattern("zebra"), "x", false);
    expect(result.count).toBe(0);
    expect(result.document).toBe(document);
  });

  it("replaces a single match in a table cell and reports where to continue", () => {
    const matches = documentMatches(document, pattern("cat"));
    const result = replaceMatch(document, matches[2], pattern("cat"), "lion", false);
    expect(result?.end).toBe(6);
    const table = result?.document.blocks[1];
    expect(table?.type === "table" ? paragraphText(table.table.rows[0].cells[0].blocks[0]) : "").toBe("a lion");
    expect(result?.document.blocks[0]).toBe(document.blocks[0]);
  });

  it("refuses a stale location", () => {
    const stale = { scope: "body" as const, path: [0], start: 1, end: 4 };
    expect(replaceMatch(document, stale, pattern("cat"), "x", false)).toBeNull();
  });

  it("walks forward and backward with wrap-around", () => {
    const matches = documentMatches(document, pattern("cat"));
    expect(nextMatchIndex(matches, null, true)).toBe(0);
    expect(nextMatchIndex(matches, null, false)).toBe(4);
    expect(nextMatchIndex(matches, matches[1], true)).toBe(2);
    expect(nextMatchIndex(matches, matches[4], true)).toBe(0);
    expect(nextMatchIndex(matches, matches[0], false)).toBe(4);
    expect(nextMatchIndex(matches, matches[3], false)).toBe(2);
    // After a replacement the search continues at the end of the new text.
    expect(nextMatchIndex(matches, { scope: "body", path: [0], start: 8 }, true, true)).toBe(1);
    expect(nextMatchIndex([], null, true)).toBe(-1);
    expect(sameMatch(matches[0], { ...matches[0] })).toBe(true);
    expect(sameMatch(matches[0], matches[1])).toBe(false);
  });
});
