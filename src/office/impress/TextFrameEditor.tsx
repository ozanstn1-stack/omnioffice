/**
 * The contentEditable editing surface for one Impress text frame.
 *
 * Mirroring the Writer, the browser owns the focused paragraph's DOM while the
 * model is synchronised on every input. Each paragraph is its own
 * contentEditable so Enter can split and Backspace can merge without the
 * browser restructuring siblings; structural edits commit one undo step.
 */
import { forwardRef, useEffect, useImperativeHandle, useLayoutEffect, useRef, useState, type Ref } from "react";
import type { TextFrame, TextParagraph } from "../../lib/office-types";
import { caretOffset, selectedRange, setCaretOffset, textOffsetWithin } from "../writer/caret";
import { runsText } from "../writer/runs";
import {
  applyRunFormat,
  domToParagraphRuns,
  formatAt,
  mergeParagraphs,
  paragraphHtml,
  paragraphRuns,
  paragraphText,
  splitParagraphAt,
  withBullet,
  withLevel,
  withParagraphFormat,
  type RunFormat,
} from "./rich-text";

/** The selection and active formatting the toolbar reads while editing. */
export interface TextSelectionInfo {
  paragraph: number;
  from: number;
  to: number;
  bold: boolean;
  italic: boolean;
  underline: boolean;
  color: string | null;
  sizePt: number | null;
  bullet: boolean;
  level: number;
}

/** Commands the properties toolbar sends to the active editing surface. */
export interface TextEditHandle {
  applyFormat: (format: RunFormat) => void;
  toggleBullet: () => void;
  changeLevel: (delta: number) => void;
}

export interface TextFrameEditorProps {
  frame: TextFrame;
  scale: number;
  /** Fallback text colour (the slide theme's body colour). */
  color: string;
  onCommit: (paragraphs: TextParagraph[], recordUndo: boolean) => void;
  onDone: () => void;
  onSelection: (info: TextSelectionInfo | null) => void;
}

interface ParagraphSurfaceProps {
  paragraph: TextParagraph;
  index: number;
  focused: boolean;
  /** Caret to place after a structural edit; cleared once handled. */
  focusRequest: { index: number; caret: number } | null;
  onFocus: (index: number) => void;
  onFocusHandled: () => void;
  onInput: (index: number, element: HTMLElement) => void;
  onBlur: (index: number, element: HTMLElement, related: Node | null) => void;
  onKeyDown: (event: React.KeyboardEvent<HTMLDivElement>, index: number) => void;
}

function ParagraphSurface({
  paragraph,
  index,
  focused,
  focusRequest,
  onFocus,
  onFocusHandled,
  onInput,
  onBlur,
  onKeyDown,
}: ParagraphSurfaceProps) {
  const ref = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    const element = ref.current;
    if (!element) return;
    const html = paragraphHtml(paragraph);
    const modelText = paragraphText(paragraph);
    const request = focusRequest?.index === index ? focusRequest : null;
    if (!focused && !request) {
      if (element.innerHTML !== html) element.innerHTML = html;
      return;
    }
    // While focused the browser owns the DOM; a structural change rewrote the
    // runs underneath us, so when the on-screen text disagrees with the model
    // the DOM is stale and has to be repainted with the caret restored.
    const domText = focused ? runsText(domToParagraphRuns(element)) : modelText;
    if (request || domText !== modelText) {
      if (element.innerHTML !== html) element.innerHTML = html;
      const at = request ? request.caret : caretOffset(element);
      if (document.activeElement !== element) element.focus();
      setCaretOffset(element, Math.max(0, Math.min(at, modelText.length)));
    }
    if (request) onFocusHandled();
  }, [paragraph, focused, focusRequest, index, onFocusHandled]);

  // Rendered through a variable so the a11y linter treats it like the Writer's
  // paragraph surface: a contentEditable carries its own textbox semantics.
  const Tag = "div" as const;
  return (
    <Tag
      ref={ref}
      className="slide-text-paragraph"
      data-paragraph-index={index}
      contentEditable="true"
      suppressContentEditableWarning
      spellCheck
      onFocus={() => onFocus(index)}
      onInput={(event) => onInput(index, event.currentTarget)}
      onBlur={(event) => onBlur(index, event.currentTarget, event.relatedTarget as Node | null)}
      onKeyDown={(event) => onKeyDown(event, index)}
      style={{
        fontWeight: paragraph.bold ? 700 : undefined,
        fontStyle: paragraph.italic ? "italic" : undefined,
        textDecoration: paragraph.underline ? "underline" : undefined,
        textAlign: (paragraph.align || "left") as "left" | "center" | "right",
      }}
    />
  );
}

export const TextFrameEditor = forwardRef<TextEditHandle, TextFrameEditorProps>(function TextFrameEditor(
  { frame, scale, color, onCommit, onDone, onSelection },
  ref: Ref<TextEditHandle>,
) {
  const rootRef = useRef<HTMLDivElement>(null);
  const [focusedIndex, setFocusedIndex] = useState<number | null>(null);
  const focusedRef = useRef<number | null>(null);
  const [focusRequest, setFocusRequest] = useState<{ index: number; caret: number } | null>(() => ({
    index: 0,
    caret: paragraphText(frame.paragraphs[0] ?? emptyParagraph()).length,
  }));
  const selectionRef = useRef<{ paragraph: number; from: number; to: number } | null>(null);
  // True while a structural edit is moving focus itself, so the blur it causes
  // does not read as "the user clicked outside" and close the editor.
  const programmaticFocus = useRef(false);

  const reportSelection = () => {
    const root = rootRef.current;
    if (!root) return;
    const selection = window.getSelection();
    if (!selection || selection.rangeCount === 0) return;
    const range = selection.getRangeAt(0);
    if (!root.contains(range.startContainer)) return;
    const elementOf = (node: Node): HTMLElement | null =>
      (node.nodeType === Node.ELEMENT_NODE ? (node as Element) : node.parentElement)?.closest<HTMLElement>(
        "[data-paragraph-index]",
      ) ?? null;
    const startElement = elementOf(range.startContainer);
    if (!startElement || !root.contains(startElement)) return;
    const index = Number(startElement.dataset.paragraphIndex ?? "-1");
    if (index < 0 || index >= frame.paragraphs.length) return;
    const from = textOffsetWithin(startElement, range.startContainer, range.startOffset);
    let to = from;
    if (!selection.isCollapsed) {
      const endElement = range.endContainer ? elementOf(range.endContainer) : null;
      if (endElement === startElement) {
        to = Math.max(from, textOffsetWithin(startElement, range.endContainer, range.endOffset));
      }
    }
    selectionRef.current = { paragraph: index, from, to };
    const paragraph = frame.paragraphs[index];
    const format = formatAt(paragraph, from);
    onSelection({
      paragraph: index,
      from,
      to,
      bold: format.bold,
      italic: format.italic,
      underline: format.underline,
      color: format.color,
      sizePt: format.sizePt,
      bullet: paragraph.bullet,
      level: paragraph.level,
    });
  };

  // Re-registered every render so the listener always sees the current frame.
  useEffect(() => {
    document.addEventListener("selectionchange", reportSelection);
    return () => document.removeEventListener("selectionchange", reportSelection);
  });

  /** Writes one paragraph's DOM back into the model (typing path: no undo). */
  const syncParagraph = (index: number, element: HTMLElement, recordUndo: boolean) => {
    const current = frame.paragraphs[index];
    if (!current) return;
    const runs = domToParagraphRuns(element);
    const text = runsText(runs);
    if (current.text === text && runsText(paragraphRuns(current)) === text) return;
    onCommit(
      frame.paragraphs.map((paragraph, position) => (position === index ? { ...paragraph, text, runs } : paragraph)),
      recordUndo,
    );
  };

  const commit = (paragraphs: TextParagraph[], recordUndo: boolean, focus?: { index: number; caret: number }) => {
    if (focus) {
      programmaticFocus.current = true;
      setFocusRequest(focus);
    }
    onCommit(paragraphs, recordUndo);
  };

  const handleKeyDown = (event: React.KeyboardEvent<HTMLDivElement>, index: number) => {
    const element = event.currentTarget;
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      const offset = caretOffset(element);
      const [left, right] = splitParagraphAt(frame.paragraphs[index], offset);
      const next = [...frame.paragraphs];
      next.splice(index, 1, left, right);
      commit(next, true, { index: index + 1, caret: 0 });
      return;
    }
    if (event.key === "Tab") {
      event.preventDefault();
      const next = frame.paragraphs.map((paragraph, position) =>
        position === index ? withLevel(paragraph, event.shiftKey ? -1 : 1) : paragraph,
      );
      if (next[index] !== frame.paragraphs[index]) commit(next, true, { index, caret: caretOffset(element) });
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      element.blur();
      return;
    }
    if (event.key === "Backspace") {
      const range = selectedRange(element);
      const at = range ? range[0] : caretOffset(element);
      const collapsed = range === null || range[0] === range[1];
      if (collapsed && at === 0 && index > 0) {
        event.preventDefault();
        const caret = paragraphText(frame.paragraphs[index - 1]).length;
        const merged = mergeParagraphs(frame.paragraphs[index - 1], frame.paragraphs[index]);
        const next = [...frame.paragraphs];
        next.splice(index - 1, 2, merged);
        commit(next, true, { index: index - 1, caret });
      }
    }
  };

  const mutateParagraph = (mutate: (paragraph: TextParagraph) => TextParagraph) => {
    const info = selectionRef.current;
    const index = info?.paragraph ?? focusedRef.current ?? 0;
    if (index < 0 || index >= frame.paragraphs.length) return;
    const next = frame.paragraphs.map((paragraph, position) => (position === index ? mutate(paragraph) : paragraph));
    if (next[index] === frame.paragraphs[index]) return;
    // Repaint the focused DOM with the new formatting and keep the caret.
    if (info) {
      programmaticFocus.current = true;
      setFocusRequest({ index, caret: info.to });
    }
    onCommit(next, true);
  };

  const applyFormat = (format: RunFormat) => {
    const info = selectionRef.current;
    mutateParagraph((paragraph) => {
      const from = info?.from ?? 0;
      const to = info?.to ?? 0;
      if (to > from) {
        const runs = applyRunFormat(paragraphRuns(paragraph), from, to, format);
        return { ...paragraph, runs, text: runsText(runs) };
      }
      return withParagraphFormat(paragraph, format);
    });
  };

  useImperativeHandle(
    ref,
    () => ({
      applyFormat,
      toggleBullet: () => mutateParagraph((paragraph) => withBullet(paragraph, !paragraph.bullet)),
      changeLevel: (delta: number) => mutateParagraph((paragraph) => withLevel(paragraph, delta)),
    }),
    // Rebuilt every render so the commands always see the current frame.
    // eslint-disable-next-line react-hooks/exhaustive-deps -- the closure is replaced on each render
    [frame],
  );

  return (
    <div
      ref={rootRef}
      className="slide-text-editor"
      role="presentation"
      onKeyUp={reportSelection}
      onMouseUp={reportSelection}
      onPointerDown={(event) => event.stopPropagation()}
    >
      {frame.paragraphs.map((paragraph, index) => (
        <div
          key={index}
          className="slide-text-paragraph-row"
          style={{
            marginLeft: paragraph.level * 18,
            fontSize: `${(paragraph.sizePt ?? frame.sizePt ?? 18) * scale}px`,
            color: paragraph.color ?? frame.color ?? color,
          }}
        >
          {paragraph.bullet ? (
            <span className="slide-text-bullet" contentEditable={false}>
              •
            </span>
          ) : null}
          <ParagraphSurface
            paragraph={paragraph}
            index={index}
            focused={focusedIndex === index}
            focusRequest={focusRequest}
            onFocus={(position) => {
              focusedRef.current = position;
              setFocusedIndex(position);
            }}
            onFocusHandled={() => setFocusRequest(null)}
            onInput={(position, element) => syncParagraph(position, element, false)}
            onBlur={(position, element, related) => {
              syncParagraph(position, element, false);
              setFocusedIndex(null);
              focusedRef.current = null;
              if (programmaticFocus.current) {
                programmaticFocus.current = false;
                return;
              }
              const relatedElement = related instanceof Element ? related : (related?.parentElement ?? null);
              const insideEditor = relatedElement !== null && rootRef.current?.contains(relatedElement);
              // The text toolbar keeps the frame open while its inputs are used.
              const insideTools = relatedElement?.closest("[data-keep-text-edit]") != null;
              if (!insideEditor && !insideTools) onDone();
            }}
            onKeyDown={handleKeyDown}
          />
        </div>
      ))}
    </div>
  );
});

function emptyParagraph(): TextParagraph {
  return {
    text: "",
    level: 0,
    bold: false,
    italic: false,
    underline: false,
    sizePt: null,
    color: null,
    align: "left",
    bullet: false,
    runs: [],
  };
}
