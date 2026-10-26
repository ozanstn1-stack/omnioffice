/**
 * Writer view layer: the static page renderers, the paginated paragraph and the
 * block dispatcher used by the editor.
 *
 * Extracted verbatim from WriterEditor.tsx (B4 editor seam 2): these components
 * are pure - they take the model and callbacks as props and never reach into
 * the editor controller.
 */
import { memo, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useT } from "../../lib/i18n";
import { type Block, type Run, type TableData, type TocEntry } from "../../lib/office-types";
import {
  caretOffset,
  caretOnFirstLine,
  caretOnLastLine,
  offsetFromPoint,
  selectedRange,
  setCaretOffset,
} from "./caret";
import type { Fragment } from "./pagination";
import { replaceRange, runsText, wordRangeAt } from "./runs";
import { TableView } from "./table-view";
import { domToRuns, fieldValuesFor, orderedListMarker, runsToHtml } from "./writerDom";

/** Structural edits the paragraph component asks the document to perform. */
export type StructureAction =
  | { kind: "replace"; index: number; block: Block }
  | { kind: "split"; index: number; offset: number; to: number }
  | { kind: "mergeBackward"; index: number }
  | { kind: "mergeForward"; index: number }
  | { kind: "indent" | "outdent"; index: number }
  | { kind: "moveCaret"; index: number; delta: number; atLine: "start" | "end" };

function effectiveFontSize(block: Extract<Block, { type: "paragraph" }>): number | null {
  return block.runs.find((run) => run.sizePt)?.sizePt ?? null;
}

// ---------------------------------------------------------------------------
// Static (read-only) rendering for the paginated pages and the layout probe
// ---------------------------------------------------------------------------

/** Replaces `{{page}}` / `{{pages}}` tokens in header/footer text. */
function substituteTokens(runs: Run[], page: number, pages: number): Run[] {
  if (page <= 0) return runs;
  return runs.map((run) => ({
    ...run,
    text: run.text.replace(/\{\{page\}\}/g, String(page)).replace(/\{\{pages\}\}/g, String(pages)),
  }));
}

interface StaticParagraphProps {
  block: Extract<Block, { type: "paragraph" }>;
  index: number;
  scope: string;
  zoom: number;
  page?: number;
  pages?: number;
  noteNumbers?: Record<string, number>;
  showRevisions?: boolean;
  /** Computed ordered-list position for this block. */
  listNumber?: number;
  /** Live field values (page/pages/date/time/title/author). */
  fieldValues?: Record<string, string>;
  /** Click target for header/footer previews; page fragments use pointer events. */
  onOpen?: () => void;
}

/** Shallow equality for the small maps threaded through the static renderers. */
function sameCounts(left?: Record<string, unknown>, right?: Record<string, unknown>): boolean {
  if (left === right) return true;
  if (!left || !right) return false;
  const keys = Object.keys(left);
  return keys.length === Object.keys(right).length && keys.every((key) => left[key] === right[key]);
}

/**
 * The probe re-renders on every keystroke, so an unchanged paragraph must not
 * be reconciled again. `onOpen` is deliberately compared by nothing: the
 * handlers are tiny closures over a stable index and would otherwise defeat
 * the memo on every parent render.
 */
function sameStaticParagraph(prev: StaticParagraphProps, next: StaticParagraphProps): boolean {
  return (
    prev.block === next.block &&
    prev.index === next.index &&
    prev.scope === next.scope &&
    prev.zoom === next.zoom &&
    prev.page === next.page &&
    prev.pages === next.pages &&
    prev.showRevisions === next.showRevisions &&
    prev.listNumber === next.listNumber &&
    sameCounts(prev.noteNumbers, next.noteNumbers) &&
    sameCounts(prev.fieldValues, next.fieldValues)
  );
}

const StaticParagraph = memo(function StaticParagraph({
  block,
  index,
  scope,
  zoom,
  page = 0,
  pages = 0,
  noteNumbers,
  showRevisions = true,
  listNumber,
  fieldValues,
  onOpen,
}: StaticParagraphProps) {
  const props = block.props;
  const listMarker = props.list ? orderedListMarker(props, listNumber) : null;
  // Callers with full document context pass the map; previews without it still
  // get page/pages/date/time.
  const fields = fieldValues ?? fieldValuesFor({ page, pages });
  return (
    <div
      className="para-row"
      data-block-index={index}
      data-scope={scope}
      {...(onOpen
        ? {
            role: "button",
            tabIndex: 0,
            onClick: onOpen,
            onKeyDown: (event: React.KeyboardEvent) => {
              if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                onOpen();
              }
            },
          }
        : { role: "presentation" })}
      style={{ marginLeft: props.list ? props.list.level * 24 : 0 }}
    >
      {listMarker ? <span className="list-marker">{listMarker}</span> : null}
      <div
        className={`para para-${props.style.toLowerCase()}${props.pageBreakBefore ? " page-break-before" : ""}`}
        style={{
          textAlign: props.align as "left" | "center" | "right" | "justify",
          lineHeight: props.lineSpacing,
          marginBottom: props.spaceAfterPt,
          marginTop: props.spaceBeforePt,
          textIndent: props.firstLinePt,
          fontSize: `${(effectiveFontSize(block) ?? 11) * zoom}pt`,
        }}
        dangerouslySetInnerHTML={{
          __html: runsToHtml(substituteTokens(block.runs, page, pages), {
            noteNumbers,
            showRevisions,
            fieldValues: fields,
          }),
        }}
      />
    </div>
  );
}, sameStaticParagraph);

function StaticTable({ table, zoom, from = 0, to }: { table: TableData; zoom: number; from?: number; to?: number }) {
  const end = to ?? table.rows.length;
  const bodyRows = table.rows.slice(from, end);
  const header = from > 0 && table.rows[0]?.header ? table.rows[0] : null;
  const renderCellBlocks = (blocks: Block[]) => (
    <StaticBlocks blocks={blocks} scope="cell" page={0} pages={0} zoom={zoom} onOpen={() => undefined} />
  );
  return (
    <div className="writer-table-wrap">
      <table
        className={`writer-table${table.borders ? "" : " no-borders"}`}
        style={{
          width: `${(table.columnWidthsPt.reduce((sum, value) => sum + value, 0) || 400) * (96 / 72) * zoom}px`,
        }}
      >
        <colgroup>
          {table.columnWidthsPt.map((width, columnIndex) => (
            <col key={columnIndex} style={{ width: `${width * (96 / 72) * zoom}px` }} />
          ))}
        </colgroup>
        <tbody>
          {[header, ...bodyRows]
            .filter((row): row is NonNullable<typeof row> => Boolean(row))
            .map((row, rowIndex) => (
              <tr key={rowIndex} className={row.header ? "is-header" : ""}>
                {row.cells.map((cell, cellIndex) => (
                  <td
                    key={cellIndex}
                    colSpan={Math.max(1, cell.colspan)}
                    rowSpan={Math.max(1, cell.rowspan)}
                    style={{
                      background: cell.background ?? undefined,
                      textAlign: (cell.align || "left") as "left" | "center" | "right",
                      verticalAlign: (cell.valign || "top") as "top" | "middle" | "bottom",
                    }}
                  >
                    {renderCellBlocks(cell.blocks)}
                  </td>
                ))}
              </tr>
            ))}
        </tbody>
      </table>
    </div>
  );
}

function TocView({ entries, onOpen }: { entries: TocEntry[]; onOpen: (anchor: number) => void }) {
  const t = useT();
  return (
    <nav className="writer-toc">
      <div className="writer-toc-title">{t("writer.tableOfContents")}</div>
      {entries.length === 0 ? <p className="muted">{t("writer.noHeadings")}</p> : null}
      {entries.map((entry) => (
        <button
          key={`${entry.anchor}-${entry.level}`}
          type="button"
          className="writer-toc-item"
          style={{ paddingLeft: (entry.level - 1) * 16 }}
          onClick={() => onOpen(entry.anchor)}
        >
          <span>{entry.text}</span>
          <span className="writer-toc-fill" />
          <span>{entry.page > 0 ? entry.page : ""}</span>
        </button>
      ))}
    </nav>
  );
}

interface StaticBlocksProps {
  blocks: Block[];
  scope: string;
  page: number;
  pages: number;
  zoom: number;
  noteNumbers?: Record<string, number>;
  showRevisions?: boolean;
  /** Ordered-list positions for `blocks` (index -> number). */
  listNumbers?: Map<number, number>;
  fieldValues?: Record<string, string>;
  onOpen: (index: number) => void;
}

/** Reference equality per block; unchanged blocks are the ones the probe reuses. */
function sameBlocks(prev: Block[], next: Block[]): boolean {
  return prev === next || (prev.length === next.length && prev.every((block, index) => block === next[index]));
}

function sameStaticBlocks(prev: StaticBlocksProps, next: StaticBlocksProps): boolean {
  return (
    prev.scope === next.scope &&
    sameBlocks(prev.blocks, next.blocks) &&
    prev.page === next.page &&
    prev.pages === next.pages &&
    prev.zoom === next.zoom &&
    prev.showRevisions === next.showRevisions &&
    prev.listNumbers === next.listNumbers &&
    sameCounts(prev.noteNumbers, next.noteNumbers) &&
    sameCounts(prev.fieldValues, next.fieldValues)
  );
}

export const StaticBlocks = memo(function StaticBlocks({
  blocks,
  scope,
  page,
  pages,
  zoom,
  noteNumbers,
  showRevisions = true,
  listNumbers,
  fieldValues,
  onOpen,
}: StaticBlocksProps) {
  return (
    <>
      {blocks.map((block, index) => {
        if (block.type === "paragraph") {
          return (
            <StaticParagraph
              key={index}
              block={block}
              index={index}
              scope={scope}
              zoom={zoom}
              page={page}
              pages={pages}
              noteNumbers={noteNumbers}
              showRevisions={showRevisions}
              listNumber={listNumbers?.get(index)}
              fieldValues={fieldValues}
              onOpen={() => onOpen(index)}
            />
          );
        }
        return (
          <div key={index} data-block-index={index} data-scope={scope}>
            {block.type === "table" ? <StaticTable table={block.table} zoom={zoom} /> : null}
            {block.type === "image" ? (
              <figure className="writer-image" style={{ textAlign: block.align as "left" | "center" | "right" }}>
                <img
                  src={`data:${block.image.mime};base64,${block.image.dataBase64}`}
                  alt={block.image.alt}
                  style={{ width: block.widthPt * (96 / 72) * zoom }}
                />
                <figcaption>{block.caption || block.image.name}</figcaption>
              </figure>
            ) : null}
            {block.type === "pageBreak" ? (
              <div className="writer-page-break">
                <span>— page break —</span>
              </div>
            ) : null}
            {block.type === "toc" ? <TocView entries={block.entries} onOpen={onOpen} /> : null}
            {block.type === "rule" ? <hr className="writer-rule" /> : null}
          </div>
        );
      })}
    </>
  );
}, sameStaticBlocks);

/** One page fragment: whole block, paragraph lines or table rows. */
export function PageFragmentView({
  fragment,
  block,
  zoom,
  page,
  pages,
  title,
  author,
  listNumbers,
  noteNumbers,
  showRevisions,
  active,
  onActivate,
  onExtendSelection,
  onSync,
  onStructure,
  onFocusParagraph,
  onRecordCaret,
  onCaretOut,
  onArrowAtEdge,
  onOpen,
}: {
  fragment: Fragment;
  block: Block | undefined;
  zoom: number;
  page: number;
  pages: number;
  title: string;
  author: string;
  listNumbers: Map<number, number>;
  noteNumbers?: Record<string, number>;
  showRevisions?: boolean;
  active: boolean;
  onActivate: (index: number, offset: number, fragment: Fragment, container: HTMLElement) => void;
  onExtendSelection: (index: number, anchor: number, x: number, y: number) => void;
  onSync: (element: HTMLElement) => void;
  onStructure: (action: StructureAction) => void;
  onFocusParagraph: (block: Extract<Block, { type: "paragraph" }>) => void;
  onRecordCaret: (index: number, offset: number, from: number) => void;
  onCaretOut: (direction: -1 | 1) => void;
  onArrowAtEdge: (direction: -1 | 1) => boolean;
  onOpen: (index: number, offset?: number) => void;
}) {
  // Anchor of a drag that started on a static fragment. The static copy cannot
  // extend the browser selection by itself, so the moves rebuild the range on
  // the active editable with pointer capture.
  const drag = useRef<number | null>(null);
  const fieldValues = useMemo(() => fieldValuesFor({ page, pages, title, author }), [page, pages, title, author]);
  if (!block) return null;

  const handlePointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    if (event.pointerType === "mouse" && event.button !== 0) return;
    const container = event.currentTarget;
    const editable = container.querySelector<HTMLElement>('[contenteditable="true"]');
    if (editable && editable.contains(event.target as Node)) return; // native caret and drags
    const para = container.querySelector<HTMLElement>(".para");
    if (!para) return;
    // `offsetFromPoint` hit-tests the paragraph, so continuation fragments map
    // the click to the character in the full block, not to the fragment start.
    const offset = offsetFromPoint(para, event.clientX, event.clientY) ?? 0;
    if (event.pointerType === "mouse") {
      // Claim the gesture: the browser would otherwise start a selection on
      // the static copy that the editable replaces a moment later.
      event.preventDefault();
      drag.current = offset;
      try {
        container.setPointerCapture(event.pointerId);
      } catch {
        // jsdom and some older webviews do not implement pointer capture.
      }
    }
    onActivate(fragment.index, offset, fragment, container);
  };

  const handlePointerMove = (event: React.PointerEvent<HTMLDivElement>) => {
    if (drag.current === null) return;
    onExtendSelection(fragment.index, drag.current, event.clientX, event.clientY);
  };

  const endDrag = () => {
    drag.current = null;
  };

  if (block.type === "paragraph") {
    const clipped = fragment.mode === "lines";
    return (
      <div
        className="writer-fragment"
        data-fragment-from={fragment.from}
        style={clipped ? { height: fragment.heightPx, overflow: "hidden" } : undefined}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
        onLostPointerCapture={endDrag}
      >
        <div style={clipped ? { marginTop: -fragment.offsetPx } : undefined}>
          {active ? (
            <PageEditableParagraph
              block={block}
              index={fragment.index}
              fragment={fragment}
              zoom={zoom}
              noteNumbers={noteNumbers}
              showRevisions={showRevisions}
              listNumber={listNumbers.get(fragment.index)}
              fieldValues={fieldValues}
              onSync={onSync}
              onStructure={onStructure}
              onFocusParagraph={onFocusParagraph}
              onRecordCaret={onRecordCaret}
              onCaretOut={onCaretOut}
              onArrowAtEdge={onArrowAtEdge}
            />
          ) : (
            <StaticParagraph
              block={block}
              index={fragment.index}
              scope="page"
              zoom={zoom}
              noteNumbers={noteNumbers}
              showRevisions={showRevisions}
              listNumber={listNumbers.get(fragment.index)}
              fieldValues={fieldValues}
            />
          )}
        </div>
      </div>
    );
  }
  if (fragment.mode === "rows" && block.type === "table") {
    return (
      <div
        className="writer-fragment"
        role="button"
        tabIndex={0}
        onClick={() => onOpen(fragment.index, 0)}
        onKeyDown={(event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            onOpen(fragment.index, 0);
          }
        }}
      >
        <StaticTable table={block.table} zoom={zoom} from={fragment.from} to={fragment.to} />
      </div>
    );
  }
  return (
    <div
      className="writer-fragment"
      role="button"
      tabIndex={0}
      onClick={() => onOpen(fragment.index, 0)}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onOpen(fragment.index, 0);
        }
      }}
    >
      <StaticBlocks
        blocks={[block]}
        scope="page"
        page={0}
        pages={0}
        zoom={zoom}
        noteNumbers={noteNumbers}
        showRevisions={showRevisions}
        onOpen={() => onOpen(fragment.index, 0)}
      />
    </div>
  );
}

/**
 * The block's editable paragraph, hosted by one page fragment.
 *
 * The element carries the *whole* paragraph; the fragment box clips it to the
 * page band, which is what lets the caret and the model text continue across a
 * page break. One active fragment per block keeps a single editing surface, so
 * typing, IME, paste and the native caret behave exactly like the continuous
 * surface and flow through `syncParagraph`.
 */
function PageEditableParagraph({
  block,
  index,
  fragment,
  zoom,
  noteNumbers,
  showRevisions,
  listNumber,
  fieldValues,
  onSync,
  onStructure,
  onFocusParagraph,
  onRecordCaret,
  onCaretOut,
  onArrowAtEdge,
}: {
  block: Extract<Block, { type: "paragraph" }>;
  index: number;
  fragment: Fragment;
  zoom: number;
  noteNumbers?: Record<string, number>;
  showRevisions?: boolean;
  listNumber?: number;
  fieldValues?: Record<string, string>;
  onSync: (element: HTMLElement) => void;
  onStructure: (action: StructureAction) => void;
  onFocusParagraph: (block: Extract<Block, { type: "paragraph" }>) => void;
  onRecordCaret: (index: number, offset: number, from: number) => void;
  onCaretOut: (direction: -1 | 1) => void;
  onArrowAtEdge: (direction: -1 | 1) => boolean;
}) {
  const t = useT();
  const ref = useRef<HTMLDivElement>(null);
  const [focused, setFocused] = useState(false);
  // Caret to restore after a structural change, consumed by the effect below.
  const pendingCaret = useRef<number | null>(null);
  const props = block.props;

  // Same contract as the continuous paragraph: while focused the browser owns
  // the DOM, but when the model and the screen disagree (a structural edit
  // rewrote the runs) the element is repainted and the caret restored.
  useLayoutEffect(() => {
    const element = ref.current;
    if (!element) return;
    const html = runsToHtml(block.runs, { noteNumbers, showRevisions, fieldValues });
    if (!focused) {
      if (element.innerHTML !== html) element.innerHTML = html;
      return;
    }
    const modelText = runsText(block.runs);
    const domText = runsText(domToRuns(element));
    if (modelText === domText && pendingCaret.current === null) return;
    const at = pendingCaret.current ?? caretOffset(element);
    pendingCaret.current = null;
    if (element.innerHTML !== html) element.innerHTML = html;
    setCaretOffset(element, Math.min(at, modelText.length));
  }, [block.runs, fieldValues, focused, noteNumbers, showRevisions]);

  // The caret can land on a line the fragment clips away (ArrowDown past the
  // visible band, typing at the page boundary). Hand the surface to the
  // neighbouring fragment instead of leaving the caret hidden. jsdom has no
  // layout, so the geometry guards keep tests on the no-op path.
  useLayoutEffect(() => {
    if (!focused || !ref.current) return;
    const element = ref.current;
    const selection = window.getSelection();
    if (!selection || selection.rangeCount === 0) return;
    const range = selection.getRangeAt(0);
    if (!element.contains(range.startContainer)) return;
    const caret = typeof range.getBoundingClientRect === "function" ? range.getBoundingClientRect() : null;
    const box = element.getBoundingClientRect();
    if (!caret || caret.height === 0 || box.height === 0) return;
    const y = caret.top - box.top;
    if (y >= fragment.offsetPx + fragment.heightPx) onCaretOut(1);
    else if (y < fragment.offsetPx) onCaretOut(-1);
  });

  const record = () => {
    const element = ref.current;
    if (element) onRecordCaret(index, caretOffset(element), fragment.from);
  };

  /** True when the collapsed caret sits on the first/last visible line. */
  const caretAtFragmentEdge = (element: HTMLElement, direction: -1 | 1): boolean => {
    const selection = window.getSelection();
    if (!selection || selection.rangeCount === 0) return false;
    const range = selection.getRangeAt(0);
    if (!element.contains(range.startContainer)) return false;
    const caret = typeof range.getBoundingClientRect === "function" ? range.getBoundingClientRect() : null;
    const box = element.getBoundingClientRect();
    if (!caret || caret.height === 0 || box.height === 0) return false;
    // Subpixel line boxes put the edge a fraction of a pixel off, so compare
    // with a one-pixel tolerance.
    const y = caret.top - box.top;
    if (direction > 0) return y + Math.max(1, caret.height) >= fragment.offsetPx + fragment.heightPx - 1;
    return y <= fragment.offsetPx + 1;
  };

  const handleKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    const element = ref.current;
    if (!element) return;
    if (
      (event.key === "ArrowUp" || event.key === "ArrowDown") &&
      !event.altKey &&
      !event.ctrlKey &&
      !event.metaKey &&
      !event.shiftKey
    ) {
      const direction = event.key === "ArrowDown" ? 1 : -1;
      // Only claim the key at the fragment edge and only when the block really
      // continues in that direction; otherwise the shared handler's
      // paragraph-end behaviour applies.
      if (caretAtFragmentEdge(element, direction) && onArrowAtEdge(direction)) {
        event.preventDefault();
        return;
      }
    }
    handleParagraphKeyDown(event, { element, block, index, onStructure, pendingCaret });
  };

  const listMarker = props.list ? orderedListMarker(props, listNumber) : null;
  return (
    <div className="para-row" style={{ marginLeft: props.list ? props.list.level * 24 : 0 }}>
      {listMarker ? (
        <span className="list-marker" contentEditable={false}>
          {listMarker}
        </span>
      ) : null}
      <div
        ref={ref}
        className={`para para-${props.style.toLowerCase()}${props.pageBreakBefore ? " page-break-before" : ""}`}
        data-block-index={index}
        data-scope="page"
        data-fragment-from={fragment.from}
        contentEditable
        role="textbox"
        aria-multiline="true"
        aria-label={`${t("writer.paragraph")} ${index + 1}`}
        suppressContentEditableWarning
        spellCheck
        tabIndex={0}
        onKeyDown={handleKeyDown}
        onKeyUp={record}
        onFocus={() => {
          setFocused(true);
          onFocusParagraph(block);
          record();
        }}
        onBlur={(event) => {
          setFocused(false);
          onSync(event.currentTarget);
        }}
        onInput={(event) => {
          record();
          onSync(event.currentTarget);
        }}
        style={{
          textAlign: props.align as "left" | "center" | "right" | "justify",
          lineHeight: props.lineSpacing,
          marginBottom: props.spaceAfterPt,
          marginTop: props.spaceBeforePt,
          textIndent: props.firstLinePt,
          fontSize: `${(effectiveFontSize(block) ?? 11) * zoom}pt`,
        }}
      />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Block rendering
// ---------------------------------------------------------------------------

export function BlockView({
  block,
  index,
  scope,
  zoom,
  noteNumbers,
  showRevisions,
  listNumber,
  fieldValues,
  selectedImage,
  onSelectImage,
  onFocusParagraph,
  onSync,
  onUpdate,
  onSyncCell,
  onStructure,
  onOpenBlock,
}: {
  block: Block;
  index: number;
  scope: string;
  zoom: number;
  noteNumbers: Record<string, number>;
  showRevisions: boolean;
  listNumber?: number;
  fieldValues?: Record<string, string>;
  selectedImage: number | null;
  onSelectImage: (index: number | null) => void;
  onFocusParagraph: (block: Extract<Block, { type: "paragraph" }>) => void;
  onSync: (element: HTMLElement) => void;
  onUpdate: (block: Block) => void;
  onSyncCell: (path: [number, number, number, number], element: HTMLElement) => void;
  onStructure: (action: StructureAction) => void;
  onOpenBlock: (index: number) => void;
}) {
  // Live size while an image handle is dragged. The model is only written on
  // pointerup, so the whole drag is one undo step.
  const [imagePreview, setImagePreview] = useState<{ widthPt: number; heightPt: number } | null>(null);
  if (block.type === "paragraph") {
    return (
      <ParagraphView
        block={block}
        index={index}
        scope={scope}
        zoom={zoom}
        noteNumbers={noteNumbers}
        showRevisions={showRevisions}
        listNumber={listNumber}
        fieldValues={fieldValues}
        onFocus={() => onFocusParagraph(block)}
        onSync={onSync}
        onUpdate={onUpdate}
        onStructure={onStructure}
      />
    );
  }
  if (block.type === "table") {
    return <TableView block={block} index={index} zoom={zoom} onUpdate={onUpdate} onSyncCell={onSyncCell} />;
  }
  if (block.type === "image") {
    const widthPt = imagePreview?.widthPt ?? block.widthPt;
    const heightPt = imagePreview?.heightPt ?? block.heightPt;
    return (
      <figure
        className={`writer-image${selectedImage === index ? " is-selected" : ""}`}
        style={{ textAlign: block.align as "left" | "center" | "right" }}
      >
        <span className="writer-image-frame">
          <button
            type="button"
            onClick={() => onSelectImage(index)}
            style={{ background: "none", border: "none", padding: 0, cursor: "pointer" }}
          >
            <img
              src={`data:${block.image.mime};base64,${block.image.dataBase64}`}
              alt={block.image.alt}
              style={{ width: widthPt * (96 / 72) * zoom, height: heightPt * (96 / 72) * zoom }}
            />
          </button>
          {selectedImage === index ? (
            <ImageSizeHandles
              widthPt={widthPt}
              heightPt={heightPt}
              zoom={zoom}
              onPreview={setImagePreview}
              onCommit={(next) => {
                setImagePreview(null);
                onUpdate({ ...block, widthPt: next.widthPt, heightPt: next.heightPt });
              }}
            />
          ) : null}
        </span>
        <figcaption>
          <button
            type="button"
            onClick={() => onSelectImage(index)}
            style={{
              background: "none",
              border: "none",
              padding: 0,
              font: "inherit",
              color: "inherit",
              cursor: "pointer",
            }}
          >
            {block.caption || block.image.name}
          </button>
        </figcaption>
      </figure>
    );
  }
  if (block.type === "pageBreak") {
    return (
      <div className="writer-page-break" contentEditable={false}>
        <span>— page break —</span>
      </div>
    );
  }
  if (block.type === "toc") {
    return <TocView entries={block.entries} onOpen={onOpenBlock} />;
  }
  return <hr className="writer-rule" />;
}

/**
 * The four corner grips of a selected image.
 *
 * A drag always preserves the aspect ratio (the horizontal delta drives the
 * width; the height follows), clamps to a 24pt minimum, previews locally and
 * commits the final size once on pointerup.
 */
function ImageSizeHandles({
  widthPt,
  heightPt,
  zoom,
  onPreview,
  onCommit,
}: {
  widthPt: number;
  heightPt: number;
  zoom: number;
  onPreview: (size: { widthPt: number; heightPt: number }) => void;
  onCommit: (size: { widthPt: number; heightPt: number }) => void;
}) {
  const dragRef = useRef<{
    startX: number;
    startWidthPt: number;
    directionX: 1 | -1;
    widthPt: number;
    heightPt: number;
    aspect: number;
  } | null>(null);

  useEffect(() => {
    const move = (event: PointerEvent) => {
      const drag = dragRef.current;
      if (!drag) return;
      const dx = ((event.clientX - drag.startX) * 72) / 96 / zoom;
      let width = Math.max(24, drag.startWidthPt + drag.directionX * dx);
      let height = width / drag.aspect;
      if (height < 24) {
        height = 24;
        width = height * drag.aspect;
      }
      drag.widthPt = width;
      drag.heightPt = height;
      onPreview({ widthPt: width, heightPt: height });
    };
    const up = () => {
      const drag = dragRef.current;
      dragRef.current = null;
      if (!drag) return;
      onCommit({ widthPt: drag.widthPt, heightPt: drag.heightPt });
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", up);
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", up);
    };
  });

  const start = (event: React.PointerEvent<HTMLSpanElement>, directionX: 1 | -1) => {
    event.preventDefault();
    event.stopPropagation();
    dragRef.current = {
      startX: event.clientX,
      startWidthPt: widthPt,
      directionX,
      widthPt,
      heightPt,
      aspect: widthPt / Math.max(1, heightPt),
    };
    try {
      event.currentTarget.setPointerCapture?.(event.pointerId);
    } catch {
      // Pointer capture is unavailable (jsdom, older webviews).
    }
  };

  const corners: Array<{ corner: string; directionX: 1 | -1 }> = [
    { corner: "nw", directionX: -1 },
    { corner: "ne", directionX: 1 },
    { corner: "sw", directionX: -1 },
    { corner: "se", directionX: 1 },
  ];
  return (
    <>
      {corners.map(({ corner, directionX }) => (
        <span
          key={corner}
          className={`writer-image-handle is-${corner}`}
          data-image-handle={corner}
          onPointerDown={(event) => start(event, directionX)}
        />
      ))}
    </>
  );
}

/**
 * The structural key handling shared by the continuous paragraph and the
 * paginated page editable.
 *
 * Normal typing is left to the browser; the keys that change the *structure* of
 * the document are intercepted here and reported upwards, because the model -
 * not the DOM - is the source of truth that DOCX/ODT export and PDF layout
 * read. Keeping one implementation means both surfaces stay in sync.
 */
function handleParagraphKeyDown(
  event: React.KeyboardEvent<HTMLElement>,
  {
    element,
    block,
    index,
    onStructure,
    pendingCaret,
  }: {
    element: HTMLElement;
    block: Extract<Block, { type: "paragraph" }>;
    index: number;
    onStructure: (action: StructureAction) => void;
    pendingCaret: { current: number | null };
  },
): void {
  if ((event.ctrlKey || event.metaKey) && event.key === "Enter") return; // page break, handled globally
  const mod = event.ctrlKey || event.metaKey;
  const offset = caretOffset(element);
  const range = selectedRange(element);
  const plain = domToRuns(element);
  const from = range ? range[0] : offset;
  const to = range ? range[1] : offset;

  switch (event.key) {
    case "Enter": {
      if (event.shiftKey) {
        // Shift+Enter is a hard line break inside the same paragraph. Building
        // it with replaceRange also removes a selection: the old split-based
        // path left the selected text in the model.
        event.preventDefault();
        onStructure({ kind: "replace", index, block: { ...block, runs: replaceRange(plain, from, to, "\n") } });
        // The repaint below puts the caret after the new line break.
        pendingCaret.current = from + 1;
        return;
      }
      event.preventDefault();
      onStructure({ kind: "split", index, offset: from, to });
      return;
    }
    case "Backspace": {
      if (from !== 0 || to !== 0) {
        if (mod) {
          event.preventDefault();
          const [start] = wordRangeAt(plain, from, "backward");
          onStructure({ kind: "replace", index, block: { ...block, runs: replaceRange(plain, start, from, "") } });
          pendingCaret.current = start;
          return;
        }
        if (range) {
          event.preventDefault();
          onStructure({ kind: "replace", index, block: { ...block, runs: replaceRange(plain, from, to, "") } });
          pendingCaret.current = from;
          return;
        }
        return; // normal character delete, the browser handles it
      }
      event.preventDefault();
      onStructure({ kind: "mergeBackward", index });
      return;
    }
    case "Delete": {
      const text = runsText(plain);
      if (to < text.length) {
        if (mod) {
          event.preventDefault();
          const [, end] = wordRangeAt(plain, to, "forward");
          onStructure({ kind: "replace", index, block: { ...block, runs: replaceRange(plain, from, end, "") } });
          return;
        }
        if (range) {
          event.preventDefault();
          onStructure({ kind: "replace", index, block: { ...block, runs: replaceRange(plain, from, to, "") } });
          pendingCaret.current = from;
          return;
        }
        return;
      }
      event.preventDefault();
      onStructure({ kind: "mergeForward", index });
      return;
    }
    case "Tab": {
      event.preventDefault();
      onStructure({ kind: event.shiftKey ? "outdent" : "indent", index });
      return;
    }
    case "ArrowUp":
      if (caretOnFirstLine(element)) {
        event.preventDefault();
        onStructure({ kind: "moveCaret", index, delta: -1, atLine: "start" });
      }
      return;
    case "ArrowDown":
      if (caretOnLastLine(element)) {
        event.preventDefault();
        onStructure({ kind: "moveCaret", index, delta: 1, atLine: "end" });
      }
      return;
    case "Home":
      if (!mod) {
        event.preventDefault();
        setCaretOffset(element, 0);
      }
      return;
    case "End": {
      if (!mod) {
        event.preventDefault();
        setCaretOffset(element, runsText(plain).length);
      }
      return;
    }
    default:
      return;
  }
}

/**
 * One editable paragraph.
 *
 * Normal typing is left to the browser. The keys that change the *structure* of
 * the document are intercepted here and reported upwards, because the model -
 * not the DOM - is the source of truth that DOCX/ODT export and PDF layout read.
 */
function ParagraphView({
  block,
  index,
  scope,
  zoom,
  noteNumbers,
  showRevisions,
  listNumber,
  fieldValues,
  onFocus,
  onSync,
  onStructure,
}: {
  block: Extract<Block, { type: "paragraph" }>;
  index: number;
  scope: string;
  zoom: number;
  noteNumbers: Record<string, number>;
  showRevisions: boolean;
  listNumber?: number;
  fieldValues?: Record<string, string>;
  onFocus: () => void;
  onSync: (element: HTMLElement) => void;
  onUpdate: (block: Block) => void;
  onStructure: (action: StructureAction) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [focused, setFocused] = useState(false);
  const props = block.props;
  // Caret to restore after a structural change, consumed by the effect below.
  const pendingCaret = useRef<number | null>(null);

  // Layout effect: the parent places the caret in its own layout effect right
  // after this one, so the content has to be in the DOM first. Child layout
  // effects run before the parent's, which makes that ordering guaranteed.
  useLayoutEffect(() => {
    if (!ref.current) return;
    const html = runsToHtml(block.runs, { noteNumbers, showRevisions, fieldValues });
    if (!focused) {
      if (ref.current.innerHTML !== html) ref.current.innerHTML = html;
      return;
    }
    // While focused the browser owns the DOM, but a structural change (Enter,
    // Backspace merge) rewrote the runs underneath us. When the text on screen
    // and the model disagree, the DOM is stale: repaint it and restore the
    // caret, otherwise the next blur writes the stale text back over the edit.
    const modelText = runsText(block.runs);
    const domText = runsText(domToRuns(ref.current));
    if (modelText === domText && pendingCaret.current === null) return;
    const at = pendingCaret.current ?? caretOffset(ref.current);
    pendingCaret.current = null;
    if (ref.current.innerHTML !== html) ref.current.innerHTML = html;
    setCaretOffset(ref.current, Math.min(at, modelText.length));
  }, [block.runs, fieldValues, focused, noteNumbers, showRevisions]);

  const heading = props.style.startsWith("Heading");
  const Tag = (heading ? `h${Math.min(6, Number(props.style.replace("Heading", "")) || 1)}` : "div") as "div";
  const listMarker = props.list ? orderedListMarker(props, listNumber) : null;

  const handleKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    handleParagraphKeyDown(event, { element: event.currentTarget, block, index, onStructure, pendingCaret });
  };

  return (
    <div className="para-row" style={{ marginLeft: props.list ? props.list.level * 24 : 0 }}>
      {listMarker ? (
        <span className="list-marker" contentEditable={false}>
          {listMarker}
        </span>
      ) : null}
      <Tag
        ref={ref as never}
        className={`para para-${props.style.toLowerCase()}${props.pageBreakBefore ? " page-break-before" : ""}`}
        data-block-index={index}
        data-scope={scope}
        contentEditable
        suppressContentEditableWarning
        spellCheck
        onKeyDown={handleKeyDown}
        onFocus={() => {
          setFocused(true);
          onFocus();
        }}
        onBlur={(event) => {
          setFocused(false);
          onSync(event.currentTarget);
        }}
        onInput={(event) => {
          // Keep the model in sync while typing (cheap: paragraph runs only).
          onSync(event.currentTarget);
        }}
        style={{
          textAlign: props.align as "left" | "center" | "right" | "justify",
          lineHeight: props.lineSpacing,
          marginBottom: props.spaceAfterPt,
          marginTop: props.spaceBeforePt,
          textIndent: props.firstLinePt,
          fontSize: `${(effectiveFontSize(block) ?? 11) * zoom}pt`,
        }}
      />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Comment composer
// ---------------------------------------------------------------------------

export function CommentReplyInput({
  placeholder,
  onSubmit,
}: {
  placeholder: string;
  onSubmit: (text: string) => void;
}) {
  const [text, setText] = useState("");
  return (
    <div className="row">
      <input value={text} placeholder={placeholder} onChange={(event) => setText(event.target.value)} />
      <button
        type="button"
        className="btn btn-soft"
        disabled={!text.trim()}
        onClick={() => {
          onSubmit(text.trim());
          setText("");
        }}
      >
        +
      </button>
    </div>
  );
}
