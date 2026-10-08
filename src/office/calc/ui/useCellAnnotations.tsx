/**
 * Notes and hyperlinks on cells: the right-click menu, the note editor and
 * hover tooltip, the Insert link dialog, and following a link. Every change is
 * one `commit` (one undo step) and is refused on a protected sheet.
 */
import { useState, type KeyboardEvent, type MouseEvent, type PointerEvent, type ReactNode } from "react";
import { useT } from "../../../lib/i18n";
import { defaultCellStyle, type Cell, type Sheet, type Workbook } from "../../../lib/office-types";
import { useToasts } from "../../../lib/store";
import { formatCellDisplay, sheetFormulaEvaluator } from "../cells";
import { formatAddress, type Scalar } from "../formula";
import type { CellPosition } from "../grid-types";
import {
  cellLinkTarget,
  clearCellLink,
  hyperlinkFormulaArgument,
  isExternalLink,
  isLinkCell,
  resolvePlaceTarget,
  safeLinkTarget,
  setCellLink,
  type LinkDraft,
} from "../links";
import { deleteNote, noteOf, setNote, toggleNoteVisible, type NoteDraft } from "../notes";
import { openExternalLink } from "../open-link";
import { isSheetProtected } from "../protection";
import { HeaderMenu, type HeaderMenuItem } from "./HeaderMenu";
import { LinkDialog } from "./LinkDialog";
import { NoteEditor, NoteTooltip } from "./NoteViews";

interface Spot {
  row: number;
  col: number;
  /** Viewport pixels. */
  x: number;
  y: number;
}

export function useCellAnnotations(host: {
  workbook: Workbook;
  sheet: Sheet;
  sheetIndex: number;
  /** The cell the keyboard acts on. */
  focus: CellPosition;
  computed: ReadonlyMap<string, Scalar>;
  commit: (mutate: (workbook: Workbook) => Workbook) => void;
  /** Selects a cell of the current sheet. */
  select: (position: CellPosition) => void;
  /** Selects a cell, switching to its sheet first. */
  jumpTo: (sheetIndex: number, row: number, col: number) => void;
  /** Hands the keyboard back to the grid. */
  restoreFocus: () => void;
  /** Whether a cell is inside the current selection (a right-click there keeps it). */
  isSelected: (row: number, col: number) => boolean;
}) {
  const t = useT();
  const { workbook, sheet, sheetIndex, focus, computed, commit } = host;
  const [menu, setMenu] = useState<Spot | null>(null);
  const [noteEditor, setNoteEditor] = useState<Spot | null>(null);
  const [linkDialog, setLinkDialog] = useState<CellPosition | null>(null);
  const [hover, setHover] = useState<Spot | null>(null);
  const author = workbook.metadata?.author?.trim() ?? "";

  const refuseIfProtected = (): boolean => {
    if (!isSheetProtected(sheet)) return false;
    useToasts.getState().push({ kind: "info", title: t("calc.sheetProtected") });
    return true;
  };

  /** Where a popover for a cell opens: beside it, or near the middle of the window when it is not drawn. */
  const anchorOf = (row: number, col: number): Spot => {
    const element = document.querySelector<HTMLElement>(`[data-cell="${row}:${col}"]`);
    const rect = element?.getBoundingClientRect();
    return rect
      ? { row, col, x: rect.right + 4, y: rect.top }
      : { row, col, x: window.innerWidth / 3, y: window.innerHeight / 3 };
  };

  const insertNote = (position: CellPosition = focus) => {
    if (refuseIfProtected()) return;
    setHover(null);
    setNoteEditor(anchorOf(position.row, position.col));
  };

  const insertLink = (position: CellPosition = focus) => {
    if (refuseIfProtected()) return;
    setLinkDialog(position);
  };

  /** Opens the link of a cell. False when the cell has none, so a plain click can carry on. */
  const follow = async (row: number, col: number): Promise<boolean> => {
    const address = formatAddress(row, col);
    const cell = sheet.cells[address];
    const push = useToasts.getState().push;
    let target = cellLinkTarget(cell);
    if (!target) {
      const argument = hyperlinkFormulaArgument(cell?.formula);
      if (argument === null) return false;
      // HYPERLINK(location, name): the location is any expression, read as the cell would.
      const value = sheetFormulaEvaluator(workbook, sheet)(`=${argument}`, row, col);
      target = safeLinkTarget(typeof value === "string" || typeof value === "number" ? String(value) : "");
    }
    if (!target) {
      push({ kind: "info", title: t("calc.linkInvalid") });
      return true;
    }
    if (isExternalLink(target)) {
      if (!(await openExternalLink(target))) push({ kind: "error", title: t("calc.linkOpenFailed"), detail: target });
      return true;
    }
    const place = resolvePlaceTarget(workbook, sheetIndex, target);
    if (place) host.jumpTo(place.sheetIndex, place.row, place.col);
    else push({ kind: "info", title: t("calc.linkBroken") });
    return true;
  };

  /** Ctrl+K inserts or edits the link of the active cell, Shift+F2 its note. True when the key was used. */
  const handleKey = (event: KeyboardEvent): boolean => {
    const mod = event.ctrlKey || event.metaKey;
    if (mod && !event.altKey && event.code === "KeyK") insertLink();
    else if (event.shiftKey && !mod && event.key === "F2") insertNote();
    else return false;
    event.preventDefault();
    return true;
  };

  /** Ctrl+click on a link cell selects it and follows the link. True when it did, so the caller starts no drag. */
  const handleCtrlClick = (event: PointerEvent, position: CellPosition): boolean => {
    if (!(event.ctrlKey || event.metaKey) || event.shiftKey) return false;
    if (!isLinkCell(sheet.cells[formatAddress(position.row, position.col)])) return false;
    host.select(position);
    void follow(position.row, position.col);
    return true;
  };

  /** The tooltip of a link cell: its screen tip or its target, and how to follow it. */
  const linkTitle = (cell: Cell | undefined): string | undefined =>
    isLinkCell(cell)
      ? [cell?.linkTooltip || cellLinkTarget(cell), t("calc.linkHint")].filter(Boolean).join(" - ")
      : undefined;

  /** Right-click on a cell: select it (unless it is in the selection) and open its menu. */
  const openMenu = (event: MouseEvent, row: number, col: number) => {
    event.preventDefault();
    if (!host.isSelected(row, col)) host.select({ row, col });
    setHover(null);
    setMenu({ row, col, x: event.clientX, y: event.clientY });
  };

  const menuItems = ({ row, col }: Spot): HeaderMenuItem[] => {
    const cell = sheet.cells[formatAddress(row, col)];
    const address = formatAddress(row, col);
    const note = noteOf(cell);
    const stored = cellLinkTarget(cell);
    const items: HeaderMenuItem[] = [];
    if (isLinkCell(cell)) items.push({ id: "open", label: t("calc.linkOpen"), onSelect: () => void follow(row, col) });
    items.push({
      id: "link",
      label: t(stored ? "calc.linkEdit" : "calc.linkInsert"),
      onSelect: () => insertLink({ row, col }),
    });
    if (stored) {
      items.push({
        id: "unlink",
        label: t("calc.linkRemove"),
        onSelect: () => {
          if (!refuseIfProtected()) commit((current) => clearCellLink(current, sheetIndex, address));
        },
      });
    }
    items.push({
      id: "note",
      label: t(note ? "calc.noteEdit" : "calc.noteInsert"),
      onSelect: () => insertNote({ row, col }),
    });
    if (note) {
      items.push(
        {
          id: "note-visible",
          label: t(note.visible ? "calc.noteHide" : "calc.noteShow"),
          onSelect: () => {
            if (!refuseIfProtected()) commit((current) => toggleNoteVisible(current, sheetIndex, address));
          },
        },
        {
          id: "note-delete",
          label: t("calc.noteDelete"),
          onSelect: () => {
            if (!refuseIfProtected()) commit((current) => deleteNote(current, sheetIndex, address));
          },
        },
      );
    }
    return items;
  };

  /** Hover over the grid: the note of the cell under the pointer, unless it stays on screen anyway. */
  const onMouseOver = (event: MouseEvent) => {
    const element = (event.target as HTMLElement).closest?.<HTMLElement>("[data-note]");
    if (!element) {
      if (hover) setHover(null);
      return;
    }
    const row = Number(element.dataset.row);
    const col = Number(element.dataset.col);
    if (hover?.row === row && hover.col === col) return;
    const rect = element.getBoundingClientRect();
    setHover({ row, col, x: rect.right + 4, y: rect.top });
  };

  const editing = noteEditor ? formatAddress(noteEditor.row, noteEditor.col) : "";
  const hovered = hover ? noteOf(sheet.cells[formatAddress(hover.row, hover.col)]) : null;
  const linkAddress = linkDialog ? formatAddress(linkDialog.row, linkDialog.col) : "";
  const linkCell = linkDialog ? sheet.cells[linkAddress] : undefined;

  const elements: ReactNode = (
    <>
      {menu ? (
        <HeaderMenu
          label={t("calc.cellMenu")}
          x={menu.x}
          y={menu.y}
          items={menuItems(menu)}
          onClose={(reason) => {
            setMenu(null);
            if (reason !== "outside") host.restoreFocus();
          }}
        />
      ) : null}
      {noteEditor ? (
        <NoteEditor
          key={editing}
          address={editing}
          initial={noteOf(sheet.cells[editing])}
          defaultAuthor={author}
          x={noteEditor.x}
          y={noteEditor.y}
          onSave={(draft: NoteDraft) => commit((current) => setNote(current, sheetIndex, editing, draft))}
          onDelete={() => commit((current) => deleteNote(current, sheetIndex, editing))}
          onClose={() => {
            setNoteEditor(null);
            host.restoreFocus();
          }}
        />
      ) : null}
      {hovered && hover && !hovered.visible && !menu && !noteEditor ? (
        <NoteTooltip note={hovered} x={hover.x} y={hover.y} />
      ) : null}
      {linkDialog ? (
        <LinkDialog
          initial={{
            text: formatCellDisplay(computed.get(linkAddress) ?? "", linkCell?.style ?? defaultCellStyle()),
            target: cellLinkTarget(linkCell),
            tooltip: linkCell?.linkTooltip ?? "",
          }}
          sheetNames={workbook.sheets.map((candidate) => candidate.name)}
          currentSheet={sheet.name}
          names={(workbook.names ?? []).map((entry) => entry.name)}
          onSubmit={(draft: LinkDraft) => {
            commit((current) => setCellLink(current, sheetIndex, linkAddress, draft));
            setLinkDialog(null);
            host.restoreFocus();
          }}
          onRemove={() => {
            commit((current) => clearCellLink(current, sheetIndex, linkAddress));
            setLinkDialog(null);
            host.restoreFocus();
          }}
          onClose={() => {
            setLinkDialog(null);
            host.restoreFocus();
          }}
        />
      ) : null}
    </>
  );

  return {
    insertNote,
    insertLink,
    handleKey,
    handleCtrlClick,
    linkTitle,
    openMenu,
    hoverHandlers: { onMouseOver, onMouseLeave: () => setHover(null) },
    elements,
  };
}
