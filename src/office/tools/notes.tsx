/** Notes: a small searchable notebook kept in the office store. */
import { useEffect, useMemo, useState } from "react";
import { Archive, FilePlus2, FolderPlus, Pin, Plus, Star, Trash2 } from "lucide-react";
import { useT } from "../../lib/i18n";
import { useNotes } from "../../lib/office-store";
import { Screen } from "../../components/layout";

// ---------------------------------------------------------------------------
// Notes
// ---------------------------------------------------------------------------

export function NotesScreen() {
  const t = useT();
  const { notes, folders, loaded, load, create, update, remove, addFolder } = useNotes();
  const [activeId, setActiveId] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [folderFilter, setFolderFilter] = useState<string | null>(null);
  const [showArchived, setShowArchived] = useState(false);

  useEffect(() => {
    if (!loaded) void load();
  }, [loaded, load]);

  const filtered = useMemo(() => {
    const query = search.trim().toLowerCase();
    return notes
      .filter((note) => (showArchived ? note.archived : !note.archived))
      .filter((note) => (folderFilter ? note.folderId === folderFilter : true))
      .filter((note) =>
        query ? `${note.title} ${note.body} ${note.tags.join(" ")}`.toLowerCase().includes(query) : true,
      )
      .sort((a, b) => Number(b.pinned) - Number(a.pinned) || b.updatedAt.localeCompare(a.updatedAt));
  }, [notes, search, folderFilter, showArchived]);

  const active = notes.find((note) => note.id === activeId) ?? filtered[0] ?? null;

  return (
    <Screen
      title={t("notes.title")}
      subtitle={t("notes.subtitle")}
      actions={
        <div className="row">
          <button
            type="button"
            className="btn btn-soft"
            onClick={() => addFolder(window.prompt(t("notes.folderName")) ?? "Folder")}
          >
            <FolderPlus size={15} /> {t("notes.newFolder")}
          </button>
          <button type="button" className="btn btn-primary" onClick={() => setActiveId(create().id)}>
            <Plus size={15} /> {t("notes.newNote")}
          </button>
        </div>
      }
    >
      <div className="notes-layout">
        <div className="notes-sidebar card">
          <input
            className="text-input"
            placeholder={t("notes.searchPlaceholder")}
            value={search}
            onChange={(event) => setSearch(event.target.value)}
          />
          <div className="notes-filters">
            <button
              type="button"
              className={`chip${folderFilter === null && !showArchived ? " is-active" : ""}`}
              onClick={() => {
                setFolderFilter(null);
                setShowArchived(false);
              }}
            >
              {t("notes.all")}
            </button>
            {folders.map((folder) => (
              <button
                key={folder.id}
                type="button"
                className={`chip${folderFilter === folder.id ? " is-active" : ""}`}
                onClick={() => setFolderFilter(folder.id)}
              >
                {folder.name}
              </button>
            ))}
            <button
              type="button"
              className={`chip${showArchived ? " is-active" : ""}`}
              onClick={() => setShowArchived(true)}
            >
              <Archive size={12} /> {t("notes.archived")}
            </button>
          </div>
          <div className="notes-list">
            {filtered.map((note) => (
              <button
                key={note.id}
                type="button"
                className={`note-item${active?.id === note.id ? " is-active" : ""}`}
                onClick={() => setActiveId(note.id)}
              >
                <div className="row">
                  <strong>{note.title || t("notes.untitled")}</strong>
                  <span className="spacer" />
                  {note.pinned ? <Pin size={12} /> : null}
                  {note.favorite ? <Star size={12} /> : null}
                </div>
                <span className="muted">{note.body.slice(0, 60) || t("notes.empty")}</span>
              </button>
            ))}
            {filtered.length === 0 ? <p className="muted">{t("notes.none")}</p> : null}
          </div>
        </div>
        <div className="notes-editor card">
          {active ? (
            <>
              <div className="row note-toolbar">
                <input
                  className="note-title"
                  value={active.title}
                  onChange={(event) => update(active.id, { title: event.target.value })}
                />
                <button
                  type="button"
                  className={`icon-btn${active.pinned ? " is-on" : ""}`}
                  title={t("notes.pin")}
                  onClick={() => update(active.id, { pinned: !active.pinned })}
                >
                  <Pin size={14} />
                </button>
                <button
                  type="button"
                  className={`icon-btn${active.favorite ? " is-on" : ""}`}
                  title={t("notes.favorite")}
                  onClick={() => update(active.id, { favorite: !active.favorite })}
                >
                  <Star size={14} />
                </button>
                <button
                  type="button"
                  className="icon-btn"
                  title={t("notes.archive")}
                  onClick={() => update(active.id, { archived: !active.archived })}
                >
                  <Archive size={14} />
                </button>
                <button
                  type="button"
                  className="icon-btn"
                  title={t("common.delete")}
                  onClick={() => {
                    remove(active.id);
                    setActiveId(null);
                  }}
                >
                  <Trash2 size={14} />
                </button>
              </div>
              <div className="row note-meta">
                <select
                  value={active.folderId ?? ""}
                  onChange={(event) => update(active.id, { folderId: event.target.value || null })}
                >
                  <option value="">{t("notes.noFolder")}</option>
                  {folders.map((folder) => (
                    <option key={folder.id} value={folder.id}>
                      {folder.name}
                    </option>
                  ))}
                </select>
                <input
                  placeholder={t("notes.tagsPlaceholder")}
                  value={active.tags.join(", ")}
                  onChange={(event) =>
                    update(active.id, {
                      tags: event.target.value
                        .split(",")
                        .map((tag) => tag.trim())
                        .filter(Boolean),
                    })
                  }
                />
              </div>
              <textarea
                className="note-body"
                value={active.body}
                placeholder={t("notes.bodyPlaceholder")}
                onChange={(event) => update(active.id, { body: event.target.value })}
              />
            </>
          ) : (
            <div className="empty-state">
              <FilePlus2 size={22} />
              <p className="muted">{t("notes.none")}</p>
            </div>
          )}
        </div>
      </div>
    </Screen>
  );
}
