import { describe, expect, it } from "vitest";
import type { Block } from "../../lib/office-types";
import { defaultParaProps } from "../../lib/office-types";
import { emptyRun } from "./runs";
import { fieldValuesFor, orderedListMarker, orderedListNumbers } from "./writerDom";

function paragraph(
  text: string,
  overrides: Partial<ReturnType<typeof defaultParaProps>> = {},
  field?: { kind: string; target: string; cached: string },
) {
  const runs = field ? [{ ...emptyRun(""), field }] : [emptyRun(text)];
  return { type: "paragraph" as const, props: { ...defaultParaProps(), ...overrides }, runs };
}

function numbered(level: number, start = 1) {
  return { list: { kind: "number" as const, level, start, marker: "" } };
}

describe("ordered list numbering", () => {
  it("increments consecutive items and restarts after a break", () => {
    const blocks: Block[] = [
      paragraph("one", numbered(0, 1)),
      paragraph("two", numbered(0)),
      paragraph("plain"),
      paragraph("restart", numbered(0, 5)),
    ];
    const numbers = orderedListNumbers(blocks);
    expect([...numbers.entries()]).toEqual([
      [0, 1],
      [1, 2],
      [3, 5],
    ]);
  });

  it("keeps a counter per level and continues it after returning", () => {
    const blocks: Block[] = [
      paragraph("a", numbered(0)),
      paragraph("a.1", numbered(1, 1)),
      paragraph("a.2", numbered(1)),
      paragraph("b", numbered(0)),
      paragraph("b.1", numbered(1, 7)),
    ];
    const numbers = orderedListNumbers(blocks);
    expect(numbers.get(0)).toBe(1);
    expect(numbers.get(1)).toBe(1);
    expect(numbers.get(2)).toBe(2);
    expect(numbers.get(3)).toBe(2);
    // A new deeper list starts at its own start value.
    expect(numbers.get(4)).toBe(7);
  });

  it("ignores bullet lists and non-paragraph blocks in the sequence", () => {
    const blocks: Block[] = [
      paragraph("bullet", { list: { kind: "bullet", level: 0, start: 1, marker: "" } }),
      paragraph("image", {}),
      paragraph("numbered", numbered(0)),
    ];
    const numbers = orderedListNumbers(blocks);
    expect(numbers.size).toBe(1);
    expect(numbers.get(2)).toBe(1);
  });

  it("renders the computed number for ordered items and level glyphs for bullets", () => {
    const item = paragraph("x", numbered(0, 3));
    if (item.type !== "paragraph") throw new Error("paragraph expected");
    expect(orderedListMarker(item.props, 7)).toBe("7.");
    expect(orderedListMarker(item.props, undefined)).toBe("3.");
    const bullet = { kind: "bullet" as const, level: 1, start: 1, marker: "" };
    expect(orderedListMarker({ ...item.props, list: bullet }, undefined)).toBe("◦");
    expect(orderedListMarker({ ...item.props, list: null }, 1)).toBe("");
  });
});

describe("document field values", () => {
  it("derives page, pages, title, author, date and time", () => {
    const values = fieldValuesFor({
      page: 3,
      pages: 12,
      title: "Report",
      author: "Ada",
      now: new Date(2026, 0, 2, 3, 4, 5),
    });
    expect(values["page:"]).toBe("3");
    expect(values["pages:"]).toBe("12");
    expect(values["title:"]).toBe("Report");
    expect(values["author:"]).toBe("Ada");
    expect(values["date:"]).toBe(new Date(2026, 0, 2).toLocaleDateString());
    expect(values["time:"]).toBe(new Date(2026, 0, 2, 3, 4, 5).toLocaleTimeString());
  });

  it("omits page numbers when the context has none", () => {
    const values = fieldValuesFor({ title: "T" });
    expect(values["page:"]).toBeUndefined();
    expect(values["pages:"]).toBeUndefined();
    expect(values["title:"]).toBe("T");
  });

  it("never overrides a cross reference, which stays cached", () => {
    const field = paragraph("", {}, { kind: "ref", target: "Bookmark1", cached: "Section 2" });
    if (field.type !== "paragraph") throw new Error("paragraph expected");
    const values = fieldValuesFor({ page: 2, pages: 4 });
    const key = `${field.runs[0].field?.kind}:${field.runs[0].field?.target}`;
    expect(values[key]).toBeUndefined();
  });
});
