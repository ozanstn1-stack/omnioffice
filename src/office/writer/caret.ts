/**
 * Caret helpers for the Writer's contentEditable paragraphs.
 *
 * The editor keeps the browser in charge of the caret so native typing, IME and
 * the clipboard keep working. Structural keys (Enter, Backspace, Tab, arrows at
 * a paragraph edge) need to know *where* the caret is in plain-text terms, and
 * need to put it back afterwards, so that logic is isolated here.
 */
import { runsToHtml } from "./writerDom";

const ATOMIC_SELECTOR = "[data-note-id], [data-field-kind]";

function isAtomicNode(node: Node): boolean {
  const element = node.nodeType === Node.ELEMENT_NODE ? (node as Element) : node.parentElement;
  return Boolean(element?.closest(ATOMIC_SELECTOR));
}

/**
 * Model length from the element start to a DOM position, excluding note
 * markers and fields. Those render visible glyphs (`<sup>1</sup>`, a cached
 * date) but exist as empty runs in the model, so counting them shifted every
 * structural edit (Enter/Backspace/bookmarks) by their rendered length.
 */
function modelOffsetWithin(element: HTMLElement, node: Node, nodeOffset: number): number {
  if (node !== element && !element.contains(node)) return 0;
  const range = element.ownerDocument.createRange();
  range.selectNodeContents(element);
  if (isAtomicNode(node)) {
    // A caret inside/at a note or field snaps to the anchor's model position.
    try {
      range.setEndBefore(atomicAnchor(node));
    } catch {
      return 0;
    }
  } else {
    try {
      range.setEnd(node, nodeOffset);
    } catch {
      return element.textContent?.length ?? 0;
    }
  }
  const fragment = range.cloneContents();
  let length = 0;
  const visit = (current: Node) => {
    if (current.nodeType === Node.TEXT_NODE) {
      length += current.textContent?.length ?? 0;
      return;
    }
    if (current.nodeType !== Node.ELEMENT_NODE) return;
    if ((current as Element).matches(ATOMIC_SELECTOR)) return;
    current.childNodes.forEach(visit);
  };
  fragment.childNodes.forEach(visit);
  return length;
}

/** The atomic ancestor of a node, used to snap an in-anchor caret to its start. */
function atomicAnchor(node: Node): Node {
  const element = node.nodeType === Node.ELEMENT_NODE ? (node as Element) : node.parentElement;
  return element?.closest(ATOMIC_SELECTOR) ?? node;
}

/** Reads the caret offset of a contentEditable element as a plain-text index. */
export function caretOffset(element: HTMLElement): number {
  const selection = window.getSelection();
  if (!selection || selection.rangeCount === 0) return element.textContent?.length ?? 0;
  const range = selection.getRangeAt(0);
  if (!element.contains(range.startContainer)) return 0;
  return modelOffsetWithin(element, range.startContainer, range.startOffset);
}

/** True when the selection covers more than a collapsed caret. */
export function hasSelection(element: HTMLElement): boolean {
  const selection = window.getSelection();
  if (!selection || selection.rangeCount === 0 || selection.isCollapsed) return false;
  return element.contains(selection.anchorNode);
}

/** Selected plain-text range inside the element, or null when collapsed. */
export function selectedRange(element: HTMLElement): [number, number] | null {
  const selection = window.getSelection();
  if (!selection || selection.rangeCount === 0 || selection.isCollapsed) return null;
  const range = selection.getRangeAt(0);
  if (!element.contains(range.startContainer) || !element.contains(range.endContainer)) return null;
  const start = modelOffsetWithin(element, range.startContainer, range.startOffset);
  const end = modelOffsetWithin(element, range.endContainer, range.endOffset);
  return [start, Math.max(start, end)];
}

/** True when the caret sits on the first visual line of the element. */
export function caretOnFirstLine(element: HTMLElement): boolean {
  const offset = caretOffset(element);
  const prefix = (element.textContent ?? "").slice(0, offset);
  return !prefix.includes("\n");
}

/** True when the caret sits on the last visual line of the element. */
export function caretOnLastLine(element: HTMLElement): boolean {
  const offset = caretOffset(element);
  const text = element.textContent ?? "";
  return !text.slice(offset).includes("\n");
}

/** Puts a collapsed caret at a plain-text offset, replacing the selection. */
export function setCaretOffset(element: HTMLElement, offset: number): void {
  const selection = window.getSelection();
  if (!selection) return;
  const textNode = findTextNodeAt(element, Math.max(0, offset));
  if (!textNode) {
    element.focus();
    const range = document.createRange();
    range.selectNodeContents(element);
    range.collapse(offset <= 0);
    selection.removeAllRanges();
    selection.addRange(range);
    return;
  }
  const range = document.createRange();
  range.setStart(textNode.node, textNode.offset);
  range.collapse(true);
  selection.removeAllRanges();
  selection.addRange(range);
}

/** Selects a plain-text range inside the element. */
export function setSelectionRange(element: HTMLElement, from: number, to: number): void {
  const selection = window.getSelection();
  if (!selection) return;
  const start = findTextNodeAt(element, Math.max(0, Math.min(from, to)));
  const end = findTextNodeAt(element, Math.max(from, to));
  if (!start || !end) {
    setCaretOffset(element, to);
    return;
  }
  const range = document.createRange();
  range.setStart(start.node, start.offset);
  range.setEnd(end.node, end.offset);
  selection.removeAllRanges();
  selection.addRange(range);
}

/**
 * Plain-text offset of a DOM position inside `element`.
 *
 * Range#toString skips element boundaries but keeps every text node, which is
 * exactly how the model counts characters. Positions outside the element clamp
 * to 0 so a caller cannot produce an offset the model does not have.
 */
export function textOffsetWithin(element: HTMLElement, node: Node, nodeOffset: number): number {
  return modelOffsetWithin(element, node, nodeOffset);
}

/** Browser caret hit-test at a viewport point, across the two API spellings. */
function pointPosition(document: Document, x: number, y: number): { node: Node; offset: number } | null {
  const doc = document as Document & {
    caretRangeFromPoint?: (x: number, y: number) => Range | null;
    caretPositionFromPoint?: (x: number, y: number) => { offsetNode: Node; offset: number } | null;
  };
  const range = doc.caretRangeFromPoint?.(x, y);
  if (range) return { node: range.startContainer, offset: range.startOffset };
  const position = doc.caretPositionFromPoint?.(x, y);
  if (position) return { node: position.offsetNode, offset: position.offset };
  return null;
}

/**
 * Maps a viewport point to a plain-text offset inside `element`.
 *
 * The paginated view renders a continuation fragment as the whole paragraph
 * shifted up inside a clipped box, so hit-testing any fragment still yields an
 * offset in the full paragraph. Returns null when the browser has no hit test
 * (jsdom) or the point falls outside `element`.
 */
export function offsetFromPoint(element: HTMLElement, x: number, y: number): number | null {
  const position = pointPosition(element.ownerDocument, x, y);
  if (!position || !element.contains(position.node)) return null;
  return textOffsetWithin(element, position.node, position.offset);
}

export interface ParagraphPoint {
  /** The `.para` element under the point. */
  element: HTMLElement;
  /** Block index of the paragraph that owns the `.para`. */
  block: number;
  /** Plain-text offset inside the paragraph. */
  offset: number;
}

/**
 * The paragraph under a viewport point, for drag selection over static
 * fragments. The block index comes from the enclosing `[data-block-index]`,
 * which both the static renderer and the editable carry.
 */
export function paragraphAtPoint(document: Document, x: number, y: number): ParagraphPoint | null {
  const position = pointPosition(document, x, y);
  if (!position) return null;
  const start =
    position.node.nodeType === Node.ELEMENT_NODE ? (position.node as HTMLElement) : position.node.parentElement;
  const element = start?.closest<HTMLElement>(".para");
  if (!element) return null;
  const owner = element.closest<HTMLElement>("[data-block-index]");
  const block = owner ? Number(owner.dataset.blockIndex) : Number.NaN;
  if (!Number.isFinite(block)) return null;
  return { element, block, offset: textOffsetWithin(element, position.node, position.offset) };
}

/** Walks the text nodes of an element to find the node holding `offset`.
 * Text inside note/field anchors is skipped (not part of the model) and a
 * `<br>` counts as the single `"\n"` it represents, so a caret restored after
 * a hard line break lands after the break instead of before it. */
function findTextNodeAt(element: HTMLElement, offset: number): { node: Node; offset: number } | null {
  const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT | NodeFilter.SHOW_ELEMENT, {
    acceptNode(node) {
      if (node.nodeType === Node.ELEMENT_NODE) {
        return (node as Element).tagName.toLowerCase() === "br" ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_SKIP;
      }
      return isAtomicNode(node) ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT;
    },
  });
  let remaining = offset;
  let last: { node: Node; offset: number } | null = null;
  let node = walker.nextNode();
  while (node) {
    if (node.nodeType === Node.TEXT_NODE) {
      const length = node.textContent?.length ?? 0;
      last = { node, offset: length };
      if (remaining <= length) return { node, offset: remaining };
      remaining -= length;
    } else {
      if (remaining === 0) {
        const parent = node.parentNode ?? element;
        return { node: parent, offset: Array.prototype.indexOf.call(parent.childNodes, node) };
      }
      remaining -= 1;
      // A break does not hold text; a later text node is still the fallback.
      last = null;
    }
    node = walker.nextNode();
  }
  return last;
}

/**
 * Replaces the inner HTML of a paragraph after a structural change.
 *
 * Called *after* React has committed the new model, so the imperative write
 * cannot race the render the way the typing path can.
 */
export function repaintParagraph(element: HTMLElement, runs: Parameters<typeof runsToHtml>[0]): void {
  const html = runsToHtml(runs);
  if (element.innerHTML !== html) element.innerHTML = html;
}
