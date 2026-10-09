import { describe, expect, it } from "vitest";
import type { Run, TextParagraph } from "../../lib/office-types";
import { emptyRun, runsText } from "../writer/runs";
import {
  applyRunFormat,
  domToParagraphRuns,
  formatAt,
  mergeParagraphs,
  paragraphHtml,
  paragraphRuns,
  paragraphText,
  remapRuns,
  splitParagraphAt,
  withBullet,
  withLevel,
  withParagraphFormat,
} from "./rich-text";

function run(text: string, extra: Partial<Run> = {}): Run {
  return { ...emptyRun(text), ...extra };
}

function para(text: string, extra: Partial<TextParagraph> = {}): TextParagraph {
  return {
    text,
    level: 0,
    bold: false,
    italic: false,
    underline: false,
    sizePt: null,
    color: null,
    align: "left",
    bullet: false,
    runs: [],
    ...extra,
  };
}

describe("paragraphRuns and rendering", () => {
  it("prefers the stored runs", () => {
    const paragraph = para("Hello", { bold: true, sizePt: 24, runs: [run("Hello", { italic: true })] });
    const runs = paragraphRuns(paragraph);
    expect(runs).toHaveLength(1);
    expect(runs[0].italic).toBe(true);
    expect(runs[0].bold).toBe(false);
    expect(paragraphText(paragraph)).toBe("Hello");
  });

  it("falls back to the paragraph formatting when there are no runs", () => {
    const paragraph = para("Fallback", { bold: true, sizePt: 31, color: "#ff0000" });
    const runs = paragraphRuns(paragraph);
    expect(runs).toHaveLength(1);
    expect(runs[0]).toMatchObject({ text: "Fallback", bold: true, sizePt: 31, color: "#ff0000" });
    expect(paragraphHtml(paragraph)).toContain("<strong>Fallback</strong>");
    expect(paragraphHtml(paragraph)).toContain("font-size:31pt");
  });

  it("renders runs to HTML with the supported formatting", () => {
    const paragraph = para("AB", {
      runs: [run("A", { bold: true }), run("B", { italic: true, underline: true, color: "#123456" })],
    });
    const html = paragraphHtml(paragraph);
    expect(html).toContain("<strong>A</strong>");
    expect(html).toContain("<u><em>B</em></u>");
    expect(html).toContain("color:#123456");
  });

  it("reads the runs back out of a DOM fragment", () => {
    const element = document.createElement("div");
    element.innerHTML =
      '<strong>Bold</strong><em>italic</em><u>under</u><span style="color:rgb(0, 255, 0);font-size:20pt">green</span>';
    const runs = domToParagraphRuns(element);
    expect(runsText(runs)).toBe("Bolditalicundergreen");
    expect(runs[0].bold).toBe(true);
    expect(runs[1].italic).toBe(true);
    expect(runs[2].underline).toBe(true);
    expect(runs[3].color).toBe("rgb(0, 255, 0)");
    expect(runs[3].sizePt).toBe(20);
  });
});

describe("applyRunFormat", () => {
  it("paints only the requested range and splits runs on the boundaries", () => {
    const runs = [run("abcdef", { italic: true })];
    const formatted = applyRunFormat(runs, 1, 4, { bold: true });
    expect(runsText(formatted)).toBe("abcdef");
    expect(formatted.map((entry) => ({ text: entry.text, bold: entry.bold }))).toEqual([
      { text: "a", bold: false },
      { text: "bcd", bold: true },
      { text: "ef", bold: false },
    ]);
    expect(formatted.every((entry) => entry.italic)).toBe(true);
  });

  it("changes color and size over a range", () => {
    const runs = [run("one two", { sizePt: 12 })];
    const formatted = applyRunFormat(runs, 4, 7, { color: "#abcdef", sizePt: 24 });
    expect(formatted[1]).toMatchObject({ text: "two", color: "#abcdef", sizePt: 24 });
    expect(formatted[0]).toMatchObject({ text: "one ", sizePt: 12 });
  });

  it("returns the runs untouched for an empty range", () => {
    const runs = [run("abc")];
    expect(applyRunFormat(runs, 2, 2, { bold: true })).toBe(runs);
  });
});

describe("withParagraphFormat", () => {
  it("patches the paragraph properties and every run", () => {
    const paragraph = para("abc", { runs: [run("a"), run("bc", { italic: true })] });
    const next = withParagraphFormat(paragraph, { bold: true, color: "#111111" });
    expect(next.bold).toBe(true);
    expect(next.color).toBe("#111111");
    expect(next.runs.every((entry) => entry.bold && entry.color === "#111111")).toBe(true);
  });

  it("keeps run-level italics when only bold changes", () => {
    const paragraph = para("abc", { runs: [run("a"), run("bc", { italic: true })] });
    const next = withParagraphFormat(paragraph, { bold: true });
    expect(next.runs[1].italic).toBe(true);
  });
});

describe("splitParagraphAt and mergeParagraphs", () => {
  it("splits runs at the offset and keeps the paragraph formatting", () => {
    const paragraph = para("Hello world", {
      level: 2,
      bullet: true,
      align: "right",
      runs: [run("Hello ", { bold: true }), run("world", { italic: true })],
    });
    const [left, right] = splitParagraphAt(paragraph, 6);
    expect(left.text).toBe("Hello ");
    expect(right.text).toBe("world");
    expect(left).toMatchObject({ level: 2, bullet: true, align: "right" });
    expect(right).toMatchObject({ level: 2, bullet: true, align: "right" });
    expect(left.runs[0]).toMatchObject({ bold: true, text: "Hello " });
    expect(right.runs[0]).toMatchObject({ italic: true, text: "world" });
  });

  it("splits a run that straddles the caret", () => {
    const paragraph = para("abcdef", { runs: [run("abcdef", { underline: true })] });
    const [left, right] = splitParagraphAt(paragraph, 2);
    expect(left.runs[0]).toMatchObject({ text: "ab", underline: true });
    expect(right.runs[0]).toMatchObject({ text: "cdef", underline: true });
  });

  it("merges the runs of both paragraphs in order", () => {
    const first = para("Hello ", { runs: [run("Hello ", { bold: true })] });
    const second = para("world", { runs: [run("world", { italic: true })] });
    const merged = mergeParagraphs(first, second);
    expect(merged.text).toBe("Hello world");
    expect(merged.runs.map((entry) => entry.text)).toEqual(["Hello ", "world"]);
    expect(merged.runs[0].bold).toBe(true);
    expect(merged.runs[1].italic).toBe(true);
  });

  it("merges paragraphs without runs through their fallback runs", () => {
    const merged = mergeParagraphs(para("a", { sizePt: 20 }), para("b", { sizePt: 20 }));
    expect(merged.text).toBe("ab");
    expect(runsText(merged.runs)).toBe("ab");
  });
});

describe("bullets and levels", () => {
  it("toggles the bullet flag without touching the level", () => {
    const paragraph = para("x", { level: 3 });
    const on = withBullet(paragraph, true);
    expect(on).toMatchObject({ bullet: true, level: 3 });
    const off = withBullet(paragraph, false);
    expect(off).toMatchObject({ bullet: false, level: 3 });
  });

  it("clamps the level shift to 0..8", () => {
    expect(withLevel(para("x"), -1).level).toBe(0);
    expect(withLevel(para("x", { level: 8 }), 1).level).toBe(8);
    expect(withLevel(para("x", { level: 2 }), 1).level).toBe(3);
    const same = para("x");
    expect(withLevel(same, 0)).toBe(same);
  });
});

describe("formatAt", () => {
  it("returns the run formatting before the caret", () => {
    const paragraph = para("abcd", { runs: [run("ab", { bold: true }), run("cd", { italic: true })] });
    expect(formatAt(paragraph, 1)).toMatchObject({ bold: true, italic: false });
    expect(formatAt(paragraph, 3)).toMatchObject({ bold: false, italic: true });
  });
});

describe("remapRuns", () => {
  it("maps run slices onto the new text at the same offsets", () => {
    const runs = [run("Hello ", { bold: true }), run("world", { italic: true })];
    const mapped = remapRuns(runs, "Hello world", "Hi there world");
    expect(mapped.map((entry) => entry.text).join("")).toBe("Hi there world");
    expect(mapped[0]).toMatchObject({ bold: true });
    expect(mapped[mapped.length - 1].italic).toBe(true);
  });

  it("extends the last run when the new text is longer", () => {
    const runs = [run("ab", { bold: true }), run("cd", { italic: true })];
    const mapped = remapRuns(runs, "abcd", "abZZ");
    expect(mapped.map((entry) => entry.text)).toEqual(["ab", "ZZ"]);
    expect(mapped[1].italic).toBe(true);
  });

  it("drops run slices past the new text length", () => {
    const runs = [run("abc", { bold: true }), run("def", { italic: true })];
    const mapped = remapRuns(runs, "abcdef", "abc");
    expect(mapped).toHaveLength(1);
    expect(mapped[0]).toMatchObject({ text: "abc", bold: true });
  });

  it("builds a fresh run for empty runs and non-empty text", () => {
    const mapped = remapRuns([emptyRun("")], "", "typed");
    expect(runsText(mapped)).toBe("typed");
  });
});
