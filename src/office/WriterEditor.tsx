/**
 * Writer editor: a page-aware rich text editor on the shared document model.
 *
 * Editing model: each paragraph is a contentEditable surface edited natively
 * (so caret behaviour, IME and clipboard work), then synchronised back into
 * the model runs on input. Structural changes (lists, tables, images, page
 * setup, styles) mutate the model directly, which keeps DOCX/ODT export exact.
 */
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { flushSync } from "react-dom";
import { pickFileBytes } from "../lib/mobile";
import {
  AlignCenter,
  AlignJustify,
  AlignLeft,
  AlignRight,
  Bold,
  Eraser,
  FileDown,
  FolderOpen,
  FileText,
  Highlighter,
  Image as ImageIcon,
  Indent,
  Italic,
  Languages,
  Link2,
  List,
  ListOrdered,
  Maximize2,
  Minimize2,
  Minus,
  Outdent,
  Printer,
  Redo2,
  Save,
  Search,
  SeparatorHorizontal,
  Sparkles,
  SpellCheck,
  Strikethrough,
  Table as TableIcon,
  Underline,
  Undo2,
  MessageSquare,
  Columns2,
  Trash2,
  Wand2,
  X,
} from "lucide-react";
import type { OfficeTab, TextDocument } from "../lib/office-store";
import { useOfficeTabs } from "../lib/office-store";
import { useToasts, reportError } from "../lib/store";
import { useT } from "../lib/i18n";
import {
  uid,
  wordCount,
  type Block,
  type DocComment,
  type FieldRef,
  type Footnote,
  type ImageData,
  type ParaProps,
  type Run,
  type SectionProps,
  type TableData,
  type TocEntry,
} from "../lib/office-types";
import {
  defaultPageSetup,
  defaultParaProps,
  defaultSectionProps,
  documentSections,
  emptyMetadata,
  newFootnote,
  newParaBlock,
  newTextDocument,
  sectionForBlock,
} from "../lib/office-types";
import {
  Dialog,
  Ribbon,
  RibbonGroup,
  ToolButton,
  ToolColor,
  ToolNumber,
  ToolSelect,
  useTablePicker,
} from "./office-ui";
import { openIntoWorkspace, useEditorShortcuts, useOfficeSession } from "./useOfficeSession";
import {
  acceptAll,
  acceptRevision,
  nextRevision,
  rejectAll,
  rejectRevision,
  revisionList,
  trackRunChanges,
} from "./writer/revisions";
import {
  emptyHistory,
  record as recordHistory,
  redo as redoHistory,
  undo as undoHistory,
  type HistoryState,
} from "./writer/history";
import {
  joinRuns,
  nextListLevel,
  nextParagraphProps,
  replaceRange,
  runsText,
  splitRuns,
  wordRangeAt,
} from "./writer/runs";
import {
  caretOffset,
  caretOnFirstLine,
  caretOnLastLine,
  offsetFromPoint,
  paragraphAtPoint,
  repaintParagraph,
  selectedRange,
  setCaretOffset,
  setSelectionRange,
} from "./writer/caret";
import {
  domToRuns,
  fieldValuesFor,
  orderedListMarker,
  orderedListNumbers,
  runsToHtml,
  wrapCellRuns,
} from "./writer/writerDom";
import { emptyRun as emptyWriterRun } from "./writer/runs";
import {
  compileSearch,
  documentMatches,
  documentTexts,
  MATCH_LIMIT,
  nextMatchIndex,
  replaceAllInDocument,
  replaceMatch,
  sameMatch,
  type MatchLocation,
} from "./writer/find-replace";
import { canProbeRegex, probeRegex, type ProbeStatus, type RegexProbe } from "./writer/regex-probe";
import { StyleGallery } from "./writer/StyleGallery";
import { applyAiText } from "./writer/ai-edit";
import { WriterAiDialog, type WriterAiTask } from "./writer/WriterAiDialog";
import { AI_EDIT_MAX_CHARS, useAiStatus } from "./ai/editor-ai";
import type { AiEditResult } from "../lib/types";
import { measureBlocks } from "./writer/measure";
import { paginate, pageOfBlock, type Fragment, type PageLayout } from "./writer/pagination";
import { LayoutList, ListTree, ListOrdered as TocIcon, RefreshCw } from "lucide-react";

export { runsToHtml, domToRuns };

type WriterTab = OfficeTab & { model: TextDocument };

/** The block whose editable paragraph is hosted by a page fragment (V3.1). */
interface PageEditState {
  /** Block index owning the editable. */
  block: number;
  /** Fragment line index hosting it, or null to pick by `edge` after a reflow. */
  from: number | null;
  /** Character offset the caret was placed at. */
  offset: number;
  /** Which end of the block the caret is at when no fragment matches `from`. */
  edge: "start" | "end";
}

/** Structural edits the paragraph component asks the document to perform. */
type StructureAction =
  | { kind: "replace"; index: number; block: Block }
  | { kind: "split"; index: number; offset: number; to: number }
  | { kind: "mergeBackward"; index: number }
  | { kind: "mergeForward"; index: number }
  | { kind: "indent" | "outdent"; index: number }
  | { kind: "moveCaret"; index: number; delta: number; atLine: "start" | "end" };

interface SelectionInfo {
  paragraph: ParaProps;
  run: Run;
}

const HIGHLIGHT_COLORS = ["#FEF08A", "#BBF7D0", "#BFDBFE", "#FBCFE8", "#FED7AA", "#E9D5FF", "#FECACA", "#A7F3D0"];

/**
 * Footnote area reservation: notes are rendered at 8.5pt with a 1.0 line
 * height, so a rough line count is accurate enough for pagination while the
 * real text is drawn by the same renderer that draws the page.
 */
function estimateNoteHeight(note: Footnote, contentWidthPx: number): number {
  const text = note.runs.map((run) => run.text).join("");
  const charsPerLine = Math.max(40, Math.floor(contentWidthPx / 4.6));
  const lines = Math.max(1, Math.ceil((text.length + 4) / charsPerLine));
  return lines * 13 + 6;
}

/** Footnote number in reference order (1-based). */
function noteNumber(document: TextDocument, id: string): number {
  const order: string[] = [];
  const collect = (runs: Run[]) => {
    for (const run of runs) {
      const reference = run.footnote ?? run.endnote;
      if (reference && !order.includes(reference)) order.push(reference);
    }
  };
  for (const block of document.blocks) {
    if (block.type === "paragraph") collect(block.runs);
    if (block.type === "table")
      for (const row of block.table.rows)
        for (const cell of row.cells)
          for (const inner of cell.blocks) if (inner.type === "paragraph") collect(inner.runs);
  }
  return order.indexOf(id) + 1;
}

const AI_RIBBON: Array<{ task: WriterAiTask; icon: typeof Wand2 }> = [
  { task: "rewrite", icon: Wand2 },
  { task: "shorten", icon: Minimize2 },
  { task: "expand", icon: Maximize2 },
  { task: "fix", icon: SpellCheck },
  { task: "translate", icon: Languages },
  { task: "tone", icon: Sparkles },
];

export function WriterEditor({ tab }: { tab: WriterTab }) {
  const t = useT();
  const { edit } = useOfficeTabs();
  const session = useOfficeSession(tab);
  const [ribbon, setRibbon] = useState("home");
  const [zoom, setZoom] = useState(1);
  const [selection, setSelection] = useState<SelectionInfo | null>(null);
  const [editingHeader, setEditingHeader] = useState<"header" | "footer" | null>(null);
  const [findOpen, setFindOpen] = useState(false);
  const [findText, setFindText] = useState("");
  const [replaceText, setReplaceText] = useState("");
  const [matchCase, setMatchCase] = useState(false);
  const [wholeWord, setWholeWord] = useState(false);
  const [useRegex, setUseRegex] = useState(false);
  // The match Find next/previous last selected; Replace acts on it.
  const [currentMatch, setCurrentMatch] = useState<MatchLocation | null>(null);
  const [commentsOpen, setCommentsOpen] = useState(false);
  const [insertTable, setInsertTable] = useState(false);
  const [selectedImage, setSelectedImage] = useState<number | null>(null);
  const [measuredPageCount, setMeasuredPageCount] = useState(1);
  // V2.5: a real paginated view. The page layout is measured from a hidden
  // probe column and computed by the pagination engine; "continuous" keeps the
  // pre-2.5 editing surface for users who prefer it.
  const [view, setView] = useState<"paginated" | "continuous">("paginated");
  const [pages, setPages] = useState<PageLayout[]>([
    { fragments: [], usedPx: 0, continuation: false, sectionIndex: 0, noteHeightPx: 0, sectionPage: 1 },
  ]);
  const pageCount = view === "paginated" ? pages.length : measuredPageCount;
  const [navOpen, setNavOpen] = useState(false);
  const [reviewOpen, setReviewOpen] = useState(false);
  const [sectionsOpen, setSectionsOpen] = useState(false);
  const [fieldOpen, setFieldOpen] = useState(false);
  const [fieldKind, setFieldKind] = useState<FieldRef["kind"]>("ref");
  const [fieldTarget, setFieldTarget] = useState("");
  const [activeRevision, setActiveRevision] = useState<string | null>(null);
  const [bookmarkName, setBookmarkName] = useState("");
  const [layoutVersion, setLayoutVersion] = useState(0);
  const probeRef = useRef<HTMLDivElement>(null);
  const bodyRef = useRef<HTMLDivElement>(null);
  // Caret the model wants placed after a structural edit. It is consumed by
  // the layout effect below, in the same commit that renders the new blocks.
  const pendingFocus = useRef<{ index: number; offset: number; scope: string } | null>(null);
  // V3.1: the paginated view edits where the page is. `pageEdit` names the
  // block whose real editable paragraph is hosted by one of its fragments; the
  // fragment renders the whole paragraph shifted by `fragment.offsetPx` and
  // clipped by the page, exactly like the static preview did. The caret request
  // below is separate from `pendingFocus` because it targets `data-scope="page"`.
  const [pageEdit, setPageEdit] = useState<PageEditState | null>(null);
  const pageEditGuard = `${view}|${editingHeader ?? ""}`;
  const [lastPageEditGuard, setLastPageEditGuard] = useState(pageEditGuard);
  if (lastPageEditGuard !== pageEditGuard) {
    setLastPageEditGuard(pageEditGuard);
    if (view !== "paginated" || editingHeader) setPageEdit(null);
  }
  const pendingPageFocus = useRef<{ block: number; offset: number } | null>(null);
  const insertPageBreakRef = useRef<() => void>(() => undefined);
  const insertTableBlockRef = useRef<(rows: number, cols: number) => void>(() => undefined);
  // Last (block, offset) recorded before a model update. A reflow can remount
  // the editable when fragment boundaries move; this is what the restore in the
  // layout effect uses so the caret never jumps to the start.
  const pageCaret = useRef<{ block: number; offset: number; from: number } | null>(null);
  const picker = useTablePicker();

  const document = tab.model;
  const revisionAuthor = document.metadata.author.trim() || "You";
  // Ordered-list numbering and live field values (page/pages/date/time/
  // title/author) are derived at render time instead of freezing at insertion
  // (audit M16: every ordered item showed "1" and PAGE never refreshed).
  const listNumbers = useMemo(() => orderedListNumbers(document.blocks), [document.blocks]);
  const documentFields = useMemo(
    () => fieldValuesFor({ pages: pageCount, title: document.metadata.title, author: document.metadata.author }),
    [pageCount, document.metadata.title, document.metadata.author],
  );
  const sections = useMemo(() => documentSections(document), [document]);
  const stats = useMemo(() => wordCount(document), [document]);
  const noteNumbers = useMemo(() => {
    const map: Record<string, number> = {};
    for (const note of [...(document.footnotes ?? []), ...(document.endnotes ?? [])]) {
      const number = noteNumber(document, note.id);
      if (number > 0) map[note.id] = number;
    }
    return map;
  }, [document]);

  useEditorShortcuts(session, { onFind: () => setFindOpen(true), onReplace: () => setFindOpen(true) });

  // Places the caret for a structural edit as part of the same commit. A
  // `setTimeout` version raced fast typing: the next keystroke reached the DOM
  // before focus had moved and was lost.
  useLayoutEffect(() => {
    const request = pendingFocus.current;
    if (request) {
      // Pagination lags the model by one commit, so the target paragraph may
      // still be missing; keep the request for the next pass instead of losing
      // the caret. Static page fragments share the scope/block attributes, so
      // the page target must be pinned to the contentEditable.
      const target = window.document.querySelector<HTMLElement>(
        request.scope === "page"
          ? `[data-scope="page"][data-block-index="${request.index}"][contenteditable="true"]`
          : `[data-scope="${request.scope}"][data-block-index="${request.index}"]`,
      );
      if (target) {
        pendingFocus.current = null;
        target.focus();
        setCaretOffset(target, request.offset);
      }
    }

    const pageRequest = pendingPageFocus.current;
    if (pageRequest) {
      const target = window.document.querySelector<HTMLElement>(
        `[data-scope="page"][data-block-index="${pageRequest.block}"][contenteditable="true"]`,
      );
      if (target) {
        pendingPageFocus.current = null;
        target.focus();
        setCaretOffset(target, pageRequest.offset);
        // Cross-page caret moves can land on a sheet outside the viewport;
        // `nearest` leaves an already visible page alone.
        target.scrollIntoView({ block: "nearest" });
        pageCaret.current = { block: pageRequest.block, offset: pageRequest.offset, from: pageEdit?.from ?? -1 };
      }
    }

    // A reflow remounts the page editable when fragment boundaries move. The
    // removed node does not fire a usable blur, so focus falls back to the
    // body; take it back at the (block, offset) recorded before the update.
    const active = pageEdit;
    const last = pageCaret.current;
    if (active && last && last.block === active.block) {
      const target = window.document.querySelector<HTMLElement>(
        `[data-scope="page"][data-block-index="${active.block}"][contenteditable="true"]`,
      );
      const focused = window.document.activeElement;
      if (target && (focused === null || focused === window.document.body)) {
        target.focus();
        setCaretOffset(target, last.offset);
      }
    }
  });

  // Ctrl+Enter inserts a real page break at the caret. The ref keeps the
  // listener stable while still reaching the latest insert helper.
  const undoRef = useRef<() => void>(() => {});
  const redoRef = useRef<() => void>(() => {});
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const mod = event.ctrlKey || event.metaKey;
      if (mod && event.key === "Enter") {
        event.preventDefault();
        insertPageBreakRef.current();
        return;
      }
      // Model-level undo/redo (Ctrl/Cmd+Z, Ctrl+Y, Ctrl/Cmd+Shift+Z). The
      // browser's native undo cannot see structural edits, so it is replaced.
      if (mod && (event.key === "z" || event.key === "Z")) {
        event.preventDefault();
        if (event.shiftKey) redoRef.current();
        else undoRef.current();
        return;
      }
      if (mod && (event.key === "y" || event.key === "Y")) {
        event.preventDefault();
        redoRef.current();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // Model-level undo/redo. The browser's own `execCommand("undo")` only undoes
  // typed DOM state and cannot see structural/format changes, so the editor
  // keeps its own bounded history of model snapshots. Every model mutation
  // goes through `recordHistory`, so Ctrl+Z / the toolbar always undo the last
  // real edit.
  const historyRef = useRef<HistoryState>(emptyHistory());
  const [historyDepth, setHistoryDepth] = useState({ undo: 0, redo: 0 });

  const applyModel = useCallback(
    (before: TextDocument, model: TextDocument) => {
      historyRef.current = recordHistory(historyRef.current, before, {
        label: "edit",
        operations: [{ kind: "replaceDocument", before, after: model }],
        after: model,
      });
      setHistoryDepth({ undo: historyRef.current.undo.length, redo: historyRef.current.redo.length });
      edit(tab.id, () => model);
    },
    [edit, tab.id],
  );

  // `update` reads the model the editor actually renders, so it never applies
  // an edit on top of a stale snapshot.
  const update = useCallback(
    (mutate: (document: TextDocument) => TextDocument) => {
      applyModel(document, mutate(document));
    },
    [applyModel, document],
  );

  const undoEdit = useCallback(() => {
    const result = undoHistory(historyRef.current);
    if (!result) return;
    historyRef.current = result.state;
    setHistoryDepth({ undo: result.state.undo.length, redo: result.state.redo.length });
    edit(tab.id, () => result.model);
  }, [edit, tab.id]);

  const redoEdit = useCallback(() => {
    const result = redoHistory(historyRef.current);
    if (!result) return;
    historyRef.current = result.state;
    setHistoryDepth({ undo: result.state.undo.length, redo: result.state.redo.length });
    edit(tab.id, () => result.model);
  }, [edit, tab.id]);

  // The global key listener stays mounted across renders, so it reaches the
  // latest undo/redo through refs updated in an effect (never during render).
  // The editor is remounted per tab (`key={active.id}` in the workspace), so
  // the history starts empty for every document without a reset effect.
  useEffect(() => {
    undoRef.current = undoEdit;
    redoRef.current = redoEdit;
  }, [undoEdit, redoEdit]);

  const contentWidthPx =
    (document.page.widthPt - document.page.marginLeftPt - document.page.marginRightPt) * (96 / 72) * zoom;
  const contentHeightPx =
    (document.page.heightPt - document.page.marginTopPt - document.page.marginBottomPt) * (96 / 72) * zoom;

  // Real pagination: the probe renders every block at the exact content width,
  // the engine measures its line and row boxes and splits them into pages.
  useLayoutEffect(() => {
    if (view !== "paginated" || editingHeader) return;
    const probe = probeRef.current;
    if (!probe) return;
    const metrics = measureBlocks(probe, document.blocks);
    const sections = documentSections(document);
    // V3: each block knows its section; a section break switches the geometry
    // and the following pages use that section's page setup. Footnote text is
    // reserved at the bottom of the page that references it.
    let breakCount = 0;
    const enriched = metrics.map((metric) => {
      const block = document.blocks[metric.index];
      if (block?.type === "sectionBreak") {
        breakCount += 1;
        const section = sections[breakCount] ?? sections[sections.length - 1];
        return { ...metric, kind: "sectionBreak" as const, sectionIndex: breakCount, sectionStart: section.start };
      }
      const section = sections[breakCount] ?? sections[0];
      let noteHeightPx = 0;
      if (block?.type === "paragraph") {
        for (const run of block.runs) {
          const note = run.footnote
            ? document.footnotes?.find((candidate) => candidate.id === run.footnote)
            : run.endnote
              ? document.endnotes?.find((candidate) => candidate.id === run.endnote)
              : undefined;
          if (note) noteHeightPx += estimateNoteHeight(note, contentWidthPx);
        }
      }
      return { ...metric, sectionIndex: breakCount, footnoteHeightPx: noteHeightPx, sectionStart: section.start };
    });
    const sectionHeights = sections.map((section) =>
      Math.max(
        120,
        (section.page.heightPt - section.page.marginTopPt - section.page.marginBottomPt) * (96 / 72) * zoom,
      ),
    );
    setPages(paginate(enriched, Math.max(120, contentHeightPx), { sectionHeights, noteAreaRatio: 0.45 }));
  }, [view, editingHeader, document, contentWidthPx, contentHeightPx, zoom, layoutVersion]);

  // The status bar shows the laid-out page count in the paginated view and the
  // height estimate in the continuous one.
  useEffect(() => {
    if (view === "paginated") return;
    if (!bodyRef.current) return;
    const height = bodyRef.current.scrollHeight;
    const pageHeight = document.page.heightPt * (96 / 72);
    setMeasuredPageCount(Math.max(1, Math.ceil(height / Math.max(200, pageHeight))));
  }, [document, zoom, view, pages]);

  useEffect(() => {
    const handler = (event: Event) => {
      const detail = (event as CustomEvent<{ rows: number; cols: number }>).detail;
      insertTableBlockRef.current(detail.rows, detail.cols);
      setInsertTable(false);
    };
    window.addEventListener("oswk-insert-table", handler);
    return () => window.removeEventListener("oswk-insert-table", handler);
  });

  // -------------------------------------------------------------------------
  // Model helpers
  // -------------------------------------------------------------------------

  const currentBlocks = () =>
    editingHeader ? (editingHeader === "header" ? document.header : document.footer) : document.blocks;

  const withBlocks = (blocks: Block[]) =>
    update((doc) =>
      editingHeader === "header"
        ? { ...doc, header: blocks }
        : editingHeader === "footer"
          ? { ...doc, footer: blocks }
          : { ...doc, blocks },
    );

  const updateBlock = (index: number, block: Block) => {
    const blocks = [...currentBlocks()];
    blocks[index] = block;
    withBlocks(blocks);
  };

  const insertBlockAfter = (index: number, block: Block) => {
    const blocks = [...currentBlocks()];
    blocks.splice(index + 1, 0, block);
    withBlocks(blocks);
  };

  const removeBlock = (index: number) => {
    const blocks = currentBlocks().filter((_, position) => position !== index);
    withBlocks(blocks.length > 0 ? blocks : [newParaBlock()]);
  };
  void removeBlock;

  // -------------------------------------------------------------------------
  // Selection tracking (focusin gives the edited paragraph)
  // -------------------------------------------------------------------------

  const handleParagraphFocus = (block: Extract<Block, { type: "paragraph" }>) => {
    setSelection({ paragraph: block.props, run: block.runs[0] ?? emptyRun() });
  };

  const activeIndex = (): number | null => {
    const active = window.document.activeElement as HTMLElement | null;
    if (active?.dataset?.blockIndex) return Number(active.dataset.blockIndex);
    return null;
  };

  const activeParagraph = (): Extract<Block, { type: "paragraph" }> | null => {
    const index = activeIndex();
    if (index === null) return null;
    const block = currentBlocks()[index];
    return block?.type === "paragraph" ? block : null;
  };

  // -------------------------------------------------------------------------
  // AI actions on the selection / current paragraph
  // -------------------------------------------------------------------------

  const aiStatus = useAiStatus();
  const [aiJob, setAiJob] = useState<{
    task: WriterAiTask;
    index: number;
    from: number;
    to: number;
    original: string;
    /** Paragraph runs the request was based on (DOM state when the model lags). */
    baseRuns: Run[];
    wholeParagraph: boolean;
  } | null>(null);

  const startAi = (task: WriterAiTask) => {
    const push = useToasts.getState().push;
    const active = window.document.activeElement as HTMLElement | null;
    const scope = active?.dataset?.scope;
    if (editingHeader || !active?.dataset?.blockIndex || (scope !== "body" && scope !== "page")) {
      push({ kind: "info", title: t("ai.edit.noTarget") });
      return;
    }
    const index = Number(active.dataset.blockIndex);
    const block = document.blocks[index];
    if (block?.type !== "paragraph") {
      push({ kind: "info", title: t("ai.edit.noTarget") });
      return;
    }
    const domRuns = domToRuns(active);
    const baseRuns = domRuns.length > 0 ? domRuns : block.runs;
    const text = runsText(baseRuns);
    const range = selectedRange(active);
    const [from, to] = range && range[1] > range[0] ? [range[0], Math.min(range[1], text.length)] : [0, text.length];
    const original = text.slice(from, to);
    if (!original.trim()) {
      push({ kind: "info", title: t("ai.edit.noText") });
      return;
    }
    if (original.length > AI_EDIT_MAX_CHARS) {
      push({ kind: "error", title: t("ai.edit.tooLong", { max: AI_EDIT_MAX_CHARS }) });
      return;
    }
    setAiJob({ task, index, from, to, original, baseRuns, wholeParagraph: !range || range[1] <= range[0] });
  };

  /** Accepting is one undoable `updateBlock` transaction on one paragraph. */
  const acceptAi = (result: AiEditResult) => {
    const job = aiJob;
    if (!job) return;
    setAiJob(null);
    const current = document.blocks[job.index];
    if (current?.type !== "paragraph" || runsText(current.runs) !== runsText(job.baseRuns)) {
      useToasts.getState().push({ kind: "error", title: t("ai.edit.docChanged") });
      return;
    }
    let runs = applyAiText(current.runs, job.from, job.to, result.text);
    if (document.trackChanges) runs = trackRunChanges(current.runs, runs, revisionAuthor);
    const after: Block = { ...current, runs };
    const blocks = document.blocks.map((block, position) => (position === job.index ? after : block));
    const model: TextDocument = { ...document, blocks };
    historyRef.current = recordHistory(historyRef.current, document, {
      label: `ai-${job.task}`,
      operations: [{ kind: "updateBlock", index: job.index, before: current, after }],
      after: model,
    });
    setHistoryDepth({ undo: historyRef.current.undo.length, redo: historyRef.current.redo.length });
    edit(tab.id, () => model);
  };

  const applyParaChange = (patch: Partial<ParaProps>) => {
    const index = activeIndex();
    if (index === null) return;
    const block = currentBlocks()[index];
    if (block?.type !== "paragraph") return;
    updateBlock(index, { ...block, props: { ...block.props, ...patch } });
  };

  const applyRunChange = (patch: Partial<Run>) => {
    const index = activeIndex();
    if (index === null) return;
    const block = currentBlocks()[index];
    if (block?.type !== "paragraph") return;
    const runs = (block.runs.length > 0 ? block.runs : [emptyRun()]).map((run) => ({ ...run, ...patch }));
    updateBlock(index, { ...block, runs });
  };

  const exec = (command: string, value?: string) => {
    try {
      window.document.execCommand(command, false, value);
    } catch {
      // execCommand can throw on unsupported commands in some webviews.
    }
    // Sync after the browser applies the change.
    window.setTimeout(() => {
      const active = window.document.activeElement as HTMLElement | null;
      if (active && active.dataset.blockIndex) {
        syncParagraph(Number(active.dataset.blockIndex), active);
      }
    }, 0);
  };

  // -------------------------------------------------------------------------
  // DOM <-> model synchronisation
  // -------------------------------------------------------------------------

  const syncParagraph = (index: number, element: HTMLElement) => {
    const block = currentBlocks()[index];
    if (!block) return;
    if (block.type === "paragraph") {
      let runs = domToRuns(element);
      // Suggest mode: text that changed since the last sync is recorded as a
      // tracked insertion/deletion instead of being applied silently.
      if (document.trackChanges && !editingHeader) {
        // Every run is passed in, including pending deletions: `trackRunChanges`
        // treats them as invisible to the diff and carries them through, so
        // typing with "show revisions" off can no longer silently accept a
        // pending deletion.
        runs = trackRunChanges(block.runs, runs, revisionAuthor);
      }
      const nextRuns = runs.length > 0 ? runs : [emptyRun()];
      // A blur that changed nothing must not push an undo step: it would sit
      // on top of the real edit (typing, an accepted AI suggestion) and the
      // first Undo would only revert this no-op.
      if (JSON.stringify(nextRuns) === JSON.stringify(block.runs)) return;
      updateBlock(index, { ...block, runs: nextRuns });
    } else if (block.type === "table") {
      // Table cells are handled by syncCell.
    }
  };

  const syncCell = (
    tableIndex: number,
    rowIndex: number,
    cellIndex: number,
    blockIndex: number,
    element: HTMLElement,
  ) => {
    const block = currentBlocks()[tableIndex];
    if (!block || block.type !== "table") return;
    const table: TableData = {
      ...block.table,
      rows: block.table.rows.map((row, r) =>
        r !== rowIndex
          ? row
          : {
              ...row,
              cells: row.cells.map((cell, c) => {
                if (c !== cellIndex) return cell;
                const synced = wrapCellRuns(domToRuns(element));
                // Splice only the edited inner paragraph: replacing the whole
                // block list dropped every other paragraph of a multi-paragraph
                // cell on the first keystroke.
                const blocks =
                  cell.blocks.length === 0
                    ? [synced]
                    : cell.blocks.map((inner, innerIndex) => {
                        if (innerIndex !== blockIndex) return inner;
                        return inner.type === "paragraph" ? { ...inner, runs: synced.runs } : synced;
                      });
                return { ...cell, blocks };
              }),
            },
      ),
    };
    updateBlock(tableIndex, { type: "table", table });
  };

  // -------------------------------------------------------------------------
  // Formatting commands
  // -------------------------------------------------------------------------

  const setParagraphStyle = (styleId: string) => {
    const active = window.document.activeElement as HTMLElement | null;
    const index = active?.dataset?.blockIndex ? Number(active.dataset.blockIndex) : null;
    if (index === null) {
      const blocks = [...currentBlocks()];
      const first = blocks[0];
      if (first && first.type === "paragraph") {
        blocks[0] = { ...first, props: { ...first.props, style: styleId } };
        withBlocks(blocks);
      }
      return;
    }
    const block = currentBlocks()[index];
    if (block?.type === "paragraph") {
      updateBlock(index, { ...block, props: { ...block.props, style: styleId } });
      // The paragraph keeps focus, so no focus event refreshes the selection
      // that the style gallery and dropdown show as active.
      setSelection((current) =>
        current ? { ...current, paragraph: { ...current.paragraph, style: styleId } } : current,
      );
    }
  };

  const setAlign = (align: string) => applyParaChange({ align });
  const setLineSpacing = (value: number) => applyParaChange({ lineSpacing: value });
  const setSpace = (before: number, after: number) => applyParaChange({ spaceBeforePt: before, spaceAfterPt: after });
  const setIndent = (delta: number) => {
    const block = activeParagraph();
    const current = block?.props.indentLeftPt ?? 0;
    applyParaChange({ indentLeftPt: Math.max(0, current + delta) });
  };

  const toggleList = (kind: "bullet" | "number") => {
    const active = window.document.activeElement as HTMLElement | null;
    const index = active?.dataset?.blockIndex ? Number(active.dataset.blockIndex) : null;
    if (index === null) return;
    const block = currentBlocks()[index];
    if (block?.type !== "paragraph") return;
    const list = block.props.list;
    const next =
      list && list.kind === kind
        ? null
        : { kind, level: list?.level ?? 0, start: 1, marker: kind === "number" ? "1." : "•" };
    updateBlock(index, { ...block, props: { ...block.props, list: next } });
  };

  const changeListLevel = (delta: number) => {
    const active = window.document.activeElement as HTMLElement | null;
    const index = active?.dataset?.blockIndex ? Number(active.dataset.blockIndex) : null;
    if (index === null) return;
    const block = currentBlocks()[index];
    if (block?.type !== "paragraph" || !block.props.list) return;
    const level = Math.max(0, Math.min(5, block.props.list.level + delta));
    updateBlock(index, { ...block, props: { ...block.props, list: { ...block.props.list, level } } });
  };

  const toggleInline = (field: "bold" | "italic" | "underline" | "strike" | "superscript" | "subscript") => {
    const command =
      field === "bold"
        ? "bold"
        : field === "italic"
          ? "italic"
          : field === "underline"
            ? "underline"
            : field === "strike"
              ? "strikeThrough"
              : field === "superscript"
                ? "superscript"
                : "subscript";
    exec(command);
  };

  const applyRunColor = (color: string, highlight: boolean) => {
    const active = window.document.activeElement as HTMLElement | null;
    if (active && active.dataset.blockIndex) {
      const index = Number(active.dataset.blockIndex);
      const block = currentBlocks()[index];
      if (block?.type === "paragraph") {
        const runs = block.runs.map((run) => (highlight ? { ...run, highlight: color } : { ...run, color }));
        updateBlock(index, { ...block, runs: runs.length ? runs : [emptyRun()] });
        // Keep the visual state in sync with the model.
        applyVisualStyle(active, highlight ? "background-color" : "color", color);
        return;
      }
    }
    exec(highlight ? "hiliteColor" : "foreColor", color);
  };

  const clearFormatting = () => {
    const active = window.document.activeElement as HTMLElement | null;
    const index = active?.dataset?.blockIndex ? Number(active.dataset.blockIndex) : null;
    if (index !== null) {
      const block = currentBlocks()[index];
      if (block?.type === "paragraph") {
        updateBlock(index, { ...block, runs: block.runs.map((run) => ({ ...emptyRun(run.text) })) });
        return;
      }
    }
    exec("removeFormat");
  };

  // -------------------------------------------------------------------------
  // Structural inserts
  // -------------------------------------------------------------------------

  const insertImageBlock = async () => {
    try {
      // pickFileBytes uses the Android system picker there; the desktop dialog
      // plugin cannot open it.
      const picked = await pickFileBytes({
        name: "Images",
        extensions: ["png", "jpg", "jpeg", "gif", "bmp", "webp", "svg"],
        mimeTypes: ["image/*"],
      });
      if (!picked) return;
      const { bytes, name } = picked;
      const mime = mimeFromName(name);
      let base64 = "";
      const chunk = 0x8000;
      for (let index = 0; index < bytes.length; index += chunk) {
        base64 += String.fromCharCode(...bytes.subarray(index, index + chunk));
      }
      const image: ImageData = { name, mime, dataBase64: btoa(base64), alt: "" };
      const active = window.document.activeElement as HTMLElement | null;
      const index = active?.dataset?.blockIndex ? Number(active.dataset.blockIndex) + 1 : currentBlocks().length;
      insertBlockAfter(index - 1, { type: "image", image, widthPt: 320, heightPt: 220, align: "center", caption: "" });
    } catch (error) {
      reportError(error, t);
    }
  };

  const insertTableBlock = (rows: number, cols: number) => {
    const width = document.page.widthPt - document.page.marginLeftPt - document.page.marginRightPt;
    const table: TableData = {
      rows: Array.from({ length: rows }, (_, rowIndex) => ({
        cells: Array.from({ length: cols }, () => ({
          blocks: [newParaBlock()],
          colspan: 1,
          rowspan: 1,
          background: null,
          align: "left",
          valign: "top",
          widthPt: null,
        })),
        heightPt: null,
        header: rowIndex === 0,
      })),
      columnWidthsPt: Array.from({ length: cols }, () => width / cols),
      borders: true,
      borderColor: "#94A3B8",
      align: "left",
    };
    const active = window.document.activeElement as HTMLElement | null;
    const index = active?.dataset?.blockIndex ? Number(active.dataset.blockIndex) : currentBlocks().length - 1;
    insertBlockAfter(index, { type: "table", table });
  };

  const insertLink = () => {
    const url = window.prompt(t("writer.linkPrompt"), "https://");
    if (!url) return;
    exec("createLink", url);
    const active = window.document.activeElement as HTMLElement | null;
    if (active?.dataset?.blockIndex) {
      const index = Number(active.dataset.blockIndex);
      const block = currentBlocks()[index];
      if (block?.type === "paragraph") {
        updateBlock(index, { ...block, runs: block.runs.map((run) => ({ ...run, link: url })) });
      }
    }
  };

  const insertPageBreak = () => {
    const active = window.document.activeElement as HTMLElement | null;
    const index = active?.dataset?.blockIndex ? Number(active.dataset.blockIndex) : currentBlocks().length - 1;
    insertBlockAfter(index, { type: "pageBreak" });
  };

  useEffect(() => {
    insertPageBreakRef.current = insertPageBreak;
    insertTableBlockRef.current = insertTableBlock;
  });

  const insertRule = () => {
    const active = window.document.activeElement as HTMLElement | null;
    const index = active?.dataset?.blockIndex ? Number(active.dataset.blockIndex) : currentBlocks().length - 1;
    insertBlockAfter(index, { type: "rule" });
  };

  // -------------------------------------------------------------------------
  // Structural editing (Enter / Backspace / Tab / arrow keys at an edge)
  // -------------------------------------------------------------------------

  /**
   * Applies a structural edit to the block list.
   *
   * Every branch works on a copy of the model, repaints the affected
   * paragraphs in the DOM (a focused contentEditable is not re-rendered by
   * React) and restores the caret once the model change has committed.
   */
  const handleStructure = (action: StructureAction, scope: "body" | "header" | "footer" | "cell" | "page") => {
    if (scope === "cell") return; // cell editing keeps the simple single-paragraph model
    const blocks = [...currentBlocks()];
    const block = blocks[action.index];
    if (!block) return;
    const paragraph = block.type === "paragraph" ? block : null;

    const focusParagraph = (index: number, offset: number, edge: "start" | "end" = offset <= 0 ? "start" : "end") => {
      pendingFocus.current = { index, offset, scope };
      // In the paginated view the caret also moves the editable to the
      // fragment that owns the offset: the first fragment for an offset at the
      // start, the last for one at the end.
      if (scope === "page") {
        pageCaret.current = { block: index, offset, from: -1 };
        setPageEdit({ block: index, from: null, offset, edge });
      }
    };

    /**
     * Writes the model's runs into a paragraph element immediately.
     *
     * React leaves a focused contentEditable's children alone (that is what
     * keeps native typing and IME working), so after a structural edit the
     * element still shows the pre-edit text. Moving focus away fires `blur`,
     * whose handler reads the DOM and writes it back into the model - undoing
     * the edit. Repainting before the focus moves keeps blur honest.
     */
    const repaintAt = (index: number, runs: Run[]) => {
      const element = window.document.querySelector<HTMLElement>(
        `[data-scope="${scope}"][data-block-index="${index}"]`,
      );
      if (element) repaintParagraph(element, runs);
    };

    switch (action.kind) {
      case "replace": {
        blocks[action.index] = action.block;
        withBlocks(blocks);
        return;
      }
      case "split": {
        if (!paragraph) return;
        const [left, right] = splitRuns(paragraph.runs, action.offset);
        const tail = replaceRange(right, 0, Math.max(0, action.to - action.offset), "");
        const headText = runsText(left);
        blocks[action.index] = { ...paragraph, runs: left };
        blocks.splice(action.index + 1, 0, {
          type: "paragraph",
          props: nextParagraphProps(paragraph.props, headText),
          runs: tail,
        });
        withBlocks(blocks);
        repaintAt(action.index, left);
        focusParagraph(action.index + 1, 0);
        return;
      }
      case "mergeBackward": {
        if (action.index === 0) return;
        const previous = blocks[action.index - 1];
        if (previous.type !== "paragraph" || !paragraph) {
          // A table, image or rule has no text to merge into: outdent a list
          // item instead, which is what every word processor does.
          if (paragraph?.props.list) {
            blocks[action.index] = { ...paragraph, props: nextListLevel(paragraph.props, -1) };
            withBlocks(blocks);
          }
          return;
        }
        const caret = runsText(previous.runs).length;
        const merged = joinRuns(previous.runs, paragraph.runs);
        blocks[action.index - 1] = { ...previous, runs: merged };
        blocks.splice(action.index, 1);
        withBlocks(blocks);
        repaintAt(action.index - 1, merged);
        focusParagraph(action.index - 1, caret, "end");
        return;
      }
      case "mergeForward": {
        const next = blocks[action.index + 1];
        if (!paragraph || !next) return;
        if (next.type !== "paragraph") {
          // Delete at the end of a paragraph must not remove the following
          // table/image/rule/page break: those live only in the model, so the
          // splice was silent, unrecoverable data loss. Word does nothing here.
          return;
        }
        const caret = runsText(paragraph.runs).length;
        const merged = joinRuns(paragraph.runs, next.runs);
        blocks[action.index] = { ...paragraph, runs: merged };
        blocks.splice(action.index + 1, 1);
        withBlocks(blocks);
        repaintAt(action.index, merged);
        focusParagraph(action.index, caret, "end");
        return;
      }
      case "indent":
      case "outdent": {
        if (!paragraph) return;
        const delta = action.kind === "indent" ? 1 : -1;
        const next = paragraph.props.list
          ? nextListLevel(paragraph.props, delta)
          : { ...paragraph.props, indentLeftPt: Math.max(0, paragraph.props.indentLeftPt + delta * 24) };
        blocks[action.index] = { ...paragraph, props: next };
        withBlocks(blocks);
        return;
      }
      case "moveCaret": {
        // Skip blocks that cannot hold a caret (page/section breaks, rules,
        // images, tables): the next paragraph may well live on the next page,
        // and stopping on a break would strand the caret.
        const direction = action.delta < 0 ? -1 : 1;
        let target = action.index + direction;
        while (target >= 0 && target < blocks.length && blocks[target].type !== "paragraph") target += direction;
        const nextParagraph = blocks[target];
        if (!nextParagraph || nextParagraph.type !== "paragraph") return;
        const at =
          direction < 0
            ? action.atLine === "start"
              ? 0
              : runsText(nextParagraph.runs).length
            : action.atLine === "end"
              ? runsText(nextParagraph.runs).length
              : 0;
        focusParagraph(target, at, action.atLine);
        return;
      }
    }
  };

  // -------------------------------------------------------------------------
  // Find / replace
  // -------------------------------------------------------------------------

  // Matching runs on the model (writer/find-replace.ts), so regular
  // expressions, the live count and Replace see exactly what Replace All
  // changes, including table cells, the header and the footer.
  const search = useMemo(
    () => (findOpen ? compileSearch(findText, { matchCase, wholeWord, regex: useRegex }) : null),
    [findOpen, findText, matchCase, wholeWord, useRegex],
  );
  // A regular expression is first run in a worker (writer/regex-probe.ts), a
  // moment after the last keystroke: a catastrophic pattern is stopped there
  // instead of freezing the editor with the unsaved document in it.
  const probing = useRegex && search?.ok === true && canProbeRegex();
  const probeKey = useMemo(() => ({ search, document }), [search, document]);
  const [probe, setProbe] = useState<{ key: object; status: ProbeStatus } | null>(null);
  useEffect(() => {
    if (!probing || !search?.ok) return;
    let run: RegexProbe | null = null;
    const timer = setTimeout(() => {
      run = probeRegex(search.pattern, documentTexts(document));
      void run.promise.then((status) => setProbe({ key: probeKey, status }));
    }, 250);
    return () => {
      clearTimeout(timer);
      run?.cancel();
    };
  }, [probing, probeKey, search, document]);
  const probeStatus: ProbeStatus | "pending" = !probing ? "ok" : probe?.key === probeKey ? probe.status : "pending";
  const matches = useMemo(
    () => (search?.ok && probeStatus === "ok" ? documentMatches(document, search.pattern) : []),
    [search, document, probeStatus],
  );
  const currentMatchIndex = currentMatch ? matches.findIndex((match) => sameMatch(match, currentMatch)) : -1;
  // A match found by Replace is selected after the commit that renders it.
  const revealRequest = useRef<MatchLocation | null>(null);

  /**
   * Selects a match in the rendered page and scrolls it into view. A split
   * paragraph renders its whole text in every page fragment, so the fragment
   * whose band actually shows the match is preferred.
   */
  const revealMatch = (match: MatchLocation) => {
    const paginated = view === "paginated" && !editingHeader;
    const scope = match.scope === "body" && paginated ? "page" : match.scope;
    const hosts =
      match.path.length === 1
        ? Array.from(
            window.document.querySelectorAll<HTMLElement>(
              `[data-scope="${scope}"][data-block-index="${match.path[0]}"]`,
            ),
          )
        : [];
    for (const host of hosts) {
      // Only paragraph hosts: a page fragment wraps a table or image in a
      // `data-block-index="0"` box of its own, which must not be picked.
      const para = host.classList.contains("para")
        ? host
        : host.classList.contains("para-row")
          ? host.querySelector<HTMLElement>(".para")
          : null;
      if (!para) continue;
      setSelectionRange(para, match.start, match.end);
      const band = para.closest<HTMLElement>(".writer-fragment");
      if (band && !selectionShownIn(band)) continue;
      (band ?? para).scrollIntoView?.({ block: "center" });
      return;
    }
    // Table cells (and a body match while the header is edited) have no
    // selectable paragraph here: bring the block's page into view instead.
    if (match.scope === "body" && paginated) {
      window.document
        .querySelector<HTMLElement>(`[data-page-index="${pageOfBlock(pages, match.path[0]) - 1}"]`)
        ?.scrollIntoView?.({ block: "start" });
    }
  };

  useEffect(() => {
    const request = revealRequest.current;
    if (!request) return;
    revealRequest.current = null;
    revealMatch(request);
  });

  const findStep = (forward: boolean) => {
    const index = nextMatchIndex(matches, currentMatch, forward);
    if (index < 0) return;
    setCurrentMatch(matches[index]);
    revealMatch(matches[index]);
  };

  const replaceCurrent = () => {
    if (!search?.ok) return;
    const current = currentMatchIndex >= 0 ? matches[currentMatchIndex] : null;
    // Word behaviour: without a selected match Replace only finds the next
    // one, so nothing changes that the user has not seen.
    const result = current ? replaceMatch(document, current, search.pattern, replaceText, useRegex) : null;
    if (!current || !result) {
      findStep(true);
      return;
    }
    update(() => result.document);
    const following = documentMatches(result.document, search.pattern);
    const index = nextMatchIndex(following, { ...current, start: result.end }, true, true);
    const next = index >= 0 ? following[index] : null;
    setCurrentMatch(next);
    revealRequest.current = next;
  };

  const replaceAll = () => {
    if (!search?.ok || probeStatus !== "ok") return;
    const result = replaceAllInDocument(document, search.pattern, replaceText, useRegex);
    if (result.count > 0) update(() => result.document);
    setCurrentMatch(null);
    useToasts.getState().push({ kind: "success", title: t("writer.replaceDone"), detail: `${result.count}` });
  };

  const matchCount = matches.length >= MATCH_LIMIT ? `${MATCH_LIMIT}+` : matches.length;
  const findStatus = !search?.ok
    ? ""
    : probeStatus === "slow"
      ? t("writer.regexTooSlow")
      : probeStatus === "pending"
        ? t("writer.findSearching")
        : matches.length === 0
          ? t("writer.findNoMatches")
          : currentMatchIndex >= 0
            ? t("writer.findMatchOf", { current: currentMatchIndex + 1, count: matchCount })
            : matches.length === 1
              ? t("writer.findOneMatch")
              : t("writer.findMatches", { count: matchCount });

  const closeFind = () => {
    setFindOpen(false);
    setCurrentMatch(null);
  };

  // -------------------------------------------------------------------------
  // Comments
  // -------------------------------------------------------------------------

  const addComment = () => {
    const text = window.prompt(t("writer.commentPrompt"));
    if (!text) return;
    const comment: DocComment = { id: uid(), author: "Me", text, created: new Date().toISOString(), resolved: false };
    const active = window.document.activeElement as HTMLElement | null;
    const index = active?.dataset?.blockIndex ? Number(active.dataset.blockIndex) : null;
    update((doc) => {
      const blocks = [...doc.blocks];
      if (index !== null) {
        const block = blocks[index];
        if (block?.type === "paragraph") {
          blocks[index] = { ...block, runs: block.runs.map((run) => ({ ...run, comment: comment.id })) };
        }
      }
      return { ...doc, blocks, comments: [...doc.comments, comment] };
    });
    setCommentsOpen(true);
  };

  // -------------------------------------------------------------------------
  // V3: sections, footnotes, tracked changes and fields
  // -------------------------------------------------------------------------

  const toggleTrackChanges = () => update((doc) => ({ ...doc, trackChanges: !doc.trackChanges }));
  const toggleShowRevisions = () => update((doc) => ({ ...doc, showRevisions: doc.showRevisions === false }));

  const caretAt = (): { index: number; offset: number } | null => {
    const active = window.document.activeElement as HTMLElement | null;
    if (!active?.dataset?.blockIndex) return null;
    return { index: Number(active.dataset.blockIndex), offset: caretOffset(active) };
  };

  const insertFootnote = (endnote: boolean) => {
    const caret = caretAt();
    if (!caret) return;
    const block = currentBlocks()[caret.index];
    if (block?.type !== "paragraph") return;
    const note = newFootnote();
    const [left, right] = splitRuns(block.runs, caret.offset);
    const reference: Run = endnote ? { ...emptyRun(""), endnote: note.id } : { ...emptyRun(""), footnote: note.id };
    update((doc) => ({
      ...doc,
      footnotes: endnote ? (doc.footnotes ?? []) : [...(doc.footnotes ?? []), note],
      endnotes: endnote ? [...(doc.endnotes ?? []), note] : (doc.endnotes ?? []),
      blocks: doc.blocks.map((candidate, index) =>
        index === caret.index && candidate.type === "paragraph"
          ? { ...candidate, runs: [...left, reference, ...right] }
          : candidate,
      ),
    }));
    setLayoutVersion((version) => version + 1);
  };

  const insertSectionBreak = (start: string) => {
    const caret = caretAt();
    const current = caret ? sectionForBlock(document, caret.index) : sections[sections.length - 1];
    const section: SectionProps = { ...defaultSectionProps(current.page), start };
    const breakBlock: Block = { type: "sectionBreak", section };
    if (caret) insertBlockAfter(caret.index, breakBlock);
    else withBlocks([...currentBlocks(), breakBlock]);
    setLayoutVersion((version) => version + 1);
  };

  const updateSection = (sectionIndex: number, patch: Partial<SectionProps>) => {
    update((doc) => {
      const updatedSections = documentSections(doc);
      const target = updatedSections[sectionIndex];
      if (!target) return doc;
      if (sectionIndex === updatedSections.length - 1) {
        // The final section lives in the document-level fields.
        return {
          ...doc,
          page: patch.page ?? doc.page,
          header: patch.header ?? doc.header,
          footer: patch.footer ?? doc.footer,
        };
      }
      let breakCount = 0;
      return {
        ...doc,
        blocks: doc.blocks.map((block) => {
          if (block.type !== "sectionBreak") return block;
          breakCount += 1;
          return breakCount === sectionIndex ? { ...block, section: { ...block.section, ...patch } } : block;
        }),
      };
    });
    setLayoutVersion((version) => version + 1);
  };

  const removeSectionBreak = (sectionIndex: number) => {
    let breakCount = 0;
    update((doc) => ({
      ...doc,
      blocks: doc.blocks.filter((block) => {
        if (block.type !== "sectionBreak") return true;
        breakCount += 1;
        return breakCount !== sectionIndex;
      }),
    }));
    setLayoutVersion((version) => version + 1);
  };

  const addBookmark = () => {
    const caret = caretAt();
    if (!caret) return;
    const name = bookmarkName.trim() || `Bookmark${(document.bookmarks?.length ?? 0) + 1}`;
    if (document.bookmarks?.some((bookmark) => bookmark.name === name)) {
      useToasts.getState().push({ kind: "error", title: t("writer.bookmarkExists") });
      return;
    }
    update((doc) => ({
      ...doc,
      bookmarks: [...(doc.bookmarks ?? []), { id: uid(), name, block: caret.index, offset: caret.offset }],
    }));
    setBookmarkName("");
  };

  const insertField = (kind: FieldRef["kind"], target = "") => {
    const caret = caretAt();
    if (!caret) return;
    const block = currentBlocks()[caret.index];
    if (block?.type !== "paragraph") return;
    const cached =
      kind === "page"
        ? "1"
        : kind === "pages"
          ? "1"
          : kind === "date"
            ? new Date().toISOString().slice(0, 10)
            : kind === "time"
              ? new Date().toISOString().slice(11, 19)
              : kind === "title"
                ? document.title
                : kind === "author"
                  ? document.metadata.author
                  : target || "?";
    const [left, right] = splitRuns(block.runs, caret.offset);
    const fieldRun: Run = { ...emptyRun(""), field: { kind, target, cached } };
    updateBlock(caret.index, { ...block, runs: [...left, fieldRun, ...right] });
  };

  const jumpToRevision = (forward: boolean) => {
    const id = nextRevision(document, activeRevision, forward);
    if (!id) return;
    setActiveRevision(id);
    const summary = revisionList(document).find((revision) => revision.id === id);
    if (summary) jumpToBlock(summary.blockIndex);
  };

  // -------------------------------------------------------------------------
  // Page setup
  // -------------------------------------------------------------------------

  const setPageSize = (size: string) =>
    update((doc) => ({ ...doc, page: { ...doc.page, ...sizeDimensions(size, doc.page.orientation), size } }));
  const setOrientation = (orientation: string) =>
    update((doc) => {
      const landscape = orientation === "landscape";
      const currentlyLandscape = doc.page.widthPt > doc.page.heightPt;
      const page = { ...doc.page, orientation };
      if (landscape !== currentlyLandscape) {
        page.widthPt = doc.page.heightPt;
        page.heightPt = doc.page.widthPt;
      }
      return { ...doc, page };
    });
  const setMargin = (key: "marginTopPt" | "marginRightPt" | "marginBottomPt" | "marginLeftPt", value: number) =>
    update((doc) => ({ ...doc, page: { ...doc.page, [key]: value } }));
  const applyMarginPreset = (preset: "normal" | "narrow" | "wide") => {
    const values = preset === "narrow" ? 36 : preset === "wide" ? 108 : 72;
    update((doc) => ({
      ...doc,
      page: {
        ...doc.page,
        marginTopPt: values,
        marginBottomPt: values,
        marginLeftPt: values === 36 ? 36 : values,
        marginRightPt: values === 36 ? 36 : values,
      },
    }));
  };
  const setColumns = (columns: number) => update((doc) => ({ ...doc, page: { ...doc.page, columns } }));

  // -------------------------------------------------------------------------
  // Rendering
  // -------------------------------------------------------------------------

  const pageWidth = document.page.widthPt * (96 / 72) * zoom;
  const pageHeight = document.page.heightPt * (96 / 72) * zoom;
  const marginTop = document.page.marginTopPt * (96 / 72) * zoom;
  const marginX = document.page.marginLeftPt * (96 / 72) * zoom;
  const styleOptions = document.styles.map((style) => ({ value: style.id, label: style.name }));
  const activeStyle = selection?.paragraph.style ?? "Normal";
  const activeRun = selection?.run;

  /**
   * Word behaviour: clicking anywhere in the page (including the empty area
   * below the text) places the caret in the nearest paragraph.
   */
  const handlePageMouseDown = (event: React.MouseEvent<HTMLDivElement>) => {
    const target = event.target as HTMLElement;
    if (target.closest('[contenteditable="true"]')) return;
    const container = event.currentTarget;
    const candidates = Array.from(container.querySelectorAll<HTMLElement>(".para"));
    if (candidates.length === 0) return;
    event.preventDefault();
    const scope = event.currentTarget.dataset.scope ?? "body";
    const pool = candidates.filter((element) => (element.dataset.scope ?? "body") === scope);
    const usable = pool.length > 0 ? pool : candidates;
    const editable = usable.reduce(
      (best, element) => {
        const rect = element.getBoundingClientRect();
        const distance = Math.abs(rect.top + rect.height / 2 - event.clientY);
        return distance < best.distance ? { element, distance } : best;
      },
      { element: usable[0], distance: Number.POSITIVE_INFINITY },
    ).element;
    editable.focus();
    const selection = window.getSelection();
    if (selection) {
      const range = window.document.createRange();
      range.selectNodeContents(editable);
      range.collapse(false);
      selection.removeAllRanges();
      selection.addRange(range);
    }
  };
  const handlePrint = () => {
    void session.print();
  };

  const handleExportPdf = () => {
    void session.exportPdf();
  };

  // -------------------------------------------------------------------------
  // TOC and navigation
  // -------------------------------------------------------------------------

  /** The heading outline shared by the navigation pane and the TOC. */
  const headingOutline = (): TocEntry[] => {
    const entries: TocEntry[] = [];
    document.blocks.forEach((block, index) => {
      if (block.type !== "paragraph") return;
      const match = /^Heading(\d)$/.exec(block.props.style);
      if (!match) return;
      entries.push({
        text: runsText(block.runs).trim() || `Heading ${index + 1}`,
        level: Number(match[1]),
        page: pageOfBlock(pages, index),
        anchor: index,
      });
    });
    return entries;
  };

  const insertToc = () => {
    const toc: Block = { type: "toc", entries: headingOutline() };
    const index = activeIndex();
    if (index !== null) insertBlockAfter(index, toc);
    else {
      const blocks = [...currentBlocks()];
      blocks.unshift(toc);
      withBlocks(blocks);
    }
  };

  const updateToc = () => {
    const entries = headingOutline();
    update((doc) => ({
      ...doc,
      blocks: doc.blocks.map((block) => (block.type === "toc" ? { ...block, entries } : block)),
    }));
    setLayoutVersion((version) => version + 1);
    useToasts.getState().push({ kind: "success", title: t("writer.tocUpdated") });
  };

  const jumpToBlock = (index: number) => {
    if (view === "continuous" || editingHeader) {
      const target = window.document.querySelector<HTMLElement>(`[data-scope="body"][data-block-index="${index}"]`);
      target?.scrollIntoView({ block: "center" });
      return;
    }
    const page = pageOfBlock(pages, index);
    window.document
      .querySelector<HTMLElement>(`[data-page-index="${page - 1}"]`)
      ?.scrollIntoView({ block: "start", behavior: "smooth" });
  };

  /**
   * Opens the editing surface on a block at the clicked position.
   *
   * Used for blocks that have no in-place surface (tables, images, TOC) and for
   * header/footer jumps: those still switch to the continuous view. Paragraph
   * fragments in the paginated view are handled by `activatePageEdit` instead.
   */
  const editBlock = (index: number, offset = 0) => {
    setView("continuous");
    const block = document.blocks[index];
    if (block?.type === "paragraph") {
      pendingFocus.current = { index, offset, scope: "body" };
      return;
    }
    window.setTimeout(() => {
      window.document
        .querySelector<HTMLElement>(`[data-scope="body"][data-block-index="${index}"]`)
        ?.scrollIntoView({ block: "center" });
    }, 0);
  };

  /** Header/footer fragments open the continuous surface on that scope. */
  const editHeaderFooter = (scope: "header" | "footer", index: number) => {
    setEditingHeader(scope);
    setView("continuous");
    pendingFocus.current = { index, offset: 0, scope };
  };

  // -------------------------------------------------------------------------
  // Paginated in-place editing (V3.1)
  // -------------------------------------------------------------------------

  /** Fragments of a block in document order (page order, then fragment order). */
  const fragmentListOf = (blockIndex: number): Fragment[] => {
    const fragments: Fragment[] = [];
    for (const page of pages)
      for (const fragment of page.fragments) if (fragment.index === blockIndex) fragments.push(fragment);
    return fragments;
  };

  /**
   * The fragment that hosts the active editable for a block.
   *
   * A reflow keeps the fragment the caret was in as long as it still exists.
   * When splitting changed the boundaries entirely, the caret edge decides:
   * offsets at the start of a block belong to its first fragment, offsets at
   * the end to its last.
   */
  const activeFragmentFrom = (blockIndex: number): number | null => {
    if (!pageEdit || pageEdit.block !== blockIndex) return null;
    const fragments = fragmentListOf(blockIndex);
    if (fragments.length === 0) return null;
    if (pageEdit.from !== null && fragments.some((fragment) => fragment.from === pageEdit.from)) return pageEdit.from;
    return pageEdit.edge === "end" ? fragments[fragments.length - 1].from : fragments[0].from;
  };

  /**
   * Activates a page fragment for editing.
   *
   * The offset comes from a real hit test, so continuation fragments place the
   * caret at the clicked character even though their DOM holds the whole
   * paragraph. `flushSync` makes the editable exist before the pointer handler
   * returns: Android webviews only open the soft keyboard for a `focus()` call
   * inside the user gesture.
   */
  const activatePageEdit = (blockIndex: number, offset: number, fragment: Fragment, container: HTMLElement) => {
    const block = document.blocks[blockIndex];
    if (block?.type !== "paragraph") {
      editBlock(blockIndex, offset);
      return;
    }
    pendingPageFocus.current = { block: blockIndex, offset };
    pageCaret.current = { block: blockIndex, offset, from: fragment.from };
    flushSync(() => setPageEdit({ block: blockIndex, from: fragment.from, offset, edge: "start" }));
    const editable = container.querySelector<HTMLElement>(
      `[data-scope="page"][data-block-index="${blockIndex}"][contenteditable="true"]`,
    );
    if (editable) {
      editable.focus();
      setCaretOffset(editable, offset);
      pendingPageFocus.current = null;
    }
  };

  /** Records the caret before a model update so a reflow can restore it. */
  const recordPageCaret = (blockIndex: number, offset: number, from: number) => {
    pageCaret.current = { block: blockIndex, offset, from };
  };

  /**
   * Extends a drag selection that started on a static fragment.
   *
   * Pointer capture keeps the moves coming; every fragment renders the whole
   * paragraph, so the point maps to a block offset even when it hovers a
   * continuation on another page. Only the active block's editable takes the
   * range, which keeps one selection owner per block.
   */
  const extendPageSelection = (blockIndex: number, anchor: number, x: number, y: number) => {
    const editable = window.document.querySelector<HTMLElement>(
      `[data-scope="page"][data-block-index="${blockIndex}"][contenteditable="true"]`,
    );
    if (!editable) return;
    const hit = paragraphAtPoint(window.document, x, y);
    if (!hit || hit.block !== blockIndex) return;
    setSelectionRange(editable, Math.min(anchor, hit.offset), Math.max(anchor, hit.offset));
    pageCaret.current = { block: blockIndex, offset: hit.offset, from: pageCaret.current?.from ?? -1 };
  };

  /**
   * Moves the active editable one fragment forward/backward, keeping the
   * character offset. Returns false when the block has no fragment that way, so
   * the caller can fall through to the normal paragraph-end behaviour.
   */
  const moveActiveFragment = (blockIndex: number, direction: -1 | 1): boolean => {
    if (!pageEdit || pageEdit.block !== blockIndex) return false;
    const fragments = fragmentListOf(blockIndex);
    const position = fragments.findIndex((fragment) => fragment.from === activeFragmentFrom(blockIndex));
    const next = fragments[position + direction];
    if (!next) return false;
    const offset = pageCaret.current?.block === blockIndex ? pageCaret.current.offset : pageEdit.offset;
    pageCaret.current = { block: blockIndex, offset, from: next.from };
    pendingPageFocus.current = { block: blockIndex, offset };
    setPageEdit({ block: blockIndex, from: next.from, offset, edge: direction < 0 ? "start" : "end" });
    return true;
  };

  const renderBlocks = (
    blocks: Block[],
    scope: "body" | "header" | "footer" | "cell",
    tablePath?: [number, number, number, number],
  ) => {
    const numbers = scope === "body" ? listNumbers : orderedListNumbers(blocks);
    const fields = scope === "cell" ? undefined : documentFields;
    return (
      <>
        {blocks.map((block, index) => (
          <BlockView
            key={`${scope}-${index}-${block.type}`}
            block={block}
            index={index}
            scope={scope}
            zoom={zoom}
            noteNumbers={noteNumbers}
            showRevisions={document.showRevisions !== false}
            listNumber={numbers.get(index)}
            fieldValues={fields}
            selectedImage={selectedImage}
            onSelectImage={setSelectedImage}
            onFocusParagraph={handleParagraphFocus}
            onSync={(element) => {
              if (scope === "cell" && tablePath)
                syncCell(tablePath[0], tablePath[1], tablePath[2], tablePath[3], element);
              else if (scope === "body" || scope === "header" || scope === "footer") syncParagraph(index, element);
            }}
            onUpdate={(next) => updateBlock(index, next)}
            onSyncCell={(path, element) => syncCell(path[0], path[1], path[2], path[3], element)}
            onStructure={(action) => handleStructure(action, scope)}
            onOpenBlock={editBlock}
          />
        ))}
      </>
    );
  };

  return (
    <div className="editor writer-editor">
      <Ribbon
        tabs={[
          { id: "home", label: t("writer.tabHome") },
          { id: "insert", label: t("writer.tabInsert") },
          { id: "layout", label: t("writer.tabLayout") },
          { id: "review", label: t("writer.tabReview") },
          { id: "view", label: t("writer.tabView") },
        ]}
        active={ribbon}
        onSelect={setRibbon}
      >
        {ribbon === "home" ? (
          <>
            <RibbonGroup label={t("writer.clipboard")}>
              <ToolButton
                icon={<Undo2 size={16} />}
                label={t("common.undo")}
                onClick={undoEdit}
                disabled={session.busy || historyDepth.undo === 0}
              />
              <ToolButton
                icon={<Redo2 size={16} />}
                label={t("common.redo")}
                onClick={redoEdit}
                disabled={session.busy || historyDepth.redo === 0}
              />
            </RibbonGroup>
            <RibbonGroup label={t("writer.font")}>
              <ToolSelect
                value={activeStyle}
                onChange={setParagraphStyle}
                options={styleOptions}
                title={t("writer.style")}
                width={132}
              />
              <ToolSelect
                value={String(
                  activeRun?.sizePt ?? document.styles.find((style) => style.id === activeStyle)?.sizePt ?? 11,
                )}
                onChange={(value) => applyRunChange({ sizePt: Number(value) })}
                options={[8, 9, 10, 11, 12, 14, 16, 18, 20, 24, 28, 32, 40, 48].map((size) => ({
                  value: String(size),
                  label: `${size}`,
                }))}
                title={t("writer.fontSize")}
                width={64}
              />
              <ToolButton
                icon={<Bold size={16} />}
                onClick={() => toggleInline("bold")}
                active={activeRun?.bold}
                title={t("writer.bold")}
              />
              <ToolButton
                icon={<Italic size={16} />}
                onClick={() => toggleInline("italic")}
                active={activeRun?.italic}
                title={t("writer.italic")}
              />
              <ToolButton
                icon={<Underline size={16} />}
                onClick={() => toggleInline("underline")}
                active={activeRun?.underline}
                title={t("writer.underline")}
              />
              <ToolButton
                icon={<Strikethrough size={16} />}
                onClick={() => toggleInline("strike")}
                active={activeRun?.strike}
                title={t("writer.strike")}
              />
              <ToolColor
                value={activeRun?.color ?? "#1f2328"}
                onChange={(color) => applyRunColor(color, false)}
                title={t("writer.textColor")}
              />
              <ToolButton
                icon={<Highlighter size={16} />}
                onClick={() => applyRunColor(activeRun?.highlight ?? HIGHLIGHT_COLORS[0], true)}
                active={Boolean(activeRun?.highlight)}
                title={t("writer.highlight")}
              />
              <ToolButton icon={<Eraser size={16} />} onClick={clearFormatting} title={t("writer.clearFormatting")} />
            </RibbonGroup>
            <RibbonGroup label={t("writer.paragraph")}>
              <ToolButton
                icon={<AlignLeft size={16} />}
                onClick={() => setAlign("left")}
                active={selection?.paragraph.align === "left"}
                title={t("writer.alignLeft")}
              />
              <ToolButton
                icon={<AlignCenter size={16} />}
                onClick={() => setAlign("center")}
                active={selection?.paragraph.align === "center"}
                title={t("writer.alignCenter")}
              />
              <ToolButton
                icon={<AlignRight size={16} />}
                onClick={() => setAlign("right")}
                active={selection?.paragraph.align === "right"}
                title={t("writer.alignRight")}
              />
              <ToolButton
                icon={<AlignJustify size={16} />}
                onClick={() => setAlign("justify")}
                active={selection?.paragraph.align === "justify"}
                title={t("writer.alignJustify")}
              />
              <ToolButton
                icon={<List size={16} />}
                onClick={() => toggleList("bullet")}
                active={selection?.paragraph.list?.kind === "bullet"}
                title={t("writer.bullets")}
              />
              <ToolButton
                icon={<ListOrdered size={16} />}
                onClick={() => toggleList("number")}
                active={selection?.paragraph.list?.kind === "number"}
                title={t("writer.numbering")}
              />
              <ToolButton
                icon={<Indent size={16} />}
                onClick={() => (selection?.paragraph.list ? changeListLevel(1) : setIndent(24))}
                title={t("writer.increaseIndent")}
              />
              <ToolButton
                icon={<Outdent size={16} />}
                onClick={() => (selection?.paragraph.list ? changeListLevel(-1) : setIndent(-24))}
                title={t("writer.decreaseIndent")}
              />
              <ToolSelect
                value={String(selection?.paragraph.lineSpacing ?? 1.15)}
                onChange={(value) => setLineSpacing(Number(value))}
                options={[1, 1.15, 1.5, 2].map((value) => ({ value: String(value), label: `${value}` }))}
                title={t("writer.lineSpacing")}
                width={70}
              />
            </RibbonGroup>
            <RibbonGroup label={t("writer.styles")}>
              <StyleGallery document={document} active={activeStyle} onApply={setParagraphStyle} />
            </RibbonGroup>
            <RibbonGroup label={t("writer.find")}>
              <ToolButton icon={<Search size={16} />} label={t("common.find")} onClick={() => setFindOpen(true)} />
            </RibbonGroup>
            <RibbonGroup label={t("ai.edit.group")}>
              {AI_RIBBON.map(({ task, icon: Icon }) => (
                <ToolButton
                  key={task}
                  icon={<Icon size={16} />}
                  keepFocus
                  disabled={!aiStatus.configured || session.busy}
                  title={
                    aiStatus.configured
                      ? t(`ai.edit.btn.${task}`)
                      : `${t(`ai.edit.btn.${task}`)} - ${t("ai.edit.notConfigured")}`
                  }
                  onClick={() => startAi(task)}
                />
              ))}
            </RibbonGroup>
          </>
        ) : null}

        {ribbon === "insert" ? (
          <>
            <RibbonGroup label={t("writer.insert")}>
              <ToolButton icon={<ImageIcon size={16} />} label={t("writer.image")} onClick={insertImageBlock} />
              <ToolButton
                icon={<TableIcon size={16} />}
                label={t("writer.table")}
                onClick={() => setInsertTable(true)}
              />
              <ToolButton icon={<Link2 size={16} />} label={t("writer.link")} onClick={insertLink} />
            </RibbonGroup>
            <RibbonGroup label={t("writer.pages")}>
              <ToolButton
                icon={<SeparatorHorizontal size={16} />}
                label={t("writer.pageBreak")}
                onClick={insertPageBreak}
              />
              <ToolButton icon={<Minus size={16} />} label={t("writer.horizontalRule")} onClick={insertRule} />
              <ToolButton
                label={t("writer.sectionBreak")}
                onClick={() => insertSectionBreak("newPage")}
                title={t("writer.sectionBreak")}
              />
              <ToolButton
                label={t("writer.sectionContinuous")}
                onClick={() => insertSectionBreak("continuous")}
                title={t("writer.sectionContinuous")}
              />
            </RibbonGroup>
            <RibbonGroup label={t("writer.notes")}>
              <ToolButton label={t("writer.insertFootnote")} onClick={() => insertFootnote(false)} />
              <ToolButton label={t("writer.insertEndnote")} onClick={() => insertFootnote(true)} />
            </RibbonGroup>
            <RibbonGroup label={t("writer.fields")}>
              <ToolButton label={t("writer.fieldPage")} onClick={() => insertField("page")} />
              <ToolButton label={t("writer.fieldPages")} onClick={() => insertField("pages")} />
              <ToolButton label={t("writer.fieldDate")} onClick={() => insertField("date")} />
              <ToolButton label={t("writer.crossReference")} onClick={() => setFieldOpen(true)} />
            </RibbonGroup>
            <RibbonGroup label={t("writer.tableOfContents")}>
              <ToolButton icon={<TocIcon size={16} />} label={t("writer.insertToc")} onClick={insertToc} />
              <ToolButton icon={<RefreshCw size={16} />} label={t("writer.updateToc")} onClick={updateToc} />
            </RibbonGroup>
            <RibbonGroup label={t("writer.headerFooter")}>
              <ToolButton
                icon={<FileText size={16} />}
                label={t("writer.header")}
                onClick={() => setEditingHeader(editingHeader === "header" ? null : "header")}
                active={editingHeader === "header"}
              />
              <ToolButton
                icon={<FileText size={16} />}
                label={t("writer.footer")}
                onClick={() => setEditingHeader(editingHeader === "footer" ? null : "footer")}
                active={editingHeader === "footer"}
              />
              <ToolButton
                label={t("writer.pageNumbers")}
                onClick={() =>
                  update((doc) => ({
                    ...doc,
                    footer: [
                      {
                        type: "paragraph",
                        props: { ...defaultParaProps(), align: "center", spaceAfterPt: 0 },
                        runs: [{ ...emptyRun("Page {{page}} / {{pages}}") }],
                      },
                    ],
                  }))
                }
              />
            </RibbonGroup>
            <RibbonGroup label={t("writer.comments")}>
              <ToolButton icon={<MessageSquare size={16} />} label={t("writer.addComment")} onClick={addComment} />
            </RibbonGroup>
          </>
        ) : null}

        {ribbon === "layout" ? (
          <>
            <RibbonGroup label={t("writer.pageSetup")}>
              <ToolSelect
                value={document.page.size}
                onChange={setPageSize}
                options={[
                  { value: "a4", label: "A4" },
                  { value: "a5", label: "A5" },
                  { value: "letter", label: "Letter" },
                  { value: "legal", label: "Legal" },
                  { value: "a3", label: "A3" },
                ]}
                title={t("writer.pageSize")}
                width={90}
              />
              <ToolSelect
                value={document.page.orientation}
                onChange={setOrientation}
                options={[
                  { value: "portrait", label: t("writer.portrait") },
                  { value: "landscape", label: t("writer.landscape") },
                ]}
                title={t("writer.orientation")}
                width={110}
              />
              <ToolSelect
                value=""
                onChange={(value) => {
                  if (value) applyMarginPreset(value as "normal" | "narrow" | "wide");
                }}
                options={[
                  { value: "", label: t("writer.margins") },
                  { value: "normal", label: t("writer.marginNormal") },
                  { value: "narrow", label: t("writer.marginNarrow") },
                  { value: "wide", label: t("writer.marginWide") },
                ]}
                title={t("writer.margins")}
                width={110}
              />
              <ToolButton
                icon={<Columns2 size={16} />}
                onClick={() => setColumns(document.page.columns > 1 ? 1 : 2)}
                active={document.page.columns > 1}
                title={t("writer.columns")}
              />
              <ToolButton
                label={t("writer.sections")}
                onClick={() => setSectionsOpen(true)}
                title={t("writer.sections")}
              />
            </RibbonGroup>
            <RibbonGroup label={t("writer.spacing")}>
              <ToolNumber
                value={selection?.paragraph.spaceBeforePt ?? 0}
                onChange={(value) => setSpace(value, selection?.paragraph.spaceAfterPt ?? 0)}
                min={0}
                max={144}
                title={t("writer.spaceBefore")}
              />
              <ToolNumber
                value={selection?.paragraph.spaceAfterPt ?? 0}
                onChange={(value) => setSpace(selection?.paragraph.spaceBeforePt ?? 0, value)}
                min={0}
                max={144}
                title={t("writer.spaceAfter")}
              />
            </RibbonGroup>
            <RibbonGroup label={t("writer.margins")}>
              <ToolNumber
                value={document.page.marginTopPt}
                onChange={(value) => setMargin("marginTopPt", value)}
                min={0}
                max={288}
                title={t("writer.marginTop")}
              />
              <ToolNumber
                value={document.page.marginBottomPt}
                onChange={(value) => setMargin("marginBottomPt", value)}
                min={0}
                max={288}
                title={t("writer.marginBottom")}
              />
              <ToolNumber
                value={document.page.marginLeftPt}
                onChange={(value) => setMargin("marginLeftPt", value)}
                min={0}
                max={288}
                title={t("writer.marginLeft")}
              />
              <ToolNumber
                value={document.page.marginRightPt}
                onChange={(value) => setMargin("marginRightPt", value)}
                min={0}
                max={288}
                title={t("writer.marginRight")}
              />
            </RibbonGroup>
          </>
        ) : null}

        {ribbon === "review" ? (
          <>
            <RibbonGroup label={t("writer.trackChanges")}>
              <ToolButton
                label={t("writer.suggesting")}
                onClick={toggleTrackChanges}
                active={document.trackChanges === true}
                title={t("writer.suggestingHint")}
              />
              <ToolButton
                label={t("writer.showRevisions")}
                onClick={toggleShowRevisions}
                active={document.showRevisions !== false}
              />
              <ToolButton
                label={t("writer.reviewPane")}
                onClick={() => setReviewOpen((open) => !open)}
                active={reviewOpen}
              />
            </RibbonGroup>
            <RibbonGroup label={t("writer.revisions")}>
              <ToolButton label={t("writer.previousChange")} onClick={() => jumpToRevision(false)} />
              <ToolButton label={t("writer.nextChange")} onClick={() => jumpToRevision(true)} />
              <ToolButton
                label={t("writer.acceptAll")}
                onClick={() => {
                  update((doc) => acceptAll(doc));
                  setActiveRevision(null);
                  setLayoutVersion((version) => version + 1);
                }}
              />
              <ToolButton
                label={t("writer.rejectAll")}
                onClick={() => {
                  update((doc) => rejectAll(doc));
                  setActiveRevision(null);
                  setLayoutVersion((version) => version + 1);
                }}
              />
            </RibbonGroup>
            <RibbonGroup label={t("writer.comments")}>
              <ToolButton icon={<MessageSquare size={16} />} label={t("writer.addComment")} onClick={addComment} />
              <ToolButton
                icon={<MessageSquare size={16} />}
                label={t("writer.comments")}
                onClick={() => setCommentsOpen(!commentsOpen)}
                active={commentsOpen}
              />
            </RibbonGroup>
            <RibbonGroup label={t("writer.find")}>
              <ToolButton
                icon={<Search size={16} />}
                label={t("writer.findReplace")}
                onClick={() => setFindOpen(true)}
              />
            </RibbonGroup>
          </>
        ) : null}

        {ribbon === "view" ? (
          <>
            <RibbonGroup label={t("writer.view")}>
              <ToolButton
                icon={<LayoutList size={16} />}
                label={t("writer.paginated")}
                onClick={() => setView("paginated")}
                active={view === "paginated"}
              />
              <ToolButton
                label={t("writer.continuous")}
                onClick={() => setView("continuous")}
                active={view === "continuous"}
              />
              <ToolButton
                icon={<ListTree size={16} />}
                label={t("writer.navigation")}
                onClick={() => setNavOpen((open) => !open)}
                active={navOpen}
              />
            </RibbonGroup>
            <RibbonGroup label={t("writer.zoom")}>
              <ToolButton label="75%" onClick={() => setZoom(0.75)} active={zoom === 0.75} />
              <ToolButton label="100%" onClick={() => setZoom(1)} active={zoom === 1} />
              <ToolButton label="125%" onClick={() => setZoom(1.25)} active={zoom === 1.25} />
              <ToolButton label="150%" onClick={() => setZoom(1.5)} active={zoom === 1.5} />
            </RibbonGroup>
            <RibbonGroup label={t("writer.print")}>
              <ToolButton icon={<Printer size={16} />} label={t("common.print")} onClick={handlePrint} />
              <ToolButton icon={<FileDown size={16} />} label={t("writer.exportPdf")} onClick={handleExportPdf} />
            </RibbonGroup>
          </>
        ) : null}

        <div className="ribbon-spacer" />
        <RibbonGroup>
          <ToolButton
            icon={<FolderOpen size={16} />}
            label={t("common.open")}
            onClick={() => void openIntoWorkspace()}
          />
          <ToolButton
            icon={<Save size={16} />}
            label={t("common.save")}
            onClick={() => void session.save()}
            disabled={session.busy}
          />
          <ToolButton label={t("common.saveAs")} onClick={() => void session.saveAs()} disabled={session.busy} />
          <ToolButton icon={<FileDown size={16} />} label={t("writer.exportPdf")} onClick={handleExportPdf} />
        </RibbonGroup>
      </Ribbon>

      <div className="editor-toolbar">
        <span className="muted">{editingHeader ? t(`writer.${editingHeader}`) : tab.title}</span>
        {editingHeader ? (
          <button type="button" className="btn btn-soft" onClick={() => setEditingHeader(null)}>
            {t("writer.doneEditing")}
          </button>
        ) : null}
        <span className="spacer" />
        <span className="muted">
          {tab.path ?? t("writer.unsaved")} {tab.dirty ? "•" : ""}
        </span>
      </div>

      <div className="writer-stage">
        {navOpen ? (
          <div className="writer-nav-pane">
            <div className="comments-head">
              <strong>{t("writer.navigation")}</strong>
              <button
                type="button"
                className="icon-btn"
                onClick={() => setNavOpen(false)}
                aria-label={t("common.close")}
              >
                <X size={14} />
              </button>
            </div>
            {headingOutline().length === 0 ? <p className="muted">{t("writer.noHeadings")}</p> : null}
            {headingOutline().map((entry) => (
              <button
                key={entry.anchor}
                type="button"
                className="writer-nav-item"
                style={{ paddingLeft: 10 + (entry.level - 1) * 12 }}
                onClick={() => jumpToBlock(entry.anchor)}
              >
                {entry.text}
              </button>
            ))}
          </div>
        ) : null}

        <div className="editor-scroll">
          {view === "paginated" && !editingHeader ? (
            <div className="writer-pages">
              {pages.map((page, pageIndex) => {
                const section = sections[page.sectionIndex] ?? sections[0];
                const setup = section.page;
                const sheetWidth = setup.widthPt * (96 / 72) * zoom;
                const sheetHeight = setup.heightPt * (96 / 72) * zoom;
                const marginX = setup.marginLeftPt * (96 / 72) * zoom;
                const marginTop = setup.marginTopPt * (96 / 72) * zoom;
                const sheetContentHeight = Math.max(
                  120,
                  (setup.heightPt - setup.marginTopPt - setup.marginBottomPt) * (96 / 72) * zoom,
                );
                const isFirstPage = page.sectionPage === 1;
                const isEvenPage = page.sectionPage % 2 === 0;
                const headerBlocks =
                  section.differentFirstPage && isFirstPage
                    ? section.firstHeader
                    : section.differentOddEven && isEvenPage
                      ? section.evenHeader
                      : section.header;
                const footerBlocks =
                  section.differentFirstPage && isFirstPage
                    ? section.firstFooter
                    : section.differentOddEven && isEvenPage
                      ? section.evenFooter
                      : section.footer;
                const noteIds: string[] = [];
                for (const fragment of page.fragments) {
                  const fragmentBlock = document.blocks[fragment.index];
                  if (fragmentBlock?.type !== "paragraph") continue;
                  for (const run of fragmentBlock.runs) {
                    const id = run.footnote ?? run.endnote;
                    if (id && !noteIds.includes(id)) noteIds.push(id);
                  }
                }
                return (
                  <div
                    key={pageIndex}
                    className="writer-page writer-page-sheet"
                    data-page-index={pageIndex}
                    data-section-index={page.sectionIndex}
                    style={{ width: sheetWidth, minHeight: sheetHeight, padding: `${marginTop}px ${marginX}px` }}
                  >
                    {headerBlocks.length > 0 ? (
                      <div className="writer-header-zone muted">
                        <StaticBlocks
                          blocks={headerBlocks}
                          scope="header"
                          page={pageIndex + 1}
                          pages={pages.length}
                          zoom={zoom}
                          noteNumbers={noteNumbers}
                          onOpen={(index) => editHeaderFooter("header", index)}
                        />
                      </div>
                    ) : null}
                    <div
                      className="writer-body"
                      style={{ height: sheetContentHeight - page.noteHeightPx, overflow: "hidden" }}
                    >
                      {page.fragments.map((fragment, fragmentIndex) => (
                        <PageFragmentView
                          key={`${fragment.index}-${fragment.from}-${fragmentIndex}`}
                          fragment={fragment}
                          block={document.blocks[fragment.index]}
                          zoom={zoom}
                          page={pageIndex + 1}
                          pages={pages.length}
                          title={document.metadata.title}
                          author={document.metadata.author}
                          listNumbers={listNumbers}
                          noteNumbers={noteNumbers}
                          showRevisions={document.showRevisions !== false}
                          active={
                            pageEdit?.block === fragment.index && activeFragmentFrom(fragment.index) === fragment.from
                          }
                          onActivate={activatePageEdit}
                          onExtendSelection={extendPageSelection}
                          onSync={(element) => syncParagraph(fragment.index, element)}
                          onStructure={(action) => handleStructure(action, "page")}
                          onFocusParagraph={handleParagraphFocus}
                          onRecordCaret={recordPageCaret}
                          onCaretOut={(direction) => {
                            moveActiveFragment(fragment.index, direction);
                          }}
                          onArrowAtEdge={(direction) => moveActiveFragment(fragment.index, direction)}
                          onOpen={editBlock}
                        />
                      ))}
                    </div>
                    {page.noteHeightPx > 0 ? (
                      <div className="writer-notes" style={{ height: page.noteHeightPx }}>
                        <div className="writer-notes-separator" />
                        {noteIds.map((id) => {
                          const note =
                            document.footnotes?.find((candidate) => candidate.id === id) ??
                            document.endnotes?.find((candidate) => candidate.id === id);
                          if (!note) return null;
                          return (
                            <div key={id} className="writer-note">
                              <sup>{noteNumbers[id] ?? ""}</sup> {runsText(note.runs)}
                            </div>
                          );
                        })}
                      </div>
                    ) : null}
                    {footerBlocks.length > 0 ? (
                      <div className="writer-footer-zone muted">
                        <StaticBlocks
                          blocks={footerBlocks}
                          scope="footer"
                          page={pageIndex + 1}
                          pages={pages.length}
                          zoom={zoom}
                          noteNumbers={noteNumbers}
                          onOpen={(index) => editHeaderFooter("footer", index)}
                        />
                      </div>
                    ) : null}
                  </div>
                );
              })}
            </div>
          ) : (
            <div
              className="writer-page"
              ref={bodyRef}
              data-scope={editingHeader ?? "body"}
              role="presentation"
              onMouseDown={handlePageMouseDown}
              style={{ width: pageWidth, minHeight: pageHeight, padding: `${marginTop}px ${marginX}px` }}
            >
              {editingHeader ? (
                <div className="writer-header-zone">
                  {renderBlocks(editingHeader === "header" ? document.header : document.footer, editingHeader)}
                </div>
              ) : (
                <>
                  {document.header.length > 0 ? (
                    <div className="writer-header-zone muted">{renderBlocks(document.header, "header")}</div>
                  ) : null}
                  <div className="writer-body">{renderBlocks(document.blocks, "body")}</div>
                  {document.footer.length > 0 ? (
                    <div className="writer-footer-zone muted">{renderBlocks(document.footer, "footer")}</div>
                  ) : null}
                </>
              )}
            </div>
          )}
        </div>
      </div>

      {/* Hidden probe: the pagination engine measures this column. */}
      <div className="writer-probe" aria-hidden="true" ref={probeRef} style={{ width: contentWidthPx }}>
        <StaticBlocks
          blocks={document.blocks}
          scope="probe"
          page={1}
          pages={1}
          zoom={zoom}
          listNumbers={listNumbers}
          fieldValues={documentFields}
          onOpen={() => undefined}
        />
      </div>

      <div className="editor-status">
        <span>
          {stats.words} {t("writer.words")}
        </span>
        <span>
          {stats.characters} {t("writer.characters")}
        </span>
        <span>
          {pageCount} {t("writer.pages")}
        </span>
        <span className="spacer" />
        <span>{Math.round(zoom * 100)}%</span>
      </div>

      {findOpen ? (
        <Dialog title={t("writer.findReplace")} onClose={closeFind}>
          <div className="stack">
            <label className="field">
              <span>{t("writer.findWhat")}</span>
              <input
                value={findText}
                onChange={(event) => setFindText(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key !== "Enter") return;
                  event.preventDefault();
                  findStep(!event.shiftKey);
                }}
                aria-invalid={search?.ok === false}
                // eslint-disable-next-line jsx-a11y/no-autofocus -- opening Find must put the caret in the search field
                autoFocus
              />
            </label>
            {search && !search.ok ? (
              <p className="find-error" role="alert">
                {t("writer.findInvalidRegex")}: {search.error}
              </p>
            ) : (
              <p className="find-count muted" role="status">
                {findStatus}
              </p>
            )}
            <label className="field">
              <span>{t("writer.replaceWith")}</span>
              <input value={replaceText} onChange={(event) => setReplaceText(event.target.value)} />
            </label>
            <div className="row">
              <label className="check">
                <input type="checkbox" checked={matchCase} onChange={(event) => setMatchCase(event.target.checked)} />{" "}
                {t("writer.matchCase")}
              </label>
              <label className="check">
                <input type="checkbox" checked={wholeWord} onChange={(event) => setWholeWord(event.target.checked)} />{" "}
                {t("writer.wholeWord")}
              </label>
              <label className="check">
                <input type="checkbox" checked={useRegex} onChange={(event) => setUseRegex(event.target.checked)} />{" "}
                {t("writer.regex")}
              </label>
            </div>
            <div className="row">
              <button type="button" className="btn btn-soft" onClick={() => findStep(true)} disabled={!search?.ok}>
                {t("writer.findNext")}
              </button>
              <button type="button" className="btn btn-soft" onClick={() => findStep(false)} disabled={!search?.ok}>
                {t("writer.findPrevious")}
              </button>
              <button type="button" className="btn btn-soft" onClick={replaceCurrent} disabled={!search?.ok}>
                {t("writer.replace")}
              </button>
              <button type="button" className="btn btn-primary" onClick={replaceAll} disabled={!search?.ok}>
                {t("writer.replaceAll")}
              </button>
            </div>
          </div>
        </Dialog>
      ) : null}

      {aiJob ? (
        <WriterAiDialog
          task={aiJob.task}
          docId={tab.id}
          status={aiStatus}
          original={aiJob.original}
          wholeParagraph={aiJob.wholeParagraph}
          onAccept={acceptAi}
          onClose={() => setAiJob(null)}
        />
      ) : null}

      {insertTable ? (
        <Dialog title={t("writer.insertTable")} onClose={() => setInsertTable(false)}>
          {picker.grid}
        </Dialog>
      ) : null}

      {selectedImage !== null ? (
        <Dialog title={t("writer.imageOptions")} onClose={() => setSelectedImage(null)}>
          <ImageOptions
            block={document.blocks[selectedImage]}
            onChange={(width, height, align, caption) =>
              update((doc) => {
                const blocks = [...doc.blocks];
                const block = blocks[selectedImage];
                if (block?.type === "image")
                  blocks[selectedImage] = { ...block, widthPt: width, heightPt: height, align, caption };
                return { ...doc, blocks };
              })
            }
          />
        </Dialog>
      ) : null}

      {commentsOpen ? (
        <div className="comments-sidebar">
          <div className="comments-head">
            <strong>{t("writer.comments")}</strong>
            <button
              type="button"
              className="icon-btn"
              onClick={() => setCommentsOpen(false)}
              aria-label={t("common.close")}
            >
              <X size={14} />
            </button>
          </div>
          {document.comments.length === 0 ? <p className="muted">{t("writer.noComments")}</p> : null}
          {document.comments.map((comment) => (
            <div key={comment.id} className={`comment-card${comment.resolved ? " is-resolved" : ""}`}>
              <div className="row">
                <strong>{comment.author}</strong>
                <span className="spacer" />
                <button
                  type="button"
                  className="icon-btn"
                  title={t("writer.resolveComment")}
                  onClick={() =>
                    update((doc) => ({
                      ...doc,
                      comments: doc.comments.map((entry) =>
                        entry.id === comment.id ? { ...entry, resolved: !entry.resolved } : entry,
                      ),
                    }))
                  }
                >
                  ✓
                </button>
                <button
                  type="button"
                  className="icon-btn"
                  title={t("common.delete")}
                  onClick={() =>
                    update((doc) => ({
                      ...doc,
                      comments: doc.comments.filter((entry) => entry.id !== comment.id),
                      blocks: doc.blocks.map((block) =>
                        block.type === "paragraph"
                          ? {
                              ...block,
                              runs: block.runs.map((run) =>
                                run.comment === comment.id ? { ...run, comment: null } : run,
                              ),
                            }
                          : block,
                      ),
                    }))
                  }
                >
                  <Trash2 size={13} />
                </button>
              </div>
              <p>{comment.text}</p>
              {(comment.replies ?? []).map((reply, replyIndex) => (
                <p key={replyIndex} className="comment-reply muted">
                  <strong>{reply.author}:</strong> {reply.text}
                </p>
              ))}
              <CommentReplyInput
                placeholder={t("writer.reply")}
                onSubmit={(text) =>
                  update((doc) => ({
                    ...doc,
                    comments: doc.comments.map((entry) =>
                      entry.id === comment.id
                        ? {
                            ...entry,
                            replies: [
                              ...(entry.replies ?? []),
                              { author: revisionAuthor, text, created: new Date().toISOString() },
                            ],
                          }
                        : entry,
                    ),
                  }))
                }
              />
            </div>
          ))}
        </div>
      ) : null}

      {reviewOpen ? (
        <div className="comments-sidebar writer-review-pane">
          <div className="comments-head">
            <strong>{t("writer.reviewPane")}</strong>
            <span className="spacer" />
            <span className="muted">{revisionList(document).length}</span>
            <button
              type="button"
              className="icon-btn"
              onClick={() => setReviewOpen(false)}
              aria-label={t("common.close")}
            >
              <X size={14} />
            </button>
          </div>
          {revisionList(document).length === 0 ? <p className="muted">{t("writer.noRevisions")}</p> : null}
          {revisionList(document).map((revision) => (
            <div
              key={revision.id}
              className={`comment-card${activeRevision === revision.id ? " is-active" : ""}`}
              role="button"
              tabIndex={0}
              aria-pressed={activeRevision === revision.id}
              onClick={() => setActiveRevision(revision.id)}
              onKeyDown={(event) => {
                if (event.key === "Enter" || event.key === " ") {
                  event.preventDefault();
                  setActiveRevision(revision.id);
                }
              }}
            >
              <div className="row">
                <strong>{t(`writer.revision_${revision.kind}`)}</strong>
                <span className="spacer" />
                <span className="muted">{revision.author}</span>
              </div>
              <p
                className={
                  revision.kind === "delete"
                    ? "writer-rev-delete"
                    : revision.kind === "insert"
                      ? "writer-rev-insert"
                      : ""
                }
              >
                {revision.text || "…"}
              </p>
              <div className="row">
                <button
                  type="button"
                  className="btn btn-soft"
                  onClick={(event) => {
                    event.stopPropagation();
                    update((doc) => acceptRevision(doc, revision.id));
                    setActiveRevision(null);
                    setLayoutVersion((version) => version + 1);
                  }}
                >
                  {t("writer.accept")}
                </button>
                <button
                  type="button"
                  className="btn btn-soft"
                  onClick={(event) => {
                    event.stopPropagation();
                    update((doc) => rejectRevision(doc, revision.id));
                    setActiveRevision(null);
                    setLayoutVersion((version) => version + 1);
                  }}
                >
                  {t("writer.reject")}
                </button>
              </div>
            </div>
          ))}
        </div>
      ) : null}

      {fieldOpen ? (
        <Dialog title={t("writer.crossReference")} onClose={() => setFieldOpen(false)}>
          <div className="stack">
            <label className="field">
              <span>{t("writer.fieldKind")}</span>
              <select value={fieldKind} onChange={(event) => setFieldKind(event.target.value as FieldRef["kind"])}>
                <option value="ref">{t("writer.fieldRef")}</option>
                <option value="refPage">{t("writer.fieldRefPage")}</option>
                <option value="bookmark">{t("writer.fieldBookmark")}</option>
              </select>
            </label>
            <label className="field">
              <span>{t("writer.bookmarkTarget")}</span>
              <select value={fieldTarget} onChange={(event) => setFieldTarget(event.target.value)}>
                <option value="">—</option>
                {(document.bookmarks ?? []).map((bookmark) => (
                  <option key={bookmark.id} value={bookmark.name}>
                    {bookmark.name}
                  </option>
                ))}
              </select>
            </label>
            <div className="row">
              <button
                type="button"
                className="btn"
                onClick={() => {
                  if (!fieldTarget) return;
                  insertField(fieldKind, fieldTarget);
                  setFieldOpen(false);
                }}
              >
                {t("writer.insertField")}
              </button>
              <input
                placeholder={t("writer.bookmarkName")}
                value={bookmarkName}
                onChange={(event) => setBookmarkName(event.target.value)}
              />
              <button type="button" className="btn btn-soft" onClick={addBookmark}>
                {t("writer.addBookmark")}
              </button>
            </div>
            <p className="muted">{t("writer.bookmarkHint")}</p>
          </div>
        </Dialog>
      ) : null}

      {sectionsOpen ? (
        <Dialog title={t("writer.sections")} onClose={() => setSectionsOpen(false)}>
          <div className="stack">
            {sections.map((section, sectionIndex) => (
              <div key={sectionIndex} className="section-card">
                <div className="row">
                  <strong>
                    {t("writer.section")} {sectionIndex + 1}
                  </strong>
                  <span className="spacer" />
                  {sectionIndex > 0 ? (
                    <button
                      type="button"
                      className="icon-btn"
                      title={t("common.delete")}
                      onClick={() => removeSectionBreak(sectionIndex)}
                    >
                      <Trash2 size={13} />
                    </button>
                  ) : null}
                </div>
                <div className="row">
                  <select
                    value={section.page.size}
                    onChange={(event) =>
                      updateSection(sectionIndex, {
                        page: {
                          ...section.page,
                          ...sizeDimensions(event.target.value, section.page.orientation),
                          size: event.target.value,
                        },
                      })
                    }
                  >
                    {["a4", "a5", "letter", "legal", "a3"].map((size) => (
                      <option key={size} value={size}>
                        {size.toUpperCase()}
                      </option>
                    ))}
                  </select>
                  <select
                    value={section.page.orientation}
                    onChange={(event) => {
                      const landscape = event.target.value === "landscape";
                      const currently = section.page.widthPt > section.page.heightPt;
                      const page = { ...section.page, orientation: event.target.value };
                      if (landscape !== currently) {
                        const width = page.widthPt;
                        page.widthPt = page.heightPt;
                        page.heightPt = width;
                      }
                      updateSection(sectionIndex, { page });
                    }}
                  >
                    <option value="portrait">{t("writer.portrait")}</option>
                    <option value="landscape">{t("writer.landscape")}</option>
                  </select>
                  <select
                    value={section.start}
                    onChange={(event) =>
                      sectionIndex === sections.length - 1
                        ? undefined
                        : updateSection(sectionIndex, { start: event.target.value })
                    }
                  >
                    <option value="newPage">{t("writer.startNewPage")}</option>
                    <option value="continuous">{t("writer.startContinuous")}</option>
                    <option value="oddPage">{t("writer.startOdd")}</option>
                    <option value="evenPage">{t("writer.startEven")}</option>
                  </select>
                </div>
              </div>
            ))}
          </div>
        </Dialog>
      ) : null}
    </div>
  );
}

function CommentReplyInput({ placeholder, onSubmit }: { placeholder: string; onSubmit: (text: string) => void }) {
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

function StaticParagraph({
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
}: {
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
}) {
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
}

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
                    colSpan={cell.colspan}
                    rowSpan={cell.rowspan}
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

function StaticBlocks({
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
}: {
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
}) {
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
}

/** One page fragment: whole block, paragraph lines or table rows. */
function PageFragmentView({
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

function BlockView({
  block,
  index,
  scope,
  zoom,
  noteNumbers,
  showRevisions,
  listNumber,
  fieldValues,
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
    return (
      <figure className="writer-image" style={{ textAlign: block.align as "left" | "center" | "right" }}>
        <button
          type="button"
          onClick={() => onSelectImage(index)}
          style={{ background: "none", border: "none", padding: 0, cursor: "pointer" }}
        >
          <img
            src={`data:${block.image.mime};base64,${block.image.dataBase64}`}
            alt={block.image.alt}
            style={{ width: block.widthPt * (96 / 72) * zoom }}
          />
        </button>
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

function TableView({
  block,
  index,
  zoom,
  onUpdate,
  onSyncCell,
}: {
  block: Extract<Block, { type: "table" }>;
  index: number;
  zoom: number;
  onUpdate: (block: Block) => void;
  onSyncCell: (path: [number, number, number, number], element: HTMLElement) => void;
}) {
  const table = block.table;
  const [menuCell, setMenuCell] = useState<{ row: number; cell: number } | null>(null);
  const updateTable = (mutate: (table: TableData) => TableData) => onUpdate({ type: "table", table: mutate(table) });

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
          {table.rows.map((row, rowIndex) => (
            <tr key={rowIndex} className={row.header ? "is-header" : ""}>
              {row.cells.map((cell, cellIndex) => (
                <td
                  key={cellIndex}
                  colSpan={cell.colspan}
                  rowSpan={cell.rowspan}
                  style={{
                    background: cell.background ?? undefined,
                    textAlign: (cell.align || "left") as "left" | "center" | "right",
                    verticalAlign: (cell.valign || "top") as "top" | "middle" | "bottom",
                  }}
                  onContextMenu={(event) => {
                    event.preventDefault();
                    setMenuCell({ row: rowIndex, cell: cellIndex });
                  }}
                >
                  {(cell.blocks.length > 0 ? cell.blocks : [newParaBlock()]).map((inner, innerIndex) =>
                    inner.type === "paragraph" ? (
                      <CellParagraph
                        key={innerIndex}
                        block={inner}
                        tablePath={[index, rowIndex, cellIndex, innerIndex]}
                        onSyncCell={onSyncCell}
                      />
                    ) : null,
                  )}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
      {menuCell ? (
        <Dialog title={`Row ${menuCell.row + 1} · Cell ${menuCell.cell + 1}`} onClose={() => setMenuCell(null)}>
          <div className="row">
            <button
              type="button"
              className="btn btn-soft"
              onClick={() => {
                updateTable((current) => ({
                  ...current,
                  rows: [
                    ...current.rows.slice(0, menuCell.row + 1),
                    { ...current.rows[menuCell.row], header: false },
                    ...current.rows.slice(menuCell.row + 1),
                  ],
                }));
                setMenuCell(null);
              }}
            >
              Add row below
            </button>
            <button
              type="button"
              className="btn btn-soft"
              onClick={() => {
                updateTable((current) =>
                  current.rows.length > 1
                    ? { ...current, rows: current.rows.filter((_, position) => position !== menuCell.row) }
                    : current,
                );
                setMenuCell(null);
              }}
            >
              Delete row
            </button>
            <button
              type="button"
              className="btn btn-soft"
              onClick={() => {
                updateTable((current) => ({
                  ...current,
                  rows: current.rows.map((row) => ({
                    ...row,
                    cells: [
                      ...row.cells.slice(0, menuCell.cell + 1),
                      { ...row.cells[menuCell.cell] },
                      ...row.cells.slice(menuCell.cell + 1),
                    ],
                  })),
                }));
                setMenuCell(null);
              }}
            >
              Add column
            </button>
            <button
              type="button"
              className="btn btn-soft"
              onClick={() => {
                updateTable((current) => ({
                  ...current,
                  rows: current.rows.map((row) =>
                    row.cells.length > 1
                      ? { ...row, cells: row.cells.filter((_, position) => position !== menuCell.cell) }
                      : row,
                  ),
                }));
                setMenuCell(null);
              }}
            >
              Delete column
            </button>
            <button
              type="button"
              className="btn btn-soft"
              onClick={() => {
                updateTable((current) => ({
                  ...current,
                  rows: current.rows.map((row, r) =>
                    r !== menuCell.row
                      ? row
                      : {
                          ...row,
                          cells: row.cells.map((cell, c) =>
                            c !== menuCell.cell ? cell : { ...cell, background: cell.background ? null : "#EEF2FF" },
                          ),
                        },
                  ),
                }));
                setMenuCell(null);
              }}
            >
              Toggle cell shade
            </button>
            <button
              type="button"
              className="btn btn-soft"
              onClick={() => {
                updateTable((current) => ({ ...current, borders: !current.borders }));
                setMenuCell(null);
              }}
            >
              Toggle borders
            </button>
          </div>
        </Dialog>
      ) : null}
    </div>
  );
}

function CellParagraph({
  block,
  tablePath,
  onSyncCell,
}: {
  block: Extract<Block, { type: "paragraph" }>;
  tablePath: [number, number, number, number];
  onSyncCell: (path: [number, number, number, number], element: HTMLElement) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [focused, setFocused] = useState(false);
  useEffect(() => {
    if (!focused && ref.current) ref.current.innerHTML = runsToHtml(block.runs);
  }, [block.runs, focused]);
  return (
    <div
      ref={ref}
      className="para cell-para"
      contentEditable
      suppressContentEditableWarning
      onFocus={() => setFocused(true)}
      onBlur={(event) => {
        setFocused(false);
        onSyncCell(tablePath, event.currentTarget);
      }}
      onInput={(event) => onSyncCell(tablePath, event.currentTarget)}
      style={{ textAlign: (block.props.align || "left") as "left" | "center" | "right" }}
    />
  );
}

function ImageOptions({
  block,
  onChange,
}: {
  block: Block;
  onChange: (width: number, height: number, align: string, caption: string) => void;
}) {
  if (block.type !== "image") return null;
  const aspect = block.widthPt / Math.max(1, block.heightPt);
  return (
    <div className="stack">
      <label className="field">
        <span>Width (pt)</span>
        <input
          type="number"
          value={Math.round(block.widthPt)}
          onChange={(event) => {
            const width = Number(event.target.value);
            onChange(width, Math.round(width / aspect), block.align, block.caption);
          }}
        />
      </label>
      <label className="field">
        <span>Caption</span>
        <input
          value={block.caption}
          onChange={(event) => onChange(block.widthPt, block.heightPt, block.align, event.target.value)}
        />
      </label>
      <label className="field">
        <span>Alignment</span>
        <select
          value={block.align}
          onChange={(event) => onChange(block.widthPt, block.heightPt, event.target.value, block.caption)}
        >
          <option value="left">Left</option>
          <option value="center">Center</option>
          <option value="right">Right</option>
        </select>
      </label>
    </div>
  );
}

// ---------------------------------------------------------------------------
// DOM helpers
// ---------------------------------------------------------------------------

const emptyRun = emptyWriterRun;

function effectiveFontSize(block: Extract<Block, { type: "paragraph" }>): number | null {
  return block.runs.find((run) => run.sizePt)?.sizePt ?? null;
}

function applyVisualStyle(element: HTMLElement, property: string, value: string) {
  element.style.setProperty(property, value);
}

/** True when the selection overlaps `box` vertically, or when there is no layout to tell (jsdom). */
function selectionShownIn(box: HTMLElement): boolean {
  const selection = window.getSelection();
  if (!selection || selection.rangeCount === 0) return true;
  const range = selection.getRangeAt(0);
  const rect = typeof range.getBoundingClientRect === "function" ? range.getBoundingClientRect() : null;
  const bounds = box.getBoundingClientRect();
  if (!rect || rect.height === 0 || bounds.height === 0) return true;
  return rect.bottom > bounds.top && rect.top < bounds.bottom;
}

function mimeFromName(name: string): string {
  const extension = name.split(".").pop()?.toLowerCase() ?? "";
  switch (extension) {
    case "jpg":
    case "jpeg":
      return "image/jpeg";
    case "gif":
      return "image/gif";
    case "bmp":
      return "image/bmp";
    case "webp":
      return "image/webp";
    case "svg":
      return "image/svg+xml";
    default:
      return "image/png";
  }
}

function sizeDimensions(size: string, orientation: string): { widthPt: number; heightPt: number } {
  const setup = defaultPageSetup(size, orientation);
  return { widthPt: setup.widthPt, heightPt: setup.heightPt };
}

export function blankWriterDocument(title: string): TextDocument {
  return newTextDocument(title);
}

export { emptyMetadata };
