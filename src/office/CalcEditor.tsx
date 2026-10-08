/**
 * Calc editor: virtualised spreadsheet grid with a real formula engine,
 * formatting, multiple sheets, sorting, conditional formatting, validation
 * and SVG charts fed from cell ranges.
 */
import { useCallback, useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from "react";
import { Check, ChevronDown, X } from "lucide-react";
import { isAndroid } from "../lib/mobile";
import type { OfficeTab, Workbook } from "../lib/office-store";
import { useOfficeTabs } from "../lib/office-store";
import { useT } from "../lib/i18n";
import { useToasts } from "../lib/store";
import {
  cellText,
  defaultCellStyle,
  defaultPrintSettings,
  emptyCell,
  newSheet,
  newSpreadsheetTable,
  type Cell,
  type CellStyle,
  type ChartData,
  type PivotTable,
  type PivotValueField,
  type Sheet,
  type SpreadsheetTable,
} from "../lib/office-types";
import {
  addressesInRange,
  columnLabel,
  formatAddress,
  functionCatalogue,
  isError,
  parseAddress,
  parseRange,
  toText,
  type Scalar,
} from "./calc/formula";
import { addressInRange } from "./calc/addresses";
import { AI_EDIT_MAX_CHARS, useAiStatus } from "./ai/editor-ai";
import { columnValueLines, headerContext, lastRowOfColumn, summaryCellText } from "./calc/ai-calc";
import { SuggestFormulaDialog, SummarizeColumnDialog } from "./calc/CalcAiDialogs";
import {
  findCircularReferences,
  invalidReferences,
  traceDependents,
  tracePrecedents,
  type AuditNode,
} from "./calc/audit";
import { validationLookup } from "./calc/validation-index";
import { revealScroll } from "./calc/grid-geometry";
import { isUnderBand, mergeFrozen, nextFreeze, pinnedPosition, resolveFrozenBands } from "./calc/freeze";
import { rowLayoutFor } from "./calc/row-layout";
import {
  applyCellEdit,
  applyCellEdits,
  computeSheetValues,
  computeWorkbookValues,
  formatCellDisplay,
  isBlankCell,
  parseInputValue,
  scalarToCellValue,
  shiftFormulaRows,
  uniqueSheetName,
  usedRange,
} from "./calc/cells";
import {
  findDuplicateRows,
  listValidationItems,
  remapMovedRows,
  type ColumnSplitPlan,
  type DuplicateOptions,
} from "./calc/data-tools";
import { applyFilterDraft, clearFilter, sheetFilterDraft, tableFilterDraft, type FilterDraft } from "./calc/filter";
import { argumentHintFor, buildSuggestions, type FormulaSuggestion } from "./calc/formula-assist";
import { clampGridZoom, pinchGridZoom, shiftFormulaColumns } from "./calc/grid-math";
import type { CellPosition, GridSelection } from "./calc/grid-types";
import { computeConditionalFills, computeDataBars, isValid } from "./calc/rules";
import { deleteColumn, deleteRow, insertColumn, insertRow, toggleMerge } from "./calc/structure";
import { uniqueColumnName, uniqueTableName } from "./calc/table-names";
import { CalcRibbon } from "./calc/ui/CalcRibbon";
import { ChartBox, ChartDialog } from "./calc/ui/ChartPanel";
import { ConditionalDialog } from "./calc/ui/ConditionalDialog";
import { RemoveDuplicatesDialog, TextToColumnsDialog } from "./calc/ui/DataToolsDialogs";
import { FilterDialog } from "./calc/ui/FilterDialog";
import { FormulaAssistPopup } from "./calc/ui/FormulaAssistPopup";
import { isValidDefinedName, NameManagerDialog } from "./calc/ui/NameManagerDialog";
import { PivotBox, PivotDialog } from "./calc/ui/PivotPanel";
import { PrintLayoutDialog } from "./calc/ui/PrintLayoutDialog";
import { SheetTabs } from "./calc/ui/SheetTabs";
import { FindReplacePanel } from "./calc/ui/FindReplacePanel";
import { InsertTableDialog, TablesPanel } from "./calc/ui/TablesUi";
import { useFindReplace } from "./calc/ui/useFindReplace";
import { ValidationDialog } from "./calc/ui/ValidationDialog";
import { useEditorShortcuts, useOfficeSession } from "./useOfficeSession";

export { scalarToCellValue, computeWorkbookValues, applyCellEdit, shiftFormulaRows, isBlankCell };
export { clampGridZoom, pinchGridZoom, shiftFormulaColumns, isValidDefinedName };

type CalcTab = OfficeTab & { model: Workbook };

const ROW_HEIGHT = 24;
/** The column header band above the first row. */
const HEADER_HEIGHT = 24;
const HEADER_WIDTH = 56;
const DEFAULT_COL_WIDTH = 96;

interface EditingCell extends CellPosition {
  value: string;
}

/** Where a committed edit sends the selection. */
type CommitMove = "down" | "up" | "right" | "left" | "none";

/** The element carrying `attribute` under a pointer event: the event target
 * when the event bubbles from it, or hit-testing at the pointer's coordinates
 * when the pointer is captured by the grid (capture retargets every move). */
function attributeAtEvent(event: PointerEvent | React.PointerEvent, attribute: string): HTMLElement | null {
  const closest = (element: Element | null | undefined) => element?.closest?.<HTMLElement>(`[${attribute}]`) ?? null;
  const fromTarget = closest(event.target as Element | null);
  if (fromTarget) return fromTarget;
  if (typeof document.elementFromPoint !== "function") return null;
  return closest(document.elementFromPoint(event.clientX, event.clientY));
}

/** The cell under a pointer event, or null. */
function cellAtEvent(event: PointerEvent | React.PointerEvent): CellPosition | null {
  const element = attributeAtEvent(event, "data-cell");
  return element ? { row: Number(element.dataset.row), col: Number(element.dataset.col) } : null;
}

interface DragBounds {
  start: CellPosition;
  end: CellPosition;
}

type GridGesture =
  | { kind: "cells"; pointerId: number; anchor: CellPosition; startX: number; startY: number; moved: boolean }
  | { kind: "rows"; pointerId: number; anchorRow: number }
  | { kind: "cols"; pointerId: number; anchorCol: number }
  | { kind: "resize"; pointerId: number; col: number; startX: number; startWidth: number }
  | { kind: "fill"; pointerId: number; source: DragBounds; target: CellPosition }
  | { kind: "pan"; pointerId: number; startX: number; startY: number; scrollLeft: number; scrollTop: number }
  // A finger on a cell: it pans the sheet once it moves and selects the cell
  // when it lifts without moving (ranges come from the selection handles).
  | {
      kind: "tap";
      pointerId: number;
      cell: CellPosition;
      startX: number;
      startY: number;
      scrollLeft: number;
      scrollTop: number;
      moved: boolean;
    }
  // A touch selection handle: the opposite corner stays, the dragged one follows.
  | { kind: "extend"; pointerId: number; fixed: CellPosition };

/** Movement (px) after which a finger on a cell pans instead of tapping. */
const TAP_SLOP = 8;

interface PinchGesture {
  startDistance: number;
  startZoom: number;
  /** Midpoint of the previous move, so two-finger pans are incremental and
   * survive a zoom change in the middle of the gesture. */
  lastMidX: number;
  lastMidY: number;
  /** Canvas coordinates under the pinch midpoint at the last move. */
  contentX: number;
  contentY: number;
  /** Midpoint relative to the grid viewport at the last move. */
  localX: number;
  localY: number;
}

export function CalcEditor({ tab }: { tab: CalcTab }) {
  const t = useT();
  const workbook = tab.model;
  const edit = useOfficeTabs((state) => state.edit);
  const session = useOfficeSession(tab);
  const [ribbon, setRibbon] = useState("home");
  const [sheetIndex, setSheetIndex] = useState(workbook.activeSheet ?? 0);
  const [selection, setSelection] = useState<GridSelection>({ anchor: { row: 0, col: 0 }, focus: { row: 0, col: 0 } });
  const [editing, setEditingState] = useState<EditingCell | null>(null);
  const [formulaDraft, setFormulaDraft] = useState("");
  const [scroll, setScroll] = useState({ top: 0, left: 0, width: 900, height: 500 });
  const [chartDialog, setChartDialog] = useState(false);
  const [pivotDialog, setPivotDialog] = useState(false);
  const [conditionalDialog, setConditionalDialog] = useState(false);
  const [validationDialog, setValidationDialog] = useState(false);
  const [nameDialog, setNameDialog] = useState(false);
  const [printDialog, setPrintDialog] = useState(false);
  const [textToColumnsDialog, setTextToColumnsDialog] = useState(false);
  const [duplicatesDialog, setDuplicatesDialog] = useState(false);
  const aiStatus = useAiStatus();
  // The AI dialogs remember the cell they were opened for: the selection may
  // move while the request runs, but the result goes where it was asked for.
  const [aiDialog, setAiDialog] = useState<
    | { kind: "summarize"; sheetIndex: number; row: number; col: number; lines: string[] }
    | { kind: "formula"; sheetIndex: number; row: number; col: number; headers: string; selection: string }
    | null
  >(null);
  // The open choice list of a list-validated active cell; `index` is the
  // highlighted choice. It always belongs to the active cell (see below).
  const [listDropdown, setListDropdown] = useState<{ index: number } | null>(null);
  const listDropdownRef = useRef<HTMLDivElement>(null);
  const listDropdownId = useId();
  const [filterOpen, setFilterOpen] = useState<FilterDraft | null>(null);
  const [undoStack, setUndoStack] = useState<Workbook[]>([]);
  const [redoStack, setRedoStack] = useState<Workbook[]>([]);
  const containerRef = useRef<HTMLDivElement>(null);
  const gridRef = useRef<HTMLDivElement>(null);
  const cellInputRef = useRef<HTMLInputElement>(null);
  // The inline editor unmounts on commit, and unmounting a focused element
  // fires `blur` - whose handler closure still sees the old `editing` value.
  // Mirroring the state in a ref lets `commitEdit` stay idempotent, and the
  // second flag hands keyboard focus back to the grid once the editor is gone
  // so consecutive typing after Enter keeps working.
  const editingRef = useRef<EditingCell | null>(null);
  const restoreGridFocusRef = useRef(false);
  const formulaInputRef = useRef<HTMLInputElement>(null);
  const pendingCaretRef = useRef<number | null>(null);
  const [focusMode, setFocusMode] = useState<"cell" | "bar" | null>(null);
  const [draftCaret, setDraftCaret] = useState(0);
  const [suggestIndex, setSuggestIndex] = useState(0);
  const [suggestDismissed, setSuggestDismissed] = useState(false);
  const [assistAnchor, setAssistAnchor] = useState<{ left: number; top: number; above?: boolean } | null>(null);
  const [tableDialog, setTableDialog] = useState(false);
  const [tablesPanel, setTablesPanel] = useState(false);
  const [trace, setTrace] = useState<{ kind: "precedents" | "dependents"; cells: AuditNode[] } | null>(null);
  // Touch zoom on the grid. 1 is the desktop layout; the pinch gesture moves
  // it inside [0.6, 2] (see pinchGridZoom).
  const [gridZoom, setGridZoom] = useState(1);
  const android = isAndroid();
  // Some Android WebViews overlay the soft keyboard instead of resizing the
  // viewport; visualViewport reports the covered height so the docked bar can
  // be lifted above it. When the viewport does resize this stays 0.
  const [keyboardInset, setKeyboardInset] = useState(0);
  const pointersRef = useRef(new Map<number, { x: number; y: number }>());
  const gestureRef = useRef<GridGesture | null>(null);
  const pinchRef = useRef<PinchGesture | null>(null);
  const pinchFocalRef = useRef<PinchGesture | null>(null);
  const lastTapRef = useRef<{ row: number; col: number; time: number } | null>(null);
  const gridZoomRef = useRef(gridZoom);
  useLayoutEffect(() => {
    gridZoomRef.current = gridZoom;
  }, [gridZoom]);

  const setEditing = useCallback((next: EditingCell | null) => {
    editingRef.current = next;
    setEditingState(next);
  }, []);

  const sheet = workbook.sheets[Math.min(sheetIndex, workbook.sheets.length - 1)] ?? workbook.sheets[0];
  // Row offsets honour custom heights and hidden (filtered) rows. The table is
  // memoized on the sheet's rowHeights object, so it is rebuilt only when a
  // height changes, not on every render or scroll.
  const rowLayout = rowLayoutFor(sheet.rowCount, sheet.rowHeights, ROW_HEIGHT);
  // Frozen panes: the rows above and the columns left of the freeze point stay
  // pinned under the headers while the rest scrolls beneath them.
  const freeze = useMemo(
    () =>
      resolveFrozenBands({
        freezeRows: sheet.freezeRows,
        freezeCols: sheet.freezeCols,
        rowCount: sheet.rowCount,
        colCount: sheet.colCount,
        rowOffset: rowLayout.offsetOf,
        colOffset: (count) => {
          let total = 0;
          for (let col = 0; col < count; col += 1) total += sheet.colWidths[String(col)] ?? DEFAULT_COL_WIDTH;
          return total;
        },
        viewportHeight: Math.max(0, scroll.height / gridZoom - HEADER_HEIGHT),
        viewportWidth: Math.max(0, scroll.width / gridZoom - HEADER_WIDTH),
      }),
    [sheet, rowLayout, scroll.height, scroll.width, gridZoom],
  );
  // Scroll offsets in canvas pixels: what frozen layers are shifted by.
  const scrollY = scroll.top / gridZoom;
  const scrollX = scroll.left / gridZoom;
  const activeCell = sheet.cells[formatAddress(selection.focus.row, selection.focus.col)];
  const computed = useMemo(() => computeSheetValues(workbook, sheet), [workbook, sheet]);
  const catalogue = useMemo(() => new Map(functionCatalogue().map((entry) => [entry.name, entry] as const)), []);

  // The draft and caret of whichever input owns the focus. The suggestion
  // popup and the argument hint are derived from these, never from the grid
  // selection, so the inline editor and the formula bar stay independent.
  const assistText = focusMode === "cell" ? (editing?.value ?? "") : focusMode === "bar" ? formulaDraft : "";
  const assistCaret = Math.min(Math.max(0, draftCaret), assistText.length);
  const suggestions = useMemo(() => {
    if (focusMode === null || suggestDismissed) return null;
    return buildSuggestions(assistText, assistCaret, workbook, sheet, catalogue, t);
  }, [focusMode, suggestDismissed, assistText, assistCaret, workbook, sheet, catalogue, t]);
  const argumentHint = useMemo(() => {
    if (focusMode === null) return null;
    return argumentHintFor(assistText, assistCaret, catalogue);
  }, [focusMode, assistText, assistCaret, catalogue]);
  const suggestionKey = suggestions?.items.map((item) => item.label).join("|") ?? "";
  const [lastSuggestionKey, setLastSuggestionKey] = useState(suggestionKey);
  if (lastSuggestionKey !== suggestionKey) {
    setLastSuggestionKey(suggestionKey);
    setSuggestIndex(0);
  }

  const [lastFocusMode, setLastFocusMode] = useState(focusMode);
  if (lastFocusMode !== focusMode) {
    setLastFocusMode(focusMode);
    if (focusMode === null) setAssistAnchor(null);
  }

  // Keep the popup glued under the focused input as the draft changes.
  useLayoutEffect(() => {
    if (focusMode === null) return;
    const input = focusMode === "cell" ? cellInputRef.current : formulaInputRef.current;
    if (!input) {
      setAssistAnchor(null);
      return;
    }
    const rect = input.getBoundingClientRect();
    // The Android bar is docked at the bottom and the soft keyboard shrinks
    // (or covers) the visible area, so the popup flips above the input when
    // space runs out. visualViewport is the usable height in overlay mode.
    const viewport = window.visualViewport;
    const usableBottom = Math.min(
      window.innerHeight,
      viewport ? viewport.height + viewport.offsetTop : window.innerHeight,
    );
    const above = rect.bottom + 200 > usableBottom;
    setAssistAnchor({ left: rect.left, top: above ? rect.top - 4 : rect.bottom + 2, above });
  }, [focusMode, assistText, assistCaret, editing]);

  /** Replaces the current token with the picked suggestion. */
  const applySuggestion = (suggestion: FormulaSuggestion) => {
    if (!suggestions) return;
    const { start, end } = suggestions;
    const next = assistText.slice(0, start) + suggestion.insert + assistText.slice(end);
    const caret = start + (suggestion.caret ?? suggestion.insert.length);
    pendingCaretRef.current = caret;
    setDraftCaret(caret);
    setSuggestDismissed(false);
    if (focusMode === "cell" && editingRef.current) {
      setEditing({ row: editingRef.current.row, col: editingRef.current.col, value: next });
    } else {
      setFormulaDraft(next);
      if (editingRef.current) setEditing({ ...editingRef.current, value: next });
    }
  };

  /** Popup keys consumed before the editor's own Enter/Tab handling. */
  const handleAssistKey = (event: React.KeyboardEvent<HTMLInputElement>): boolean => {
    if (!suggestions || suggestions.items.length === 0) return false;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      setSuggestIndex((index) => {
        const count = suggestions.items.length;
        return event.key === "ArrowDown" ? (index + 1) % count : (index - 1 + count) % count;
      });
      return true;
    }
    if (event.key === "Tab" || event.key === "Enter") {
      event.preventDefault();
      applySuggestion(suggestions.items[Math.min(suggestIndex, suggestions.items.length - 1)] ?? suggestions.items[0]);
      return true;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      setSuggestDismissed(true);
      return true;
    }
    return false;
  };

  const update = useCallback(
    (mutate: (workbook: Workbook) => Workbook, recordUndo = true) => {
      if (recordUndo) {
        setUndoStack((stack) => [...stack.slice(-40), workbook]);
        setRedoStack([]);
      }
      edit(tab.id, (model) => mutate(model as Workbook));
    },
    [edit, tab.id, workbook],
  );

  const updateSheet = useCallback(
    (mutate: (sheet: Sheet) => Sheet, recordUndo = true) => {
      update(
        (current) => ({
          ...current,
          sheets: current.sheets.map((candidate, index) => (index === sheetIndex ? mutate(candidate) : candidate)),
        }),
        recordUndo,
      );
    },
    [sheetIndex, update],
  );

  useEffect(() => {
    const element = containerRef.current;
    if (!element) return;
    const observer = new ResizeObserver(() => {
      setScroll((current) => ({ ...current, width: element.clientWidth, height: element.clientHeight }));
    });
    observer.observe(element);
    setScroll((current) => ({ ...current, width: element.clientWidth, height: element.clientHeight }));
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (!android || !window.visualViewport) return;
    const viewport = window.visualViewport;
    const update = () =>
      setKeyboardInset(Math.max(0, Math.round(window.innerHeight - viewport.height - viewport.offsetTop)));
    viewport.addEventListener("resize", update);
    viewport.addEventListener("scroll", update);
    update();
    return () => {
      viewport.removeEventListener("resize", update);
      viewport.removeEventListener("scroll", update);
    };
  }, [android]);

  const formulaDraftValue = activeCell?.formula ?? cellText(activeCell);
  const draftKey = `${selection.focus.row}:${selection.focus.col}|${formulaDraftValue}`;
  const [lastDraftKey, setLastDraftKey] = useState<string | null>(null);
  if (lastDraftKey !== draftKey) {
    setLastDraftKey(draftKey);
    setFormulaDraft(formulaDraftValue);
  }

  // Focus the inline editor as soon as it opens and put the caret at the end,
  // so fast typing never loses characters. When the editor closes again the
  // focus goes back to the grid, which is what keeps arrow keys, Tab and plain
  // character entry alive after Enter.
  //
  // This is a layout effect on purpose. A passive effect runs after paint, so
  // a keystroke that arrives between Enter and the effect was dispatched to
  // `<body>` and swallowed; the next cell only accepted input after a click.
  // Layout effects are flushed at the end of the same discrete event, before
  // the browser can deliver the following keydown.
  useLayoutEffect(() => {
    if (editing) {
      const input = cellInputRef.current;
      if (!input) return;
      input.focus();
      // A suggestion insertion places the caret itself (inside `NAME(`); a
      // plain edit keeps it at the end so fast typing never loses characters.
      const caret = pendingCaretRef.current ?? input.value.length;
      pendingCaretRef.current = null;
      try {
        input.setSelectionRange(caret, caret);
      } catch {
        // setSelectionRange is not supported for every input type.
      }
      return;
    }
    if (!restoreGridFocusRef.current) return;
    restoreGridFocusRef.current = false;
    const grid = gridRef.current;
    if (grid && document.activeElement !== grid) grid.focus({ preventScroll: true });
  }, [editing]);

  // The formula bar is a controlled input, so an inserted suggestion changes
  // the value without a selection event; this restores the caret it asked for.
  useLayoutEffect(() => {
    const caret = pendingCaretRef.current;
    if (caret === null || editing) return;
    const input = formulaInputRef.current;
    if (!input || document.activeElement !== input) return;
    pendingCaretRef.current = null;
    try {
      input.setSelectionRange(caret, caret);
    } catch {
      // Not every input type supports setSelectionRange.
    }
  }, [formulaDraft, editing]);

  // -------------------------------------------------------------------------
  // Cell helpers
  // -------------------------------------------------------------------------

  const setCells = (entries: Array<{ row: number; col: number; cell: Cell }>) => {
    updateSheet((current) => {
      const cells = { ...current.cells };
      for (const entry of entries) {
        const address = formatAddress(entry.row, entry.col);
        cells[address] = entry.cell;
      }
      return { ...current, cells };
    });
  };

  /** Keeps a cell position inside the sheet and scrolls it into view. */
  const revealCell = useCallback(
    (position: CellPosition) => {
      const clamped = {
        row: Math.min(Math.max(0, position.row), Math.max(0, sheet.rowCount - 1)),
        col: Math.min(Math.max(0, position.col), Math.max(0, sheet.colCount - 1)),
      };
      const grid = gridRef.current;
      if (!grid) return clamped;
      // Scroll offsets are outer pixels; the cell geometry is in canvas
      // coordinates, so the zoom the canvas is laid out with maps between.
      let left = 0;
      for (let col = 0; col < clamped.col; col += 1) left += sheet.colWidths[String(col)] ?? DEFAULT_COL_WIDTH;
      const next = revealScroll({
        cell: {
          top: rowLayout.offsetOf(clamped.row),
          height: rowLayout.heightOf(clamped.row),
          left,
          width: sheet.colWidths[String(clamped.col)] ?? DEFAULT_COL_WIDTH,
        },
        scrollTop: grid.scrollTop,
        scrollLeft: grid.scrollLeft,
        clientHeight: grid.clientHeight,
        clientWidth: grid.clientWidth,
        zoom: gridZoomRef.current,
        headerHeight: HEADER_HEIGHT,
        headerWidth: HEADER_WIDTH,
        frozenHeight: freeze.height,
        frozenWidth: freeze.width,
        rowFrozen: clamped.row < freeze.rows,
        colFrozen: clamped.col < freeze.cols,
      });
      if (next.scrollTop !== grid.scrollTop) grid.scrollTop = next.scrollTop;
      if (next.scrollLeft !== grid.scrollLeft) grid.scrollLeft = next.scrollLeft;
      return clamped;
    },
    [sheet, rowLayout, freeze],
  );

  // Selecting a cell of another sheet (Find next) switches the sheet first;
  // the scroll waits for the commit that renders it, where `revealCell` sees
  // that sheet's geometry.
  const pendingRevealRef = useRef<CellPosition | null>(null);
  const jumpToCell = useCallback((target: number, row: number, col: number) => {
    setSheetIndex(target);
    pendingRevealRef.current = { row, col };
    setSelection({ anchor: { row, col }, focus: { row, col } });
  }, []);
  useLayoutEffect(() => {
    const pending = pendingRevealRef.current;
    if (!pending) return;
    pendingRevealRef.current = null;
    revealCell(pending);
  }, [sheetIndex, selection, revealCell]);

  const find = useFindReplace({
    workbook,
    sheetIndex,
    focus: selection.focus,
    jumpTo: jumpToCell,
    commit: (next) => update(() => next),
    onClosed: () => gridRef.current?.focus({ preventScroll: true }),
  });
  useEditorShortcuts(session, { onFind: () => find.show("find"), onReplace: () => find.show("replace") });

  const commitEdit = (move: CommitMove = "down", restoreFocus = true) => {
    // Read through the ref: the blur handler triggered by unmounting the editor
    // would otherwise commit the same value a second time.
    const current = editingRef.current;
    if (!current) return;
    editingRef.current = null;
    setEditingState(null);
    // A blur commit means focus has already moved somewhere on purpose (the
    // formula bar, another editor); only keyboard commits pull it back to the
    // grid, otherwise clicking the formula bar would lose focus to the grid.
    restoreGridFocusRef.current = restoreFocus;

    const { row, col, value } = current;
    update((current_workbook) => applyCellEdit(current_workbook, sheetIndex, row, col, value));

    if (move === "none") return;
    const delta: Record<Exclude<CommitMove, "none">, CellPosition> = {
      down: { row: 1, col: 0 },
      up: { row: -1, col: 0 },
      right: { row: 0, col: 1 },
      left: { row: 0, col: -1 },
    };
    const step = delta[move];
    const next = revealCell({ row: row + step.row, col: col + step.col });
    setSelection({ anchor: next, focus: next });
    // Move focus in the same event, not in the effect: when the commit came
    // from the formula bar the editor state was already null, so no effect
    // would run at all and the grid stayed unfocused.
    if (restoreFocus) gridRef.current?.focus({ preventScroll: true });
  };

  // -------------------------------------------------------------------------
  // Pointer interaction (mouse, pen and touch share one code path)
  //
  // Pointer events are the single source of truth: Chromium fires them for
  // mouse input too, so a parallel mouse handler would process every drag
  // twice. The window-level move/up listeners keep a gesture alive when the
  // pointer leaves the grid, which happens on every touch drag.
  // -------------------------------------------------------------------------

  const pointerMoveRef = useRef<(event: PointerEvent) => void>(() => undefined);
  const pointerUpRef = useRef<(event: PointerEvent) => void>(() => undefined);

  /** Touch keeps receiving move events outside the element; the mouse does not
   * capture so double-click still targets the cell it happened on. */
  const captureGridPointer = (event: React.PointerEvent<HTMLDivElement>) => {
    if (event.pointerType === "mouse") return;
    try {
      event.currentTarget.setPointerCapture?.(event.pointerId);
    } catch {
      // Pointer capture is unavailable (older webviews, jsdom).
    }
  };

  const releaseGridPointer = (pointerId: number) => {
    try {
      gridRef.current?.releasePointerCapture?.(pointerId);
    } catch {
      // Not captured.
    }
  };

  /** Scrolls the grid when a selection drag reaches an edge. */
  const autoScrollGrid = (clientX: number, clientY: number) => {
    const grid = gridRef.current;
    if (!grid) return;
    const rect = grid.getBoundingClientRect();
    const margin = 32;
    const step = 24;
    if (clientX < rect.left + HEADER_WIDTH + margin) grid.scrollLeft = Math.max(0, grid.scrollLeft - step);
    else if (clientX > rect.right - margin) grid.scrollLeft += step;
    if (clientY < rect.top + ROW_HEIGHT + margin) grid.scrollTop = Math.max(0, grid.scrollTop - step);
    else if (clientY > rect.bottom - margin) grid.scrollTop += step;
  };

  /**
   * Repeats the fill source over the dragged range, shifting relative
   * references the way Ctrl+D does. The source block tiles, so a two-row
   * source repeats those two rows instead of duplicating the last one.
   */
  const fillFromHandle = (source: DragBounds, target: CellPosition) => {
    const height = source.end.row - source.start.row + 1;
    const width = source.end.col - source.start.col + 1;
    const entries: Array<{ row: number; col: number; cell: Cell }> = [];
    for (let row = source.start.row; row <= target.row; row += 1) {
      for (let col = source.start.col; col <= target.col; col += 1) {
        if (row <= source.end.row && col <= source.end.col) continue;
        const fromRow = source.start.row + ((row - source.start.row) % height);
        const fromCol = source.start.col + ((col - source.start.col) % width);
        const cell = sheet.cells[formatAddress(fromRow, fromCol)] ?? emptyCell();
        entries.push({
          row,
          col,
          cell: { ...cell, formula: shiftFormulaColumns(shiftFormulaRows(cell.formula, row - fromRow), col - fromCol) },
        });
      }
    }
    if (entries.length) setCells(entries);
    setSelection({ anchor: source.start, focus: target });
  };

  /** Ends a gesture and applies whatever it previewed. */
  const endGridGesture = (pointerId: number) => {
    const gesture = gestureRef.current;
    if (!gesture || gesture.pointerId !== pointerId) return;
    gestureRef.current = null;
    if (gesture.kind === "fill") fillFromHandle(gesture.source, gesture.target);
  };

  const handleGridPointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    gridRef.current?.focus({ preventScroll: true });
    pointersRef.current.set(event.pointerId, { x: event.clientX, y: event.clientY });
    if (pointersRef.current.size >= 2) {
      // A second finger turns the gesture into pinch zoom plus two-finger pan;
      // that is also how the sheet scrolls, because touch-action: none on the
      // grid keeps the browser from scrolling it natively.
      gestureRef.current = null;
      const [a, b] = [...pointersRef.current.values()];
      const grid = gridRef.current;
      const rect = grid?.getBoundingClientRect();
      const zoom = gridZoomRef.current;
      const localX = (a.x + b.x) / 2 - (rect?.left ?? 0);
      const localY = (a.y + b.y) / 2 - (rect?.top ?? 0);
      pinchRef.current = {
        startDistance: Math.max(1, Math.hypot(a.x - b.x, a.y - b.y)),
        startZoom: zoom,
        lastMidX: (a.x + b.x) / 2,
        lastMidY: (a.y + b.y) / 2,
        contentX: ((grid?.scrollLeft ?? 0) + localX) / zoom,
        contentY: ((grid?.scrollTop ?? 0) + localY) / zoom,
        localX,
        localY,
      };
      pinchFocalRef.current = null;
      return;
    }
    if (event.button !== 0) return;
    const target = event.target as HTMLElement;
    // Text inputs inside cells and the suggestion popup keep their own
    // hit-testing: capturing the pointer for the grid would steal the caret.
    if (target.closest?.("input, textarea, select, [contenteditable=true]")) return;
    const resize = target.closest?.<HTMLElement>("[data-col-resize]");
    if (resize) {
      // Checked before the column header because the grip lives inside it.
      const col = Number(resize.dataset.col);
      gestureRef.current = {
        kind: "resize",
        pointerId: event.pointerId,
        col,
        startX: event.clientX,
        startWidth: sheet.colWidths[String(col)] ?? DEFAULT_COL_WIDTH,
      };
      captureGridPointer(event);
      return;
    }
    const handle = target.closest?.<HTMLElement>("[data-fill-handle]");
    if (handle) {
      const bounds = selectionBounds;
      gestureRef.current = {
        kind: "fill",
        pointerId: event.pointerId,
        source: { start: { ...bounds.start }, end: { ...bounds.end } },
        target: { ...bounds.end },
      };
      captureGridPointer(event);
      return;
    }
    const selectHandle = target.closest?.<HTMLElement>("[data-select-handle]");
    if (selectHandle) {
      const bounds = selectionBounds;
      gestureRef.current = {
        kind: "extend",
        pointerId: event.pointerId,
        fixed: selectHandle.dataset.selectHandle === "start" ? { ...bounds.end } : { ...bounds.start },
      };
      captureGridPointer(event);
      return;
    }
    const cell = target.closest?.<HTMLElement>("[data-cell]");
    if (cell && event.pointerType === "touch") {
      const grid = gridRef.current;
      gestureRef.current = {
        kind: "tap",
        pointerId: event.pointerId,
        cell: { row: Number(cell.dataset.row), col: Number(cell.dataset.col) },
        startX: event.clientX,
        startY: event.clientY,
        scrollLeft: grid?.scrollLeft ?? 0,
        scrollTop: grid?.scrollTop ?? 0,
        moved: false,
      };
      captureGridPointer(event);
      return;
    }
    if (cell) {
      const position = { row: Number(cell.dataset.row), col: Number(cell.dataset.col) };
      gestureRef.current = {
        kind: "cells",
        pointerId: event.pointerId,
        anchor: position,
        startX: event.clientX,
        startY: event.clientY,
        moved: false,
      };
      setSelection(
        event.shiftKey ? { anchor: selection.anchor, focus: position } : { anchor: position, focus: position },
      );
      captureGridPointer(event);
      return;
    }
    const rowHeader = target.closest?.<HTMLElement>("[data-row-header]");
    if (rowHeader) {
      const row = Number(rowHeader.dataset.row);
      gestureRef.current = { kind: "rows", pointerId: event.pointerId, anchorRow: row };
      setSelection({ anchor: { row, col: 0 }, focus: { row, col: sheet.colCount - 1 } });
      captureGridPointer(event);
      return;
    }
    const colHeader = target.closest?.<HTMLElement>("[data-col-header]");
    if (colHeader) {
      const col = Number(colHeader.dataset.col);
      gestureRef.current = { kind: "cols", pointerId: event.pointerId, anchorCol: col };
      setSelection({ anchor: { row: 0, col }, focus: { row: sheet.rowCount - 1, col } });
      captureGridPointer(event);
      return;
    }
    // Empty canvas: one-finger pan for touch, drag-pan for the mouse.
    const grid = gridRef.current;
    if (grid) {
      gestureRef.current = {
        kind: "pan",
        pointerId: event.pointerId,
        startX: event.clientX,
        startY: event.clientY,
        scrollLeft: grid.scrollLeft,
        scrollTop: grid.scrollTop,
      };
      captureGridPointer(event);
    }
  };

  const handleWindowPointerMove = (event: PointerEvent) => {
    const points = pointersRef.current;
    if (!points.has(event.pointerId)) return;
    points.set(event.pointerId, { x: event.clientX, y: event.clientY });

    const pinch = pinchRef.current;
    if (pinch && points.size >= 2) {
      const [a, b] = [...points.values()];
      const midX = (a.x + b.x) / 2;
      const midY = (a.y + b.y) / 2;
      const grid = gridRef.current;
      const rect = grid?.getBoundingClientRect();
      const localX = midX - (rect?.left ?? 0);
      const localY = midY - (rect?.top ?? 0);
      const current = gridZoomRef.current;
      const nextZoom = pinchGridZoom(pinch.startZoom, pinch.startDistance, Math.hypot(a.x - b.x, a.y - b.y));
      if (nextZoom !== current) {
        // The zoom commits asynchronously; the focal point is applied in the
        // layout effect once the canvas has its new size.
        pinch.localX = localX;
        pinch.localY = localY;
        pinch.contentX = ((grid?.scrollLeft ?? 0) + localX) / current;
        pinch.contentY = ((grid?.scrollTop ?? 0) + localY) / current;
        pinchFocalRef.current = { ...pinch };
        setGridZoom(nextZoom);
      } else if (grid) {
        // Two-finger pan without a zoom change. Zoom changes pan through the
        // focal point instead, so this branch and the effect never both apply.
        // Panning is relative to the previous midpoint so it composes with a
        // zoom change in the middle of the same gesture.
        grid.scrollLeft = Math.max(0, grid.scrollLeft - (midX - pinch.lastMidX));
        grid.scrollTop = Math.max(0, grid.scrollTop - (midY - pinch.lastMidY));
      }
      pinch.lastMidX = midX;
      pinch.lastMidY = midY;
      return;
    }

    const gesture = gestureRef.current;
    if (!gesture || gesture.pointerId !== event.pointerId) return;
    if (gesture.kind === "pan") {
      const grid = gridRef.current;
      if (!grid) return;
      grid.scrollLeft = gesture.scrollLeft - (event.clientX - gesture.startX);
      grid.scrollTop = gesture.scrollTop - (event.clientY - gesture.startY);
      return;
    }
    if (gesture.kind === "resize") {
      const width = Math.max(32, gesture.startWidth + (event.clientX - gesture.startX) / gridZoomRef.current);
      updateSheet((current) => ({ ...current, colWidths: { ...current.colWidths, [gesture.col]: width } }));
      return;
    }
    if (gesture.kind === "tap") {
      const dx = event.clientX - gesture.startX;
      const dy = event.clientY - gesture.startY;
      if (!gesture.moved && Math.abs(dx) + Math.abs(dy) > TAP_SLOP) gesture.moved = true;
      const grid = gridRef.current;
      if (gesture.moved && grid) {
        grid.scrollLeft = Math.max(0, gesture.scrollLeft - dx);
        grid.scrollTop = Math.max(0, gesture.scrollTop - dy);
      }
      return;
    }
    autoScrollGrid(event.clientX, event.clientY);
    if (gesture.kind === "extend") {
      const cell = cellAtEvent(event);
      if (!cell) return;
      setSelection({
        anchor: gesture.fixed,
        focus: {
          row: Math.min(Math.max(0, cell.row), sheet.rowCount - 1),
          col: Math.min(Math.max(0, cell.col), sheet.colCount - 1),
        },
      });
      return;
    }
    if (gesture.kind === "cells") {
      const cell = cellAtEvent(event);
      if (!cell) return;
      if (Math.abs(event.clientX - gesture.startX) + Math.abs(event.clientY - gesture.startY) > 4) gesture.moved = true;
      setSelection({
        anchor: gesture.anchor,
        focus: {
          row: Math.min(Math.max(0, cell.row), sheet.rowCount - 1),
          col: Math.min(Math.max(0, cell.col), sheet.colCount - 1),
        },
      });
      return;
    }
    if (gesture.kind === "fill") {
      const cell = cellAtEvent(event);
      if (!cell) return;
      const end = {
        row: Math.max(gesture.source.end.row, Math.min(sheet.rowCount - 1, cell.row)),
        col: Math.max(gesture.source.end.col, Math.min(sheet.colCount - 1, cell.col)),
      };
      gesture.target = end;
      setSelection({ anchor: gesture.source.start, focus: end });
      return;
    }
    if (gesture.kind === "rows") {
      const header = attributeAtEvent(event, "data-row-header");
      if (!header) return;
      const row = Math.min(Math.max(0, Number(header.dataset.row)), sheet.rowCount - 1);
      setSelection({ anchor: { row: gesture.anchorRow, col: 0 }, focus: { row, col: sheet.colCount - 1 } });
      return;
    }
    if (gesture.kind === "cols") {
      const header = attributeAtEvent(event, "data-col-header");
      if (!header) return;
      const col = Math.min(Math.max(0, Number(header.dataset.col)), sheet.colCount - 1);
      setSelection({ anchor: { row: 0, col: gesture.anchorCol }, focus: { row: sheet.rowCount - 1, col } });
    }
  };

  const handleWindowPointerUp = (event: PointerEvent) => {
    const points = pointersRef.current;
    if (!points.has(event.pointerId)) return;
    points.delete(event.pointerId);
    releaseGridPointer(event.pointerId);

    if (pinchRef.current) {
      if (points.size < 2) {
        pinchRef.current = null;
        pinchFocalRef.current = null;
        // A finger remains: let it keep panning.
        const [remaining] = points.entries();
        const grid = gridRef.current;
        if (remaining && grid && event.pointerType !== "mouse") {
          gestureRef.current = {
            kind: "pan",
            pointerId: remaining[0],
            startX: remaining[1].x,
            startY: remaining[1].y,
            scrollLeft: grid.scrollLeft,
            scrollTop: grid.scrollTop,
          };
        }
      }
      return;
    }

    const gesture = gestureRef.current;
    if (!gesture || gesture.pointerId !== event.pointerId) return;
    endGridGesture(event.pointerId);
    const tapped =
      gesture.kind === "tap" && !gesture.moved
        ? gesture.cell
        : gesture.kind === "cells" && !gesture.moved && event.pointerType !== "mouse"
          ? gesture.anchor
          : null;
    if (gesture.kind === "tap" && tapped) setSelection({ anchor: tapped, focus: tapped });
    // Double-tap on a cell opens the inline editor: touch never produces a
    // native dblclick once the grid claims the gesture.
    if (tapped) {
      const now = event.timeStamp;
      const previous = lastTapRef.current;
      if (previous && now - previous.time < 350 && previous.row === tapped.row && previous.col === tapped.col) {
        lastTapRef.current = null;
        const cell = sheet.cells[formatAddress(tapped.row, tapped.col)];
        setEditing({ row: tapped.row, col: tapped.col, value: cell?.formula ?? cellText(cell) });
      } else {
        lastTapRef.current = { row: tapped.row, col: tapped.col, time: now };
      }
    }
  };

  useEffect(() => {
    pointerMoveRef.current = handleWindowPointerMove;
    pointerUpRef.current = handleWindowPointerUp;
  });

  useEffect(() => {
    const move = (event: PointerEvent) => pointerMoveRef.current(event);
    const up = (event: PointerEvent) => pointerUpRef.current(event);
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", up);
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", up);
    };
  }, []);

  // Keeps the sheet point that sat under the pinch midpoint under it after the
  // canvas has been laid out at the new zoom (CSS zoom resizes the scroll
  // extent, so adjusting the scroll before the commit would be clamped).
  useLayoutEffect(() => {
    const focal = pinchFocalRef.current;
    if (!focal || !pinchRef.current) return;
    const grid = gridRef.current;
    if (!grid) return;
    const zoom = gridZoomRef.current;
    grid.scrollLeft = focal.contentX * zoom - focal.localX;
    grid.scrollTop = focal.contentY * zoom - focal.localY;
  }, [gridZoom]);

  const applyStyle = (patch: Partial<CellStyle>) => {
    const addresses = addressesInRange(
      `${formatAddress(selection.anchor.row, selection.anchor.col)}:${formatAddress(selection.focus.row, selection.focus.col)}`,
    );
    const entries = addresses.map((address) => {
      const position = parseAddress(address)!;
      const cell = sheet.cells[address] ?? emptyCell();
      return { row: position.row, col: position.col, cell: { ...cell, style: { ...cell.style, ...patch } } };
    });
    setCells(entries);
  };

  const applyBorder = (side: "top" | "right" | "bottom" | "left" | "all") => {
    const addresses = addressesInRange(
      `${formatAddress(selection.anchor.row, selection.anchor.col)}:${formatAddress(selection.focus.row, selection.focus.col)}`,
    );
    const entries = addresses.map((address) => {
      const position = parseAddress(address)!;
      const cell = sheet.cells[address] ?? emptyCell();
      const borders = { ...cell.style.borders };
      const style = { style: "thin", color: "#334155" };
      if (side === "all") {
        borders.top = style;
        borders.right = side === "all" ? style : borders.right;
        borders.bottom = style;
        borders.left = style;
      } else {
        borders[side] = style;
      }
      return { row: position.row, col: position.col, cell: { ...cell, style: { ...cell.style, borders } } };
    });
    setCells(entries);
  };

  const clearSelection = () => {
    const addresses = addressesInRange(
      `${formatAddress(selection.anchor.row, selection.anchor.col)}:${formatAddress(selection.focus.row, selection.focus.col)}`,
    );
    updateSheet((current) => {
      const cells = { ...current.cells };
      for (const address of addresses) delete cells[address];
      return { ...current, cells };
    });
  };

  const clipboardRef = useRef<{ rows: Scalar[][]; start: CellPosition } | null>(null);

  const copySelection = () => {
    const parts = parseRange(
      `${formatAddress(selection.anchor.row, selection.anchor.col)}:${formatAddress(selection.focus.row, selection.focus.col)}`,
    );
    if (!parts) return;
    const rows: Scalar[][] = [];
    for (let row = parts.start.row; row <= parts.end.row; row += 1) {
      const line: Scalar[] = [];
      for (let col = parts.start.col; col <= parts.end.col; col += 1) {
        line.push(computed.get(formatAddress(row, col)) ?? "");
      }
      rows.push(line);
    }
    clipboardRef.current = { rows, start: { row: parts.start.row, col: parts.start.col } };
    const text = rows.map((line) => line.map((value) => String(value ?? "")).join("\t")).join("\n");
    void navigator.clipboard?.writeText(text).catch(() => undefined);
  };

  const pasteAtSelection = async () => {
    try {
      const text = await navigator.clipboard.readText();
      if (text) {
        const rows = text
          .split(/\r?\n/)
          .filter((line) => line.length > 0)
          .map((line) => line.split("\t"));
        applyPasted(rows);
        return;
      }
    } catch {
      // Clipboard read may be blocked; fall back to the in-app clipboard.
    }
    if (clipboardRef.current) {
      applyPasted(clipboardRef.current.rows.map((line) => line.map((value) => String(value ?? ""))));
    }
  };

  const applyPasted = (rows: string[][]) => {
    update((current) => applyCellEdits(current, sheetIndex, selection.focus, rows));
  };

  // -------------------------------------------------------------------------
  // Keyboard handling
  // -------------------------------------------------------------------------

  const handleKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    // Keys typed inside the inline editor belong to the editor only.
    if ((event.target as HTMLElement).closest("input, textarea, [contenteditable=true]")) return;
    // IME composition (dead keys, accented input) must never open the inline
    // editor or move the selection; the composition events own those keys.
    if (event.nativeEvent.isComposing) return;
    const { row, col } = selection.focus;
    const mod = event.ctrlKey || event.metaKey;
    const moveTo = (nextRow: number, dCol: number, extend = false) => {
      event.preventDefault();
      const next = revealCell({ row: nextRow, col: col + dCol });
      setSelection(extend ? { anchor: selection.anchor, focus: next } : { anchor: next, focus: next });
    };
    // Vertical steps count visible rows: a filter-hidden row is never landed on.
    const move = (dRow: number, dCol: number, extend = false) =>
      moveTo(dRow === 0 ? row : rowLayout.nextVisible(row, dRow), dCol, extend);
    // A page is one row short of the viewport, measured in pixels so custom
    // row heights count for what they are.
    const movePage = (direction: 1 | -1, extend: boolean) => {
      const span = Math.max(ROW_HEIGHT, scroll.height / gridZoom - HEADER_HEIGHT - ROW_HEIGHT);
      const target = rowLayout.rowAtY(rowLayout.offsetOf(row) + direction * span);
      moveTo(target === row ? rowLayout.nextVisible(row, direction) : target, 0, extend);
    };
    const openEditor = (value?: string) => {
      event.preventDefault();
      setEditing({ row, col, value: value ?? activeCell?.formula ?? cellText(activeCell) });
    };
    // Read through the ref, not the state: a keystroke that lands right after
    // `Enter` commits the cell is handled by the previous render's closure,
    // whose `editing` is still non-null. That stale check was what made
    // consecutive typing unreliable.
    if (editingRef.current) return;
    // Alt+Down opens the choices of a list-validated cell, as in Excel.
    if (event.altKey && event.key === "ArrowDown" && activeListItems.length > 0) {
      event.preventDefault();
      openListDropdown();
      return;
    }
    switch (event.key) {
      case "ArrowUp":
        move(-1, 0, event.shiftKey);
        break;
      case "ArrowDown":
        move(1, 0, event.shiftKey);
        break;
      case "ArrowLeft":
        move(0, -1, event.shiftKey);
        break;
      case "ArrowRight":
        move(0, 1, event.shiftKey);
        break;
      case "Home":
        event.preventDefault();
        if (mod) {
          const next = revealCell({ row: 0, col: 0 });
          setSelection({ anchor: next, focus: next });
        } else {
          const next = revealCell({ row, col: 0 });
          setSelection(event.shiftKey ? { anchor: selection.anchor, focus: next } : { anchor: next, focus: next });
        }
        break;
      case "End": {
        event.preventDefault();
        const next = mod ? revealCell({ row: lastRow, col: lastCol }) : revealCell({ row, col: lastCol });
        setSelection(event.shiftKey ? { anchor: selection.anchor, focus: next } : { anchor: next, focus: next });
        break;
      }
      case "PageDown":
        movePage(1, event.shiftKey);
        break;
      case "PageUp":
        movePage(-1, event.shiftKey);
        break;
      case "Tab":
        event.preventDefault();
        move(0, event.shiftKey ? -1 : 1);
        break;
      case "Enter":
        // Enter opens the cell editor; Shift+Enter and Ctrl+Enter commit the
        // current value and step to the previous / next row.
        if (mod || event.shiftKey) {
          event.preventDefault();
          commitEdit(event.shiftKey ? "up" : "down");
        } else {
          openEditor();
        }
        break;
      case "F2":
        openEditor();
        break;
      case "Delete":
      case "Backspace":
        event.preventDefault();
        clearSelection();
        break;
      case "Escape":
        setEditing(null);
        break;
      case " ":
        // Space selects the whole column, or the whole sheet when a full column
        // is already selected.
        event.preventDefault();
        if (row === 0 && selection.focus.row === lastRow) {
          setSelection({ anchor: { row: 0, col: 0 }, focus: { row: lastRow, col: lastCol } });
        } else {
          setSelection({ anchor: { row: 0, col }, focus: { row: lastRow, col } });
        }
        break;
      case "a":
      case "A":
        if (mod) {
          event.preventDefault();
          setSelection({ anchor: { row: 0, col: 0 }, focus: { row: lastRow, col: lastCol } });
        }
        break;
      case "d":
      case "D":
        if (mod) {
          event.preventDefault();
          fillFromSelection();
        }
        break;
      case "b":
      case "B":
        if (mod) {
          event.preventDefault();
          applyStyle({ bold: !activeCell?.style.bold });
        }
        break;
      case "i":
      case "I":
        if (mod) {
          event.preventDefault();
          applyStyle({ italic: !activeCell?.style.italic });
        }
        break;
      case "c":
        if (mod) {
          event.preventDefault();
          copySelection();
        }
        break;
      case "v":
        if (mod) {
          event.preventDefault();
          void pasteAtSelection();
        }
        break;
      case "x":
        if (mod) {
          event.preventDefault();
          copySelection();
          clearSelection();
        }
        break;
      case "z":
        if (mod) {
          event.preventDefault();
          if (event.shiftKey) redo();
          else undo();
        }
        break;
      case "y":
        if (mod) {
          event.preventDefault();
          redo();
        }
        break;
      default:
        if (!mod && !event.altKey && event.key.length === 1) {
          event.preventDefault();
          setEditing({ row, col, value: event.key });
        }
    }
  };

  const undo = () => {
    setUndoStack((stack) => {
      const previous = stack[stack.length - 1];
      if (!previous) return stack;
      setRedoStack((redos) => [...redos, workbook]);
      edit(tab.id, () => previous);
      return stack.slice(0, -1);
    });
  };

  const redo = () => {
    setRedoStack((stack) => {
      const next = stack[stack.length - 1];
      if (!next) return stack;
      setUndoStack((undos) => [...undos, workbook]);
      edit(tab.id, () => next);
      return stack.slice(0, -1);
    });
  };

  // -------------------------------------------------------------------------
  // Sheets
  // -------------------------------------------------------------------------

  const addSheet = () => {
    update((current) => {
      const name = uniqueSheetName(current, "Sheet");
      const sheets = [...current.sheets, newSheet(name)];
      return { ...current, sheets, activeSheet: sheets.length - 1 };
    });
    setSheetIndex(workbook.sheets.length);
  };

  const removeSheet = (index: number) => {
    if (workbook.sheets.length <= 1) return;
    const sheets = workbook.sheets.filter((_, position) => position !== index);
    update((current) => ({ ...current, sheets, activeSheet: Math.max(0, Math.min(index, sheets.length - 1)) }));
    setSheetIndex(Math.max(0, Math.min(index, sheets.length - 1)));
  };

  const renameSheet = (index: number) => {
    const name = window.prompt(t("calc.sheetName"), workbook.sheets[index]?.name ?? "");
    if (!name) return;
    update((current) => ({
      ...current,
      sheets: current.sheets.map((candidate, position) => (position === index ? { ...candidate, name } : candidate)),
    }));
  };

  // -------------------------------------------------------------------------
  // Sort / filter / conditional / validation
  // -------------------------------------------------------------------------

  const sortByColumn = (column: number, ascending: boolean) => {
    const parts = parseRange(sheet.filter?.range ?? usedRange(sheet));
    if (!parts) return;
    const header = sheet.cells[formatAddress(parts.start.row, column)] !== undefined && parts.start.row === 0;
    const startRow = header ? parts.start.row + 1 : parts.start.row;
    const rows: Array<{ values: Array<{ col: number; cell: Cell | undefined }>; key: Scalar }> = [];
    for (let row = startRow; row <= parts.end.row; row += 1) {
      const values = [];
      for (let col = parts.start.col; col <= parts.end.col; col += 1) {
        values.push({ col, cell: sheet.cells[formatAddress(row, col)] });
      }
      rows.push({ values, key: computed.get(formatAddress(row, column)) ?? "" });
    }
    rows.sort((a, b) => {
      const left = a.key;
      const right = b.key;
      const leftNumber = typeof left === "number" ? left : Number(left);
      const rightNumber = typeof right === "number" ? right : Number(right);
      if (Number.isFinite(leftNumber) && Number.isFinite(rightNumber))
        return ascending ? leftNumber - rightNumber : rightNumber - leftNumber;
      const compare = String(left).localeCompare(String(right));
      return ascending ? compare : -compare;
    });
    updateSheet((current) => {
      const cells = { ...current.cells };
      for (const address of addressesInRange(
        `${formatAddress(startRow, parts.start.col)}:${formatAddress(parts.end.row, parts.end.col)}`,
      ))
        delete cells[address];
      rows.forEach((row, rowOffset) => {
        row.values.forEach((value) => {
          if (value.cell) cells[formatAddress(startRow + rowOffset, value.col)] = value.cell;
        });
      });
      return { ...current, cells };
    });
  };

  const openFilter = (col: number) => {
    const draft = sheetFilterDraft(sheet, computed, col);
    if (draft) setFilterOpen(draft);
  };

  /** Opens the filter dialog scoped to one structured table column. */
  const openTableFilter = (table: SpreadsheetTable, columnName: string) => {
    const draft = tableFilterDraft(table, computed, columnName);
    if (draft) setFilterOpen(draft);
  };

  const applyFilter = () => {
    if (!filterOpen) return;
    const draft = filterOpen;
    updateSheet((current) => applyFilterDraft(current, draft, computed));
    setFilterOpen(null);
  };

  const addChart = (kind: string) => {
    const parts = parseRange(
      `${formatAddress(selection.anchor.row, selection.anchor.col)}:${formatAddress(selection.focus.row, selection.focus.col)}`,
    );
    if (!parts || parts.start.row === parts.end.row) {
      useToasts.getState().push({ kind: "info", title: t("calc.chartNeedsData") });
      return;
    }
    const categories = `${formatAddress(parts.start.row + 1, parts.start.col)}:${formatAddress(parts.end.row, parts.start.col)}`;
    const series: ChartData["series"] = [];
    for (let col = parts.start.col + 1; col <= parts.end.col; col += 1) {
      series.push({
        name: String(computed.get(formatAddress(parts.start.row, col)) ?? `Series ${col}`),
        range: `${formatAddress(parts.start.row + 1, col)}:${formatAddress(parts.end.row, col)}`,
        color: null,
      });
    }
    const chart: ChartData = {
      kind,
      title: t("calc.chartTitle"),
      categories,
      series,
      legend: true,
      xTitle: "",
      yTitle: "",
      stacked: false,
      showLabels: false,
    };
    const anchor = formatAddress(parts.end.row + 2, parts.start.col);
    updateSheet((current) => ({
      ...current,
      charts: [...current.charts, { id: crypto.randomUUID(), chart, anchor, widthPx: 420, heightPx: 260 }],
    }));
    setChartDialog(false);
  };

  /** Adds a pivot over the current selection, anchored below it. */
  const addPivot = (config: {
    rows: string[];
    columns: string[];
    values: PivotValueField[];
    filters: PivotTable["filters"];
  }) => {
    const parts = parseRange(usedRange(sheet));
    if (!parts) {
      useToasts.getState().push({ kind: "info", title: t("calc.pivotNeedsData") });
      return;
    }
    const source = usedRange(sheet);
    const anchor = formatAddress(parts.end.row + 2, parts.start.col);
    const pivot: PivotTable = {
      id: crypto.randomUUID(),
      name: `Pivot${(sheet.pivotTables?.length ?? 0) + 1}`,
      sourceSheet: sheet.name,
      source,
      rows: config.rows,
      columns: config.columns,
      values: config.values,
      filters: config.filters,
      anchor,
    };
    updateSheet((current) => ({ ...current, pivotTables: [...(current.pivotTables ?? []), pivot] }));
    setPivotDialog(false);
  };

  const addConditional = (rule: { kind: string; values: string[]; fill: string; topN?: number }) => {
    const range = usedRange(sheet);
    updateSheet((current) => ({
      ...current,
      conditional: [
        ...current.conditional,
        {
          id: crypto.randomUUID(),
          range,
          kind: rule.kind,
          values: rule.values,
          fill: rule.fill,
          color: null,
          topN: rule.topN ?? null,
          stopIfTrue: false,
        },
      ],
    }));
    setConditionalDialog(false);
  };

  const addValidation = (validation: {
    kind: string;
    values: string[];
    min: number | null;
    max: number | null;
    message: string;
  }) => {
    const range = `${formatAddress(selection.anchor.row, selection.anchor.col)}:${formatAddress(selection.focus.row, selection.focus.col)}`;
    updateSheet((current) => ({
      ...current,
      validations: [
        ...current.validations,
        {
          id: crypto.randomUUID(),
          range,
          kind: validation.kind,
          values: validation.values,
          min: validation.min,
          max: validation.max,
          message: validation.message,
          allowBlank: true,
        },
      ],
    }));
    setValidationDialog(false);
  };

  const toggleFreeze = () => {
    const focus = selection.focus;
    updateSheet((current) => {
      const next = nextFreeze({ rows: current.freezeRows, cols: current.freezeCols }, focus);
      return { ...current, freezeRows: next.rows, freezeCols: next.cols };
    });
  };

  // -------------------------------------------------------------------------
  // Data tools: text to columns, remove duplicates, list dropdown
  // -------------------------------------------------------------------------

  /**
   * The selection clipped to the last row that holds a cell in its columns,
   * so a whole-column selection does not walk the empty rows below the data.
   */
  const dataBounds = () => {
    const { start, end } = selectionBounds;
    let lastDataRow = start.row;
    for (const address of Object.keys(sheet.cells)) {
      const position = parseAddress(address);
      if (position && position.col >= start.col && position.col <= end.col && position.row <= end.row) {
        lastDataRow = Math.max(lastDataRow, position.row);
      }
    }
    return { start, end: { row: lastDataRow, col: end.col } };
  };

  const openTextToColumns = () => {
    if (selectionBounds.start.col !== selectionBounds.end.col) {
      useToasts.getState().push({ kind: "info", title: t("calc.textToColumnsOneColumn") });
      return;
    }
    setTextToColumnsDialog(true);
  };

  /** The displayed values of the selected column, which is what gets split. */
  const textToColumnsTexts = () => {
    const { start, end } = dataBounds();
    const texts: string[] = [];
    for (let row = start.row; row <= end.row; row += 1) {
      texts.push(toText(computed.get(formatAddress(row, start.col)) ?? ""));
    }
    return texts;
  };

  /** True when the cell at an offset from the selection start holds a value or formula. */
  const holdsData = (rowOffset: number, colOffset: number) => {
    const cell =
      sheet.cells[formatAddress(selectionBounds.start.row + rowOffset, selectionBounds.start.col + colOffset)];
    return cell !== undefined && (cell.formula !== null || cell.value.kind !== "empty");
  };

  /**
   * Writes a split into the source column and the columns to its right as
   * one undo step. Pieces go through the same parser as typed input, so
   * "12" and "1,5" become numbers; the target cells keep their formatting.
   */
  const applyTextToColumns = (plan: ColumnSplitPlan) => {
    const { start, end } = dataBounds();
    updateSheet((current) => {
      const cells = { ...current.cells };
      plan.rows.forEach((pieces, rowOffset) => {
        pieces?.forEach((piece, colOffset) => {
          const address = formatAddress(start.row + rowOffset, start.col + colOffset);
          const cell: Cell = {
            ...(current.cells[address] ?? emptyCell()),
            formula: null,
            value: parseInputValue(piece),
          };
          if (isBlankCell(cell)) delete cells[address];
          else cells[address] = cell;
        });
      });
      return { ...current, cells, colCount: Math.max(current.colCount, start.col + plan.width) };
    });
    setSelection({ anchor: start, focus: { row: end.row, col: start.col + plan.width - 1 } });
    setTextToColumnsDialog(false);
  };

  const openRemoveDuplicates = () => {
    const { start, end } = dataBounds();
    if (end.row <= start.row) {
      useToasts.getState().push({ kind: "info", title: t("calc.removeDuplicatesNeedsRange") });
      return;
    }
    setDuplicatesDialog(true);
  };

  const openAiSummarize = () => {
    const push = useToasts.getState().push;
    const col = selection.focus.col;
    const last = lastRowOfColumn(sheet, col);
    const { start, end } = selectionBounds;
    const inColumn = start.col === end.col && end.row > start.row;
    const lines = columnValueLines(computed, col, inColumn ? start.row : 0, inColumn ? Math.min(end.row, last) : last);
    if (lines.length === 0) {
      push({ kind: "info", title: t("ai.edit.columnEmpty", { column: columnLabel(col) }) });
      return;
    }
    if (lines.join("\n").length > AI_EDIT_MAX_CHARS) {
      push({ kind: "error", title: t("ai.edit.tooLong", { max: AI_EDIT_MAX_CHARS }) });
      return;
    }
    setAiDialog({ kind: "summarize", sheetIndex, row: last + 1, col, lines });
  };

  const openAiFormula = () => {
    const { anchor, focus } = selection;
    const single = anchor.row === focus.row && anchor.col === focus.col;
    const focusAddress = formatAddress(focus.row, focus.col);
    setAiDialog({
      kind: "formula",
      sheetIndex,
      row: focus.row,
      col: focus.col,
      headers: headerContext(computed, sheet.colCount),
      selection: single ? focusAddress : `${formatAddress(anchor.row, anchor.col)}:${focusAddress}`,
    });
  };

  /** One undo step: the AI text goes into one cell. */
  const acceptAiCell = (text: string, message?: string) => {
    const dialog = aiDialog;
    if (!dialog) return;
    setAiDialog(null);
    update((current) => applyCellEdit(current, dialog.sheetIndex, dialog.row, dialog.col, text));
    if (message) useToasts.getState().push({ kind: "success", title: message });
  };

  /** Column letters and first-row values of the selection, for the dialog. */
  const duplicateColumns = () => {
    const { start, end } = selectionBounds;
    const columns: Array<{ letter: string; header: string }> = [];
    for (let col = start.col; col <= end.col; col += 1) {
      columns.push({ letter: columnLabel(col), header: toText(computed.get(formatAddress(start.row, col)) ?? "") });
    }
    return columns;
  };

  /**
   * Removes the rows of the selection that repeat an earlier row, moves the
   * remaining rows up and clears the vacated rows at the bottom, all inside
   * the selected columns and as one undo step. A moved formula's references
   * into the block follow the moved cells; references outside it stay put.
   */
  const removeDuplicates = (options: DuplicateOptions) => {
    setDuplicatesDialog(false);
    const { start, end } = dataBounds();
    const rows: Scalar[][] = [];
    for (let row = start.row; row <= end.row; row += 1) {
      const line: Scalar[] = [];
      for (let col = start.col; col <= end.col; col += 1) line.push(computed.get(formatAddress(row, col)) ?? "");
      rows.push(line);
    }
    const result = findDuplicateRows(rows, options);
    if (result.removed.length === 0) {
      useToasts.getState().push({ kind: "info", title: t("calc.duplicatesNone") });
      return;
    }
    const block = { top: start.row, bottom: end.row, left: start.col, right: end.col };
    const rowMap = new Map(result.keep.map((fromOffset, toOffset) => [start.row + fromOffset, start.row + toOffset]));
    updateSheet((current) => {
      const cells = { ...current.cells };
      for (let row = start.row; row <= end.row; row += 1) {
        for (let col = start.col; col <= end.col; col += 1) delete cells[formatAddress(row, col)];
      }
      result.keep.forEach((fromOffset, toOffset) => {
        for (let col = start.col; col <= end.col; col += 1) {
          const cell = current.cells[formatAddress(start.row + fromOffset, col)];
          if (!cell) continue;
          cells[formatAddress(start.row + toOffset, col)] = {
            ...cell,
            formula: remapMovedRows(cell.formula, block, rowMap),
          };
        }
      });
      return { ...current, cells };
    });
    // The header row is not a unique data row.
    const kept = result.keep.length - (options.hasHeaders ? 1 : 0);
    useToasts.getState().push({
      kind: "success",
      title: t("calc.duplicatesRemoved", { removed: result.removed.length, kept }),
    });
  };

  const openListDropdown = () => {
    // Start on the cell's current value when it is one of the choices.
    const current = toText(computed.get(selectionAddress) ?? "")
      .trim()
      .toLowerCase();
    const index = activeListItems.findIndex((item) => item.toLowerCase() === current);
    setListDropdown({ index: Math.max(0, index) });
  };

  /** Closes the choice list and gives the keyboard back to the grid. */
  const closeListDropdown = () => {
    setListDropdown(null);
    gridRef.current?.focus({ preventScroll: true });
  };

  /** Writes a chosen list value into the active cell, like typing it. */
  const chooseListValue = (value: string) => {
    const { row, col } = selection.focus;
    closeListDropdown();
    update((current) => applyCellEdit(current, sheetIndex, row, col, value));
  };

  const handleListDropdownKey = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (!listDropdown) return;
    const last = activeListItems.length - 1;
    const index = Math.min(listDropdown.index, last);
    const keys: Record<string, () => void> = {
      ArrowDown: () => setListDropdown({ index: Math.min(last, index + 1) }),
      ArrowUp: () => (event.altKey ? closeListDropdown() : setListDropdown({ index: Math.max(0, index - 1) })),
      Home: () => setListDropdown({ index: 0 }),
      End: () => setListDropdown({ index: last }),
      Enter: () => chooseListValue(activeListItems[index]),
      Escape: closeListDropdown,
      Tab: closeListDropdown,
    };
    const action = keys[event.key];
    if (!action) return;
    // Handled keys stay out of the grid, which would otherwise move the
    // selection or start typing into the cell.
    event.preventDefault();
    event.stopPropagation();
    action();
  };

  // -------------------------------------------------------------------------
  // Structured tables (V3)
  // -------------------------------------------------------------------------

  /** Creates a table over the dialog's range, naming columns from the header row. */
  const addTable = (config: {
    name: string;
    range: string;
    hasHeaders: boolean;
    hasTotals: boolean;
    bandedRows: boolean;
  }) => {
    const parts = parseRange(config.range);
    if (!parts) return;
    const columns: string[] = [];
    for (let col = parts.start.col; col <= parts.end.col; col += 1) {
      const header = config.hasHeaders ? String(computed.get(formatAddress(parts.start.row, col)) ?? "").trim() : "";
      columns.push(uniqueColumnName(columns, header || `Column${col - parts.start.col + 1}`));
    }
    const table = newSpreadsheetTable(
      uniqueTableName(sheet.tables ?? [], config.name.trim() || "Table1"),
      config.range,
      columns,
    );
    table.hasHeaders = config.hasHeaders;
    table.hasTotals = config.hasTotals;
    table.bandedRows = config.bandedRows;
    updateSheet((current) => ({ ...current, tables: [...(current.tables ?? []), table] }));
    setTableDialog(false);
    setTablesPanel(true);
  };

  const patchTable = (tableId: string, patch: (table: SpreadsheetTable) => SpreadsheetTable) => {
    updateSheet((current) => ({
      ...current,
      tables: (current.tables ?? []).map((table) => (table.id === tableId ? patch(table) : table)),
    }));
  };

  const deleteTable = (tableId: string) => {
    updateSheet((current) => ({ ...current, tables: (current.tables ?? []).filter((table) => table.id !== tableId) }));
  };

  const renameTable = (table: SpreadsheetTable) => {
    const name = window.prompt(t("calc.tableName"), table.name);
    if (!name || name.trim() === "") return;
    patchTable(table.id, (current) => ({
      ...current,
      name: uniqueTableName(
        (sheet.tables ?? []).filter((candidate) => candidate.id !== table.id),
        name.trim(),
      ),
    }));
  };

  const jumpToTable = (table: SpreadsheetTable) => {
    const parts = parseRange(table.range);
    if (!parts) return;
    revealCell(parts.start);
    setSelection({ anchor: parts.start, focus: parts.end });
  };

  /**
   * Appends a calculated column: the header goes into the header row, the
   * row-shifted formula into every body cell, and the unshifted formula into
   * `column.formula` so the table remembers what the column computes.
   */
  const addCalculatedColumn = (table: SpreadsheetTable, columnName: string, formula: string) => {
    const name = columnName.trim();
    const source = formula.trim();
    if (name === "" || source === "") return;
    const bodyFormula = source.startsWith("=") ? source : `=${source}`;
    update((current) => {
      const index = Math.min(Math.max(0, sheetIndex), current.sheets.length - 1);
      const target = current.sheets[index];
      const table2 = (target.tables ?? []).find((candidate) => candidate.id === table.id);
      const parts = table2 ? parseRange(table2.range) : null;
      if (!table2 || !parts) return current;
      const column = parts.end.col + 1;
      const resolved = uniqueColumnName(
        table2.columns.map((entry) => entry.name),
        name,
      );
      let next: Workbook = {
        ...current,
        sheets: current.sheets.map((candidate, at) =>
          at === index
            ? {
                ...candidate,
                tables: (candidate.tables ?? []).map((entry) =>
                  entry.id === table2.id
                    ? {
                        ...entry,
                        range: `${formatAddress(parts.start.row, parts.start.col)}:${formatAddress(parts.end.row, column)}`,
                        columns: [...entry.columns, { name: resolved, formula: bodyFormula }],
                      }
                    : entry,
                ),
              }
            : candidate,
        ),
      };
      const bodyStart = parts.start.row + (table2.hasHeaders ? 1 : 0);
      const bodyEnd = parts.end.row - (table2.hasTotals ? 1 : 0);
      for (let row = bodyStart; row <= bodyEnd; row += 1) {
        next = applyCellEdit(next, index, row, column, shiftFormulaRows(bodyFormula, row - bodyStart) ?? bodyFormula);
      }
      if (table2.hasHeaders) next = applyCellEdit(next, index, parts.start.row, column, resolved);
      return next;
    });
  };

  // -------------------------------------------------------------------------
  // Formula auditing (V3)
  // -------------------------------------------------------------------------

  const traceFromSelection = (kind: "precedents" | "dependents") => {
    const address = formatAddress(selection.focus.row, selection.focus.col);
    const cells =
      kind === "precedents"
        ? tracePrecedents(workbook, sheet.name, address)
        : traceDependents(workbook, sheet.name, address);
    if (cells.length === 0) {
      setTrace(null);
      useToasts.getState().push({ kind: "info", title: t("calc.traceEmpty") });
      return;
    }
    setTrace({ kind, cells });
  };

  // -------------------------------------------------------------------------
  // Rendering
  // -------------------------------------------------------------------------

  const totalWidth = useMemo(() => {
    let total = 0;
    for (let col = 0; col < sheet.colCount; col += 1) total += sheet.colWidths[String(col)] ?? DEFAULT_COL_WIDTH;
    return total;
  }, [sheet]);

  const visible = useMemo(() => {
    // The grid reports scroll offsets in outer pixels, while the canvas is
    // laid out in canvas coordinates scaled by the pinch zoom; the window of
    // cell indices comes from the unscaled numbers.
    const viewTop = scroll.top / gridZoom;
    const viewLeft = scroll.left / gridZoom;
    const viewWidth = scroll.width / gridZoom;
    const viewHeight = scroll.height / gridZoom;
    // Hidden rows are left out and the window follows the real row heights;
    // the cells start one header band below the top of the canvas.
    const windowRows = rowLayout.visibleRows(viewTop - HEADER_HEIGHT, viewTop + viewHeight - HEADER_HEIGHT);
    // The frozen rows stay on screen however far the window has scrolled.
    const rows = mergeFrozen(freeze.rows, windowRows, (row) => rowLayout.isHidden(row));
    const windowColumns: Array<{ col: number; x: number }> = [];
    const frozenColumns: Array<{ col: number; x: number }> = [];
    let x = 0;
    let startCol = 0;
    for (let col = 0; col < sheet.colCount; col += 1) {
      const width = sheet.colWidths[String(col)] ?? DEFAULT_COL_WIDTH;
      if (col < freeze.cols) frozenColumns.push({ col, x });
      if (x + width < viewLeft) {
        x += width;
        startCol = col + 1;
        continue;
      }
      if (x > viewLeft + viewWidth + 200) {
        if (col >= freeze.cols) break;
        x += width;
        continue;
      }
      windowColumns.push({ col, x });
      x += width;
    }
    const columns =
      freeze.cols > 0 ? [...frozenColumns, ...windowColumns.filter(({ col }) => col >= freeze.cols)] : windowColumns;
    return { rows, columns, startCol };
  }, [scroll, sheet, gridZoom, rowLayout, freeze]);

  const parsedSelectionBounds = parseRange(
    `${formatAddress(selection.anchor.row, selection.anchor.col)}:${formatAddress(selection.focus.row, selection.focus.col)}`,
  );
  const selectionBounds = parsedSelectionBounds ?? {
    start: { row: selection.anchor.row, col: selection.anchor.col },
    end: { row: selection.focus.row, col: selection.focus.col },
  };

  // Structured tables and audit overlays are derived once per sheet/selection
  // change; the per-cell render only looks up whether a cell participates.
  const sheetTables = useMemo(() => {
    const out: Array<{ table: SpreadsheetTable; parts: { start: CellPosition; end: CellPosition } }> = [];
    for (const table of sheet.tables ?? []) {
      const parts = parseRange(table.range);
      if (parts) out.push({ table, parts });
    }
    return out;
  }, [sheet]);

  const tracedCells = useMemo(() => {
    const cells = new Map<string, "precedents" | "dependents">();
    if (!trace) return cells;
    for (const node of trace.cells) {
      if (node.sheet === sheet.name) cells.set(node.address, trace.kind);
    }
    return cells;
  }, [trace, sheet.name]);

  // The selected cell decides the audit banner: a cycle path for a circular
  // #REF!, the offending reference otherwise.
  const selectedError = useMemo(() => {
    const address = formatAddress(selection.focus.row, selection.focus.col);
    const value = computed.get(address);
    if (!isError(value) || value.code !== "#REF!") return null;
    const key = `${sheet.name}!${address}`;
    const cycle = findCircularReferences(workbook).find((candidate) => candidate.includes(key));
    if (cycle) return `${t("calc.circularReference")}: ${cycle.join(" → ")}`;
    const issue = invalidReferences(workbook).find((entry) => entry.sheet === sheet.name && entry.address === address);
    return issue ? `${t("calc.invalidReference")}: ${issue.reference}` : t("calc.invalidReference");
  }, [computed, selection.focus, sheet.name, workbook, t]);

  // Conditional formatting is evaluated once per sheet/data change instead of
  // per visible cell (see calc/rules.ts).
  const conditionalFills = useMemo(() => computeConditionalFills(sheet.conditional, computed), [sheet, computed]);
  const dataBars = useMemo(() => computeDataBars(sheet.conditional, computed), [sheet, computed]);

  // Rule ranges are parsed once per rule list, so the per-cell lookup below is a
  // few integer comparisons however large the validated range is.
  const validationAt = validationLookup(sheet.validations);

  // The choices of every list validation, resolved once per data change: the
  // inline list as stored, or the values of a referenced range (`=A1:A5`).
  const listItems = useMemo(() => {
    const resolve = (sheetName: string | null, range: string): Scalar[] | null => {
      const target =
        sheetName === null
          ? sheet
          : workbook.sheets.find((candidate) => candidate.name.toLowerCase() === sheetName.toLowerCase());
      if (!target) return null;
      const values = target === sheet ? computed : computeSheetValues(workbook, target);
      return addressesInRange(range, 1000).map((address) => values.get(address) ?? "");
    };
    const items = new Map<string, string[]>();
    for (const rule of sheet.validations) {
      if (rule.kind === "list") items.set(rule.id, listValidationItems(rule.values, resolve));
    }
    return items;
  }, [sheet, workbook, computed]);

  /** Column offset inside the canvas; the browser scroll and the canvas zoom
   * already move it on screen. */
  const columnX = (col: number) => {
    let x = 0;
    for (let index = 0; index < col; index += 1) x += sheet.colWidths[String(index)] ?? DEFAULT_COL_WIDTH;
    return x + HEADER_WIDTH;
  };

  // The touch-only fill handle sits on the selection's bottom-right corner.
  const fillHandle = {
    x: columnX(selectionBounds.end.col) + (sheet.colWidths[String(selectionBounds.end.col)] ?? DEFAULT_COL_WIDTH),
    y: HEADER_HEIGHT + rowLayout.offsetOf(selectionBounds.end.row) + rowLayout.heightOf(selectionBounds.end.row),
  };
  // Touch selection handles: round grips on the top-left and bottom-right
  // corners (centred on the corner by CSS) that drag the selection's extent.
  const selectHandleStart = {
    x: columnX(selectionBounds.start.col),
    y: HEADER_HEIGHT + rowLayout.offsetOf(selectionBounds.start.row),
  };
  // A handle on a frozen row or column is pinned like the cell it belongs to;
  // one whose cell has scrolled beneath the frozen band goes away with it.
  const placeHandle = (point: { x: number; y: number }, row: number, col: number) => ({
    left: pinnedPosition(col, freeze.cols, point.x, scrollX),
    top: pinnedPosition(row, freeze.rows, point.y, scrollY),
    hidden:
      isUnderBand(row, freeze.rows, point.y - HEADER_HEIGHT, scrollY, freeze.height) ||
      isUnderBand(col, freeze.cols, point.x - HEADER_WIDTH, scrollX, freeze.width),
  });
  const fillHandlePlace = placeHandle(fillHandle, selectionBounds.end.row, selectionBounds.end.col);
  const selectHandleStartPlace = placeHandle(selectHandleStart, selectionBounds.start.row, selectionBounds.start.col);

  const selectionAddress = formatAddress(selection.focus.row, selection.focus.col);
  const selectedTable = (sheet.tables ?? []).find((table) => addressInRange(selectionAddress, table.range));
  const nameBox = selectedTable
    ? selectedTable.name
    : `${selectionAddress}${selection.anchor.row !== selection.focus.row || selection.anchor.col !== selection.focus.col ? `:${formatAddress(selection.anchor.row, selection.anchor.col)}` : ""}`;

  // The choices offered on the active cell: the first list validation whose
  // range (one area, or several separated by spaces as XLSX writes them)
  // covers it.
  const activeListRule = sheet.validations.find(
    (rule) => rule.kind === "list" && rule.range.split(/\s+/).some((area) => addressInRange(selectionAddress, area)),
  );
  const activeListItems = activeListRule ? (listItems.get(activeListRule.id) ?? []) : [];

  // The choice list belongs to one cell: moving the selection, switching
  // sheets or starting an edit closes it.
  const listDropdownKey = `${sheetIndex}:${selectionAddress}:${editing ? "editing" : ""}`;
  const [lastListDropdownKey, setLastListDropdownKey] = useState(listDropdownKey);
  if (lastListDropdownKey !== listDropdownKey) {
    setLastListDropdownKey(listDropdownKey);
    setListDropdown(null);
  }
  const listDropdownOpen = listDropdown !== null && activeListItems.length > 0;
  const listDropdownIndex = Math.min(listDropdown?.index ?? 0, Math.max(0, activeListItems.length - 1));

  // The arrow sits inside the right edge of the active cell, in canvas
  // coordinates like the fill handle. The list opens below the cell, or above
  // it when the viewport has no room left underneath.
  const listCellHeight = rowLayout.heightOf(selection.focus.row);
  const listCell =
    activeListItems.length > 0 &&
    !editing &&
    listCellHeight > 0 &&
    !isUnderBand(selection.focus.row, freeze.rows, rowLayout.offsetOf(selection.focus.row), scrollY, freeze.height) &&
    !isUnderBand(selection.focus.col, freeze.cols, columnX(selection.focus.col) - HEADER_WIDTH, scrollX, freeze.width)
      ? (() => {
          const left = pinnedPosition(selection.focus.col, freeze.cols, columnX(selection.focus.col), scrollX);
          const width = sheet.colWidths[String(selection.focus.col)] ?? DEFAULT_COL_WIDTH;
          const top = pinnedPosition(
            selection.focus.row,
            freeze.rows,
            HEADER_HEIGHT + rowLayout.offsetOf(selection.focus.row),
            scrollY,
          );
          const popupHeight = Math.min(activeListItems.length, 8) * (android ? 40 : 26) + 8;
          const viewTop = scroll.top / gridZoom + HEADER_HEIGHT;
          const viewBottom = (scroll.top + scroll.height) / gridZoom;
          const above = top + listCellHeight + popupHeight > viewBottom && top - popupHeight >= viewTop;
          return { left, width, top, height: listCellHeight, above };
        })()
      : null;

  // Opening the list moves the keyboard into it; the highlighted choice stays
  // in view while the arrow keys move it.
  useLayoutEffect(() => {
    if (listDropdownOpen) listDropdownRef.current?.focus({ preventScroll: true });
  }, [listDropdownOpen]);
  useLayoutEffect(() => {
    if (!listDropdownOpen) return;
    listDropdownRef.current
      ?.querySelector<HTMLElement>('[aria-selected="true"]')
      ?.scrollIntoView?.({ block: "nearest" });
  }, [listDropdownOpen, listDropdownIndex]);

  // The last valid row/column, used for Ctrl+End, Space and the data extent.
  const lastRow = Math.max(0, sheet.rowCount - 1);
  const lastCol = Math.max(0, sheet.colCount - 1);

  /** Ctrl+D: copy the first row of the selection down the rest of it. */
  const fillFromSelection = () => {
    const bounds = selectionBounds;
    if (bounds.start.row === bounds.end.row && bounds.start.col === bounds.end.col) return;
    const source: Cell[] = [];
    for (let col = bounds.start.col; col <= bounds.end.col; col += 1) {
      source.push(sheet.cells[formatAddress(bounds.start.row, col)] ?? emptyCell());
    }
    const entries: Array<{ row: number; col: number; cell: Cell }> = [];
    for (let row = bounds.start.row + 1; row <= bounds.end.row; row += 1) {
      source.forEach((cell, offset) => {
        const col = bounds.start.col + offset;
        entries.push({ row, col, cell: { ...cell, formula: shiftFormulaRows(cell.formula, row - bounds.start.row) } });
      });
    }
    if (entries.length === 0) return;
    setCells(entries);
  };

  /** Commits the docked bar's draft; the Android twin of pressing Enter. */
  const commitFormulaBar = () => {
    if (!editingRef.current) setEditing({ row: selection.focus.row, col: selection.focus.col, value: formulaDraft });
    commitEdit("none", false);
  };

  /** Reverts the docked bar; the Android twin of pressing Escape. */
  const cancelFormulaBar = () => {
    setEditing(null);
    setFormulaDraft(activeCell?.formula ?? cellText(activeCell));
    restoreGridFocusRef.current = false;
  };

  // On phones the bar is docked above the keyboard at the bottom of the
  // editor; on desktop it keeps its place under the ribbon, byte for byte.
  const formulaBar = (
    <div className={`calc-formula-bar${android ? " is-docked" : ""}`}>
      <input
        className="name-box"
        value={nameBox}
        onChange={(event) => {
          const parts = parseRange(event.target.value.trim());
          if (parts) setSelection({ anchor: parts.start, focus: parts.end });
        }}
      />
      <span className="fx">fx</span>
      <input
        className="formula-input"
        ref={formulaInputRef}
        value={editing ? editing.value : formulaDraft}
        placeholder={t("calc.formulaHint")}
        onFocus={(event) => {
          setFocusMode("bar");
          setSuggestDismissed(false);
          setDraftCaret(event.currentTarget.selectionStart ?? event.currentTarget.value.length);
        }}
        onSelect={(event) => setDraftCaret(event.currentTarget.selectionStart ?? event.currentTarget.value.length)}
        onBlur={() => setFocusMode(null)}
        onChange={(event) => {
          setFormulaDraft(event.target.value);
          setDraftCaret(event.target.selectionStart ?? event.target.value.length);
          setSuggestDismissed(false);
          // Read through the ref so the draft and the open editor can never
          // disagree about the current text.
          const current = editingRef.current;
          if (current) setEditing({ row: current.row, col: current.col, value: event.target.value });
        }}
        onKeyDown={(event) => {
          if (event.nativeEvent.isComposing) return;
          // The popup owns Tab/Enter/Escape/arrows while it is open.
          if (handleAssistKey(event)) return;
          if (event.key === "Enter") {
            event.preventDefault();
            if (editingRef.current) {
              commitEdit(event.shiftKey ? "up" : "down");
            } else {
              // Typing straight into the formula bar and pressing Enter has to
              // commit the draft, not open the cell editor with a stale value.
              setEditing({ row: selection.focus.row, col: selection.focus.col, value: formulaDraft });
              commitEdit(event.shiftKey ? "up" : "down");
            }
            restoreGridFocusRef.current = true;
          }
          if (event.key === "Tab") {
            event.preventDefault();
            if (editingRef.current) commitEdit(event.shiftKey ? "left" : "right");
          }
          if (event.key === "Escape") {
            event.preventDefault();
            restoreGridFocusRef.current = true;
            setEditing(null);
            setFormulaDraft(activeCell?.formula ?? cellText(activeCell));
          }
        }}
      />
      {android ? (
        <>
          <button
            type="button"
            className="icon-btn calc-bar-action"
            aria-label={t("common.cancel")}
            onClick={cancelFormulaBar}
          >
            <X size={17} />
          </button>
          <button
            type="button"
            className="icon-btn calc-bar-action is-primary"
            aria-label={t("common.apply")}
            onClick={commitFormulaBar}
          >
            <Check size={17} />
          </button>
        </>
      ) : null}
    </div>
  );

  return (
    <div
      className={`editor calc-editor${android ? " is-android" : ""}`}
      style={android && keyboardInset > 0 ? { paddingBottom: keyboardInset } : undefined}
    >
      <CalcRibbon
        active={ribbon}
        onSelect={setRibbon}
        canUndo={undoStack.length > 0}
        canRedo={redoStack.length > 0}
        activeStyle={activeCell?.style}
        frozen={sheet.freezeRows > 0 || sheet.freezeCols > 0}
        showGridlines={sheet.showGridlines}
        tablesPanelOpen={tablesPanel}
        traceActive={trace !== null}
        aiConfigured={aiStatus.configured}
        busy={session.busy}
        actions={{
          undo,
          redo,
          copy: copySelection,
          clear: clearSelection,
          applyStyle,
          merge: () => toggleMerge(sheet, selection, updateSheet),
          borders: () => applyBorder("all"),
          openChart: () => setChartDialog(true),
          openPivot: () => setPivotDialog(true),
          openTable: () => setTableDialog(true),
          toggleTablesPanel: () => setTablesPanel((open) => !open),
          trace: traceFromSelection,
          clearTrace: () => setTrace(null),
          openConditional: () => setConditionalDialog(true),
          openValidation: () => setValidationDialog(true),
          sort: (ascending) => sortByColumn(selection.focus.col, ascending),
          filter: () => openFilter(selection.focus.col),
          textToColumns: openTextToColumns,
          removeDuplicates: openRemoveDuplicates,
          aiSummarize: openAiSummarize,
          aiFormula: openAiFormula,
          insertRow: () => insertRow(sheet, selection.focus.row, updateSheet),
          deleteRow: () => deleteRow(sheet, selection.focus.row, updateSheet),
          insertColumn: () => insertColumn(sheet, selection.focus.col, updateSheet),
          deleteColumn: () => deleteColumn(sheet, selection.focus.col, updateSheet),
          openNames: () => setNameDialog(true),
          openPrint: () => setPrintDialog(true),
          toggleFreeze,
          toggleGridlines: () => updateSheet((current) => ({ ...current, showGridlines: !current.showGridlines })),
          addSheet,
          save: () => void session.save(),
          saveAs: () => void session.saveAs(),
          print: () => void session.print(),
        }}
      />

      {!android ? formulaBar : null}

      {trace || selectedError ? (
        <div
          className="calc-audit-banner"
          style={{
            display: "flex",
            alignItems: "center",
            gap: 8,
            padding: "4px 10px",
            borderBottom: "1px solid var(--border)",
            background: "var(--surface-2)",
            fontSize: 12,
          }}
        >
          {trace ? (
            <>
              <strong>{trace.kind === "precedents" ? t("calc.tracePrecedents") : t("calc.traceDependents")}</strong>
              <span className="muted" style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                {trace.cells
                  .map((node) => (node.sheet === sheet.name ? node.address : `${node.sheet}!${node.address}`))
                  .join(", ")}
              </span>
            </>
          ) : (
            <span>{selectedError}</span>
          )}
          <span className="spacer" />
          {trace ? (
            <button type="button" className="btn btn-soft" onClick={() => setTrace(null)}>
              {t("calc.clearTrace")}
            </button>
          ) : null}
        </div>
      ) : null}

      <div className="calc-grid-wrap" ref={containerRef}>
        <div
          className="calc-grid"
          tabIndex={0}
          role="grid"
          aria-label={t("calc.gridLabel")}
          ref={gridRef}
          onPointerDown={handleGridPointerDown}
          onKeyDown={handleKeyDown}
          onScroll={(event) =>
            setScroll((current) => ({
              ...current,
              top: (event.target as HTMLDivElement).scrollTop,
              left: (event.target as HTMLDivElement).scrollLeft,
            }))
          }
          style={{ width: "100%", height: "100%" }}
        >
          <div
            className="calc-canvas"
            style={{ width: HEADER_WIDTH + totalWidth, height: HEADER_HEIGHT + rowLayout.total, zoom: gridZoom }}
          >
            <div
              className="calc-col-headers"
              style={{ transform: `translate(${HEADER_WIDTH}px, ${scroll.top / gridZoom}px)` }}
            >
              {visible.columns.map(({ col, x }) => (
                <div
                  key={col}
                  data-col-header={col}
                  className={`calc-col-header${selection.focus.col === col ? " is-active" : ""}`}
                  style={{
                    left: pinnedPosition(col, freeze.cols, x, scrollX),
                    width: sheet.colWidths[String(col)] ?? DEFAULT_COL_WIDTH,
                    zIndex: col < freeze.cols ? 1 : undefined,
                  }}
                  onContextMenu={(event) => {
                    event.preventDefault();
                    insertColumn(sheet, col, updateSheet);
                  }}
                >
                  {columnLabel(col)}
                  <span className="col-resize" data-col-resize={col} />
                </div>
              ))}
            </div>
            <div className="calc-row-headers" style={{ transform: `translate(${scroll.left / gridZoom}px, 0)` }}>
              {visible.rows.map((row) => (
                <div
                  key={row}
                  data-row-header={row}
                  className={`calc-row-header${selection.focus.row === row ? " is-active" : ""}`}
                  style={{
                    top: pinnedPosition(row, freeze.rows, rowLayout.offsetOf(row), scrollY),
                    height: rowLayout.heightOf(row),
                    zIndex: row < freeze.rows ? 1 : undefined,
                  }}
                >
                  {row + 1}
                </div>
              ))}
            </div>
            <div className="calc-corner" style={{ transform: `translate(${scrollX}px, ${scrollY}px)` }} />
            <div
              className="calc-cells"
              style={{
                transform: `translate(${HEADER_WIDTH}px, 0)`,
                width: totalWidth,
                height: rowLayout.total,
              }}
            >
              {visible.rows.map((row) =>
                visible.columns.map(({ col, x }) => {
                  const address = formatAddress(row, col);
                  const value = computed.get(address) ?? "";
                  const cell = sheet.cells[address];
                  const width = sheet.colWidths[String(col)] ?? DEFAULT_COL_WIDTH;
                  const isEditing = editing?.row === row && editing?.col === col;
                  const inSelection =
                    row >= selectionBounds.start.row &&
                    row <= selectionBounds.end.row &&
                    col >= selectionBounds.start.col &&
                    col <= selectionBounds.end.col;
                  const fill = conditionalFills.get(address);
                  const style = cell?.style ?? defaultCellStyle();
                  const validation = validationAt.find(row, col);
                  const invalid = validation ? !isValid(validation, value, listItems.get(validation.id)) : false;
                  // The structured table (if any) that owns this cell decides
                  // header/banding/outline; the cell's own formatting still wins.
                  const tableEntry = sheetTables.find(
                    ({ parts }) =>
                      row >= parts.start.row && row <= parts.end.row && col >= parts.start.col && col <= parts.end.col,
                  );
                  let tableFill: string | undefined;
                  let tableHeader = false;
                  if (tableEntry) {
                    const { table, parts } = tableEntry;
                    tableHeader = table.hasHeaders && row === parts.start.row;
                    const totalsRow = table.hasTotals && row === parts.end.row;
                    if (tableHeader) tableFill = table.headerFill ?? undefined;
                    else if (!totalsRow && table.bandedRows) {
                      const bodyStart = parts.start.row + (table.hasHeaders ? 1 : 0);
                      if ((row - bodyStart) % 2 === 1) tableFill = "#EFF6FF";
                    }
                  }
                  const traceKind = tracedCells.get(address);
                  const frozenRow = row < freeze.rows;
                  const frozenCol = col < freeze.cols;
                  return (
                    <div
                      key={address}
                      data-cell={`${row}:${col}`}
                      data-row={row}
                      data-col={col}
                      className={`calc-cell${inSelection ? " is-selected" : ""}${invalid ? " is-invalid" : ""}`}
                      style={{
                        left: pinnedPosition(col, freeze.cols, x, scrollX),
                        top: pinnedPosition(row, freeze.rows, rowLayout.offsetOf(row), scrollY),
                        width,
                        height: rowLayout.heightOf(row),
                        // Frozen cells cover what scrolls under them, so they need a fill.
                        zIndex: frozenRow && frozenCol ? 3 : frozenRow || frozenCol ? 2 : undefined,
                        background:
                          fill ?? tableFill ?? style.fill ?? (frozenRow || frozenCol ? "var(--bg)" : undefined),
                        fontWeight: style.bold || (tableHeader && tableEntry!.table.headerBold) ? 700 : undefined,
                        fontStyle: style.italic ? "italic" : undefined,
                        textDecoration:
                          [style.underline ? "underline" : "", style.strike ? "line-through" : ""]
                            .filter(Boolean)
                            .join(" ") || undefined,
                        color: style.color ?? (tableFill && tableHeader ? "#ffffff" : undefined),
                        textAlign: (style.align === "general"
                          ? typeof value === "number"
                            ? "right"
                            : "left"
                          : style.align) as "left" | "right" | "center",
                        justifyContent:
                          style.align === "center"
                            ? "center"
                            : style.align === "right" || (style.align === "general" && typeof value === "number")
                              ? "flex-end"
                              : "flex-start",
                      }}
                      onDoubleClick={() => setEditing({ row, col, value: cell?.formula ?? cellText(cell) })}
                    >
                      {isEditing ? (
                        <input
                          className="cell-editor"
                          ref={cellInputRef}
                          value={editing!.value}
                          // eslint-disable-next-line jsx-a11y/no-autofocus -- typing replaces the cell content; focusing the editor is the whole point of the interaction
                          autoFocus
                          onFocus={(event) => {
                            setFocusMode("cell");
                            setSuggestDismissed(false);
                            setDraftCaret(event.currentTarget.selectionStart ?? event.currentTarget.value.length);
                          }}
                          onSelect={(event) =>
                            setDraftCaret(event.currentTarget.selectionStart ?? event.currentTarget.value.length)
                          }
                          onChange={(event) => {
                            setDraftCaret(event.target.selectionStart ?? event.target.value.length);
                            setSuggestDismissed(false);
                            setEditing({ row, col, value: event.target.value });
                          }}
                          onBlur={() => {
                            setFocusMode(null);
                            commitEdit("none", false);
                          }}
                          onKeyDown={(event) => {
                            if (event.nativeEvent.isComposing) return;
                            // The popup owns Tab/Enter/Escape/arrows while it is open.
                            if (handleAssistKey(event)) return;
                            if (event.key === "Enter") {
                              event.preventDefault();
                              commitEdit(event.shiftKey ? "up" : "down");
                            }
                            if (event.key === "Tab") {
                              event.preventDefault();
                              commitEdit(event.shiftKey ? "left" : "right");
                            }
                            if (event.key === "Escape") {
                              event.preventDefault();
                              event.stopPropagation();
                              restoreGridFocusRef.current = true;
                              setEditing(null);
                            }
                          }}
                        />
                      ) : (
                        <span className="cell-text">{formatCellDisplay(value, style)}</span>
                      )}
                      {style.borders.top ? <span className="cell-border top" /> : null}
                      {style.borders.bottom ? <span className="cell-border bottom" /> : null}
                      {style.borders.left ? <span className="cell-border left" /> : null}
                      {style.borders.right ? <span className="cell-border right" /> : null}
                      {tableEntry ? (
                        <span
                          className="table-outline"
                          style={{
                            position: "absolute",
                            inset: 0,
                            pointerEvents: "none",
                            borderTop: row === tableEntry.parts.start.row ? "2px solid #1d4ed8" : undefined,
                            borderBottom: row === tableEntry.parts.end.row ? "2px solid #1d4ed8" : undefined,
                            borderLeft: col === tableEntry.parts.start.col ? "2px solid #1d4ed8" : undefined,
                            borderRight: col === tableEntry.parts.end.col ? "2px solid #1d4ed8" : undefined,
                          }}
                        />
                      ) : null}
                      {traceKind ? (
                        <span
                          className="cell-trace"
                          style={{
                            position: "absolute",
                            inset: 0,
                            pointerEvents: "none",
                            boxShadow: `inset 0 0 0 2px ${traceKind === "precedents" ? "#2563eb" : "#dc2626"}`,
                          }}
                        />
                      ) : null}
                      {(() => {
                        const bar = dataBars.get(address);
                        if (!bar) return null;
                        const width = bar.max > 0 ? Math.min(100, (Math.abs(Number(value) || 0) / bar.max) * 100) : 0;
                        return <span className="data-bar" style={{ width: `${width}%`, background: bar.fill }} />;
                      })()}
                      {cell?.comment ? <span className="cell-comment-dot" title={cell.comment} /> : null}
                    </div>
                  );
                }),
              )}
              {freeze.rows > 0 ? (
                <div
                  className="calc-freeze-line is-row"
                  aria-hidden
                  style={{
                    top: scrollY + freeze.height - 1,
                    left: scrollX,
                    width: Math.max(0, scroll.width / gridZoom - HEADER_WIDTH),
                  }}
                />
              ) : null}
              {freeze.cols > 0 ? (
                <div
                  className="calc-freeze-line is-col"
                  aria-hidden
                  style={{
                    left: scrollX + freeze.width - 1,
                    top: scrollY,
                    height: Math.max(0, scroll.height / gridZoom - HEADER_HEIGHT),
                  }}
                />
              ) : null}
            </div>
            <span
              className="calc-fill-handle"
              data-fill-handle=""
              style={{
                left: fillHandlePlace.left - 5,
                top: fillHandlePlace.top - 5,
                display: fillHandlePlace.hidden ? "none" : undefined,
              }}
            />
            <span
              className="calc-select-handle"
              data-select-handle="start"
              aria-hidden
              style={{
                left: selectHandleStartPlace.left,
                top: selectHandleStartPlace.top,
                display: selectHandleStartPlace.hidden ? "none" : undefined,
              }}
            />
            <span
              className="calc-select-handle"
              data-select-handle="end"
              aria-hidden
              style={{
                left: fillHandlePlace.left,
                top: fillHandlePlace.top,
                display: fillHandlePlace.hidden ? "none" : undefined,
              }}
            />
            {sheet.charts.map((chart) => {
              const position = parseAddress(chart.anchor) ?? { row: 0, col: 0 };
              return (
                <ChartBox
                  key={chart.id}
                  chart={chart}
                  sheet={sheet}
                  workbook={workbook}
                  x={columnX(position.col)}
                  y={HEADER_HEIGHT + rowLayout.offsetOf(position.row)}
                  onRemove={() =>
                    updateSheet((current) => ({
                      ...current,
                      charts: current.charts.filter((candidate) => candidate.id !== chart.id),
                    }))
                  }
                />
              );
            })}
            {(sheet.pivotTables ?? []).map((pivot) => {
              const position = parseAddress(pivot.anchor) ?? { row: 0, col: 0 };
              return (
                <PivotBox
                  key={pivot.id}
                  pivot={pivot}
                  sheet={sheet}
                  workbook={workbook}
                  x={columnX(position.col)}
                  y={HEADER_HEIGHT + rowLayout.offsetOf(position.row)}
                  onRemove={() =>
                    updateSheet((current) => ({
                      ...current,
                      pivotTables: (current.pivotTables ?? []).filter((candidate) => candidate.id !== pivot.id),
                    }))
                  }
                />
              );
            })}
            {listCell ? (
              <button
                type="button"
                className="calc-list-arrow"
                data-list-arrow=""
                tabIndex={-1}
                aria-label={t("calc.listShow")}
                aria-haspopup="listbox"
                aria-expanded={listDropdownOpen}
                style={{ left: listCell.left + listCell.width, top: listCell.top, height: listCell.height }}
                // The grid would start a selection or pan gesture and capture
                // the pointer; the arrow only needs its click.
                onPointerDown={(event) => event.stopPropagation()}
                onClick={() => (listDropdownOpen ? closeListDropdown() : openListDropdown())}
              >
                <ChevronDown size={13} />
              </button>
            ) : null}
            {listCell && listDropdownOpen ? (
              <div
                ref={listDropdownRef}
                className="calc-list-popup"
                role="listbox"
                tabIndex={-1}
                aria-label={t("calc.listValues")}
                aria-activedescendant={`${listDropdownId}-${listDropdownIndex}`}
                style={{
                  left: listCell.left,
                  top: listCell.above ? listCell.top : listCell.top + listCell.height,
                  minWidth: listCell.width,
                  transform: listCell.above ? "translateY(-100%)" : undefined,
                }}
                onPointerDown={(event) => event.stopPropagation()}
                onKeyDown={handleListDropdownKey}
                // One click handler for every option; a tap is a click too,
                // while a touch scroll of the list never produces one.
                onClick={(event) => {
                  const option = (event.target as HTMLElement).closest<HTMLElement>("[data-list-index]");
                  if (option) chooseListValue(activeListItems[Number(option.dataset.listIndex)]);
                }}
                onBlur={(event) => {
                  // Focus moving to the arrow belongs to the arrow's own toggle.
                  const next = event.relatedTarget as HTMLElement | null;
                  if (next?.closest?.("[data-list-arrow]") || event.currentTarget.contains(next)) return;
                  setListDropdown(null);
                }}
              >
                {activeListItems.map((item, index) => (
                  <div
                    key={item}
                    id={`${listDropdownId}-${index}`}
                    role="option"
                    tabIndex={-1}
                    data-list-index={index}
                    aria-selected={index === listDropdownIndex}
                    className={`calc-list-option${index === listDropdownIndex ? " is-active" : ""}`}
                    onMouseEnter={() => setListDropdown({ index })}
                  >
                    {item}
                  </div>
                ))}
              </div>
            ) : null}
          </div>
        </div>
      </div>

      <SheetTabs
        sheets={workbook.sheets}
        activeIndex={sheetIndex}
        path={tab.path ?? null}
        dirty={tab.dirty}
        onSelect={(index) => {
          setSheetIndex(index);
          setSelection({ anchor: { row: 0, col: 0 }, focus: { row: 0, col: 0 } });
        }}
        onAdd={addSheet}
        onRename={renameSheet}
        onRemove={removeSheet}
      />

      <div className="editor-status">
        <span>{nameBox}</span>
        <span>
          {activeCell?.formula
            ? t("calc.formula")
            : formatCellDisplay(
                computed.get(formatAddress(selection.focus.row, selection.focus.col)) ?? "",
                activeCell?.style ?? defaultCellStyle(),
              )}
        </span>
        <span className="spacer" />
        <span>
          {sheet.rowCount} × {sheet.colCount}
        </span>
      </div>

      {android ? formulaBar : null}

      {chartDialog ? <ChartDialog onPick={addChart} onClose={() => setChartDialog(false)} /> : null}

      {pivotDialog ? (
        <PivotDialog workbook={workbook} sheet={sheet} onClose={() => setPivotDialog(false)} onApply={addPivot} />
      ) : null}

      {conditionalDialog ? (
        <ConditionalDialog onClose={() => setConditionalDialog(false)} onApply={addConditional} />
      ) : null}

      {validationDialog ? (
        <ValidationDialog onClose={() => setValidationDialog(false)} onApply={addValidation} />
      ) : null}

      {textToColumnsDialog ? (
        <TextToColumnsDialog
          texts={textToColumnsTexts()}
          startColumn={selectionBounds.start.col}
          holdsData={holdsData}
          onClose={() => setTextToColumnsDialog(false)}
          onApply={applyTextToColumns}
        />
      ) : null}

      {duplicatesDialog ? (
        <RemoveDuplicatesDialog
          columns={duplicateColumns()}
          onClose={() => setDuplicatesDialog(false)}
          onApply={removeDuplicates}
        />
      ) : null}

      {aiDialog?.kind === "summarize" ? (
        <SummarizeColumnDialog
          docId={tab.id}
          status={aiStatus}
          column={columnLabel(aiDialog.col)}
          lines={aiDialog.lines}
          onInsert={(summary) =>
            acceptAiCell(
              summaryCellText(summary),
              t("ai.edit.insertedBelow", { cell: formatAddress(aiDialog.row, aiDialog.col) }),
            )
          }
          onClose={() => setAiDialog(null)}
        />
      ) : null}

      {aiDialog?.kind === "formula" ? (
        <SuggestFormulaDialog
          docId={tab.id}
          status={aiStatus}
          cell={formatAddress(aiDialog.row, aiDialog.col)}
          headers={aiDialog.headers}
          selection={aiDialog.selection}
          onAccept={(formula) => acceptAiCell(formula)}
          onClose={() => setAiDialog(null)}
        />
      ) : null}

      {nameDialog ? (
        <NameManagerDialog
          names={workbook.names ?? []}
          currentSheet={sheet.name}
          selection={`${formatAddress(selection.anchor.row, selection.anchor.col)}:${formatAddress(selection.focus.row, selection.focus.col)}`}
          onClose={() => setNameDialog(false)}
          onChange={(names) => {
            update((current) => ({ ...current, names }));
            setNameDialog(false);
          }}
        />
      ) : null}

      {printDialog ? (
        <PrintLayoutDialog
          print={sheet.print ?? defaultPrintSettings()}
          sheetName={sheet.name}
          onClose={() => setPrintDialog(false)}
          onApply={(print) => {
            updateSheet((current) => ({ ...current, print }));
            setPrintDialog(false);
          }}
        />
      ) : null}

      {filterOpen ? (
        <FilterDialog
          draft={filterOpen}
          onChange={setFilterOpen}
          onApply={applyFilter}
          onClear={() => {
            const tableId = filterOpen.tableId;
            updateSheet((current) => clearFilter(current, tableId));
            setFilterOpen(null);
          }}
          onClose={() => setFilterOpen(null)}
        />
      ) : null}

      {tableDialog ? (
        <InsertTableDialog
          defaultName={uniqueTableName(sheet.tables ?? [], `Table${(sheet.tables?.length ?? 0) + 1}`)}
          defaultRange={`${formatAddress(selectionBounds.start.row, selectionBounds.start.col)}:${formatAddress(selectionBounds.end.row, selectionBounds.end.col)}`}
          onClose={() => setTableDialog(false)}
          onApply={addTable}
        />
      ) : null}

      {tablesPanel ? (
        <TablesPanel
          tables={sheet.tables ?? []}
          onClose={() => setTablesPanel(false)}
          onInsert={() => setTableDialog(true)}
          onJump={jumpToTable}
          onRename={renameTable}
          onDelete={(table) => deleteTable(table.id)}
          onToggleTotals={(table) => patchTable(table.id, (current) => ({ ...current, hasTotals: !current.hasTotals }))}
          onToggleBanded={(table) =>
            patchTable(table.id, (current) => ({ ...current, bandedRows: !current.bandedRows }))
          }
          onAddColumn={addCalculatedColumn}
          onFilter={(table, column) => openTableFilter(table, column)}
        />
      ) : null}

      {find.panel ? <FindReplacePanel panel={find.panel} /> : null}

      {assistAnchor && focusMode !== null && (suggestions || argumentHint) ? (
        <FormulaAssistPopup
          anchor={assistAnchor}
          hint={argumentHint}
          suggestions={suggestions}
          selectedIndex={suggestIndex}
          android={android}
          onPick={applySuggestion}
          onHover={setSuggestIndex}
        />
      ) : null}
    </div>
  );
}
