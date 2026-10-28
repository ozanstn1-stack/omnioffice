import { useLayoutEffect, useMemo, useState } from "react";
import type { Sheet, Workbook } from "../../../lib/office-types";
import type { Translate } from "../../../lib/i18n";
import { argumentHintFor, buildSuggestions, type CatalogueEntry, type FormulaSuggestion } from "../formula-assist";

/** The inline editor's cell and text. */
export interface DraftEditingCell {
  row: number;
  col: number;
  value: string;
}

export interface FormulaDraftOptions {
  /** The open inline editor, if any. */
  editing: DraftEditingCell | null;
  setEditing: (next: DraftEditingCell | null) => void;
  /** The editor state as of the latest event (see CalcEditor). */
  editingRef: React.MutableRefObject<DraftEditingCell | null>;
  cellInputRef: React.RefObject<HTMLInputElement | null>;
  formulaInputRef: React.RefObject<HTMLInputElement | null>;
  /** Where a suggestion insertion wants the caret; the editor applies it. */
  pendingCaretRef: React.MutableRefObject<number | null>;
  workbook: Workbook;
  sheet: Sheet;
  catalogue: Map<string, CatalogueEntry>;
  t: Translate;
}

/**
 * The formula bar draft and the formula assist for the Calc editor (#415,
 * slice 4): which input has the focus, its caret, the suggestion list with
 * its highlighted index and dismissal, the argument hint, and where the
 * popup sits. The code is the editor's, moved as it was.
 */
export function useFormulaDraft(options: FormulaDraftOptions) {
  const {
    editing,
    setEditing,
    editingRef,
    cellInputRef,
    formulaInputRef,
    pendingCaretRef,
    workbook,
    sheet,
    catalogue,
    t,
  } = options;
  const [formulaDraft, setFormulaDraft] = useState("");
  const [focusMode, setFocusMode] = useState<"cell" | "bar" | null>(null);
  const [draftCaret, setDraftCaret] = useState(0);
  const [suggestIndex, setSuggestIndex] = useState(0);
  const [suggestDismissed, setSuggestDismissed] = useState(false);
  const [assistAnchor, setAssistAnchor] = useState<{ left: number; top: number; above?: boolean } | null>(null);

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
  }, [focusMode, assistText, assistCaret, editing, cellInputRef, formulaInputRef]);

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

  return {
    formulaDraft,
    setFormulaDraft,
    focusMode,
    setFocusMode,
    setDraftCaret,
    suggestIndex,
    setSuggestIndex,
    setSuggestDismissed,
    assistAnchor,
    suggestions,
    argumentHint,
    applySuggestion,
    handleAssistKey,
  };
}
