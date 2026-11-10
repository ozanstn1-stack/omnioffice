import { Dialog } from "../../office-ui";
import { useT } from "../../../lib/i18n";
import type { Bookmark, FieldRef } from "../../../lib/office-types";

/** Writer's cross-reference dialog: insert a field to a bookmark, or add one. */
export function CrossReferenceDialog({
  kind,
  onKind,
  target,
  onTarget,
  bookmarks,
  bookmarkName,
  onBookmarkName,
  onInsert,
  onAddBookmark,
  onClose,
}: {
  kind: FieldRef["kind"];
  onKind: (kind: FieldRef["kind"]) => void;
  target: string;
  onTarget: (target: string) => void;
  bookmarks: Bookmark[];
  bookmarkName: string;
  onBookmarkName: (name: string) => void;
  /** Called only with a chosen target; the dialog closes afterwards. */
  onInsert: (kind: FieldRef["kind"], target: string) => void;
  onAddBookmark: () => void;
  onClose: () => void;
}) {
  const t = useT();
  return (
    <Dialog title={t("writer.crossReference")} onClose={onClose}>
      <div className="stack">
        <label className="field">
          <span>{t("writer.fieldKind")}</span>
          <select value={kind} onChange={(event) => onKind(event.target.value as FieldRef["kind"])}>
            <option value="ref">{t("writer.fieldRef")}</option>
            <option value="refPage">{t("writer.fieldRefPage")}</option>
            <option value="bookmark">{t("writer.fieldBookmark")}</option>
          </select>
        </label>
        <label className="field">
          <span>{t("writer.bookmarkTarget")}</span>
          <select value={target} onChange={(event) => onTarget(event.target.value)}>
            <option value="">—</option>
            {bookmarks.map((bookmark) => (
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
              if (!target) return;
              onInsert(kind, target);
              onClose();
            }}
          >
            {t("writer.insertField")}
          </button>
          <input
            placeholder={t("writer.bookmarkName")}
            value={bookmarkName}
            onChange={(event) => onBookmarkName(event.target.value)}
          />
          <button type="button" className="btn btn-soft" onClick={onAddBookmark}>
            {t("writer.addBookmark")}
          </button>
        </div>
        <p className="muted">{t("writer.bookmarkHint")}</p>
      </div>
    </Dialog>
  );
}
