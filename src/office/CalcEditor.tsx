/**
 * Calc editor: virtualised spreadsheet grid with a real formula engine,
 * formatting, multiple sheets, sorting, conditional formatting, validation
 * and SVG charts fed from cell ranges.
 */
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import {
  AlignCenter,
  AlignLeft,
  AlignRight,
  ArrowDownAZ,
  ArrowUpAZ,
  BarChart3,
  Bold,
  Copy,
  Eraser,
  Eye,
  Filter,
  FolderOpen,
  GitBranch,
  Grid3x3,
  Printer,
  RefreshCw,
  Italic,
  Merge,
  Minus,
  Plus,
  Redo2,
  Save,
  Sigma,
  Table2,
  Trash2,
  Tag,
  Underline,
  Undo2,
  Snowflake,
  XCircle,
} from "lucide-react";
import type { OfficeTab, Workbook } from "../lib/office-store";
import { useOfficeTabs } from "../lib/office-store";
import { useT, type Translate } from "../lib/i18n";
import { useToasts } from "../lib/store";
import { cellText, defaultCellStyle, defaultPrintSettings, emptyCell, newSheet, newSpreadsheetTable, type Cell, type CellStyle, type ChartData, type CondRule, type NamedRange, type PivotTable, type PivotValueField, type PrintSettings, type Sheet, type SpreadsheetTable } from "../lib/office-types";
import { computePivot, pivotFields } from "./calc/pivot";
import {
  addressesInRange,
  columnLabel,
  formatAddress,
  functionCatalogue,
  isError,
  parseAddress,
  parseRange,
  suggestFunctions,
  type Scalar,
} from "./calc/formula";
import { addressInRange } from "./calc/addresses";
import {
  findCircularReferences,
  invalidReferences,
  traceDependents,
  tracePrecedents,
  type AuditNode,
} from "./calc/audit";
import { tableByName, tableColumnBodyRange } from "./calc/structured";
import {
  applyCellEdit,
  applyCellEdits,
  computeSheetValues,
  computeWorkbookValues,
  formatCellDisplay,
  isBlankCell,
  scalarToCellValue,
  shiftFormulaRows,
  uniqueSheetName,
  usedRange,
} from "./calc/cells";
import { Dialog, Ribbon, RibbonGroup, ToolButton, ToolColor, ToolSelect } from "./office-ui";
import { openIntoWorkspace, useEditorShortcuts, useOfficeSession } from "./useOfficeSession";

export { scalarToCellValue, computeWorkbookValues, applyCellEdit, shiftFormulaRows, isBlankCell };

type CalcTab = OfficeTab & { model: Workbook };

const ROW_HEIGHT = 24;
const HEADER_WIDTH = 56;
const DEFAULT_COL_WIDTH = 96;

interface Selection {
  anchor: { row: number; col: number };
  focus: { row: number; col: number };
}

interface CellPosition {
  row: number;
  col: number;
}

interface EditingCell extends CellPosition {
  value: string;
}

/** Where a committed edit sends the selection. */
type CommitMove = "down" | "up" | "right" | "left" | "none";

// ---------------------------------------------------------------------------
// Formula assistance (autocomplete + argument hints)
// ---------------------------------------------------------------------------

type SuggestionKind = "function" | "name" | "sheet" | "table" | "column";

interface FormulaSuggestion {
  kind: SuggestionKind;
  label: string;
  insert: string;
  detail: string;
  /** Caret offset inside `insert` after the insertion (defaults to the end). */
  caret?: number;
}

interface SuggestionList {
  items: FormulaSuggestion[];
  /** Range inside the draft the selected item replaces. */
  start: number;
  end: number;
}

interface ArgumentHint {
  name: string;
  parts: string[];
  active: number;
}

interface CatalogueEntry {
  name: string;
  signature: string;
  category: string;
}

/** The identifier or partial structured reference ending at `caret`. */
function suggestionWord(text: string, caret: number): string {
  return /[A-Za-z_$][A-Za-z0-9_$.]*$/.exec(text.slice(0, caret))?.[0] ?? "";
}

/**
 * The suggestion popup contents for a draft.
 *
 * Inside `Table[...]` the items are the table's columns; otherwise functions
 * (prefix match), defined names, sheet names and table names are offered.
 * Returns null when the draft is not a formula or nothing matches.
 */
function buildSuggestions(
  text: string,
  caret: number,
  workbook: Workbook,
  sheet: Sheet,
  catalogue: Map<string, CatalogueEntry>,
  t: Translate,
): SuggestionList | null {
  if (!text.startsWith("=") || caret < 1 || caret > text.length) return null;
  const before = text.slice(0, caret);
  const bracket = /([A-Za-z_][A-Za-z0-9_$. ]*)\[([^\[\]]*)$/.exec(before);
  if (bracket) {
    const table = tableByName(sheet.tables, bracket[1]);
    if (!table) return null;
    const partial = bracket[2];
    const items = table.columns
      .filter((column) => column.name.toUpperCase().startsWith(partial.toUpperCase()))
      .map<FormulaSuggestion>((column) => ({
        kind: "column",
        label: column.name,
        insert: text[caret] === "]" ? column.name : `${column.name}]`,
        detail: `${table.name}[${column.name}]`,
      }));
    if (items.length === 0) return null;
    return { items, start: caret - partial.length, end: caret };
  }

  const word = suggestionWord(text, caret);
  if (!word) return null;
  const upper = word.toUpperCase();
  const items: FormulaSuggestion[] = [];
  for (const name of suggestFunctions(word, 6)) {
    const meta = catalogue.get(name);
    items.push({
      kind: "function",
      label: name,
      insert: `${name}(`,
      caret: name.length + 1,
      detail: meta ? `${meta.signature} · ${meta.category}` : name,
    });
  }
  for (const entry of workbook.names ?? []) {
    if (entry.name.toUpperCase().startsWith(upper)) {
      items.push({ kind: "name", label: entry.name, insert: entry.name, detail: entry.definition });
    }
  }
  for (const candidate of workbook.sheets) {
    if (candidate.name.toUpperCase().startsWith(upper)) {
      items.push({ kind: "sheet", label: candidate.name, insert: `${candidate.name}!`, detail: t("calc.sheetReference") });
    }
  }
  for (const table of sheet.tables ?? []) {
    if (table.name.toUpperCase().startsWith(upper)) {
      items.push({ kind: "table", label: table.name, insert: `${table.name}[`, detail: t("calc.tableReference") });
    }
  }
  if (items.length === 0) return null;
  return { items, start: caret - word.length, end: caret };
}

/** Splits a signature body on top-level commas (`VLOOKUP(a, [b], c)`). */
function splitSignature(signature: string): string[] {
  const open = signature.indexOf("(");
  const close = signature.lastIndexOf(")");
  if (open < 0 || close <= open) return [signature];
  const body = signature.slice(open + 1, close);
  const parts: string[] = [];
  let depth = 0;
  let current = "";
  for (const character of body) {
    if (character === "(" || character === "[") depth += 1;
    else if (character === ")" || character === "]") depth -= 1;
    if (character === "," && depth === 0) {
      parts.push(current);
      current = "";
      continue;
    }
    current += character;
  }
  parts.push(current);
  return parts.map((part) => part.trim());
}

/**
 * The argument hint for the call the caret is inside, or null.
 *
 * Scanning backwards from the caret, the innermost unmatched `(` names the
 * function and the separators at that depth count the current argument.
 */
function argumentHintFor(text: string, caret: number, catalogue: Map<string, CatalogueEntry>): ArgumentHint | null {
  if (!text.startsWith("=") || caret < 1) return null;
  const before = text.slice(0, caret);
  let depth = 0;
  let separators = 0;
  for (let index = before.length - 1; index >= 0; index -= 1) {
    const character = before[index];
    if (character === '"') {
      // Skip a quoted string backwards; an unmatched quote just ends the scan.
      index -= 1;
      while (index >= 0 && before[index] !== '"') index -= 1;
      continue;
    }
    if (character === ")") {
      depth += 1;
      continue;
    }
    if (character === "(") {
      if (depth > 0) {
        depth -= 1;
        continue;
      }
      const match = /([A-Za-z_][A-Za-z0-9_.]*)$/.exec(before.slice(0, index));
      const name = match?.[1]?.toUpperCase() ?? "";
      const meta = catalogue.get(name);
      if (!meta) return null;
      const parts = splitSignature(meta.signature);
      if (parts.length === 0) return null;
      return { name, parts, active: Math.min(separators, parts.length - 1) };
    }
    if ((character === "," || character === ";") && depth === 0) separators += 1;
  }
  return null;
}

/** Appends a number until the name is unique inside the sheet's tables. */
function uniqueTableName(tables: readonly SpreadsheetTable[], base: string): string {
  let name = base;
  let index = 1;
  while (tables.some((table) => table.name.toLowerCase() === name.toLowerCase())) {
    index += 1;
    name = `${base}${index}`;
  }
  return name;
}

/** Appends a number until the column name is unique inside the table. */
function uniqueColumnName(columns: readonly string[], base: string): string {
  let name = base;
  let index = 1;
  while (columns.some((column) => column.toLowerCase() === name.toLowerCase())) {
    index += 1;
    name = `${base}${index}`;
  }
  return name;
}

export function CalcEditor({ tab }: { tab: CalcTab }) {
  const t = useT();
  const workbook = tab.model;
  const edit = useOfficeTabs((state) => state.edit);
  const session = useOfficeSession(tab);
  const [ribbon, setRibbon] = useState("home");
  const [sheetIndex, setSheetIndex] = useState(workbook.activeSheet ?? 0);
  const [selection, setSelection] = useState<Selection>({ anchor: { row: 0, col: 0 }, focus: { row: 0, col: 0 } });
  const [editing, setEditingState] = useState<EditingCell | null>(null);
  const [formulaDraft, setFormulaDraft] = useState("");
  const [scroll, setScroll] = useState({ top: 0, left: 0, width: 900, height: 500 });
  const [chartDialog, setChartDialog] = useState(false);
  const [pivotDialog, setPivotDialog] = useState(false);
  const [conditionalDialog, setConditionalDialog] = useState(false);
  const [validationDialog, setValidationDialog] = useState(false);
  const [nameDialog, setNameDialog] = useState(false);
  const [printDialog, setPrintDialog] = useState(false);
  const [filterOpen, setFilterOpen] = useState<{ col: number; values: Array<{ value: string; checked: boolean }>; tableId?: string; tableName?: string } | null>(null);
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
  const [assistAnchor, setAssistAnchor] = useState<{ left: number; top: number } | null>(null);
  const [tableDialog, setTableDialog] = useState(false);
  const [tablesPanel, setTablesPanel] = useState(false);
  const [trace, setTrace] = useState<{ kind: "precedents" | "dependents"; cells: AuditNode[] } | null>(null);

  const setEditing = useCallback((next: EditingCell | null) => {
    editingRef.current = next;
    setEditingState(next);
  }, []);

  const sheet = workbook.sheets[Math.min(sheetIndex, workbook.sheets.length - 1)] ?? workbook.sheets[0];
  const activeCell = sheet.cells[formatAddress(selection.focus.row, selection.focus.col)];
  const computed = useMemo(() => computeSheetValues(workbook, sheet), [workbook, sheet]);
  const catalogue = useMemo(() => new Map(functionCatalogue().map((entry) => [entry.name, entry] as const)), []);

  // The draft and caret of whichever input owns the focus. The suggestion
  // popup and the argument hint are derived from these, never from the grid
  // selection, so the inline editor and the formula bar stay independent.
  const assistText = focusMode === "cell" ? editing?.value ?? "" : focusMode === "bar" ? formulaDraft : "";
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
  useEffect(() => setSuggestIndex(0), [suggestionKey]);

  // Keep the popup glued under the focused input as the draft changes.
  useLayoutEffect(() => {
    if (focusMode === null) {
      setAssistAnchor(null);
      return;
    }
    const input = focusMode === "cell" ? cellInputRef.current : formulaInputRef.current;
    if (!input) {
      setAssistAnchor(null);
      return;
    }
    const rect = input.getBoundingClientRect();
    setAssistAnchor({ left: rect.left, top: rect.bottom + 2 });
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

  useEditorShortcuts(session);

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
        (current) => ({ ...current, sheets: current.sheets.map((candidate, index) => (index === sheetIndex ? mutate(candidate) : candidate)) }),
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
    setFormulaDraft(activeCell?.formula ?? cellText(activeCell));
  }, [selection.focus.row, selection.focus.col, activeCell]);

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
      let left = 0;
      for (let col = 0; col < clamped.col; col += 1) left += sheet.colWidths[String(col)] ?? DEFAULT_COL_WIDTH;
      const width = sheet.colWidths[String(clamped.col)] ?? DEFAULT_COL_WIDTH;
      const top = clamped.row * (sheet.rowHeights[String(clamped.row)] ?? ROW_HEIGHT);
      const height = sheet.rowHeights[String(clamped.row)] ?? ROW_HEIGHT;
      if (top < grid.scrollTop) grid.scrollTop = top;
      else if (top + height > grid.scrollTop + grid.clientHeight) grid.scrollTop = top + height - grid.clientHeight;
      if (left < grid.scrollLeft) grid.scrollLeft = Math.max(0, left);
      else if (left + width > grid.scrollLeft + grid.clientWidth - HEADER_WIDTH) {
        grid.scrollLeft = left + width - grid.clientWidth + HEADER_WIDTH;
      }
      return clamped;
    },
    [sheet],
  );

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

  const applyStyle = (patch: Partial<CellStyle>) => {
    const addresses = addressesInRange(`${formatAddress(selection.anchor.row, selection.anchor.col)}:${formatAddress(selection.focus.row, selection.focus.col)}`);
    const entries = addresses.map((address) => {
      const position = parseAddress(address)!;
      const cell = sheet.cells[address] ?? emptyCell();
      return { row: position.row, col: position.col, cell: { ...cell, style: { ...cell.style, ...patch } } };
    });
    setCells(entries);
  };

  const applyBorder = (side: "top" | "right" | "bottom" | "left" | "all") => {
    const addresses = addressesInRange(`${formatAddress(selection.anchor.row, selection.anchor.col)}:${formatAddress(selection.focus.row, selection.focus.col)}`);
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
    const addresses = addressesInRange(`${formatAddress(selection.anchor.row, selection.anchor.col)}:${formatAddress(selection.focus.row, selection.focus.col)}`);
    updateSheet((current) => {
      const cells = { ...current.cells };
      for (const address of addresses) delete cells[address];
      return { ...current, cells };
    });
  };

  const clipboardRef = useRef<{ rows: Scalar[][]; start: CellPosition } | null>(null);

  const copySelection = () => {
    const parts = parseRange(`${formatAddress(selection.anchor.row, selection.anchor.col)}:${formatAddress(selection.focus.row, selection.focus.col)}`);
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
        const rows = text.split(/\r?\n/).filter((line) => line.length > 0).map((line) => line.split("\t"));
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
    const move = (dRow: number, dCol: number, extend = false) => {
      event.preventDefault();
      const next = revealCell({ row: row + dRow, col: col + dCol });
      setSelection(extend ? { anchor: selection.anchor, focus: next } : { anchor: next, focus: next });
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
        move(pageSize, 0, event.shiftKey);
        break;
      case "PageUp":
        move(-pageSize, 0, event.shiftKey);
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
    update((current) => ({ ...current, sheets: current.sheets.map((candidate, position) => (position === index ? { ...candidate, name } : candidate)) }));
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
      if (Number.isFinite(leftNumber) && Number.isFinite(rightNumber)) return ascending ? leftNumber - rightNumber : rightNumber - leftNumber;
      const compare = String(left).localeCompare(String(right));
      return ascending ? compare : -compare;
    });
    updateSheet((current) => {
      const cells = { ...current.cells };
      for (const address of addressesInRange(`${formatAddress(startRow, parts.start.col)}:${formatAddress(parts.end.row, parts.end.col)}`)) delete cells[address];
      rows.forEach((row, rowOffset) => {
        row.values.forEach((value) => {
          if (value.cell) cells[formatAddress(startRow + rowOffset, value.col)] = value.cell;
        });
      });
      return { ...current, cells };
    });
  };

  const openFilter = (col: number) => {
    const parts = parseRange(usedRange(sheet));
    if (!parts) return;
    const values = new Map<string, boolean>();
    for (let row = parts.start.row + 1; row <= parts.end.row; row += 1) {
      const text = String(computed.get(formatAddress(row, col)) ?? "");
      if (!values.has(text)) values.set(text, true);
    }
    setFilterOpen({ col, values: [...values.entries()].map(([value, checked]) => ({ value, checked })) });
  };

  /** Opens the filter dialog scoped to one structured table column. */
  const openTableFilter = (table: SpreadsheetTable, columnName: string) => {
    const parts = parseRange(table.range);
    const columnIndex = table.columns.findIndex((column) => column.name === columnName);
    if (!parts || columnIndex < 0) return;
    const body = tableColumnBodyRange(table, columnName);
    const values = new Map<string, boolean>();
    if (body) {
      for (const address of addressesInRange(`${body.start}:${body.end}`)) {
        const text = String(computed.get(address) ?? "");
        if (!values.has(text)) values.set(text, true);
      }
    }
    setFilterOpen({
      col: parts.start.col + columnIndex,
      tableId: table.id,
      tableName: table.name,
      values: [...values.entries()].map(([value, checked]) => ({ value, checked })),
    });
  };

  const applyFilter = () => {
    if (!filterOpen) return;
    const state = filterOpen;
    const allowed = new Set(state.values.filter((entry) => entry.checked).map((entry) => entry.value));
    updateSheet((current) => {
      const table = state.tableId ? (current.tables ?? []).find((candidate) => candidate.id === state.tableId) : undefined;
      const parts = parseRange(table ? table.range : usedRange(current));
      if (!parts) return current;
      // A structured table filters its body only; the plain sheet filter keeps
      // its original behaviour of scanning the whole used range.
      const startRow = table ? parts.start.row + (table.hasHeaders ? 1 : 0) : parts.start.row;
      const endRow = table ? parts.end.row - (table.hasTotals ? 1 : 0) : parts.end.row;
      let rowHeights = current.rowHeights;
      for (let row = startRow; row <= endRow; row += 1) {
        const address = formatAddress(row, state.col);
        const value = String(computed.get(address) ?? "");
        const position = parseAddress(address);
        if (!position) continue;
        const hidden = !allowed.has(value);
        if (hidden) {
          rowHeights = { ...rowHeights, [position.row]: 0 };
        } else if (rowHeights[position.row] === 0) {
          rowHeights = { ...rowHeights };
          delete rowHeights[position.row];
        }
      }
      if (table) {
        return {
          ...current,
          rowHeights,
          tables: (current.tables ?? []).map((candidate) =>
            candidate.id === table.id ? { ...candidate, filter: { range: table.range, column: state.col, values: [...allowed] } } : candidate,
          ),
        };
      }
      return { ...current, rowHeights, filter: { range: usedRange(current), column: state.col, values: [...allowed] } };
    });
    setFilterOpen(null);
  };

  const addChart = (kind: string) => {
    const parts = parseRange(`${formatAddress(selection.anchor.row, selection.anchor.col)}:${formatAddress(selection.focus.row, selection.focus.col)}`);
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
    const chart: ChartData = { kind, title: t("calc.chartTitle"), categories, series, legend: true, xTitle: "", yTitle: "", stacked: false, showLabels: false };
    const anchor = formatAddress(parts.end.row + 2, parts.start.col);
    updateSheet((current) => ({ ...current, charts: [...current.charts, { id: crypto.randomUUID(), chart, anchor, widthPx: 420, heightPx: 260 }] }));
    setChartDialog(false);
  };

  /** Adds a pivot over the current selection, anchored below it. */
  const addPivot = (config: { rows: string[]; columns: string[]; values: PivotValueField[]; filters: PivotTable["filters"] }) => {
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
        { id: crypto.randomUUID(), range, kind: rule.kind, values: rule.values, fill: rule.fill, color: null, topN: rule.topN ?? null, stopIfTrue: false },
      ],
    }));
    setConditionalDialog(false);
  };

  const addValidation = (validation: { kind: string; values: string[]; min: number | null; max: number | null; message: string }) => {
    const range = `${formatAddress(selection.anchor.row, selection.anchor.col)}:${formatAddress(selection.focus.row, selection.focus.col)}`;
    updateSheet((current) => ({
      ...current,
      validations: [...current.validations, { id: crypto.randomUUID(), range, kind: validation.kind, values: validation.values, min: validation.min, max: validation.max, message: validation.message, allowBlank: true }],
    }));
    setValidationDialog(false);
  };

  const toggleFreeze = () => {
    const row = selection.focus.row;
    const col = selection.focus.col;
    updateSheet((current) => (current.freezeRows === row && current.freezeCols === col ? { ...current, freezeRows: 0, freezeCols: 0 } : { ...current, freezeRows: row, freezeCols: col }));
  };

  // -------------------------------------------------------------------------
  // Structured tables (V3)
  // -------------------------------------------------------------------------

  /** Creates a table over the dialog's range, naming columns from the header row. */
  const addTable = (config: { name: string; range: string; hasHeaders: boolean; hasTotals: boolean; bandedRows: boolean }) => {
    const parts = parseRange(config.range);
    if (!parts) return;
    const columns: string[] = [];
    for (let col = parts.start.col; col <= parts.end.col; col += 1) {
      const header = config.hasHeaders ? String(computed.get(formatAddress(parts.start.row, col)) ?? "").trim() : "";
      columns.push(uniqueColumnName(columns, header || `Column${col - parts.start.col + 1}`));
    }
    const table = newSpreadsheetTable(uniqueTableName(sheet.tables ?? [], config.name.trim() || "Table1"), config.range, columns);
    table.hasHeaders = config.hasHeaders;
    table.hasTotals = config.hasTotals;
    table.bandedRows = config.bandedRows;
    updateSheet((current) => ({ ...current, tables: [...(current.tables ?? []), table] }));
    setTableDialog(false);
    setTablesPanel(true);
  };

  const patchTable = (tableId: string, patch: (table: SpreadsheetTable) => SpreadsheetTable) => {
    updateSheet((current) => ({ ...current, tables: (current.tables ?? []).map((table) => (table.id === tableId ? patch(table) : table)) }));
  };

  const deleteTable = (tableId: string) => {
    updateSheet((current) => ({ ...current, tables: (current.tables ?? []).filter((table) => table.id !== tableId) }));
  };

  const renameTable = (table: SpreadsheetTable) => {
    const name = window.prompt(t("calc.tableName"), table.name);
    if (!name || name.trim() === "") return;
    patchTable(table.id, (current) => ({
      ...current,
      name: uniqueTableName((sheet.tables ?? []).filter((candidate) => candidate.id !== table.id), name.trim()),
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
      const resolved = uniqueColumnName(table2.columns.map((entry) => entry.name), name);
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
    const cells = kind === "precedents" ? tracePrecedents(workbook, sheet.name, address) : traceDependents(workbook, sheet.name, address);
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
    const rows: number[] = [];
    const startRow = Math.max(0, Math.floor(scroll.top / ROW_HEIGHT) - 2);
    const endRow = Math.min(sheet.rowCount - 1, startRow + Math.ceil(scroll.height / ROW_HEIGHT) + 4);
    for (let row = startRow; row <= endRow; row += 1) rows.push(row);
    const columns: Array<{ col: number; x: number }> = [];
    let x = 0;
    let startCol = 0;
    for (let col = 0; col < sheet.colCount; col += 1) {
      const width = sheet.colWidths[String(col)] ?? DEFAULT_COL_WIDTH;
      if (x + width < scroll.left) {
        x += width;
        startCol = col + 1;
        continue;
      }
      if (x > scroll.left + scroll.width + 200) break;
      columns.push({ col, x: x - scroll.left });
      x += width;
    }
    return { rows, columns, startCol };
  }, [scroll, sheet]);

  const selectionBounds = useMemo(() => {
    const parts = parseRange(`${formatAddress(selection.anchor.row, selection.anchor.col)}:${formatAddress(selection.focus.row, selection.focus.col)}`);
    return parts ?? { start: selection.anchor, end: selection.focus };
  }, [selection]);

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
  // per visible cell. The old per-cell `conditionalFill` rescanned the rule
  // range for every cell in the viewport, which made a 5 000-cell rule
  // quadratic on the render path.
  const conditionalFills = useMemo(() => {
    const fills = new Map<string, string>();
    for (const rule of sheet.conditional) {
      if (rule.kind === "dataBar" || fills.size >= 50_000) continue;
      const addresses = addressesInRange(rule.range, 5000);
      if (addresses.length === 0) continue;
      const values = addresses.map((address) => computed.get(address) ?? "");
      const numbers = values.map((value) => (typeof value === "number" ? value : Number(value)));
      const limit = Math.max(1, rule.topN ?? 10);
      const sorted = [...numbers.filter(Number.isFinite)].sort((a, b) => b - a);
      const threshold = sorted[Math.min(limit, sorted.length) - 1] ?? Number.POSITIVE_INFINITY;
      const counts = new Map<string, number>();
      if (rule.kind === "duplicate") {
        for (const value of values) {
          const key = String(value ?? "");
          if (key !== "") counts.set(key, (counts.get(key) ?? 0) + 1);
        }
      }
      addresses.forEach((address, index) => {
        // First matching rule wins, in the order the rules are listed.
        if (fills.has(address)) return;
        const fill = ruleFill(rule, values[index], numbers[index], counts, threshold);
        if (fill) fills.set(address, fill);
      });
    }
    return fills;
  }, [sheet, computed]);

  // One pass over the data-bar rules: the scale of a bar is relative to the
  // largest value in its range, which is how every spreadsheet draws them.
  const dataBars = useMemo(() => {
    const bars = new Map<string, { max: number; fill: string }>();
    for (const rule of sheet.conditional) {
      if (rule.kind !== "dataBar") continue;
      const addresses = addressesInRange(rule.range, 5000);
      let max = 0;
      for (const address of addresses) {
        const value = Math.abs(Number(computed.get(address) ?? 0));
        if (Number.isFinite(value)) max = Math.max(max, value);
      }
      for (const address of addresses) bars.set(address, { max, fill: rule.fill ?? "#638EC6" });
    }
    return bars;
  }, [sheet, computed]);

  const columnX = (col: number) => {
    let x = 0;
    for (let index = 0; index < col; index += 1) x += sheet.colWidths[String(index)] ?? DEFAULT_COL_WIDTH;
    return x - scroll.left + HEADER_WIDTH;
  };

  const selectionAddress = formatAddress(selection.focus.row, selection.focus.col);
  const selectedTable = (sheet.tables ?? []).find((table) => addressInRange(selectionAddress, table.range));
  const nameBox = selectedTable
    ? selectedTable.name
    : `${selectionAddress}${selection.anchor.row !== selection.focus.row || selection.anchor.col !== selection.focus.col ? `:${formatAddress(selection.anchor.row, selection.anchor.col)}` : ""}`;

  // The last valid row/column, used for Ctrl+End, Space and the data extent.
  const lastRow = Math.max(0, sheet.rowCount - 1);
  const lastCol = Math.max(0, sheet.colCount - 1);
  const pageSize = Math.max(1, Math.floor(scroll.height / ROW_HEIGHT) - 1);

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

  return (
    <div className="editor calc-editor">
      <Ribbon
        tabs={[
          { id: "home", label: t("calc.tabHome") },
          { id: "insert", label: t("calc.tabInsert") },
          { id: "formulas", label: t("calc.tabFormulas") },
          { id: "data", label: t("calc.tabData") },
          { id: "view", label: t("calc.tabView") },
        ]}
        active={ribbon}
        onSelect={setRibbon}
      >
        {ribbon === "home" ? (
          <>
            <RibbonGroup label={t("writer.clipboard")}>
              <ToolButton icon={<Undo2 size={16} />} onClick={undo} disabled={undoStack.length === 0} title={t("common.undo")} />
              <ToolButton icon={<Redo2 size={16} />} onClick={redo} disabled={redoStack.length === 0} title={t("common.redo")} />
              <ToolButton icon={<Copy size={16} />} onClick={copySelection} title={t("common.copy")} />
              <ToolButton icon={<Eraser size={16} />} onClick={clearSelection} title={t("calc.clearCells")} />
            </RibbonGroup>
            <RibbonGroup label={t("writer.font")}>
              <ToolButton icon={<Bold size={16} />} onClick={() => applyStyle({ bold: !(activeCell?.style.bold ?? false) })} active={activeCell?.style.bold} title={t("writer.bold")} />
              <ToolButton icon={<Italic size={16} />} onClick={() => applyStyle({ italic: !(activeCell?.style.italic ?? false) })} active={activeCell?.style.italic} title={t("writer.italic")} />
              <ToolButton icon={<Underline size={16} />} onClick={() => applyStyle({ underline: !(activeCell?.style.underline ?? false) })} active={activeCell?.style.underline} title={t("writer.underline")} />
              <ToolColor value={activeCell?.style.color ?? "#1f2328"} onChange={(color) => applyStyle({ color })} title={t("writer.textColor")} />
              <ToolColor value={activeCell?.style.fill ?? "#ffffff"} onChange={(fill) => applyStyle({ fill })} title={t("calc.fillColor")} />
            </RibbonGroup>
            <RibbonGroup label={t("writer.paragraph")}>
              <ToolButton icon={<AlignLeft size={16} />} onClick={() => applyStyle({ align: "left" })} active={activeCell?.style.align === "left"} title={t("writer.alignLeft")} />
              <ToolButton icon={<AlignCenter size={16} />} onClick={() => applyStyle({ align: "center" })} active={activeCell?.style.align === "center"} title={t("writer.alignCenter")} />
              <ToolButton icon={<AlignRight size={16} />} onClick={() => applyStyle({ align: "right" })} active={activeCell?.style.align === "right"} title={t("writer.alignRight")} />
              <ToolButton icon={<Merge size={16} />} onClick={() => toggleMerge(sheet, selection, updateSheet)} title={t("calc.mergeCells")} />
              <ToolButton icon={<Grid3x3 size={16} />} onClick={() => applyBorder("all")} title={t("calc.borders")} />
            </RibbonGroup>
            <RibbonGroup label={t("calc.numberFormat")}>
              <ToolSelect
                value={activeCell?.style.numberFormat ?? "General"}
                onChange={(numberFormat) => applyStyle({ numberFormat })}
                options={[
                  { value: "General", label: t("calc.formatGeneral") },
                  { value: "0", label: "1234" },
                  { value: "0.00", label: "12.34" },
                  { value: "#,##0", label: "1,234" },
                  { value: "#,##0.00", label: "1,234.56" },
                  { value: "0%", label: "12%" },
                  { value: "$#,##0.00", label: "$1,234.56" },
                  { value: "dd.mm.yyyy", label: "31.12.2025" },
                  { value: "hh:mm", label: "13:45" },
                ]}
                width={120}
              />
            </RibbonGroup>
          </>
        ) : null}

        {ribbon === "insert" ? (
          <>
            <RibbonGroup label={t("calc.charts")}>
              <ToolButton icon={<BarChart3 size={16} />} label={t("calc.chart")} onClick={() => setChartDialog(true)} />
            </RibbonGroup>
            <RibbonGroup label={t("calc.pivotTable")}>
              <ToolButton icon={<Grid3x3 size={16} />} label={t("calc.pivotTable")} onClick={() => setPivotDialog(true)} />
            </RibbonGroup>
            <RibbonGroup label={t("calc.structuredTables")}>
              <ToolButton icon={<Table2 size={16} />} label={t("calc.insertTable")} onClick={() => setTableDialog(true)} />
              <ToolButton icon={<Eye size={16} />} label={t("calc.tableList")} onClick={() => setTablesPanel((open) => !open)} active={tablesPanel} />
            </RibbonGroup>
          </>
        ) : null}

        {ribbon === "formulas" ? (
          <>
            <RibbonGroup label={t("calc.functions")}>
              <ToolButton icon={<Sigma size={16} />} label="SUM" onClick={() => insertFunction("SUM")} />
              <ToolButton label="AVERAGE" onClick={() => insertFunction("AVERAGE")} />
              <ToolButton label="IF" onClick={() => insertFunction("IF")} />
              <ToolButton label="COUNT" onClick={() => insertFunction("COUNT")} />
              <ToolButton label="ROUND" onClick={() => insertFunction("ROUND")} />
              <ToolButton label="VLOOKUP" onClick={() => insertFunction("VLOOKUP")} />
            </RibbonGroup>
            <RibbonGroup label={t("calc.auditing")}>
              <ToolButton icon={<GitBranch size={16} />} label={t("calc.tracePrecedents")} onClick={() => traceFromSelection("precedents")} />
              <ToolButton icon={<GitBranch size={16} />} label={t("calc.traceDependents")} onClick={() => traceFromSelection("dependents")} />
              <ToolButton icon={<XCircle size={16} />} label={t("calc.clearTrace")} onClick={() => setTrace(null)} disabled={trace === null} />
            </RibbonGroup>
            <RibbonGroup label={t("calc.structuredTables")}>
              <ToolButton icon={<Table2 size={16} />} label={t("calc.insertTable")} onClick={() => setTableDialog(true)} />
              <ToolButton icon={<Eye size={16} />} label={t("calc.tableList")} onClick={() => setTablesPanel((open) => !open)} active={tablesPanel} />
            </RibbonGroup>
            <RibbonGroup label={t("calc.conditional")}>
              <ToolButton icon={<Filter size={16} />} label={t("calc.conditionalFormatting")} onClick={() => setConditionalDialog(true)} />
              <ToolButton label={t("calc.dataValidation")} onClick={() => setValidationDialog(true)} />
            </RibbonGroup>
          </>
        ) : null}

        {ribbon === "data" ? (
          <>
            <RibbonGroup label={t("calc.sort")}>
              <ToolButton icon={<ArrowUpAZ size={16} />} label={t("calc.sortAsc")} onClick={() => sortByColumn(selection.focus.col, true)} />
              <ToolButton icon={<ArrowDownAZ size={16} />} label={t("calc.sortDesc")} onClick={() => sortByColumn(selection.focus.col, false)} />
              <ToolButton icon={<Filter size={16} />} label={t("calc.filter")} onClick={() => openFilter(selection.focus.col)} />
            </RibbonGroup>
            <RibbonGroup label={t("calc.structure")}>
              <ToolButton icon={<Plus size={16} />} label={t("calc.insertRow")} onClick={() => insertRow(sheet, selection.focus.row, updateSheet)} />
              <ToolButton icon={<Minus size={16} />} label={t("calc.deleteRow")} onClick={() => deleteRow(sheet, selection.focus.row, updateSheet)} />
              <ToolButton icon={<Plus size={16} />} label={t("calc.insertColumn")} onClick={() => insertColumn(sheet, selection.focus.col, updateSheet)} />
              <ToolButton icon={<Minus size={16} />} label={t("calc.deleteColumn")} onClick={() => deleteColumn(sheet, selection.focus.col, updateSheet)} />
            </RibbonGroup>
            <RibbonGroup label={t("calc.names")}>
              <ToolButton icon={<Tag size={16} />} label={t("calc.nameManager")} onClick={() => setNameDialog(true)} />
            </RibbonGroup>
            <RibbonGroup label={t("calc.printLayout")}>
              <ToolButton icon={<Printer size={16} />} label={t("calc.printSetup")} onClick={() => setPrintDialog(true)} />
            </RibbonGroup>
          </>
        ) : null}

        {ribbon === "view" ? (
          <>
            <RibbonGroup label={t("calc.view")}>
              <ToolButton icon={<Snowflake size={16} />} label={t("calc.freezePanes")} onClick={toggleFreeze} active={sheet.freezeRows > 0 || sheet.freezeCols > 0} />
              <ToolButton label={sheet.showGridlines ? t("calc.hideGridlines") : t("calc.showGridlines")} onClick={() => updateSheet((current) => ({ ...current, showGridlines: !current.showGridlines }))} />
            </RibbonGroup>
            <RibbonGroup label={t("calc.sheets")}>
              <ToolButton icon={<Plus size={16} />} label={t("calc.addSheet")} onClick={addSheet} />
            </RibbonGroup>
          </>
        ) : null}

        <div className="ribbon-spacer" />
        <RibbonGroup>
          <ToolButton icon={<FolderOpen size={16} />} label={t("common.open")} onClick={() => void openIntoWorkspace()} />
          <ToolButton icon={<Save size={16} />} label={t("common.save")} onClick={() => void session.save()} disabled={session.busy} />
          <ToolButton label={t("common.saveAs")} onClick={() => void session.saveAs()} disabled={session.busy} />
          <ToolButton icon={<Printer size={16} />} label={t("common.print")} onClick={() => window.print()} />
        </RibbonGroup>
      </Ribbon>

      <div className="calc-formula-bar">
        <input className="name-box" value={nameBox} onChange={(event) => {
          const parts = parseRange(event.target.value.trim());
          if (parts) setSelection({ anchor: parts.start, focus: parts.end });
        }} />
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
      </div>

      {trace || selectedError ? (
        <div className="calc-audit-banner" style={{ display: "flex", alignItems: "center", gap: 8, padding: "4px 10px", borderBottom: "1px solid var(--border)", background: "var(--surface-2)", fontSize: 12 }}>
          {trace ? (
            <>
              <strong>{trace.kind === "precedents" ? t("calc.tracePrecedents") : t("calc.traceDependents")}</strong>
              <span className="muted" style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                {trace.cells.map((node) => (node.sheet === sheet.name ? node.address : `${node.sheet}!${node.address}`)).join(", ")}
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
          ref={gridRef}
          onMouseDown={() => gridRef.current?.focus()}
          onKeyDown={handleKeyDown}
          onScroll={(event) => setScroll((current) => ({ ...current, top: (event.target as HTMLDivElement).scrollTop, left: (event.target as HTMLDivElement).scrollLeft }))}
          style={{ width: "100%", height: "100%" }}
        >
          <div className="calc-canvas" style={{ width: HEADER_WIDTH + totalWidth, height: ROW_HEIGHT * (sheet.rowCount + 1) }}>
            <div className="calc-col-headers" style={{ transform: `translate(${HEADER_WIDTH - scroll.left}px, 0)` }}>
              {visible.columns.map(({ col, x }) => (
                <div
                  key={col}
                  className={`calc-col-header${selection.focus.col === col ? " is-active" : ""}`}
                  style={{ left: x, width: sheet.colWidths[String(col)] ?? DEFAULT_COL_WIDTH }}
                  onMouseDown={() => setSelection({ anchor: { row: 0, col }, focus: { row: sheet.rowCount - 1, col } })}
                  onContextMenu={(event) => {
                    event.preventDefault();
                    insertColumn(sheet, col, updateSheet);
                  }}
                >
                  {columnLabel(col)}
                  <span
                    className="col-resize"
                    onMouseDown={(event) => {
                      event.stopPropagation();
                      startColumnResize(event, col, sheet, updateSheet);
                    }}
                  />
                </div>
              ))}
            </div>
            <div className="calc-row-headers" style={{ transform: `translate(0, ${ROW_HEIGHT - scroll.top}px)` }}>
              {visible.rows.map((row) => (
                <div
                  key={row}
                  className={`calc-row-header${selection.focus.row === row ? " is-active" : ""}`}
                  style={{ top: row * ROW_HEIGHT, height: sheet.rowHeights[String(row)] ?? ROW_HEIGHT }}
                  onMouseDown={() => setSelection({ anchor: { row, col: 0 }, focus: { row, col: sheet.colCount - 1 } })}
                >
                  {row + 1}
                </div>
              ))}
            </div>
            <div className="calc-cells" style={{ transform: `translate(${HEADER_WIDTH - scroll.left}px, ${ROW_HEIGHT - scroll.top}px)`, width: totalWidth, height: ROW_HEIGHT * sheet.rowCount }}>
              {visible.rows.map((row) =>
                visible.columns.map(({ col, x }) => {
                  const address = formatAddress(row, col);
                  const value = computed.get(address) ?? "";
                  const cell = sheet.cells[address];
                  const width = sheet.colWidths[String(col)] ?? DEFAULT_COL_WIDTH;
                  const isEditing = editing?.row === row && editing?.col === col;
                  const inSelection = row >= selectionBounds.start.row && row <= selectionBounds.end.row && col >= selectionBounds.start.col && col <= selectionBounds.end.col;
                  const fill = conditionalFills.get(address);
                  const style = cell?.style ?? defaultCellStyle();
                  const validation = sheet.validations.find((rule) => addressesInRange(rule.range, 100).includes(address));
                  const invalid = validation ? !isValid(validation, value) : false;
                  // The structured table (if any) that owns this cell decides
                  // header/banding/outline; the cell's own formatting still wins.
                  const tableEntry = sheetTables.find(
                    ({ parts }) => row >= parts.start.row && row <= parts.end.row && col >= parts.start.col && col <= parts.end.col,
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
                  return (
                    <div
                      key={address}
                      className={`calc-cell${inSelection ? " is-selected" : ""}${invalid ? " is-invalid" : ""}`}
                      style={{
                        left: x,
                        top: row * ROW_HEIGHT,
                        width,
                        height: sheet.rowHeights[String(row)] ?? ROW_HEIGHT,
                        background: fill ?? tableFill ?? style.fill ?? undefined,
                        fontWeight: style.bold || (tableHeader && tableEntry!.table.headerBold) ? 700 : undefined,
                        fontStyle: style.italic ? "italic" : undefined,
                        textDecoration: [style.underline ? "underline" : "", style.strike ? "line-through" : ""].filter(Boolean).join(" ") || undefined,
                        color: style.color ?? (tableFill && tableHeader ? "#ffffff" : undefined),
                        textAlign: (style.align === "general" ? (typeof value === "number" ? "right" : "left") : style.align) as "left" | "right" | "center",
                        justifyContent: style.align === "center" ? "center" : style.align === "right" || (style.align === "general" && typeof value === "number") ? "flex-end" : "flex-start",
                      }}
                      onMouseDown={(event) => {
                        gridRef.current?.focus();
                        if (event.shiftKey) setSelection({ anchor: selection.anchor, focus: { row, col } });
                        else setSelection({ anchor: { row, col }, focus: { row, col } });
                      }}
                      onDoubleClick={() => setEditing({ row, col, value: cell?.formula ?? cellText(cell) })}
                    >
                      {isEditing ? (
                        <input
                          className="cell-editor"
                          ref={cellInputRef}
                          value={editing!.value}
                          autoFocus
                          onFocus={(event) => {
                            setFocusMode("cell");
                            setSuggestDismissed(false);
                            setDraftCaret(event.currentTarget.selectionStart ?? event.currentTarget.value.length);
                          }}
                          onSelect={(event) => setDraftCaret(event.currentTarget.selectionStart ?? event.currentTarget.value.length)}
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
            </div>
            {sheet.charts.map((chart) => {
              const position = parseAddress(chart.anchor) ?? { row: 0, col: 0 };
              return (
                <ChartBox
                  key={chart.id}
                  chart={chart}
                  sheet={sheet}
                  workbook={workbook}
                  x={columnX(position.col)}
                  y={position.row * ROW_HEIGHT + ROW_HEIGHT - scroll.top}
                  onRemove={() => updateSheet((current) => ({ ...current, charts: current.charts.filter((candidate) => candidate.id !== chart.id) }))}
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
                  y={position.row * ROW_HEIGHT + ROW_HEIGHT - scroll.top}
                  onRemove={() => updateSheet((current) => ({ ...current, pivotTables: (current.pivotTables ?? []).filter((candidate) => candidate.id !== pivot.id) }))}
                />
              );
            })}
          </div>
        </div>
      </div>

      <div className="calc-sheet-tabs">
        <button type="button" className="icon-btn" onClick={addSheet} title={t("calc.addSheet")}>
          <Plus size={14} />
        </button>
        {workbook.sheets.map((candidate, index) => (
          <div key={candidate.id} className={`sheet-tab${index === sheetIndex ? " is-active" : ""}`} onClick={() => { setSheetIndex(index); setSelection({ anchor: { row: 0, col: 0 }, focus: { row: 0, col: 0 } }); }}>
            <span onDoubleClick={() => renameSheet(index)}>{candidate.name}</span>
            {workbook.sheets.length > 1 ? (
              <button
                type="button"
                className="icon-btn"
                onClick={(event) => {
                  event.stopPropagation();
                  removeSheet(index);
                }}
                title={t("common.delete")}
              >
                <Trash2 size={11} />
              </button>
            ) : null}
          </div>
        ))}
        <span className="spacer" />
        <span className="muted">
          {tab.path ?? t("writer.unsaved")} {tab.dirty ? "•" : ""}
        </span>
      </div>

      <div className="editor-status">
        <span>{nameBox}</span>
        <span>{activeCell?.formula ? t("calc.formula") : formatCellDisplay(computed.get(formatAddress(selection.focus.row, selection.focus.col)) ?? "", activeCell?.style ?? defaultCellStyle())}</span>
        <span className="spacer" />
        <span>
          {sheet.rowCount} × {sheet.colCount}
        </span>
      </div>

      {chartDialog ? (
        <Dialog title={t("calc.chart")} onClose={() => setChartDialog(false)}>
          <div className="chart-kind-grid">
            {["column", "bar", "line", "pie", "area"].map((kind) => (
              <button key={kind} type="button" className="btn btn-soft" onClick={() => addChart(kind)}>
                {t(`calc.chart_${kind}`)}
              </button>
            ))}
          </div>
          <p className="muted">{t("calc.chartHint")}</p>
        </Dialog>
      ) : null}

      {pivotDialog ? (
        <PivotDialog workbook={workbook} sheet={sheet} onClose={() => setPivotDialog(false)} onApply={addPivot} />
      ) : null}

      {conditionalDialog ? (
        <ConditionalDialog onClose={() => setConditionalDialog(false)} onApply={addConditional} />
      ) : null}

      {validationDialog ? (
        <ValidationDialog onClose={() => setValidationDialog(false)} onApply={addValidation} />
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
        <Dialog title={filterOpen.tableName ? `${t("calc.filter")} · ${filterOpen.tableName}` : t("calc.filter")} onClose={() => setFilterOpen(null)}>
          <div className="stack filter-list">
            {filterOpen.values.map((entry, index) => (
              <label key={entry.value} className="check">
                <input
                  type="checkbox"
                  checked={entry.checked}
                  onChange={(event) =>
                    setFilterOpen((current) =>
                      current
                        ? { ...current, values: current.values.map((candidate, position) => (position === index ? { ...candidate, checked: event.target.checked } : candidate)) }
                        : current,
                    )
                  }
                />
                {entry.value || t("calc.filterBlank")}
              </label>
            ))}
          </div>
          <div className="row">
            <button type="button" className="btn btn-soft" onClick={() => setFilterOpen((current) => (current ? { ...current, values: current.values.map((entry) => ({ ...entry, checked: true })) } : current))}>
              {t("calc.selectAll")}
            </button>
            <button type="button" className="btn btn-primary" onClick={applyFilter}>
              {t("calc.applyFilter")}
            </button>
            <button
              type="button"
              className="btn btn-soft"
              onClick={() => {
                const tableId = filterOpen.tableId;
                updateSheet((current) =>
                  tableId
                    ? { ...current, rowHeights: {}, tables: (current.tables ?? []).map((table) => (table.id === tableId ? { ...table, filter: null } : table)) }
                    : { ...current, rowHeights: {}, filter: null },
                );
                setFilterOpen(null);
              }}
            >
              {t("calc.clearFilter")}
            </button>
          </div>
        </Dialog>
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
          onToggleBanded={(table) => patchTable(table.id, (current) => ({ ...current, bandedRows: !current.bandedRows }))}
          onAddColumn={addCalculatedColumn}
          onFilter={(table, column) => openTableFilter(table, column)}
        />
      ) : null}

      {assistAnchor && focusMode !== null && (suggestions || argumentHint) ? (
        <div className="calc-assist" style={{ position: "fixed", left: assistAnchor.left, top: assistAnchor.top, zIndex: 60, display: "flex", flexDirection: "column", gap: 4, maxWidth: 460 }}>
          {argumentHint ? (
            <div
              style={{
                background: "var(--surface)",
                border: "1px solid var(--border)",
                borderRadius: 6,
                boxShadow: "var(--shadow)",
                padding: "4px 8px",
                fontFamily: "Consolas, monospace",
                fontSize: 11.5,
                display: "flex",
                gap: 2,
                flexWrap: "wrap",
              }}
            >
              <strong>{argumentHint.name}</strong>
              <span>(</span>
              {argumentHint.parts.map((part, index) => (
                <span key={`${index}:${part}`}>
                  {index > 0 ? <span>, </span> : null}
                  <span
                    style={
                      index === argumentHint.active
                        ? { background: "var(--accent-weak)", color: "var(--accent-text)", borderRadius: 3, padding: "0 3px", fontWeight: 600 }
                        : undefined
                    }
                  >
                    {part}
                  </span>
                </span>
              ))}
              <span>)</span>
            </div>
          ) : null}
          {suggestions && suggestions.items.length > 0 ? (
            <div
              role="listbox"
              aria-label={t("calc.suggestions")}
              style={{ background: "var(--surface)", border: "1px solid var(--border)", borderRadius: 6, boxShadow: "var(--shadow)", maxHeight: 220, overflowY: "auto" }}
            >
              {suggestions.items.map((item, index) => (
                <div
                  key={`${item.kind}:${item.label}`}
                  role="option"
                  aria-selected={index === suggestIndex}
                  onMouseDown={(event) => event.preventDefault()}
                  onClick={() => applySuggestion(item)}
                  onMouseEnter={() => setSuggestIndex(index)}
                  style={{
                    display: "flex",
                    alignItems: "baseline",
                    gap: 8,
                    padding: "3px 8px",
                    cursor: "pointer",
                    background: index === suggestIndex ? "var(--accent-weak)" : undefined,
                    color: index === suggestIndex ? "var(--accent-text)" : undefined,
                    fontSize: 12,
                  }}
                >
                  <span style={{ fontWeight: 600, fontFamily: "Consolas, monospace" }}>{item.label}</span>
                  <span className="muted small" style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                    {item.detail}
                  </span>
                </div>
              ))}
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Derived data
// ---------------------------------------------------------------------------

/** The highlight one rule paints on one cell, or null when it does not match. */
function ruleFill(rule: CondRule, value: Scalar, number: number, counts: Map<string, number>, threshold: number): string | null {
  const [first, second] = rule.values;
  switch (rule.kind) {
    case "greater":
      return Number.isFinite(number) && number > Number(first) ? rule.fill ?? "#FEE2E2" : null;
    case "less":
      return Number.isFinite(number) && number < Number(first) ? rule.fill ?? "#FEE2E2" : null;
    case "between":
      return Number.isFinite(number) && number >= Number(first) && number <= Number(second) ? rule.fill ?? "#FEF3C7" : null;
    case "equal":
      return String(value) === String(first) ? rule.fill ?? "#DBEAFE" : null;
    case "textContains":
      return String(value).toLowerCase().includes(String(first).toLowerCase()) ? rule.fill ?? "#E0E7FF" : null;
    case "duplicate":
      return String(value ?? "") !== "" && (counts.get(String(value ?? "")) ?? 0) > 1 ? rule.fill ?? "#FECACA" : null;
    case "top":
      return Number.isFinite(number) && number >= threshold ? rule.fill ?? "#BBF7D0" : null;
    default:
      return null;
  }
}

function isValid(rule: { kind: string; values: string[]; min: number | null; max: number | null }, value: Scalar): boolean {
  if (value === "" || value === undefined) return true;
  if (rule.kind === "list") return rule.values.map((entry) => entry.trim().toLowerCase()).includes(String(value).trim().toLowerCase());
  const number = Number(value);
  if (!Number.isFinite(number)) return rule.kind !== "number";
  if (rule.kind === "number") {
    if (rule.min !== null && number < rule.min) return false;
    if (rule.max !== null && number > rule.max) return false;
  }
  return true;
}

function insertFunction(name: string) {
  const active = window.document.activeElement as HTMLElement | null;
  const input = window.document.querySelector<HTMLInputElement>(".formula-input");
  if (input) {
    input.focus();
    const start = input.selectionStart ?? input.value.length;
    input.value = `${input.value.slice(0, start)}=${name}(`;
    input.dispatchEvent(new Event("input", { bubbles: true }));
  }
  void active;
}

function insertRow(_sheet: Sheet, row: number, updateSheet: (mutate: (sheet: Sheet) => Sheet) => void) {
  updateSheet((current) => {
    const cells: Sheet["cells"] = {};
    for (const [address, cell] of Object.entries(current.cells)) {
      const position = parseAddress(address);
      if (!position) continue;
      cells[formatAddress(position.row >= row ? position.row + 1 : position.row, position.col)] = cell;
    }
    return { ...current, cells, rowCount: current.rowCount + 1 };
  });
}

function deleteRow(_sheet: Sheet, row: number, updateSheet: (mutate: (sheet: Sheet) => Sheet) => void) {
  updateSheet((current) => {
    const cells: Sheet["cells"] = {};
    for (const [address, cell] of Object.entries(current.cells)) {
      const position = parseAddress(address);
      if (!position || position.row === row) continue;
      cells[formatAddress(position.row > row ? position.row - 1 : position.row, position.col)] = cell;
    }
    return { ...current, cells, rowCount: Math.max(10, current.rowCount - 1) };
  });
}

function insertColumn(_sheet: Sheet, col: number, updateSheet: (mutate: (sheet: Sheet) => Sheet) => void) {
  updateSheet((current) => {
    const cells: Sheet["cells"] = {};
    for (const [address, cell] of Object.entries(current.cells)) {
      const position = parseAddress(address);
      if (!position) continue;
      cells[formatAddress(position.row, position.col >= col ? position.col + 1 : position.col)] = cell;
    }
    return { ...current, cells, colCount: current.colCount + 1 };
  });
}

function deleteColumn(_sheet: Sheet, col: number, updateSheet: (mutate: (sheet: Sheet) => Sheet) => void) {
  updateSheet((current) => {
    const cells: Sheet["cells"] = {};
    for (const [address, cell] of Object.entries(current.cells)) {
      const position = parseAddress(address);
      if (!position || position.col === col) continue;
      cells[formatAddress(position.row, position.col > col ? position.col - 1 : position.col)] = cell;
    }
    return { ...current, cells, colCount: Math.max(5, current.colCount - 1) };
  });
}

function toggleMerge(_sheet: Sheet, selection: Selection, updateSheet: (mutate: (sheet: Sheet) => Sheet) => void) {
  const start = formatAddress(Math.min(selection.anchor.row, selection.focus.row), Math.min(selection.anchor.col, selection.focus.col));
  const end = formatAddress(Math.max(selection.anchor.row, selection.focus.row), Math.max(selection.anchor.col, selection.focus.col));
  updateSheet((current) => {
    const existing = current.merges.findIndex((merge) => merge.start === start && merge.end === end);
    if (existing >= 0) return { ...current, merges: current.merges.filter((_, index) => index !== existing) };
    return { ...current, merges: [...current.merges, { start, end }] };
  });
}

function startColumnResize(event: React.MouseEvent, col: number, sheet: Sheet, updateSheet: (mutate: (sheet: Sheet) => Sheet) => void) {
  const startX = event.clientX;
  const startWidth = sheet.colWidths[String(col)] ?? DEFAULT_COL_WIDTH;
  const onMove = (move: MouseEvent) => {
    const width = Math.max(32, startWidth + (move.clientX - startX));
    updateSheet((current) => ({ ...current, colWidths: { ...current.colWidths, [col]: width } }));
  };
  const onUp = () => {
    window.removeEventListener("mousemove", onMove);
    window.removeEventListener("mouseup", onUp);
  };
  window.addEventListener("mousemove", onMove);
  window.addEventListener("mouseup", onUp);
}

// ---------------------------------------------------------------------------
// Charts
// ---------------------------------------------------------------------------

function ChartBox({
  chart,
  sheet,
  workbook,
  x,
  y,
  onRemove,
}: {
  chart: { id: string; chart: ChartData; anchor: string; widthPx: number; heightPx: number };
  sheet: Sheet;
  workbook: Workbook;
  x: number;
  y: number;
  onRemove: () => void;
}) {
  const values = computeSheetValues(workbook, sheet);
  const categories = addressesInRange(chart.chart.categories, 5000).map((address) => String(values.get(address) ?? ""));
  const series = chart.chart.series.map((entry) => ({
    name: entry.name,
    values: addressesInRange(entry.range, 5000).map((address) => Number(values.get(address) ?? 0)),
    color: entry.color,
  }));
  const palette = ["#2563eb", "#059669", "#d97706", "#dc2626", "#7c3aed", "#0891b2"];
  const all = series.flatMap((entry) => entry.values).filter(Number.isFinite);
  const max = Math.max(1, ...all);
  const min = Math.min(0, ...all);
  const width = chart.widthPx;
  const height = chart.heightPx;
  const plotWidth = width - 48;
  const plotHeight = height - 56;
  const count = Math.max(1, categories.length);

  const pointsFor = (values2: number[]) =>
    values2
      .map((value, index) => {
        const px = 40 + (count === 1 ? plotWidth / 2 : (index / (count - 1)) * plotWidth);
        const py = 34 + plotHeight - ((value - min) / (max - min || 1)) * plotHeight;
        return `${px},${py}`;
      })
      .join(" ");

  return (
    <div className="chart-box" style={{ left: x, top: y, width, height }}>
      <div className="chart-head">
        <strong>{chart.chart.title}</strong>
        <button type="button" className="icon-btn" onClick={onRemove} title="Delete chart">
          <Trash2 size={12} />
        </button>
      </div>
      <svg width={width} height={height - 26} viewBox={`0 0 ${width} ${height - 26}`}>
        <line x1={40} y1={height - 22} x2={width - 8} y2={height - 22} stroke="#cbd5e1" />
        <line x1={40} y1={34} x2={40} y2={height - 22} stroke="#cbd5e1" />
        {chart.chart.kind === "pie"
          ? pieSlices(series[0]?.values ?? [], palette).map((slice, index) => (
              <path key={index} d={slice.path} fill={slice.color} opacity={0.85} />
            ))
          : null}
        {chart.chart.kind === "column" || chart.chart.kind === "bar"
          ? series.map((entry, seriesIndex) =>
              entry.values.map((value, index) => {
                const bandWidth = plotWidth / count;
                const barWidth = Math.max(2, (bandWidth * 0.7) / series.length);
                const px = 40 + index * bandWidth + bandWidth * 0.15 + seriesIndex * barWidth;
                const py = 34 + plotHeight - ((value - min) / (max - min || 1)) * plotHeight;
                return <rect key={`${seriesIndex}-${index}`} x={chart.chart.kind === "bar" ? py : px} y={chart.chart.kind === "bar" ? 34 + index * bandWidth : py} width={chart.chart.kind === "bar" ? 40 + plotHeight - py : barWidth} height={chart.chart.kind === "bar" ? barWidth : 34 + plotHeight - py} fill={entry.color ?? palette[seriesIndex % palette.length]} opacity={0.85} />;
              }),
            )
          : null}
        {chart.chart.kind === "line" || chart.chart.kind === "area"
          ? series.map((entry, index) => (
              <g key={index}>
                {chart.chart.kind === "area" ? <polygon points={`40,${34 + plotHeight} ${pointsFor(entry.values)} ${40 + plotWidth},${34 + plotHeight}`} fill={entry.color ?? palette[index % palette.length]} opacity={0.25} /> : null}
                <polyline points={pointsFor(entry.values)} fill="none" stroke={entry.color ?? palette[index % palette.length]} strokeWidth={2} />
              </g>
            ))
          : null}
        {categories.map((label, index) => (
          <text key={index} x={40 + (index + 0.5) * (plotWidth / count)} y={height - 8} fontSize={9} textAnchor="middle" fill="#64748b">
            {label.length > 8 ? `${label.slice(0, 7)}…` : label}
          </text>
        ))}
      </svg>
      {chart.chart.legend ? (
        <div className="chart-legend">
          {series.map((entry, index) => (
            <span key={index}>
              <i style={{ background: entry.color ?? palette[index % palette.length] }} />
              {entry.name}
            </span>
          ))}
        </div>
      ) : null}
    </div>
  );
}

function pieSlices(values: number[], palette: string[]): Array<{ path: string; color: string }> {
  const total = values.reduce((sum, value) => sum + Math.max(0, value), 0) || 1;
  let angle = -Math.PI / 2;
  const radius = 60;
  const cx = 110;
  const cy = 90;
  return values.map((value, index) => {
    const sweep = (Math.max(0, value) / total) * Math.PI * 2;
    const x1 = cx + radius * Math.cos(angle);
    const y1 = cy + radius * Math.sin(angle);
    angle += sweep;
    const x2 = cx + radius * Math.cos(angle);
    const y2 = cy + radius * Math.sin(angle);
    const large = sweep > Math.PI ? 1 : 0;
    return { path: `M ${cx} ${cy} L ${x1} ${y1} A ${radius} ${radius} 0 ${large} 1 ${x2} ${y2} Z`, color: palette[index % palette.length] };
  });
}

// ---------------------------------------------------------------------------
// Dialogs
// ---------------------------------------------------------------------------

/** The live pivot grid, rendered over the sheet at the pivot's anchor. */
function PivotBox({
  pivot,
  sheet,
  workbook,
  x,
  y,
  onRemove,
}: {
  pivot: PivotTable;
  sheet: Sheet;
  workbook: Workbook;
  x: number;
  y: number;
  onRemove: () => void;
}) {
  const t = useT();
  const [refreshToken, setRefreshToken] = useState(0);
  // Recomputing on every relevant render keeps the pivot live; the refresh
  // button is for an explicit "show me the current data" action and bumps a
  // token so the memo is invalidated even when nothing else changed.
  const result = useMemo(() => computePivot(workbook, pivot), [workbook, pivot, refreshToken, sheet]);
  return (
    <div className="pivot-box" style={{ left: x, top: y }}>
      <div className="chart-head">
        <strong>{pivot.name}</strong>
        <span className="spacer" />
        <button type="button" className="icon-btn" onClick={() => setRefreshToken((value) => value + 1)} title={t("calc.pivotRefresh")}>
          <RefreshCw size={12} />
        </button>
        <button type="button" className="icon-btn" onClick={onRemove} title={t("common.delete")}>
          <Trash2 size={12} />
        </button>
      </div>
      {result ? (
        <table className="pivot-grid">
          <tbody>
            {result.grid.map((line, rowIndex) => (
              <tr key={rowIndex}>
                {line.map((value, colIndex) => (
                  <td key={colIndex} className={colIndex < result.rowFieldCount || rowIndex === 0 ? "is-label" : undefined}>
                    {isError(value) ? value.code : value === "" ? "" : String(value)}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      ) : (
        <p className="muted" style={{ padding: "6px 10px" }}>
          {t("calc.pivotNeedsData")}
        </p>
      )}
    </div>
  );
}

/** Configure a pivot over the sheet's used range. */
function PivotDialog({
  workbook,
  sheet,
  onClose,
  onApply,
}: {
  workbook: Workbook;
  sheet: Sheet;
  onClose: () => void;
  onApply: (config: { rows: string[]; columns: string[]; values: PivotValueField[]; filters: PivotTable["filters"] }) => void;
}) {
  const t = useT();
  const source = usedRange(sheet);
  const fields = pivotFields(workbook, sheet.name, source);
  const [row, setRow] = useState(fields[0] ?? "");
  const [column, setColumn] = useState("");
  const [value, setValue] = useState(fields[fields.length - 1] ?? fields[0] ?? "");
  const [aggregation, setAggregation] = useState<PivotValueField["aggregation"]>("sum");
  return (
    <Dialog title={t("calc.pivotTable")} onClose={onClose}>
      <div className="stack">
        <p className="muted">
          {t("calc.pivotHint")} · {source}
        </p>
        <label className="field">
          <span>{t("calc.pivotRows")}</span>
          <select value={row} onChange={(event) => setRow(event.target.value)}>
            {fields.map((field) => (
              <option key={field} value={field}>
                {field}
              </option>
            ))}
          </select>
        </label>
        <label className="field">
          <span>{t("calc.pivotColumns")}</span>
          <select value={column} onChange={(event) => setColumn(event.target.value)}>
            <option value="">—</option>
            {fields.map((field) => (
              <option key={field} value={field}>
                {field}
              </option>
            ))}
          </select>
        </label>
        <label className="field">
          <span>{t("calc.pivotValues")}</span>
          <select value={value} onChange={(event) => setValue(event.target.value)}>
            {fields.map((field) => (
              <option key={field} value={field}>
                {field}
              </option>
            ))}
          </select>
        </label>
        <label className="field">
          <span>{t("calc.pivotAggregation")}</span>
          <select value={aggregation} onChange={(event) => setAggregation(event.target.value as PivotValueField["aggregation"])}>
            {(["sum", "count", "average", "min", "max"] as const).map((kind) => (
              <option key={kind} value={kind}>
                {t(`calc.agg_${kind}`)}
              </option>
            ))}
          </select>
        </label>
        <button
          type="button"
          className="btn btn-primary"
          disabled={fields.length < 2}
          onClick={() => onApply({ rows: row ? [row] : [], columns: column ? [column] : [], values: value ? [{ field: value, aggregation }] : [], filters: [] })}
        >
          {t("calc.pivotInsert")}
        </button>
      </div>
    </Dialog>
  );
}

function ConditionalDialog({ onClose, onApply }: { onClose: () => void; onApply: (rule: { kind: string; values: string[]; fill: string; topN?: number }) => void }) {
  const t = useT();
  const [kind, setKind] = useState("greater");
  const [first, setFirst] = useState("100");
  const [second, setSecond] = useState("0");
  const [fill, setFill] = useState("#FEE2E2");
  return (
    <Dialog title={t("calc.conditionalFormatting")} onClose={onClose}>
      <div className="stack">
        <label className="field">
          <span>{t("calc.rule")}</span>
          <select value={kind} onChange={(event) => { setKind(event.target.value); if (event.target.value === "dataBar") setFill("#638EC6"); }}>
            <option value="greater">{t("calc.ruleGreater")}</option>
            <option value="less">{t("calc.ruleLess")}</option>
            <option value="between">{t("calc.ruleBetween")}</option>
            <option value="equal">{t("calc.ruleEqual")}</option>
            <option value="textContains">{t("calc.ruleText")}</option>
            <option value="duplicate">{t("calc.ruleDuplicate")}</option>
            <option value="top">{t("calc.ruleTop")}</option>
            <option value="dataBar">{t("calc.ruleDataBar")}</option>
          </select>
        </label>
        {kind === "duplicate" || kind === "dataBar" ? null : (
          <div className="row">
            <label className="field">
              <span>{t("calc.value")}</span>
              <input value={first} onChange={(event) => setFirst(event.target.value)} />
            </label>
            {kind === "between" ? (
              <label className="field">
                <span>{t("calc.and")}</span>
                <input value={second} onChange={(event) => setSecond(event.target.value)} />
              </label>
            ) : null}
          </div>
        )}
        <label className="field">
          <span>{t("calc.fillColor")}</span>
          <input type="color" value={fill} onChange={(event) => setFill(event.target.value)} />
        </label>
        <button type="button" className="btn btn-primary" onClick={() => onApply({ kind, values: [first, second], fill, topN: kind === "top" ? Number(first) || 10 : undefined })}>
          {t("common.apply")}
        </button>
      </div>
    </Dialog>
  );
}

function ValidationDialog({ onClose, onApply }: { onClose: () => void; onApply: (validation: { kind: string; values: string[]; min: number | null; max: number | null; message: string }) => void }) {
  const t = useT();
  const [kind, setKind] = useState("list");
  const [list, setList] = useState("Open,In progress,Done");
  const [min, setMin] = useState("0");
  const [max, setMax] = useState("100");
  const [message, setMessage] = useState("");
  return (
    <Dialog title={t("calc.dataValidation")} onClose={onClose}>
      <div className="stack">
        <label className="field">
          <span>{t("calc.validationType")}</span>
          <select value={kind} onChange={(event) => setKind(event.target.value)}>
            <option value="list">{t("calc.validationList")}</option>
            <option value="number">{t("calc.validationNumber")}</option>
          </select>
        </label>
        {kind === "list" ? (
          <label className="field">
            <span>{t("calc.validationValues")}</span>
            <input value={list} onChange={(event) => setList(event.target.value)} />
          </label>
        ) : (
          <div className="row">
            <label className="field">
              <span>{t("calc.minimum")}</span>
              <input value={min} onChange={(event) => setMin(event.target.value)} />
            </label>
            <label className="field">
              <span>{t("calc.maximum")}</span>
              <input value={max} onChange={(event) => setMax(event.target.value)} />
            </label>
          </div>
        )}
        <label className="field">
          <span>{t("calc.validationMessage")}</span>
          <input value={message} onChange={(event) => setMessage(event.target.value)} />
        </label>
        <button type="button" className="btn btn-primary" onClick={() => onApply({ kind, values: list.split(",").map((entry) => entry.trim()), min: kind === "number" ? Number(min) : null, max: kind === "number" ? Number(max) : null, message })}>
          {t("common.apply")}
        </button>
      </div>
    </Dialog>
  );
}

/** Configures a new structured table over a cell range. */
function InsertTableDialog({
  defaultName,
  defaultRange,
  onClose,
  onApply,
}: {
  defaultName: string;
  defaultRange: string;
  onClose: () => void;
  onApply: (config: { name: string; range: string; hasHeaders: boolean; hasTotals: boolean; bandedRows: boolean }) => void;
}) {
  const t = useT();
  const [name, setName] = useState(defaultName);
  const [range, setRange] = useState(defaultRange);
  const [hasHeaders, setHasHeaders] = useState(true);
  const [hasTotals, setHasTotals] = useState(false);
  const [bandedRows, setBandedRows] = useState(true);
  const valid = name.trim() !== "" && parseRange(range) !== null;
  return (
    <Dialog title={t("calc.insertTable")} onClose={onClose}>
      <div className="stack">
        <label className="field">
          <span>{t("calc.tableName")}</span>
          <input className="input" value={name} onChange={(event) => setName(event.target.value)} />
        </label>
        <label className="field">
          <span>{t("calc.tableRange")}</span>
          <input className="input" value={range} onChange={(event) => setRange(event.target.value)} />
        </label>
        <label className="check">
          <input type="checkbox" checked={hasHeaders} onChange={(event) => setHasHeaders(event.target.checked)} />
          {t("calc.tableHeaders")}
        </label>
        <label className="check">
          <input type="checkbox" checked={hasTotals} onChange={(event) => setHasTotals(event.target.checked)} />
          {t("calc.tableTotals")}
        </label>
        <label className="check">
          <input type="checkbox" checked={bandedRows} onChange={(event) => setBandedRows(event.target.checked)} />
          {t("calc.tableBanded")}
        </label>
        <p className="muted small">=SUM(Name[Column])</p>
        <button type="button" className="btn btn-primary" disabled={!valid} onClick={() => onApply({ name, range, hasHeaders, hasTotals, bandedRows })}>
          {t("common.apply")}
        </button>
      </div>
    </Dialog>
  );
}

interface TablePanelDraft {
  name: string;
  formula: string;
  filter: string;
}

/**
 * Side panel listing the active sheet's structured tables.
 *
 * Selecting a table jumps to it; the panel renames, deletes, toggles the
 * totals/banding flags, appends a calculated column, and opens the shared
 * filter dialog scoped to one table column.
 */
function TablesPanel({
  tables,
  onClose,
  onInsert,
  onJump,
  onRename,
  onDelete,
  onToggleTotals,
  onToggleBanded,
  onAddColumn,
  onFilter,
}: {
  tables: SpreadsheetTable[];
  onClose: () => void;
  onInsert: () => void;
  onJump: (table: SpreadsheetTable) => void;
  onRename: (table: SpreadsheetTable) => void;
  onDelete: (table: SpreadsheetTable) => void;
  onToggleTotals: (table: SpreadsheetTable) => void;
  onToggleBanded: (table: SpreadsheetTable) => void;
  onAddColumn: (table: SpreadsheetTable, name: string, formula: string) => void;
  onFilter: (table: SpreadsheetTable, column: string) => void;
}) {
  const t = useT();
  const [drafts, setDrafts] = useState<Record<string, TablePanelDraft>>({});
  const draftFor = (table: SpreadsheetTable): TablePanelDraft => drafts[table.id] ?? { name: "", formula: "", filter: table.columns[0]?.name ?? "" };
  const patchDraft = (table: SpreadsheetTable, patch: Partial<TablePanelDraft>) => {
    setDrafts((current) => ({
      ...current,
      [table.id]: { ...(current[table.id] ?? { name: "", formula: "", filter: table.columns[0]?.name ?? "" }), ...patch },
    }));
  };

  return (
    <aside
      className="calc-tables-panel"
      style={{
        position: "fixed",
        top: 150,
        right: 14,
        width: 320,
        maxHeight: "62vh",
        overflowY: "auto",
        background: "var(--surface)",
        border: "1px solid var(--border)",
        borderRadius: 10,
        boxShadow: "var(--shadow)",
        padding: 10,
        zIndex: 30,
      }}
    >
      <div className="row" style={{ alignItems: "center" }}>
        <strong>{t("calc.tableList")}</strong>
        <span className="spacer" />
        <button type="button" className="btn btn-soft" onClick={onInsert}>
          {t("calc.insertTable")}
        </button>
        <button type="button" className="icon-btn" onClick={onClose} aria-label={t("common.close")}>
          ×
        </button>
      </div>
      {tables.length === 0 ? <p className="muted small">{t("calc.noTables")}</p> : null}
      <div className="stack">
        {tables.map((table) => {
          const draft = draftFor(table);
          return (
            <div key={table.id} className="stack" style={{ border: "1px solid var(--border)", borderRadius: 8, padding: 8 }}>
              <div className="row" style={{ alignItems: "center", gap: 6 }}>
                <button type="button" className="btn btn-soft" onClick={() => onJump(table)} title={t("calc.tableJump")}>
                  {table.name}
                </button>
                <span className="muted small">
                  {table.range} · {table.columns.length}
                </span>
                <span className="spacer" />
                <button type="button" className="icon-btn" onClick={() => onRename(table)} title={t("calc.tableRename")}>
                  <Tag size={13} />
                </button>
                <button type="button" className="icon-btn" onClick={() => onDelete(table)} title={t("common.delete")}>
                  <Trash2 size={13} />
                </button>
              </div>
              <div className="row wrap" style={{ gap: 10 }}>
                <label className="check">
                  <input type="checkbox" checked={table.hasTotals} onChange={() => onToggleTotals(table)} />
                  {t("calc.tableTotals")}
                </label>
                <label className="check">
                  <input type="checkbox" checked={table.bandedRows} onChange={() => onToggleBanded(table)} />
                  {t("calc.tableBanded")}
                </label>
              </div>
              <div className="row wrap" style={{ gap: 6, alignItems: "flex-end" }}>
                <label className="field" style={{ flex: 1 }}>
                  <span>{t("calc.tableFilter")}</span>
                  <select className="input" value={draft.filter} onChange={(event) => patchDraft(table, { filter: event.target.value })}>
                    {table.columns.map((column) => (
                      <option key={column.name} value={column.name}>
                        {column.name}
                      </option>
                    ))}
                  </select>
                </label>
                <button type="button" className="btn btn-soft" onClick={() => onFilter(table, draft.filter)} disabled={draft.filter === ""}>
                  {t("calc.filter")}
                </button>
              </div>
              <div className="row wrap" style={{ gap: 6, alignItems: "flex-end" }}>
                <label className="field" style={{ flex: 1 }}>
                  <span>{t("calc.tableNewColumn")}</span>
                  <input className="input" value={draft.name} onChange={(event) => patchDraft(table, { name: event.target.value })} />
                </label>
                <label className="field" style={{ flex: 1.4 }}>
                  <span>{t("calc.tableFormula")}</span>
                  <input
                    className="input"
                    placeholder={`=${table.name}[${table.columns[0]?.name ?? "Column"}]`}
                    value={draft.formula}
                    onChange={(event) => patchDraft(table, { formula: event.target.value })}
                  />
                </label>
                <button
                  type="button"
                  className="btn btn-primary"
                  disabled={draft.name.trim() === "" || draft.formula.trim() === ""}
                  onClick={() => {
                    onAddColumn(table, draft.name, draft.formula);
                    patchDraft(table, { name: "", formula: "" });
                  }}
                >
                  {t("common.add")}
                </button>
              </div>
            </div>
          );
        })}
      </div>
    </aside>
  );
}

/**
 * Workbook- and sheet-scoped name manager.
 *
 * A name points at a range, a cell, a constant or a formula; the "this sheet
 * only" checkbox decides whether other sheets can see it. The scope rules match
 * what the formula evaluator does, so what is listed here is what resolves.
 */
function NameManagerDialog({
  names,
  currentSheet,
  selection,
  onClose,
  onChange,
}: {
  names: NamedRange[];
  currentSheet: string;
  selection: string;
  onClose: () => void;
  onChange: (names: NamedRange[]) => void;
}) {
  const t = useT();
  const [entryName, setEntryName] = useState("");
  const [entryTarget, setEntryTarget] = useState(selection);
  const [entryScope, setEntryScope] = useState<"workbook" | "sheet">("workbook");

  const problem = entryName.trim() === "" ? t("calc.nameRequired") : !isValidDefinedName(entryName.trim()) ? t("calc.nameInvalid") : "";

  const save = () => {
    if (problem) return;
    const definition = entryTarget.trim();
    const next: NamedRange = {
      name: entryName.trim().toUpperCase(),
      definition,
      sheet: entryScope === "sheet" ? currentSheet : null,
    };
    // Replace an existing name with the same identifier and scope.
    const without = names.filter((entry) => !(entry.name === next.name && entry.sheet === next.sheet));
    onChange([...without, next]);
  };

  return (
    <Dialog title={t("calc.nameManager")} onClose={onClose} wide>
      <div className="stack">
        <div className="row wrap" style={{ gap: 8 }}>
          <input className="input" value={entryName} placeholder={t("calc.namePlaceholder")} onChange={(event) => setEntryName(event.target.value)} />
          <input className="input" value={entryTarget} placeholder={t("calc.nameTarget")} onChange={(event) => setEntryTarget(event.target.value)} />
          <label className="check">
            <input type="checkbox" checked={entryScope === "sheet"} onChange={(event) => setEntryScope(event.target.checked ? "sheet" : "workbook")} />
            {t("calc.nameThisSheetOnly")}
          </label>
          <button type="button" className="btn btn-primary" onClick={save} disabled={problem !== ""}>
            {t("common.add")}
          </button>
        </div>
        {problem ? <p className="muted small">{problem}</p> : null}

        <table className="data-table">
          <thead>
            <tr>
              <th>{t("calc.nameColumn")}</th>
              <th>{t("calc.nameTarget")}</th>
              <th>{t("calc.nameScope")}</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {names.length === 0 ? (
              <tr>
                <td colSpan={4} className="muted">
                  {t("calc.noNames")}
                </td>
              </tr>
            ) : null}
            {names.map((entry) => (
              <tr key={`${entry.sheet ?? ""}:${entry.name}`}>
                <td>
                  <input
                    className="input"
                    defaultValue={entry.name}
                    onBlur={(event) => {
                      const next = event.target.value.trim().toUpperCase();
                      if (!isValidDefinedName(next) || next === entry.name) return;
                      onChange(names.map((candidate) => (candidate === entry ? { ...candidate, name: next } : candidate)));
                    }}
                  />
                </td>
                <td>
                  <input
                    className="input"
                    defaultValue={entry.definition}
                    onBlur={(event) => onChange(names.map((candidate) => (candidate === entry ? { ...candidate, definition: event.target.value.trim() } : candidate)))}
                  />
                </td>
                <td className="muted">{entry.sheet ?? t("calc.nameWorkbookScope")}</td>
                <td>
                  <button
                    type="button"
                    className="icon-btn"
                    aria-label={t("common.remove")}
                    onClick={() => onChange(names.filter((candidate) => candidate !== entry))}
                  >
                    <Trash2 size={13} />
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </Dialog>
  );
}

/**
 * Excel's rule for a legal name: it must start with a letter or underscore,
 * may contain letters, digits, dots and underscores, and must not look like a
 * cell reference (otherwise the formula parser reads `A1` as a cell).
 */
export function isValidDefinedName(name: string): boolean {
  if (!/^[A-Za-z_][A-Za-z0-9_.]*$/.test(name)) return false;
  return parseAddress(name) === null;
}

/** Paper, orientation, scaling and header/footer for printing and PDF export. */
function PrintLayoutDialog({
  print,
  sheetName,
  onClose,
  onApply,
}: {
  print: PrintSettings;
  sheetName: string;
  onClose: () => void;
  onApply: (print: PrintSettings) => void;
}) {
  const t = useT();
  const [draft, setDraft] = useState<PrintSettings>({ ...print });
  const patch = (next: Partial<PrintSettings>) => setDraft((current) => ({ ...current, ...next }));

  return (
    <Dialog title={t("calc.printSetup")} onClose={onClose} wide>
      <div className="stack">
        <label className="field">
          <span>{t("calc.paperSize")}</span>
          <select className="input" value={draft.paperSize} onChange={(event) => patch({ paperSize: Number(event.target.value) })}>
            <option value={9}>A4</option>
            <option value={1}>Letter</option>
            <option value={5}>Legal</option>
            <option value={8}>A3</option>
            <option value={9}>A4</option>
            <option value={11}>A5</option>
          </select>
        </label>
        <label className="check">
          <input type="checkbox" checked={draft.landscape} onChange={(event) => patch({ landscape: event.target.checked })} />
          {t("calc.landscape")}
        </label>
        <label className="field">
          <span>{t("calc.scale")}</span>
          <input className="input" type="number" min={10} max={400} value={draft.scale} onChange={(event) => patch({ scale: Math.min(400, Math.max(10, Number(event.target.value) || 100)) })} />
        </label>
        <label className="field">
          <span>{t("calc.fitToWidth")}</span>
          <input className="input" type="number" min={0} max={10} value={draft.fitToWidth} onChange={(event) => patch({ fitToWidth: Math.max(0, Number(event.target.value) || 0) })} />
        </label>
        <label className="field">
          <span>{t("calc.printTitlesRows")}</span>
          <input
            className="input"
            placeholder="1:1"
            value={draft.printTitlesRows ?? ""}
            onChange={(event) => patch({ printTitlesRows: event.target.value.trim() === "" ? null : event.target.value.trim() })}
          />
        </label>
        <label className="check">
          <input type="checkbox" checked={draft.printGridlines} onChange={(event) => patch({ printGridlines: event.target.checked })} />
          {t("calc.printGridlines")}
        </label>
        <label className="check">
          <input type="checkbox" checked={draft.printHeadings} onChange={(event) => patch({ printHeadings: event.target.checked })} />
          {t("calc.printHeadings")}
        </label>
        <label className="check">
          <input type="checkbox" checked={draft.centerHorizontally} onChange={(event) => patch({ centerHorizontally: event.target.checked })} />
          {t("calc.centerHorizontally")}
        </label>
        <label className="field">
          <span>{t("calc.header")}</span>
          <input className="input" value={draft.header} onChange={(event) => patch({ header: event.target.value })} />
        </label>
        <p className="muted small">{t("calc.printSheetNote", { sheet: sheetName })}</p>
      </div>
      <div className="row" style={{ justifyContent: "flex-end", marginTop: 14 }}>
        <button type="button" className="btn btn-soft" onClick={onClose}>
          {t("common.cancel")}
        </button>
        <button type="button" className="btn btn-primary" onClick={() => onApply(draft)}>
          {t("common.apply")}
        </button>
      </div>
    </Dialog>
  );
}
