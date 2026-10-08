/** What a cell note looks like: the editor popover, the hover tooltip and the notes pinned on screen. */
import { useEffect, useMemo, useRef, useState } from "react";
import { useT } from "../../../lib/i18n";
import type { Cell } from "../../../lib/office-types";
import { parseAddress } from "../formula";
import { noteOf, visibleNoteAddresses, type CellNote, type NoteDraft } from "../notes";

/** Keeps a box of `width` x `height` inside the window, `margin` from its edges. */
function clampToWindow(x: number, y: number, width: number, height: number, margin = 8) {
  return {
    left: Math.max(margin, Math.min(x, window.innerWidth - width - margin)),
    top: Math.max(margin, Math.min(y, window.innerHeight - height - margin)),
  };
}

/**
 * The popover that inserts or edits a note. It saves with the button, with
 * Ctrl+Enter and when the pointer goes down outside it; Escape and Cancel throw
 * the changes away. A blank text saves as "no note".
 */
export function NoteEditor({
  address,
  initial,
  defaultAuthor,
  x,
  y,
  onSave,
  onDelete,
  onClose,
}: {
  address: string;
  /** The note being edited; null for a new one. */
  initial: CellNote | null;
  defaultAuthor: string;
  /** Where the popover opens, in viewport pixels. */
  x: number;
  y: number;
  onSave: (draft: NoteDraft) => void;
  onDelete: () => void;
  onClose: () => void;
}) {
  const t = useT();
  const [text, setText] = useState(initial?.text ?? "");
  const [author, setAuthor] = useState(initial ? initial.author : defaultAuthor);
  const [visible, setVisible] = useState(initial?.visible ?? false);
  const boxRef = useRef<HTMLDivElement>(null);
  const closedRef = useRef(false);

  const finish = (save: boolean) => {
    if (closedRef.current) return;
    closedRef.current = true;
    const unchanged =
      text === (initial?.text ?? "") &&
      author === (initial ? initial.author : defaultAuthor) &&
      visible === (initial?.visible ?? false);
    if (save && !unchanged) onSave({ text, author, visible });
    onClose();
  };

  // Subscribed again after every render, so the handler always sees the latest draft.
  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      if (!boxRef.current?.contains(event.target as Node)) finish(true);
    };
    window.addEventListener("pointerdown", onPointerDown, true);
    return () => window.removeEventListener("pointerdown", onPointerDown, true);
  });

  const place = clampToWindow(x, y, 300, 250);
  return (
    // eslint-disable-next-line jsx-a11y/no-noninteractive-element-interactions -- Escape cancels and Ctrl+Enter saves wherever the focus is inside the popover
    <div
      ref={boxRef}
      className="note-editor"
      role="dialog"
      aria-label={t("calc.noteTitle", { cell: address })}
      style={place}
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          event.stopPropagation();
          finish(false);
        } else if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
          event.preventDefault();
          finish(true);
        }
      }}
    >
      <strong>{t("calc.noteTitle", { cell: address })}</strong>
      <label className="field">
        <span>{t("calc.noteText")}</span>
        <textarea
          rows={5}
          value={text}
          // eslint-disable-next-line jsx-a11y/no-autofocus -- the popover opens to type the note
          autoFocus
          onChange={(event) => setText(event.target.value)}
        />
      </label>
      <label className="field">
        <span>{t("calc.noteAuthor")}</span>
        <input value={author} onChange={(event) => setAuthor(event.target.value)} />
      </label>
      <label className="note-editor-check">
        <input type="checkbox" checked={visible} onChange={(event) => setVisible(event.target.checked)} />
        <span>{t("calc.noteAlwaysShow")}</span>
      </label>
      <div className="note-editor-actions">
        <button type="button" className="btn btn-primary" onClick={() => finish(true)}>
          {t("common.save")}
        </button>
        {initial ? (
          <button
            type="button"
            className="btn btn-soft"
            onClick={() => {
              closedRef.current = true;
              onDelete();
              onClose();
            }}
          >
            {t("common.delete")}
          </button>
        ) : null}
        <button type="button" className="btn btn-soft" onClick={() => finish(false)}>
          {t("common.cancel")}
        </button>
      </div>
    </div>
  );
}

/** The note shown while the pointer rests on a cell that has one. */
export function NoteTooltip({ note, x, y }: { note: CellNote; x: number; y: number }) {
  const place = clampToWindow(x, y, 260, 120);
  return (
    <div className="note-tooltip" role="tooltip" style={place}>
      {note.author ? <strong>{note.author}</strong> : null}
      <p>{note.text}</p>
    </div>
  );
}

/**
 * The notes set to stay on screen: a small box at the top-right corner of each
 * cell, in the sheet's own coordinates so it scrolls with the cells.
 */
export function VisibleNotes({
  cells,
  place,
}: {
  cells: Record<string, Cell>;
  /** The top-right corner of a cell in canvas pixels. */
  place: (row: number, col: number) => { left: number; top: number };
}) {
  const addresses = useMemo(() => visibleNoteAddresses(cells), [cells]);
  return (
    <>
      {addresses.map((address) => {
        const position = parseAddress(address);
        const note = noteOf(cells[address]);
        if (!position || !note) return null;
        const { left, top } = place(position.row, position.col);
        return (
          <div key={address} className="cell-note-box" role="note" style={{ left: left + 2, top }}>
            {note.author ? <strong>{note.author}</strong> : null}
            <p>{note.text}</p>
          </div>
        );
      })}
    </>
  );
}
