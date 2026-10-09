import { describe, expect, it } from "vitest";
import type { Run, TextFrame, TextParagraph } from "../../lib/office-types";
import { editedParagraphs, withFrameAlign, withFrameSize } from "./textFrame";

function run(text: string, extra: Partial<Run> = {}): Run {
  return {
    text,
    bold: false,
    italic: false,
    underline: false,
    strike: false,
    color: null,
    highlight: null,
    font: null,
    sizePt: null,
    link: null,
    comment: null,
    superscript: false,
    subscript: false,
    ...extra,
  };
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

/** An imported slide body: three paragraphs, nested bullets, formatted runs. */
function imported(): TextParagraph[] {
  return [
    para("Agenda", { sizePt: 28, runs: [run("Agenda", { bold: true, sizePt: 28 })] }),
    para("First point", { level: 0, bullet: true, sizePt: 20, runs: [run("First point", { sizePt: 20 })] }),
    para("Nested point", {
      level: 1,
      bullet: true,
      sizePt: 16,
      align: "right",
      runs: [run("Nested ", { sizePt: 16 }), run("point", { sizePt: 16, italic: true })],
    }),
  ];
}

describe("editedParagraphs", () => {
  it("returns the same paragraphs when the text did not change", () => {
    const before = imported();
    const after = editedParagraphs(before, "Agenda\nFirst point\nNested point");
    expect(after).toBe(before);
    expect(after[2].runs).toHaveLength(2);
  });

  it("splits the edited text into one paragraph per line", () => {
    const after = editedParagraphs(imported(), "Agenda\nFirst point\nNested point");
    expect(after.map((paragraph) => paragraph.text)).toEqual(["Agenda", "First point", "Nested point"]);
    const edited = editedParagraphs(imported(), "Agenda\nFirst point\nNested point\nFourth");
    expect(edited).toHaveLength(4);
    expect(edited.map((paragraph) => paragraph.text)).toEqual(["Agenda", "First point", "Nested point", "Fourth"]);
  });

  it("keeps level, bullet, align and size per paragraph when the line count lines up", () => {
    const after = editedParagraphs(imported(), "Agenda\nSecond point\nNested point");
    expect(after[1]).toMatchObject({ text: "Second point", level: 0, bullet: true, sizePt: 20 });
    expect(after[2]).toMatchObject({ text: "Nested point", level: 1, bullet: true, sizePt: 16, align: "right" });
  });

  it("keeps the runs of an edited paragraph by mapping them onto the new text", () => {
    const before = imported();
    const after = editedParagraphs(before, "Agenda\nSecond point\nNested point");
    expect(after[0]).toBe(before[0]);
    expect(after[0].runs).toHaveLength(1);
    // The old run's formatting survives; only its text is remapped.
    expect(after[1].runs).toHaveLength(1);
    expect(after[1].runs[0]).toMatchObject({ text: "Second point", sizePt: 20 });
    expect(after[2]).toBe(before[2]);
    expect(after[2].runs).toHaveLength(2);
  });

  it("lets an inserted line inherit the formatting of the line above it", () => {
    const after = editedParagraphs(imported(), "Agenda\nFirst point\nNested point\nAnother nested");
    expect(after).toHaveLength(4);
    expect(after[3]).toMatchObject({ text: "Another nested", level: 1, bullet: true, sizePt: 16, align: "right" });
    // The inherited paragraph keeps the above line's run formatting, remapped.
    expect(after[3].runs.map((entry) => entry.text).join("")).toBe("Another nested");
    expect(after[3].runs[after[3].runs.length - 1].italic).toBe(true);
  });

  it("keeps the paragraphs around a line inserted in the middle", () => {
    const before = imported();
    const after = editedParagraphs(before, "Agenda\nFirst point\nBrand new\nNested point");
    expect(after.map((paragraph) => paragraph.text)).toEqual(["Agenda", "First point", "Brand new", "Nested point"]);
    expect(after[0]).toBe(before[0]);
    expect(after[1]).toBe(before[1]);
    expect(after[2]).toMatchObject({ level: 0, bullet: true, sizePt: 20 });
    expect(after[2].runs).toHaveLength(1);
    expect(after[2].runs[0]).toMatchObject({ text: "Brand new", sizePt: 20 });
    expect(after[3]).toBe(before[2]);
  });

  it("removes the paragraph of a deleted line and keeps the rest intact", () => {
    const before = imported();
    const after = editedParagraphs(before, "Agenda\nNested point");
    expect(after).toHaveLength(2);
    expect(after[0]).toBe(before[0]);
    expect(after[1]).toBe(before[2]);
  });

  it("splits a paste of several lines over one paragraph without losing its formatting", () => {
    const after = editedParagraphs([imported()[1]], "One\nTwo\nThree");
    expect(after.map((paragraph) => paragraph.text)).toEqual(["One", "Two", "Three"]);
    for (const paragraph of after) {
      expect(paragraph).toMatchObject({ level: 0, bullet: true, sizePt: 20 });
      expect(paragraph.runs.map((entry) => entry.text).join("")).toBe(paragraph.text);
    }
  });

  it("normalises Windows line endings and keeps empty lines as empty paragraphs", () => {
    const after = editedParagraphs([para("a")], "a\r\n\r\nb");
    expect(after.map((paragraph) => paragraph.text)).toEqual(["a", "", "b"]);
  });

  it("clearing the text leaves one empty paragraph that keeps its formatting", () => {
    const after = editedParagraphs(imported(), "");
    expect(after).toHaveLength(1);
    expect(after[0]).toMatchObject({ text: "", sizePt: 28 });
    expect(after[0].runs).toHaveLength(1);
    expect(after[0].runs[0].text).toBe("");
  });

  it("builds plain paragraphs for a frame that had none", () => {
    const after = editedParagraphs([], "x\ny");
    expect(after.map((paragraph) => paragraph.text)).toEqual(["x", "y"]);
    expect(after[0]).toMatchObject({ level: 0, bullet: false, runs: [] });
  });

  it("does not mutate its input", () => {
    const before = imported();
    const snapshot = JSON.stringify(before);
    editedParagraphs(before, "Changed\nFirst point\nPlus\nMore");
    expect(JSON.stringify(before)).toBe(snapshot);
  });
});

describe("frame-wide size and alignment", () => {
  const frame = (): TextFrame => ({
    paragraphs: imported(),
    valign: "top",
    font: null,
    sizePt: null,
    color: null,
    align: "left",
  });

  it("applies the size to every paragraph and to the runs that would override it", () => {
    const next = withFrameSize(frame(), 24);
    expect(next.paragraphs).toHaveLength(3);
    expect(next.paragraphs.map((paragraph) => paragraph.sizePt)).toEqual([24, 24, 24]);
    expect(next.paragraphs.flatMap((paragraph) => paragraph.runs).every((candidate) => candidate.sizePt === null)).toBe(
      true,
    );
    expect(next.paragraphs[2].runs[1].italic).toBe(true);
    expect(next.paragraphs[2]).toMatchObject({ level: 1, bullet: true });
  });

  it("applies the alignment to every paragraph", () => {
    const next = withFrameAlign(frame(), "center");
    expect(next.paragraphs.map((paragraph) => paragraph.align)).toEqual(["center", "center", "center"]);
    expect(next.paragraphs[1]).toMatchObject({ level: 0, bullet: true, sizePt: 20 });
  });
});
