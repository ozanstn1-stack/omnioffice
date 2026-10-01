/**
 * Regression tests for the Writer's structural editing.
 *
 * Before these, pressing Enter inside a paragraph produced no new block at all
 * (the browser's own `<div>` was flattened back into a single run), and
 * Backspace at the start of a paragraph did nothing. These lock down the pure
 * run/offset logic those keys are built from.
 */
import { describe, expect, it } from "vitest";
import { defaultParaProps, type Run } from "../../lib/office-types";
import {
  emptyRun,
  formatAtOffset,
  insertText,
  joinRuns,
  lineCount,
  lineEndOffset,
  lineStartOffset,
  nextListLevel,
  nextParagraphProps,
  normalizeParagraphRuns,
  normalizeRuns,
  replaceRange,
  runsText,
  sameFormat,
  splitAtLineBreak,
  splitRuns,
  wordRangeAt,
} from "./runs";
import { domToRuns, runsToHtml, safeHref, textToBlocks } from "./writerDom";

function run(text: string, extra: Partial<Run> = {}): Run {
  return { ...emptyRun(text), ...extra };
}

describe("normalizeRuns", () => {
  it("drops empty runs", () => {
    expect(normalizeRuns([run(""), run("a"), run(""), run("b")])).toHaveLength(1);
  });

  it("merges neighbours that share formatting", () => {
    const merged = normalizeRuns([run("Hel"), run("lo"), run(" world")]);
    expect(merged).toHaveLength(1);
    expect(merged[0].text).toBe("Hello world");
  });

  it("keeps runs apart when formatting differs", () => {
    const merged = normalizeRuns([run("plain"), run("bold", { bold: true })]);
    expect(merged).toHaveLength(2);
  });

  it("merges around a differently formatted run in the middle", () => {
    const merged = normalizeRuns([run("a"), run("b", { italic: true }), run("c")]);
    expect(merged.map((entry) => entry.text)).toEqual(["a", "b", "c"]);
  });

  it("never returns an empty paragraph", () => {
    expect(normalizeParagraphRuns([])).toHaveLength(1);
    expect(normalizeParagraphRuns([run("")])[0].text).toBe("");
  });
});

describe("splitRuns", () => {
  it("splits inside a single run and keeps formatting on both halves", () => {
    const [left, right] = splitRuns([run("Hello world", { bold: true })], 5);
    expect(runsText(left)).toBe("Hello");
    expect(runsText(right)).toBe(" world");
    expect(left[0].bold).toBe(true);
    expect(right[0].bold).toBe(true);
  });

  it("splits on a run boundary without creating empty runs", () => {
    const [left, right] = splitRuns([run("ab"), run("cd")], 2);
    expect(runsText(left)).toBe("ab");
    expect(runsText(right)).toBe("cd");
    expect(left.every((entry) => entry.text !== "")).toBe(true);
  });

  it("produces an empty right side at the end of the text", () => {
    const [left, right] = splitRuns([run("abc")], 3);
    expect(runsText(left)).toBe("abc");
    expect(runsText(right)).toBe("");
  });

  it("produces an empty left side at offset zero", () => {
    const [left, right] = splitRuns([run("abc")], 0);
    expect(runsText(left)).toBe("");
    expect(runsText(right)).toBe("abc");
  });

  it("clamps an offset past the end", () => {
    const [left, right] = splitRuns([run("abc")], 99);
    expect(runsText(left)).toBe("abc");
    expect(runsText(right)).toBe("");
  });

  it("handles a negative offset", () => {
    const [left, right] = splitRuns([run("abc")], -5);
    expect(runsText(left)).toBe("");
    expect(runsText(right)).toBe("abc");
  });
});

describe("joinRuns (Backspace / Delete merge)", () => {
  it("concatenates two paragraphs", () => {
    expect(runsText(joinRuns([run("Hello ")], [run("world")]))).toBe("Hello world");
  });

  it("merges the boundary runs when the formatting matches", () => {
    const joined = joinRuns([run("Hello ", { bold: true })], [run("world", { bold: true })]);
    expect(joined).toHaveLength(1);
  });

  it("keeps differing formatting separate", () => {
    const joined = joinRuns([run("a")], [run("b", { italic: true })]);
    expect(joined).toHaveLength(2);
  });

  it("survives an empty side", () => {
    expect(runsText(joinRuns([], [run("only")]))).toBe("only");
    expect(runsText(joinRuns([run("only")], []))).toBe("only");
  });
});

describe("insertText", () => {
  it("inherits the formatting of the run before the caret", () => {
    const out = insertText([run("Hello", { bold: true }), run(" world")], 5, " there");
    const inserted = out.find((entry) => entry.text.includes(" there"));
    expect(inserted?.bold).toBe(true);
    expect(runsText(out)).toBe("Hello there world");
  });

  it("uses the first run's formatting at offset zero", () => {
    const out = insertText([run("abc", { italic: true })], 0, "X");
    expect(out[0].italic).toBe(true);
    expect(runsText(out)).toBe("Xabc");
  });
});

describe("replaceRange", () => {
  it("deletes a selection across two runs", () => {
    const runs = [run("Hello "), run("world")];
    expect(runsText(replaceRange(runs, 0, 11, ""))).toBe("");
  });

  it("deletes a word in the middle", () => {
    expect(runsText(replaceRange([run("the quick brown")], 4, 9, ""))).toBe("the  brown");
  });

  it("replaces a selection with new text", () => {
    expect(runsText(replaceRange([run("Hello world")], 6, 11, "there"))).toBe("Hello there");
  });
});

describe("wordRangeAt", () => {
  const runs = [run("alpha beta gamma")];

  it("finds the word before the caret", () => {
    expect(wordRangeAt(runs, 9, "backward")).toEqual([6, 9]);
  });

  it("skips whitespace going backwards", () => {
    expect(wordRangeAt(runs, 10, "backward")).toEqual([6, 10]);
  });

  it("finds the word after the caret", () => {
    expect(wordRangeAt(runs, 3, "forward")).toEqual([3, 5]);
  });

  it("stays in range at the edges", () => {
    expect(wordRangeAt(runs, 0, "backward")).toEqual([0, 0]);
    expect(wordRangeAt(runs, 16, "forward")).toEqual([16, 16]);
  });

  it("treats digits and underscores as one word", () => {
    // `a1_b2` is a single identifier-like token, so Backspace clears all of it.
    expect(wordRangeAt([run("a1_b2")], 4, "backward")).toEqual([0, 4]);
    expect(wordRangeAt([run("id = a1_b2")], 10, "backward")).toEqual([5, 10]);
  });

  it("stops at punctuation", () => {
    expect(wordRangeAt([run("one,two")], 7, "backward")).toEqual([4, 7]);
  });
});

describe("splitAtLineBreak (Shift+Enter)", () => {
  it("splits at the offset when there is no earlier break", () => {
    const [left, right, cut] = splitAtLineBreak([run("abcdef")], 3);
    expect(runsText(left)).toBe("abc");
    expect(runsText(right)).toBe("def");
    expect(cut).toBe(3);
  });

  it("splits after the previous hard break", () => {
    const [left, right, cut] = splitAtLineBreak([run("one\ntwo\nthree")], 9);
    expect(runsText(left)).toBe("one\ntwo\n");
    expect(runsText(right)).toBe("three");
    expect(cut).toBe(8);
  });

  it("handles a caret right after a break", () => {
    const [left, right] = splitAtLineBreak([run("one\ntwo")], 4);
    expect(runsText(left)).toBe("one\n");
    expect(runsText(right)).toBe("two");
  });
});

describe("line helpers", () => {
  it("counts visual lines", () => {
    expect(lineCount([run("")])).toBe(1);
    expect(lineCount([run("one line")])).toBe(1);
    expect(lineCount([run("one\ntwo\nthree")])).toBe(3);
  });

  it("finds line boundaries", () => {
    const runs = [run("aa\nbb\ncc")];
    expect(lineStartOffset(runs, 0)).toBe(0);
    expect(lineStartOffset(runs, 1)).toBe(3);
    expect(lineStartOffset(runs, 2)).toBe(6);
    expect(lineEndOffset(runs, 0)).toBe(2);
    expect(lineEndOffset(runs, 1)).toBe(5);
    expect(lineEndOffset(runs, 2)).toBe(8);
  });
});

describe("nextParagraphProps", () => {
  it("keeps the style when there is text after the caret", () => {
    const props = { ...defaultParaProps(), style: "Heading1" };
    expect(nextParagraphProps(props, "Some text").style).toBe("Heading1");
  });

  it("falls back to Normal after an empty heading, like every word processor", () => {
    const props = { ...defaultParaProps(), style: "Heading2" };
    expect(nextParagraphProps(props, "   ").style).toBe("Normal");
  });

  it("ends a list when the new paragraph is empty", () => {
    const props = { ...defaultParaProps(), list: { kind: "bullet" as const, level: 0, start: 1, marker: "•" } };
    expect(nextParagraphProps(props, "").list).toBeNull();
  });

  it("continues a list when there is trailing text", () => {
    const props = { ...defaultParaProps(), list: { kind: "number" as const, level: 0, start: 1, marker: "1." } };
    expect(nextParagraphProps(props, "first").list).toEqual(props.list);
  });
});

describe("nextListLevel", () => {
  const listProps = { ...defaultParaProps(), list: { kind: "bullet" as const, level: 0, start: 1, marker: "•" } };

  it("indents one level", () => {
    expect(nextListLevel(listProps, 1).list?.level).toBe(1);
  });

  it("clamps at level 0 when outdenting further", () => {
    expect(nextListLevel(listProps, -1).list?.level).toBe(0);
    expect(nextListLevel(listProps, -5).list?.level).toBe(0);
  });

  it("clamps at level 8", () => {
    const deep = { ...defaultParaProps(), list: { kind: "bullet" as const, level: 8, start: 1, marker: "•" } };
    expect(nextListLevel(deep, 1).list?.level).toBe(8);
  });

  it("is a no-op without a list", () => {
    const plain = defaultParaProps();
    expect(nextListLevel(plain, 1)).toBe(plain);
  });
});

describe("formatAtOffset", () => {
  it("inherits the preceding run", () => {
    const runs = [run("plain"), run("bold", { bold: true })];
    expect(formatAtOffset(runs, 8).bold).toBe(true);
  });

  it("inherits the first run at offset zero", () => {
    expect(formatAtOffset([run("x", { italic: true })], 0).italic).toBe(true);
  });
});

describe("runsToHtml / domToRuns round trip", () => {
  function toDom(html: string): HTMLElement {
    const element = document.createElement("div");
    element.innerHTML = html;
    return element;
  }

  it("preserves plain text", () => {
    expect(runsText(domToRuns(toDom(runsToHtml([run("Hello world")]))))).toBe("Hello world");
  });

  it("preserves bold, italic, underline and strike", () => {
    const source = [
      run("b", { bold: true }),
      run("i", { italic: true }),
      run("u", { underline: true }),
      run("s", { strike: true }),
    ];
    const parsed = domToRuns(toDom(runsToHtml(source)));
    expect(parsed[0].bold).toBe(true);
    expect(parsed[1].italic).toBe(true);
    expect(parsed[2].underline).toBe(true);
    expect(parsed[3].strike).toBe(true);
  });

  it("preserves the font size", () => {
    const parsed = domToRuns(toDom(runsToHtml([run("x", { sizePt: 18 })])));
    expect(parsed[0].sizePt).toBe(18);
  });

  it("preserves the font family, which used to be lost on every save", () => {
    const parsed = domToRuns(toDom(runsToHtml([run("x", { font: "Georgia" })])));
    expect(parsed[0].font).toBe("Georgia");
  });

  it("preserves colour and highlight", () => {
    const parsed = domToRuns(toDom(runsToHtml([run("x", { color: "rgb(255, 0, 0)", highlight: "rgb(255, 255, 0)" })])));
    expect(parsed[0].color).toBe("rgb(255, 0, 0)");
    expect(parsed[0].highlight).toBe("rgb(255, 255, 0)");
  });

  it("preserves links", () => {
    const parsed = domToRuns(toDom(runsToHtml([run("site", { link: "https://example.com" })])));
    expect(parsed[0].link).toBe("https://example.com");
  });

  it("turns a br element into a hard line break run", () => {
    const parsed = domToRuns(toDom("one<br>two"));
    expect(runsText(parsed)).toBe("one\ntwo");
  });

  it("escapes HTML in the text", () => {
    expect(runsToHtml([run("<script>&")])).toContain("&lt;script&gt;&amp;");
  });

  it("returns a single empty run for an empty paragraph", () => {
    const parsed = domToRuns(toDom(""));
    expect(parsed).toHaveLength(1);
    expect(parsed[0].text).toBe("");
  });

  it("survives a full split/join cycle without losing formatting", () => {
    const source = [run("Hello ", { bold: true }), run("world", { italic: true })];
    const [left, right] = splitRuns(source, 6);
    const rejoined = joinRuns(left, right);
    expect(runsText(rejoined)).toBe("Hello world");
    expect(rejoined[0].bold).toBe(true);
    expect(rejoined[1].italic).toBe(true);
  });
});

describe("runsToHtml injection hardening", () => {
  function toDom(html: string): HTMLElement {
    const element = document.createElement("div");
    element.innerHTML = html;
    return element;
  }

  it("drops a javascript: link and keeps the run text", () => {
    const html = runsToHtml([run("click me", { link: "javascript:alert(1)" })]);
    expect(html).toBe("click me");
    expect(toDom(html).querySelector("a")).toBeNull();
    expect(html.toLowerCase()).not.toContain("javascript:");
  });

  it("drops a link that tries to break out of the href attribute", () => {
    const html = runsToHtml([run("link text", { link: ' x" onmouseover="alert(1) ' })]);
    expect(html).toBe("link text");
    expect(html).not.toContain("<a");
    expect(html.toLowerCase()).not.toContain("onmouseover");
  });

  it("escapes a crafted revision author instead of letting it open an attribute", () => {
    const html = runsToHtml([
      run("<b>bold</b>", {
        revision: {
          id: 'rev-1" onmouseover="alert(1)',
          kind: "insert",
          author: 'A" onmouseover="alert(1) <i>',
          date: '2024" onmouseover="alert(1)',
        },
      }),
    ]);
    // The literal payload text survives escaped, so the assertion that can
    // actually catch a breakout is the parsed DOM: no element may carry the
    // handler attribute, and the quote never closes an attribute early.
    expect(toDom(html).querySelector("[onmouseover]")).toBeNull();
    expect(html).not.toContain('" onmouseover="');
    expect(html).toContain('data-revision-id="rev-1&quot; onmouseover=&quot;alert(1)"');
    expect(html).toContain('title="A&quot; onmouseover=&quot;alert(1) &lt;i&gt;"');
    // Markup supplied as a text value is escaped rather than parsed.
    expect(html).toContain("&lt;b&gt;bold&lt;/b&gt;");
    expect(html).not.toContain("<b>");
  });

  it("drops a colour value that tries to open an attribute", () => {
    const html = runsToHtml([run("x", { color: ' red" onfocus="alert(1) ' })]);
    expect(html).toBe("x");
    expect(html.toLowerCase()).not.toContain("onfocus");
    expect(html).not.toContain("style=");
  });

  it("drops a font family that tries to open an attribute", () => {
    const html = runsToHtml([run("x", { font: " A'; onmouseover='alert(1) " })]);
    expect(html).toBe("x");
    expect(html.toLowerCase()).not.toContain("onmouseover");
    expect(html).not.toContain("font-family");
  });

  it("still renders https links, with the ampersand escaped", () => {
    const html = runsToHtml([run("site", { link: "https://example.com/a?b=1&c=2" })]);
    expect(html).toContain('<a href="https://example.com/a?b=1&amp;c=2" target="_blank" rel="noreferrer">site</a>');
    expect(domToRuns(toDom(html))[0].link).toBe("https://example.com/a?b=1&c=2");
  });

  it("still renders mailto: and same-document links", () => {
    expect(runsToHtml([run("mail", { link: "mailto:a@b.c" })])).toContain('<a href="mailto:a@b.c"');
    expect(runsToHtml([run("jump", { link: "#anchor" })])).toContain('<a href="#anchor"');
  });

  it("still renders hex and rgb colours", () => {
    expect(runsToHtml([run("x", { color: "#ff0000" })])).toContain("color:#ff0000");
    expect(runsToHtml([run("x", { color: "rgb(1, 2, 3)" })])).toContain("color:rgb(1, 2, 3)");
  });

  it("does not read an unsafe href back out of the DOM", () => {
    const parsed = domToRuns(toDom('<a href="javascript:alert(1)">safe text</a>'));
    expect(parsed[0].link).toBeNull();
    expect(runsText(parsed)).toBe("safe text");
  });

  it("trims whitespace and control characters around a safe href", () => {
    expect(safeHref("  \u0000https://example.com  ")).toBe("https://example.com");
    expect(safeHref("javascript\u0000:alert(1)")).toBeNull();
    expect(safeHref("data:text/html,<script>alert(1)</script>")).toBeNull();
    expect(safeHref("vbscript:msgbox(1)")).toBeNull();
    expect(safeHref("   ")).toBeNull();
  });
});

describe("textToBlocks", () => {
  it("makes one block per line", () => {
    expect(textToBlocks("a\nb\nc")).toHaveLength(3);
  });
});

describe("sameFormat", () => {
  it("ignores the text", () => {
    expect(sameFormat(run("a"), run("b"))).toBe(true);
    expect(sameFormat(run("a"), run("b", { bold: true }))).toBe(false);
  });
});

describe("structural run preservation", () => {
  it("keeps an empty footnote/field run through normalization", () => {
    // Regression: typing one character in a paragraph with a footnote marker
    // used to delete the note from the model (empty structural runs were
    // dropped by normalizeRuns).
    const runs = [run("before "), run("", { footnote: "note-1" }), run(" after")];
    const normalized = normalizeRuns(runs);
    expect(normalized.some((entry) => entry.footnote === "note-1")).toBe(true);
    const field = run("", { field: { kind: "page", target: "", cached: "1" } });
    expect(normalizeRuns([run("x"), field]).some((entry) => entry.field?.kind === "page")).toBe(true);
  });

  it("keeps the anchor when text is inserted next to it", () => {
    const runs = [run("ab"), run("", { footnote: "n1" })];
    expect(insertText(runs, 2, "X").some((entry) => entry.footnote === "n1")).toBe(true);
    expect(splitRuns(runs, 1)[1].some((entry) => entry.footnote === "n1")).toBe(true);
  });

  it("does not inherit a note or field when typing next to it", () => {
    const runs = [run("", { footnote: "n1" }), run("plain")];
    const template = formatAtOffset(runs, 0);
    expect(template.footnote ?? null).toBeNull();
    expect(template.field ?? null).toBeNull();
  });

  it("renders a hard line break as <br> and reads it back", () => {
    const html = runsToHtml([run("one\ntwo")]);
    expect(html).toContain("<br>");
    const element = document.createElement("div");
    element.innerHTML = html;
    expect(runsText(domToRuns(element))).toBe("one\ntwo");
  });

  it("hides a tracked deletion when revisions are off", () => {
    const deleted = run("old", { revision: { id: "r1", kind: "delete", author: "A", date: "" } });
    const kept = run("new", { revision: { id: "r2", kind: "insert", author: "A", date: "" } });
    const final = runsToHtml([deleted, kept], { showRevisions: false });
    expect(final).not.toContain("old");
    expect(final).toContain("new");
    expect(runsToHtml([deleted, kept], { showRevisions: true })).toContain("old");
  });
});
