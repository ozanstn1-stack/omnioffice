/**
 * Cell notes (comments): reading one off a cell and the model operations behind
 * Insert note, Edit note, Delete note and Show/Hide note. Pure functions over the
 * workbook model; each returns a new workbook, so one call is one undo step.
 */
import { emptyCell, type Cell, type Workbook } from "../../lib/office-types";
import { parseAddress } from "./formula";
import { withCellAt } from "./cells";

/** The note of a cell as the editor shows it. */
export interface CellNote {
  text: string;
  /** Empty when the writer is unknown. */
  author: string;
  /** The note stays on screen instead of appearing on hover. */
  visible: boolean;
}

/** What the note editor collects. */
export type NoteDraft = CellNote;

export function noteOf(cell: Cell | undefined): CellNote | null {
  if (!cell?.comment) return null;
  return { text: cell.comment, author: cell.commentAuthor ?? "", visible: cell.commentVisible === true };
}

/** `Ada:` and the text on the next line; just the text when the author is unknown. */
export function noteTooltip(note: CellNote): string {
  return note.author ? `${note.author}:\n${note.text}` : note.text;
}

/**
 * Adds or replaces the note of a cell. A note whose text is blank is no note:
 * it deletes the existing one instead of storing an empty comment.
 */
export function setNote(workbook: Workbook, sheetIndex: number, address: string, draft: NoteDraft): Workbook {
  const sheet = workbook.sheets[sheetIndex];
  if (!sheet || !parseAddress(address)) return workbook;
  if (draft.text.trim() === "") return deleteNote(workbook, sheetIndex, address);
  const current = sheet.cells[address] ?? emptyCell();
  const author = draft.author.trim();
  return withCellAt(workbook, sheetIndex, address, {
    ...current,
    comment: draft.text,
    commentAuthor: author === "" ? null : author,
    commentVisible: draft.visible ? true : undefined,
  });
}

/** Removes the note of a cell; a cell that held nothing else disappears with it. */
export function deleteNote(workbook: Workbook, sheetIndex: number, address: string): Workbook {
  const current = workbook.sheets[sheetIndex]?.cells[address];
  if (!current || (!current.comment && !current.commentAuthor && !current.commentVisible)) return workbook;
  return withCellAt(workbook, sheetIndex, address, {
    ...current,
    comment: null,
    commentAuthor: null,
    commentVisible: undefined,
  });
}

/** Flips whether the note stays on screen; a cell without a note is left alone. */
export function toggleNoteVisible(workbook: Workbook, sheetIndex: number, address: string): Workbook {
  const current = workbook.sheets[sheetIndex]?.cells[address];
  if (!current?.comment) return workbook;
  return withCellAt(workbook, sheetIndex, address, {
    ...current,
    commentVisible: current.commentVisible ? undefined : true,
  });
}

/** The addresses of the notes that stay on screen. */
export function visibleNoteAddresses(cells: Record<string, Cell>): string[] {
  const out: string[] = [];
  for (const [address, cell] of Object.entries(cells)) {
    if (cell.comment && cell.commentVisible) out.push(address);
  }
  return out;
}
