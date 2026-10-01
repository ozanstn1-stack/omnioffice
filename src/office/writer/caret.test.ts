/**
 * Tests for the point-to-offset mapping used by the paginated view.
 *
 * A continuation fragment renders the whole paragraph shifted up and clipped
 * by the page, so a hit test in any fragment must map to a character offset in
 * the full block - that is what fixes the old `clickOffset` returning 0 for
 * continuation fragments. jsdom has no `caretRangeFromPoint`, so the tests stub
 * the hit test and pin the DOM-to-text offset arithmetic.
 */
import { afterEach, describe, expect, it } from "vitest";
import { caretOffset, offsetFromPoint, paragraphAtPoint, setCaretOffset, textOffsetWithin } from "./caret";

function buildParagraph(html: string, blockIndex = 0): HTMLElement {
  const row = document.createElement("div");
  row.className = "para-row";
  row.dataset.blockIndex = String(blockIndex);
  const para = document.createElement("div");
  para.className = "para";
  para.innerHTML = html;
  row.appendChild(para);
  document.body.appendChild(row);
  return para;
}

function stubCaretRangeFromPoint(node: Node, offset: number): () => void {
  const doc = document as unknown as { caretRangeFromPoint?: (x: number, y: number) => Range | null };
  const range = document.createRange();
  range.setStart(node, offset);
  range.collapse(true);
  const previous = doc.caretRangeFromPoint;
  doc.caretRangeFromPoint = () => range;
  return () => {
    if (previous) doc.caretRangeFromPoint = previous;
    else delete doc.caretRangeFromPoint;
  };
}

afterEach(() => {
  document.body.innerHTML = "";
});

describe("caret point mapping", () => {
  it("counts text across nested formatting", () => {
    const para = buildParagraph("Hello <strong>World</strong>");
    const strong = para.querySelector("strong");
    expect(strong).not.toBeNull();
    const text = strong?.firstChild as Text;
    expect(textOffsetWithin(para, text, 3)).toBe(9); // "Hello Wor"
  });

  it("maps a point in a fragment to the offset in the full paragraph", () => {
    const para = buildParagraph("Hello World");
    const restore = stubCaretRangeFromPoint(para.firstChild as Text, 6);
    expect(offsetFromPoint(para, 10, 10)).toBe(6);
    restore();
  });

  it("returns null without a hit test or outside the paragraph", () => {
    const para = buildParagraph("Hello");
    expect(offsetFromPoint(para, 10, 10)).toBeNull();

    const other = buildParagraph("Other", 1);
    const restore = stubCaretRangeFromPoint(other.firstChild as Text, 2);
    expect(offsetFromPoint(para, 10, 10)).toBeNull();
    restore();
  });

  it("resolves the block index and offset under a point", () => {
    const para = buildParagraph("Hello <em>there</em>", 4);
    const em = para.querySelector("em");
    expect(em).not.toBeNull();
    const restore = stubCaretRangeFromPoint(em?.firstChild as Text, 2);
    const hit = paragraphAtPoint(document, 5, 5);
    expect(hit?.element).toBe(para);
    expect(hit?.block).toBe(4);
    expect(hit?.offset).toBe(8); // "Hello th"
    restore();
  });

  it("returns null when the point is not on a paragraph", () => {
    const div = document.createElement("div");
    document.body.appendChild(div);
    const restore = stubCaretRangeFromPoint(div, 0);
    expect(paragraphAtPoint(document, 1, 1)).toBeNull();
    restore();
  });

  it("does not count note markers or fields in the model offset", () => {
    // Regression: the rendered `<sup>1</sup>` was counted, so structural edits
    // around a footnote were offset by the marker's glyph length.
    const para = buildParagraph('ab<sup data-note-id="n1">1</sup>cd');
    const after = Array.from(para.childNodes).find(
      (node) => node.nodeType === Node.TEXT_NODE && node.textContent === "cd",
    ) as Text;
    expect(textOffsetWithin(para, after, 0)).toBe(2);
    expect(textOffsetWithin(para, after, 2)).toBe(4);
    // A caret inside the marker snaps to its model position (before it).
    const marker = para.querySelector("sup")?.firstChild as Text;
    expect(textOffsetWithin(para, marker, 1)).toBe(2);
  });

  it("restores a model offset without landing inside an atomic marker", () => {
    const para = buildParagraph('ab<sup data-note-id="n1">1</sup>cd');
    setCaretOffset(para, 2);
    const anchor = window.getSelection()?.anchorNode;
    expect(anchor?.parentElement?.closest("[data-note-id]") ?? null).toBeNull();
    expect(caretOffset(para)).toBe(2);
  });
});
