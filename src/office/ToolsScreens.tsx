/**
 * Productivity screens: Notes, Planner, Data sheets, Draw, Templates,
 * Universal Converter, Document Cleaner and PDF Forms.
 */
import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { appCacheDir, join } from "@tauri-apps/api/path";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { readFile } from "@tauri-apps/plugin-fs";
import {
  Archive,
  ArrowLeft,
  Check,
  Download,
  FileDown,
  FilePlus2,
  FileSpreadsheet,
  FileText,
  FolderPlus,
  Image as ImageIcon,
  Pin,
  Plus,
  RefreshCw,
  Save,
  Search,
  Star,
  Trash2,
  Wand2,
  FileImage,
} from "lucide-react";
import { useT } from "../lib/i18n";
import { errorMessage, reportError, useToasts } from "../lib/store";
import {
  isAndroid,
  pickAndroidFiles,
  pickAndroidFolder,
  pickAndroidSaveTarget,
  pickFileBytes,
  pickOfficeFiles,
  publishOutputs,
  saveFileBytes,
  type AndroidTarget,
} from "../lib/mobile";
import { fileBaseName } from "../lib/format";
import { jpegToPdf } from "../lib/image-pdf";
import { uid } from "../lib/office-types";
import { useDataSheets, useDraw, useNotes, useOfficeTabs, usePlanner, type DrawDocument } from "../lib/office-store";
import * as api from "../lib/office-api";
import { Screen } from "../components/layout";
import { compatibilityReport, gatingLossItems } from "../components/compatibility";
import { useDataLossPrompt } from "../components/data-loss-dialog";
import { TEMPLATES, templatesFor } from "./templates";

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

// ---------------------------------------------------------------------------
// Planner
// ---------------------------------------------------------------------------

export function PlannerScreen() {
  const t = useT();
  const { tasks, loaded, load, add, update, remove } = usePlanner();
  const [month, setMonth] = useState(() => new Date());
  const [draft, setDraft] = useState("");
  const [selectedDate, setSelectedDate] = useState(() => new Date().toISOString().slice(0, 10));

  useEffect(() => {
    if (!loaded) void load();
  }, [loaded, load]);

  const days = useMemo(() => {
    const first = new Date(month.getFullYear(), month.getMonth(), 1);
    const start = new Date(first);
    start.setDate(first.getDate() - ((first.getDay() + 6) % 7));
    const list: Date[] = [];
    for (let index = 0; index < 42; index += 1) {
      const date = new Date(start);
      date.setDate(start.getDate() + index);
      list.push(date);
    }
    return list;
  }, [month]);

  const tasksFor = (date: string) => tasks.filter((task) => task.date === date);
  const weekAhead = useMemo(() => {
    const today = new Date();
    return tasks
      .filter((task) => new Date(task.date) >= new Date(today.getFullYear(), today.getMonth(), today.getDate()))
      .sort((a, b) => a.date.localeCompare(b.date))
      .slice(0, 12);
  }, [tasks]);

  return (
    <Screen
      title={t("planner.title")}
      subtitle={t("planner.subtitle")}
      actions={
        <div className="row">
          <button
            type="button"
            className="btn btn-soft"
            onClick={() => setMonth(new Date(month.getFullYear(), month.getMonth() - 1, 1))}
          >
            ‹
          </button>
          <strong>{month.toLocaleDateString(undefined, { month: "long", year: "numeric" })}</strong>
          <button
            type="button"
            className="btn btn-soft"
            onClick={() => setMonth(new Date(month.getFullYear(), month.getMonth() + 1, 1))}
          >
            ›
          </button>
        </div>
      }
    >
      <div className="planner-layout">
        <div className="card planner-calendar">
          <div className="planner-weekdays">
            {["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"].map((day) => (
              <span key={day}>{day}</span>
            ))}
          </div>
          <div className="planner-grid">
            {days.map((date) => {
              const iso = date.toISOString().slice(0, 10);
              const isCurrentMonth = date.getMonth() === month.getMonth();
              const dayTasks = tasksFor(iso);
              return (
                <button
                  key={iso}
                  type="button"
                  className={`planner-day${isCurrentMonth ? "" : " is-outside"}${iso === new Date().toISOString().slice(0, 10) ? " is-today" : ""}${selectedDate === iso ? " is-selected" : ""}`}
                  onClick={() => setSelectedDate(iso)}
                >
                  <span>{date.getDate()}</span>
                  {dayTasks.slice(0, 3).map((task) => (
                    <i key={task.id} className={task.done ? "is-done" : ""} />
                  ))}
                </button>
              );
            })}
          </div>
        </div>
        <div className="card planner-side">
          <h3>{selectedDate}</h3>
          <div className="row">
            <input
              className="text-input"
              placeholder={t("planner.addTask")}
              value={draft}
              onChange={(event) => setDraft(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && draft.trim()) {
                  add(selectedDate, draft.trim());
                  setDraft("");
                }
              }}
            />
            <button
              type="button"
              className="btn btn-primary"
              onClick={() => {
                if (draft.trim()) {
                  add(selectedDate, draft.trim());
                  setDraft("");
                }
              }}
            >
              <Plus size={15} />
            </button>
          </div>
          <div className="stack">
            {tasksFor(selectedDate).map((task) => (
              <div key={task.id} className="row task-row">
                <input
                  type="checkbox"
                  checked={task.done}
                  onChange={(event) => update(task.id, { done: event.target.checked })}
                />
                <span className={task.done ? "is-done" : ""}>{task.title}</span>
                <span className="spacer" />
                <select
                  value={task.priority}
                  onChange={(event) => update(task.id, { priority: event.target.value as "low" | "normal" | "high" })}
                >
                  <option value="low">{t("planner.low")}</option>
                  <option value="normal">{t("planner.normal")}</option>
                  <option value="high">{t("planner.high")}</option>
                </select>
                <button type="button" className="icon-btn" onClick={() => remove(task.id)}>
                  <Trash2 size={13} />
                </button>
              </div>
            ))}
          </div>
          <h3>{t("planner.upcoming")}</h3>
          <div className="stack">
            {weekAhead.map((task) => (
              <div key={task.id} className="row">
                <span className="muted">{task.date}</span>
                <span className={task.done ? "is-done" : ""}>{task.title}</span>
              </div>
            ))}
            {weekAhead.length === 0 ? <p className="muted">{t("planner.noTasks")}</p> : null}
          </div>
        </div>
      </div>
    </Screen>
  );
}

// ---------------------------------------------------------------------------
// Data sheets
// ---------------------------------------------------------------------------

/**
 * Parses CSV text (RFC 4180 quoting: quoted delimiters, doubled quotes and
 * line breaks inside quotes). The delimiter is `;` when the header line has
 * more semicolons than commas outside quotes, `,` otherwise. Empty lines are
 * skipped and short rows are padded to the header width.
 */
export function parseCsv(text: string): { columns: string[]; rows: string[][] } {
  const source = text.replace(/^\uFEFF/, "");
  const firstLine = source.split(/\r?\n/, 1)[0] ?? "";
  const unquoted = firstLine.replace(/"[^"]*"/g, "");
  const delimiter = (unquoted.match(/;/g)?.length ?? 0) > (unquoted.match(/,/g)?.length ?? 0) ? ";" : ",";
  const records: string[][] = [];
  let record: string[] = [];
  let cell = "";
  let quoted = false;
  for (let index = 0; index < source.length; index += 1) {
    const char = source[index];
    if (quoted) {
      if (char === '"') {
        if (source[index + 1] === '"') {
          cell += '"';
          index += 1;
        } else {
          quoted = false;
        }
      } else {
        cell += char;
      }
    } else if (char === '"' && cell === "") {
      quoted = true;
    } else if (char === delimiter) {
      record.push(cell);
      cell = "";
    } else if (char === "\n" || char === "\r") {
      if (char === "\r" && source[index + 1] === "\n") index += 1;
      record.push(cell);
      records.push(record);
      record = [];
      cell = "";
    } else {
      cell += char;
    }
  }
  if (cell !== "" || record.length) {
    record.push(cell);
    records.push(record);
  }
  const nonEmpty = records.filter((row) => row.some((value) => value.trim() !== ""));
  const columns = nonEmpty[0] ?? [];
  const rows = nonEmpty
    .slice(1)
    .map((row) =>
      row.length < columns.length ? [...row, ...Array<string>(columns.length - row.length).fill("")] : row,
    );
  return { columns, rows };
}

/**
 * Turns JSON into a table: an array of objects (columns are the union of the
 * keys, in first-seen order), a single object (one row) or an array of
 * arrays (first row is the header). Anything else is rejected.
 */
export function parseJsonTable(text: string): { columns: string[]; rows: string[][] } {
  const parsed: unknown = JSON.parse(text);
  const cellText = (value: unknown): string =>
    value === null || value === undefined ? "" : typeof value === "object" ? JSON.stringify(value) : String(value);
  const items = Array.isArray(parsed) ? parsed : [parsed];
  if (items.length && items.every((item) => Array.isArray(item))) {
    const [header = [], ...body] = items as unknown[][];
    return { columns: header.map(cellText), rows: body.map((row) => row.map(cellText)) };
  }
  if (!items.every((item) => item !== null && typeof item === "object" && !Array.isArray(item))) {
    throw new Error("The JSON file must contain an object or a list of objects.");
  }
  const records = items as Record<string, unknown>[];
  const columns: string[] = [];
  for (const record of records) {
    for (const key of Object.keys(record)) if (!columns.includes(key)) columns.push(key);
  }
  return { columns, rows: records.map((record) => columns.map((column) => cellText(record[column]))) };
}

export function DataScreen() {
  const t = useT();
  const { sheets, loaded, load, create, update, remove } = useDataSheets();
  const [activeId, setActiveId] = useState<string | null>(null);
  const active = sheets.find((sheet) => sheet.id === activeId) ?? sheets[0] ?? null;

  useEffect(() => {
    if (!loaded) void load();
  }, [loaded, load]);

  const importFile = async () => {
    try {
      const picked = await pickFileBytes({
        name: "Data",
        extensions: ["csv", "json"],
        mimeTypes: ["text/csv", "text/comma-separated-values", "application/json", "text/plain", "*/*"],
      });
      if (!picked) return;
      const text = new TextDecoder().decode(picked.bytes);
      const { columns, rows } = picked.name.toLowerCase().endsWith(".json") ? parseJsonTable(text) : parseCsv(text);
      const sheet = create(picked.name.replace(/\.(csv|json)$/i, "") || "Imported");
      update(sheet.id, { columns, rows });
      setActiveId(sheet.id);
      useToasts.getState().push({ kind: "success", title: t("data.imported") });
    } catch (error) {
      reportError(error, t);
    }
  };

  const exportFile = async (format: "csv" | "json") => {
    if (!active) return;
    try {
      const content =
        format === "csv"
          ? [active.columns, ...active.rows]
              .map((row) =>
                row.map((cell) => (/[",;\r\n]/.test(cell) ? `"${cell.replace(/"/g, '""')}"` : cell)).join(","),
              )
              .join("\r\n")
          : JSON.stringify(
              active.rows.map((row) =>
                Object.fromEntries(active.columns.map((column, index) => [column, row[index] ?? ""])),
              ),
              null,
              2,
            );
      const saved = await saveFileBytes(new TextEncoder().encode(content), `${active.name}.${format}`, {
        name: format.toUpperCase(),
        extensions: [format],
      });
      if (!saved) return;
      useToasts.getState().push({ kind: "success", title: t("data.exported"), detail: saved });
    } catch (error) {
      reportError(error, t);
    }
  };

  return (
    <Screen
      title={t("data.title")}
      subtitle={t("data.subtitle")}
      actions={
        <div className="row">
          <button type="button" className="btn btn-soft" onClick={importFile}>
            <Download size={15} /> {t("data.import")}
          </button>
          <button type="button" className="btn btn-soft" onClick={() => exportFile("csv")} disabled={!active}>
            <FileDown size={15} /> CSV
          </button>
          <button type="button" className="btn btn-soft" onClick={() => exportFile("json")} disabled={!active}>
            <FileDown size={15} /> JSON
          </button>
          <button type="button" className="btn btn-primary" onClick={() => setActiveId(create().id)}>
            <Plus size={15} /> {t("data.newTable")}
          </button>
        </div>
      }
    >
      <div className="data-layout">
        <div className="card data-list">
          {sheets.map((sheet) => (
            <button
              key={sheet.id}
              type="button"
              className={`note-item${active?.id === sheet.id ? " is-active" : ""}`}
              onClick={() => setActiveId(sheet.id)}
              aria-label={sheet.name}
            >
              <div className="row">
                <strong>{sheet.name}</strong>
                <span className="spacer" />
                <span className="muted">{sheet.rows.length}</span>
              </div>
            </button>
          ))}
          {sheets.length === 0 ? <p className="muted">{t("data.none")}</p> : null}
        </div>
        <div className="card data-table-card">
          {active ? (
            <>
              <div className="row">
                <input
                  className="note-title"
                  value={active.name}
                  onChange={(event) => update(active.id, { name: event.target.value })}
                />
                <button
                  type="button"
                  className="btn btn-soft"
                  onClick={() => update(active.id, { rows: [...active.rows, active.columns.map(() => "")] })}
                >
                  <Plus size={14} /> {t("data.addRow")}
                </button>
                <button
                  type="button"
                  className="btn btn-soft"
                  onClick={() => {
                    const column = window.prompt(t("data.columnName"));
                    if (column)
                      update(active.id, {
                        columns: [...active.columns, column],
                        rows: active.rows.map((row) => [...row, ""]),
                      });
                  }}
                >
                  <Plus size={14} /> {t("data.addColumn")}
                </button>
                <button
                  type="button"
                  className="btn btn-soft"
                  onClick={() => {
                    remove(active.id);
                    setActiveId(null);
                  }}
                  aria-label={t("data.deleteTable")}
                >
                  <Trash2 size={14} />
                </button>
              </div>
              <div className="data-grid-wrap">
                <table className="table data-grid">
                  <thead>
                    <tr>
                      <th>
                        <span className="sr-only">{t("data.row")}</span>
                      </th>
                      {active.columns.map((column, columnIndex) => (
                        <th key={columnIndex}>
                          <input
                            aria-label={t("data.columnName", { index: columnIndex + 1 })}
                            value={column}
                            onChange={(event) =>
                              update(active.id, {
                                columns: active.columns.map((candidate, index) =>
                                  index === columnIndex ? event.target.value : candidate,
                                ),
                              })
                            }
                          />
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {active.rows.map((row, rowIndex) => (
                      <tr key={rowIndex}>
                        <td className="row-index">{rowIndex + 1}</td>
                        {active.columns.map((column, columnIndex) => (
                          <td key={columnIndex}>
                            <input
                              aria-label={`${column || t("data.columnName", { index: columnIndex + 1 })} — ${t("data.row")} ${rowIndex + 1}`}
                              value={row[columnIndex] ?? ""}
                              onChange={(event) =>
                                update(active.id, {
                                  rows: active.rows.map((candidate, index) =>
                                    index === rowIndex
                                      ? candidate.map((cell, cellIndex) =>
                                          cellIndex === columnIndex ? event.target.value : cell,
                                        )
                                      : candidate,
                                  ),
                                })
                              }
                            />
                          </td>
                        ))}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </>
          ) : (
            <p className="muted">{t("data.none")}</p>
          )}
        </div>
      </div>
    </Screen>
  );
}

// ---------------------------------------------------------------------------
// Draw
// ---------------------------------------------------------------------------

export function DrawScreen() {
  const t = useT();
  const { documents, loaded, load, save, remove } = useDraw();
  const [activeId, setActiveId] = useState<string | null>(null);
  const [tool, setTool] = useState<DrawDocument["elements"][number]["kind"]>("rect");
  const [color, setColor] = useState("#2563eb");
  const [stroke, setStroke] = useState("#1e293b");
  const [drawing, setDrawing] = useState<{
    startX: number;
    startY: number;
    points: Array<{ x: number; y: number }>;
  } | null>(null);
  const svgRef = useRef<SVGSVGElement>(null);

  useEffect(() => {
    if (!loaded) void load();
  }, [loaded, load]);

  const active = documents.find((document) => document.id === activeId) ?? documents[0] ?? null;

  const createDocument = () => {
    const document: DrawDocument = {
      id: uid(),
      name: `Drawing ${documents.length + 1}`,
      width: 800,
      height: 520,
      elements: [],
      updatedAt: new Date().toISOString(),
    };
    save(document);
    setActiveId(document.id);
  };

  const addElement = (element: DrawDocument["elements"][number]) => {
    if (!active) return;
    save({ ...active, elements: [...active.elements, element], updatedAt: new Date().toISOString() });
  };

  const pointerDown = (event: React.MouseEvent<SVGSVGElement>) => {
    const rect = svgRef.current?.getBoundingClientRect();
    if (!rect) return;
    const x = event.clientX - rect.left;
    const y = event.clientY - rect.top;
    if (tool === "path") {
      setDrawing({ startX: x, startY: y, points: [{ x, y }] });
    } else {
      setDrawing({ startX: x, startY: y, points: [] });
    }
  };

  const pointerMove = (event: React.MouseEvent<SVGSVGElement>) => {
    if (!drawing) return;
    const rect = svgRef.current?.getBoundingClientRect();
    if (!rect) return;
    const x = event.clientX - rect.left;
    const y = event.clientY - rect.top;
    if (tool === "path") setDrawing({ ...drawing, points: [...drawing.points, { x, y }] });
  };

  const pointerUp = (event: React.MouseEvent<SVGSVGElement>) => {
    if (!drawing) return;
    const rect = svgRef.current?.getBoundingClientRect();
    if (!rect) return;
    const x = event.clientX - rect.left;
    const y = event.clientY - rect.top;
    const width = Math.abs(x - drawing.startX);
    const height = Math.abs(y - drawing.startY);
    const left = Math.min(x, drawing.startX);
    const top = Math.min(y, drawing.startY);
    if (tool === "path") {
      if (drawing.points.length > 2)
        addElement({ id: uid(), kind: "path", x: 0, y: 0, w: 0, h: 0, points: drawing.points, stroke, strokeWidth: 2 });
    } else if (tool === "line" || tool === "arrow") {
      addElement({
        id: uid(),
        kind: tool,
        x: drawing.startX,
        y: drawing.startY,
        w: x - drawing.startX,
        h: y - drawing.startY,
        stroke,
        strokeWidth: 2,
      });
    } else if (tool === "text") {
      const text = window.prompt(t("draw.textPrompt")) ?? "Text";
      addElement({ id: uid(), kind: "text", x: left, y: top + 18, w: Math.max(80, width), h: 28, text, fill: color });
    } else {
      addElement({
        id: uid(),
        kind: tool,
        x: left,
        y: top,
        w: Math.max(6, width),
        h: Math.max(6, height),
        fill: tool === "rect" || tool === "ellipse" ? color : null,
        stroke: tool === "rect" || tool === "ellipse" ? undefined : stroke,
        strokeWidth: 2,
      });
    }
    setDrawing(null);
  };

  // Every export goes through saveFileBytes: the desktop save dialog or the
  // Android SAF picker, one write to the chosen destination. (The old PDF
  // export staged a temporary PNG next to the target, which the fs scope
  // refuses on the desktop and which cannot exist on Android.)
  const rasterize = async (mime: "image/png" | "image/jpeg"): Promise<Uint8Array> => {
    if (!active) return new Uint8Array();
    const blob = new Blob([buildSvg(active)], { type: "image/svg+xml" });
    const url = URL.createObjectURL(blob);
    try {
      const image = new Image();
      await new Promise<void>((resolve, reject) => {
        image.onload = () => resolve();
        image.onerror = () => reject(new Error("Could not rasterise the drawing."));
        image.src = url;
      });
      const canvas = document.createElement("canvas");
      canvas.width = active.width * 2;
      canvas.height = active.height * 2;
      const context = canvas.getContext("2d")!;
      context.fillStyle = "#ffffff";
      context.fillRect(0, 0, canvas.width, canvas.height);
      context.drawImage(image, 0, 0, canvas.width, canvas.height);
      const base64 = canvas.toDataURL(mime, 0.92).split(",")[1] ?? "";
      return Uint8Array.from(atob(base64), (char) => char.charCodeAt(0));
    } finally {
      URL.revokeObjectURL(url);
    }
  };

  const exportAs = async (format: "svg" | "png" | "pdf") => {
    if (!active) return;
    try {
      let bytes: Uint8Array;
      if (format === "svg") {
        bytes = new TextEncoder().encode(buildSvg(active));
      } else if (format === "png") {
        bytes = await rasterize("image/png");
      } else {
        // One page the size of the drawing (1 px = 1 pt), rendered at 2x.
        bytes = jpegToPdf(
          await rasterize("image/jpeg"),
          active.width * 2,
          active.height * 2,
          active.width,
          active.height,
        );
      }
      const saved = await saveFileBytes(bytes, `${active.name}.${format}`, {
        name: format.toUpperCase(),
        extensions: [format],
      });
      if (!saved) return;
      useToasts.getState().push({ kind: "success", title: t("draw.exported"), detail: saved });
    } catch (error) {
      reportError(error, t);
    }
  };
  const exportSvg = () => void exportAs("svg");
  const exportPng = () => void exportAs("png");
  const exportPdf = () => void exportAs("pdf");

  return (
    <Screen
      title={t("draw.title")}
      subtitle={t("draw.subtitle")}
      actions={
        <div className="row">
          <button type="button" className="btn btn-soft" onClick={exportSvg} disabled={!active}>
            SVG
          </button>
          <button type="button" className="btn btn-soft" onClick={exportPng} disabled={!active}>
            <FileImage size={15} /> PNG
          </button>
          <button type="button" className="btn btn-soft" onClick={exportPdf} disabled={!active}>
            <FileDown size={15} /> PDF
          </button>
          <button type="button" className="btn btn-primary" onClick={createDocument}>
            <Plus size={15} /> {t("draw.newDrawing")}
          </button>
        </div>
      }
    >
      <div className="draw-layout">
        <div className="card draw-toolbar">
          {(["rect", "ellipse", "line", "arrow", "text", "path"] as const).map((candidate) => (
            <button
              key={candidate}
              type="button"
              className={`chip${tool === candidate ? " is-active" : ""}`}
              onClick={() => setTool(candidate)}
            >
              {t(`draw.tool_${candidate}`)}
            </button>
          ))}
          <label className="field">
            <span>{t("draw.fill")}</span>
            <input type="color" value={color} onChange={(event) => setColor(event.target.value)} />
          </label>
          <label className="field">
            <span>{t("draw.stroke")}</span>
            <input type="color" value={stroke} onChange={(event) => setStroke(event.target.value)} />
          </label>
        </div>
        <div className="card draw-canvas-card">
          {active ? (
            <svg
              ref={svgRef}
              className="draw-canvas"
              viewBox={`0 0 ${active.width} ${active.height}`}
              onMouseDown={pointerDown}
              onMouseMove={pointerMove}
              onMouseUp={pointerUp}
              onMouseLeave={() => setDrawing(null)}
            >
              <rect width={active.width} height={active.height} fill="#ffffff" />
              {active.elements.map((element) => (
                <DrawElementView key={element.id} element={element} />
              ))}
              {drawing && tool === "path" ? (
                <polyline
                  points={drawing.points.map((point) => `${point.x},${point.y}`).join(" ")}
                  fill="none"
                  stroke={stroke}
                  strokeWidth={2}
                />
              ) : null}
            </svg>
          ) : (
            <div className="empty-state">
              <ImageIcon size={22} />
              <p className="muted">{t("draw.none")}</p>
              <button type="button" className="btn btn-primary" onClick={createDocument}>
                {t("draw.newDrawing")}
              </button>
            </div>
          )}
        </div>
        <div className="card draw-list">
          {documents.map((document) => (
            <div key={document.id} className="row">
              <button
                type="button"
                className={`note-item${active?.id === document.id ? " is-active" : ""}`}
                onClick={() => setActiveId(document.id)}
              >
                {document.name}
              </button>
              <button type="button" className="icon-btn" onClick={() => remove(document.id)}>
                <Trash2 size={12} />
              </button>
            </div>
          ))}
        </div>
      </div>
    </Screen>
  );
}

function DrawElementView({ element }: { element: DrawDocument["elements"][number] }) {
  if (element.kind === "rect")
    return (
      <rect
        x={element.x}
        y={element.y}
        width={element.w}
        height={element.h}
        fill={element.fill ?? "none"}
        stroke={element.stroke === null ? undefined : element.stroke}
        strokeWidth={element.strokeWidth ?? 0}
        rx={6}
      />
    );
  if (element.kind === "ellipse")
    return (
      <ellipse
        cx={element.x + element.w / 2}
        cy={element.y + element.h / 2}
        rx={element.w / 2}
        ry={element.h / 2}
        fill={element.fill ?? "none"}
        stroke={element.stroke === null ? undefined : element.stroke}
        strokeWidth={element.strokeWidth ?? 0}
      />
    );
  if (element.kind === "line" || element.kind === "arrow")
    return (
      <g>
        <line
          x1={element.x}
          y1={element.y}
          x2={element.x + element.w}
          y2={element.y + element.h}
          stroke={element.stroke ?? "#1e293b"}
          strokeWidth={element.strokeWidth ?? 2}
        />
        {element.kind === "arrow" ? (
          <polygon
            points={`${element.x + element.w},${element.y + element.h} ${element.x + element.w - 10},${element.y + element.h - 4} ${element.x + element.w - 4},${element.y + element.h - 10}`}
            fill={element.stroke ?? "#1e293b"}
          />
        ) : null}
      </g>
    );
  if (element.kind === "text")
    return (
      <text x={element.x} y={element.y} fill={element.fill ?? "#1e293b"} fontSize={18}>
        {element.text}
      </text>
    );
  if (element.kind === "path" && element.points)
    return (
      <polyline
        points={element.points.map((point) => `${point.x},${point.y}`).join(" ")}
        fill="none"
        stroke={element.stroke ?? "#1e293b"}
        strokeWidth={element.strokeWidth ?? 2}
      />
    );
  return null;
}

function buildSvg(document: DrawDocument): string {
  const elements = document.elements
    .map((element) => {
      if (element.kind === "rect")
        return `<rect x="${element.x}" y="${element.y}" width="${element.w}" height="${element.h}" fill="${element.fill ?? "none"}" stroke="${element.stroke ?? "none"}" stroke-width="${element.strokeWidth ?? 0}" rx="6"/>`;
      if (element.kind === "ellipse")
        return `<ellipse cx="${element.x + element.w / 2}" cy="${element.y + element.h / 2}" rx="${element.w / 2}" ry="${element.h / 2}" fill="${element.fill ?? "none"}" stroke="${element.stroke ?? "none"}" stroke-width="${element.strokeWidth ?? 0}"/>`;
      if (element.kind === "line" || element.kind === "arrow")
        return `<line x1="${element.x}" y1="${element.y}" x2="${element.x + element.w}" y2="${element.y + element.h}" stroke="${element.stroke ?? "#1e293b"}" stroke-width="${element.strokeWidth ?? 2}"/>`;
      if (element.kind === "text")
        return `<text x="${element.x}" y="${element.y}" fill="${element.fill ?? "#1e293b"}" font-size="18" font-family="Segoe UI, sans-serif">${escapeXml(element.text ?? "")}</text>`;
      if (element.kind === "path" && element.points)
        return `<polyline points="${element.points.map((point) => `${point.x},${point.y}`).join(" ")}" fill="none" stroke="${element.stroke ?? "#1e293b"}" stroke-width="${element.strokeWidth ?? 2}"/>`;
      return "";
    })
    .join("\n");
  return `<?xml version="1.0" encoding="UTF-8"?>\n<svg xmlns="http://www.w3.org/2000/svg" width="${document.width}" height="${document.height}" viewBox="0 0 ${document.width} ${document.height}">\n<rect width="${document.width}" height="${document.height}" fill="#ffffff"/>\n${elements}\n</svg>`;
}

function escapeXml(value: string): string {
  return value.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

// ---------------------------------------------------------------------------
// Templates
// ---------------------------------------------------------------------------

export function TemplatesScreen({ onOpen }: { onOpen?: () => void } = {}) {
  const t = useT();
  const create = useOfficeTabs((state) => state.create);
  const [filter, setFilter] = useState<"all" | "writer" | "calc" | "impress">("all");
  const visible = filter === "all" ? TEMPLATES : templatesFor(filter);
  return (
    <Screen title={t("templates.title")} subtitle={t("templates.subtitle")}>
      <div className="row">
        {(["all", "writer", "calc", "impress"] as const).map((candidate) => (
          <button
            key={candidate}
            type="button"
            className={`chip${filter === candidate ? " is-active" : ""}`}
            onClick={() => setFilter(candidate)}
          >
            {t(`templates.${candidate}`)}
          </button>
        ))}
      </div>
      <div className="template-grid">
        {visible.map((template) => (
          <button
            key={template.id}
            type="button"
            className="card template-card"
            onClick={() => {
              create(template.kind, template.name, template.build() as never);
              onOpen?.();
            }}
          >
            <span className={`template-icon kind-${template.kind}`}>
              {template.kind === "writer" ? (
                <FileText size={20} />
              ) : template.kind === "calc" ? (
                <FileSpreadsheet size={20} />
              ) : (
                <ImageIcon size={20} />
              )}
            </span>
            <strong>{template.name}</strong>
            <span className="muted">{template.description}</span>
          </button>
        ))}
      </div>
    </Screen>
  );
}

// ---------------------------------------------------------------------------
// Universal converter
// ---------------------------------------------------------------------------

export function ConverterScreen() {
  const t = useT();
  const [files, setFiles] = useState<string[]>([]);
  const [target, setTarget] = useState("pdf");
  const [targets, setTargets] = useState<string[]>([
    "pdf",
    "docx",
    "xlsx",
    "pptx",
    "odt",
    "ods",
    "odp",
    "csv",
    "html",
    "oswk",
  ]);
  const [outputDir, setOutputDir] = useState("");
  const [androidDir, setAndroidDir] = useState<AndroidTarget | null>(null);
  const [busy, setBusy] = useState(false);
  const [results, setResults] = useState<Array<{ input: string; output: string; ok: boolean; detail: string }>>([]);

  const pickFiles = async () => {
    let list: string[] = [];
    if (isAndroid()) {
      // The desktop dialog plugin cannot open the Android picker; the SAF
      // bridge copies the chosen documents into the app cache as real paths.
      list = await pickOfficeFiles(true).catch(() => []);
    } else {
      const selection = await openDialog({
        multiple: true,
        filters: [
          {
            name: "Documents",
            extensions: [
              "docx",
              "docm",
              "dotx",
              "odt",
              "rtf",
              "txt",
              "md",
              "html",
              "xlsx",
              "xlsm",
              "xls",
              "ods",
              "csv",
              "tsv",
              "pptx",
              "pptm",
              "odp",
              "oswk",
              "pdf",
            ],
          },
        ],
      });
      list = Array.isArray(selection) ? selection : typeof selection === "string" ? [selection] : [];
    }
    setFiles(list);
    if (list[0]) {
      const extension = list[0].split(".").pop() ?? "";
      const available = await api.conversionTargets(extension).catch(() => []);
      if (available.length > 0) {
        setTargets(available);
        setTarget(available[0]);
      }
    }
  };

  const pickOutputDir = async () => {
    if (isAndroid()) {
      const target = await pickAndroidFolder().catch(() => null);
      if (target) setAndroidDir(target);
      return;
    }
    const selection = await openDialog({ directory: true });
    if (typeof selection === "string") setOutputDir(selection);
  };

  const run = async () => {
    if (files.length === 0) return;
    setBusy(true);
    // Data Loss Protection covers the converter as well: convert a file to any
    // non-native target only after the compatibility report has been shown.
    // The report needs the parsed model, so office inputs are opened read-only
    // for the check; PDFs and unreadable files skip it (fail-open).
    let effectiveTarget = target;
    if (target !== "oswk") {
      for (const input of files) {
        try {
          const opened = await api.openDocument(input);
          if (opened.kind !== "writer" && opened.kind !== "calc" && opened.kind !== "impress") continue;
          const report = await compatibilityReport(opened.kind, opened.model, target);
          if (gatingLossItems(target, report).length === 0) continue;
          const choice = await useDataLossPrompt.getState().ask(target, report);
          if (choice === "cancel") {
            setBusy(false);
            return;
          }
          if (choice === "oswk") {
            effectiveTarget = "oswk";
            setTarget("oswk");
          }
          break;
        } catch {
          // Not an office document we can inspect (e.g. PDF) - convert as asked.
        }
      }
    }
    const converted: typeof results = [];
    // Android cannot write next to the picked document (SAF paths), so the
    // converter stages its output in the app cache and copies each finished
    // file to the chosen folder or to Downloads afterwards.
    let stageDir = "";
    if (isAndroid()) {
      const cache = await appCacheDir().catch(() => "");
      if (cache) {
        stageDir = await join(cache, "converts", uid());
        await invoke("ensure_dir", { path: stageDir }).catch(() => undefined);
      }
    }
    for (const input of files) {
      const base = input.replace(/\.[^.\\/]+$/, "");
      const directory = isAndroid() ? stageDir : outputDir || input.replace(/[\\/][^\\/]+$/, "");
      const output = `${base}.${effectiveTarget === "html" ? "html" : effectiveTarget}`.replace(
        /^.*[\\/]/,
        `${directory}/`,
      );
      try {
        const info = await api.convertFile(input, output);
        if (isAndroid()) {
          const failures = await publishOutputs([info.output], androidDir ? { dir: androidDir } : undefined);
          if (failures.length > 0) {
            converted.push({ input, output: info.output, ok: false, detail: failures[0].error });
          } else {
            converted.push({ input, output: info.output, ok: true, detail: info.warnings.join(" ") });
          }
        } else {
          converted.push({ input, output: info.output, ok: true, detail: info.warnings.join(" ") });
        }
      } catch (error) {
        converted.push({ input, output: directory, ok: false, detail: errorMessage(error, t) });
      }
    }
    setResults(converted);
    setBusy(false);
  };

  return (
    <Screen
      title={t("converter.title")}
      subtitle={t("converter.subtitle")}
      actions={
        <div className="row">
          <button type="button" className="btn btn-soft" onClick={pickFiles}>
            <Plus size={15} /> {t("converter.pickFiles")}
          </button>
          <button type="button" className="btn btn-soft" onClick={pickOutputDir}>
            {t("converter.outputFolder")}
          </button>
          <button type="button" className="btn btn-primary" onClick={run} disabled={busy || files.length === 0}>
            <RefreshCw size={15} /> {t("converter.convert")}
          </button>
        </div>
      }
    >
      <div className="card">
        <div className="row">
          <label className="field">
            <span>{t("converter.targetFormat")}</span>
            <select value={target} onChange={(event) => setTarget(event.target.value)}>
              {targets.map((candidate) => (
                <option key={candidate} value={candidate}>
                  {candidate.toUpperCase()}
                </option>
              ))}
            </select>
          </label>
          <label className="field grow">
            <span>{t("converter.outputFolder")}</span>
            {isAndroid() ? (
              <input
                readOnly
                value={
                  androidDir
                    ? t("common.androidChosenDestination", { name: androidDir.name })
                    : t("common.androidDefaultDestination")
                }
              />
            ) : (
              <input
                value={outputDir}
                placeholder={t("converter.sameFolder")}
                onChange={(event) => setOutputDir(event.target.value)}
              />
            )}
          </label>
        </div>
        <ul className="file-pick-list">
          {files.map((file) => (
            <li key={file}>{file}</li>
          ))}
          {files.length === 0 ? <li className="muted">{t("converter.noFiles")}</li> : null}
        </ul>
      </div>
      {results.length > 0 ? (
        <div className="card">
          <h3>{t("converter.results")}</h3>
          {results.map((result) => (
            <div key={result.input} className="row">
              <span className={result.ok ? "badge-ok" : "badge-danger"}>{result.ok ? <Check size={12} /> : "!"}</span>
              <span className="grow">{result.input}</span>
              <span className="muted">{result.detail || result.output}</span>
            </div>
          ))}
        </div>
      ) : null}
      <p className="muted">{t("converter.hint")}</p>
    </Screen>
  );
}

// ---------------------------------------------------------------------------
// Document cleaner
// ---------------------------------------------------------------------------

export function CleanerScreen() {
  const t = useT();
  const [path, setPath] = useState("");
  const [options, setOptions] = useState({
    removeMetadata: true,
    removeComments: false,
    optimizeImages: true,
    imageMaxPixels: 1600,
    imageQuality: 82,
  });
  const [footprint, setFootprint] = useState<number | null>(null);
  const [result, setResult] = useState<api.CleanResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [showCleaner, setShowCleaner] = useState(true);
  void setShowCleaner;

  const pick = async () => {
    let selection: string | null = null;
    if (isAndroid()) {
      const [picked] = await pickOfficeFiles(false).catch(() => []);
      selection = picked ?? null;
    } else {
      const chosen = await openDialog({
        multiple: false,
        filters: [{ name: "Office documents", extensions: ["docx", "xlsx", "pptx", "odt", "ods", "odp"] }],
      });
      selection = typeof chosen === "string" ? chosen : null;
    }
    if (selection) {
      setPath(selection);
      setFootprint(await api.imageFootprint(selection).catch(() => null));
    }
  };

  const run = async () => {
    if (!path) return;
    setBusy(true);
    try {
      const cleaned = await api.cleanDocument(path, options);
      setResult(cleaned);
      setFootprint(await api.imageFootprint(path).catch(() => null));
      // Android: the cleaned document lives in the app cache; copy it to a
      // visible location so the user can open or share it.
      if (isAndroid()) await publishOutputs([path]);
    } catch (error) {
      reportError(error, t);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Screen
      title={t("cleaner.title")}
      subtitle={t("cleaner.subtitle")}
      actions={
        <div className="row">
          <button type="button" className="btn btn-soft" onClick={pick}>
            {t("cleaner.pickFile")}
          </button>
          <button type="button" className="btn btn-primary" onClick={run} disabled={!path || busy}>
            <Wand2 size={15} /> {t("cleaner.clean")}
          </button>
        </div>
      }
    >
      {showCleaner ? (
        <div className="card">
          <p className="mono">{path || t("cleaner.noFile")}</p>
          {footprint !== null ? (
            <p className="muted">
              {t("cleaner.images")}: {(footprint / 1024).toFixed(0)} KB
            </p>
          ) : null}
          <div className="stack">
            <label className="check">
              <input
                type="checkbox"
                checked={options.removeMetadata}
                onChange={(event) => setOptions({ ...options, removeMetadata: event.target.checked })}
              />{" "}
              {t("cleaner.removeMetadata")}
            </label>
            <label className="check">
              <input
                type="checkbox"
                checked={options.removeComments}
                onChange={(event) => setOptions({ ...options, removeComments: event.target.checked })}
              />{" "}
              {t("cleaner.removeComments")}
            </label>
            <label className="check">
              <input
                type="checkbox"
                checked={options.optimizeImages}
                onChange={(event) => setOptions({ ...options, optimizeImages: event.target.checked })}
              />{" "}
              {t("cleaner.optimizeImages")}
            </label>
            <div className="row">
              <label className="field">
                <span>{t("cleaner.maxPixels")}</span>
                <input
                  type="number"
                  value={options.imageMaxPixels}
                  onChange={(event) => setOptions({ ...options, imageMaxPixels: Number(event.target.value) })}
                />
              </label>
              <label className="field">
                <span>{t("cleaner.quality")}</span>
                <input
                  type="number"
                  min={40}
                  max={95}
                  value={options.imageQuality}
                  onChange={(event) => setOptions({ ...options, imageQuality: Number(event.target.value) })}
                />
              </label>
            </div>
          </div>
          <p className="muted">{t("cleaner.note")}</p>
        </div>
      ) : null}
      {result ? (
        <div className="card">
          <h3>{t("cleaner.results")}</h3>
          <p>
            {(result.bytesBefore / 1024).toFixed(0)} KB → {(result.bytesAfter / 1024).toFixed(0)} KB
          </p>
          <ul>
            {result.actions.map((action, index) => (
              <li key={index}>{action}</li>
            ))}
            {result.warnings.map((warning, index) => (
              <li key={`w-${index}`} className="muted">
                {warning}
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </Screen>
  );
}

// ---------------------------------------------------------------------------
// PDF forms
// ---------------------------------------------------------------------------

export function PdfFormsScreen() {
  const t = useT();
  const [path, setPath] = useState("");
  const [fields, setFields] = useState<api.PdfFormField[]>([]);
  const [busy, setBusy] = useState(false);

  const pick = async () => {
    try {
      // Android: the SAF picker copies the PDF into the app cache.
      const selection = isAndroid()
        ? (await pickAndroidFiles({ multiple: false, accept: "pdf" }))[0]
        : await openDialog({ multiple: false, filters: [{ name: "PDF", extensions: ["pdf"] }] });
      if (typeof selection === "string") {
        setPath(selection);
        setFields(await api.pdfListForm(selection).catch(() => []));
      }
    } catch (error) {
      reportError(error, t);
    }
  };

  const addField = () => {
    setFields([
      ...fields,
      {
        kind: "text",
        name: `field_${fields.length + 1}`,
        page: 1,
        x: 72,
        y: 120 + fields.length * 36,
        w: 200,
        h: 22,
        value: "",
        options: [],
        fontSize: 11,
        required: false,
      },
    ]);
  };

  const save = async () => {
    if (!path) return;
    const defaultPath = path.replace(/\.pdf$/i, "-form.pdf");
    let output: string | null;
    let androidTarget: AndroidTarget | null = null;
    if (isAndroid()) {
      androidTarget = await pickAndroidSaveTarget(fileBaseName(defaultPath), "application/pdf").catch(() => null);
      if (!androidTarget) return;
      const stageDir = await join(await appCacheDir(), "forms", `form-${uid()}`);
      await invoke("ensure_dir", { path: stageDir });
      output = await join(stageDir, fileBaseName(defaultPath));
    } else {
      output = await saveDialog({ defaultPath, filters: [{ name: "PDF", extensions: ["pdf"] }] });
    }
    if (!output) return;
    setBusy(true);
    try {
      // The save dialog already confirmed replacing an existing file, so the
      // backend must not refuse it again ("error" made every overwrite fail).
      const written = await api.pdfAddForm({ input: path, output, fields, overwrite: "replace" });
      if (androidTarget) {
        const failures = await publishOutputs([written || output], { file: androidTarget });
        if (failures.length) throw new Error(failures[0].error);
      }
      useToasts.getState().push({
        kind: "success",
        title: t("forms.saved"),
        detail: androidTarget ? androidTarget.name : output,
      });
    } catch (error) {
      reportError(error, t);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Screen
      title={t("forms.title")}
      subtitle={t("forms.subtitle")}
      actions={
        <div className="row">
          <button type="button" className="btn btn-soft" onClick={pick}>
            {t("forms.pickPdf")}
          </button>
          <button type="button" className="btn btn-soft" onClick={addField}>
            <Plus size={15} /> {t("forms.addField")}
          </button>
          <button
            type="button"
            className="btn btn-primary"
            onClick={save}
            disabled={!path || busy || fields.length === 0}
          >
            <Save size={15} /> {t("forms.save")}
          </button>
        </div>
      }
    >
      <div className="card">
        <p className="mono">{path || t("forms.noPdf")}</p>
        <table className="table">
          <thead>
            <tr>
              <th>{t("forms.type")}</th>
              <th>{t("forms.name")}</th>
              <th>{t("forms.page")}</th>
              <th>X</th>
              <th>Y</th>
              <th>W</th>
              <th>H</th>
              <th>{t("forms.value")}</th>
              <th>{t("forms.options")}</th>
              <th>
                <span className="sr-only">{t("common.actions")}</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {fields.map((field, index) => (
              <tr key={index}>
                <td>
                  <select
                    aria-label={t("forms.type")}
                    value={field.kind}
                    onChange={(event) =>
                      setFields(
                        fields.map((candidate, position) =>
                          position === index ? { ...candidate, kind: event.target.value } : candidate,
                        ),
                      )
                    }
                  >
                    <option value="text">text</option>
                    <option value="checkbox">checkbox</option>
                    <option value="radio">radio</option>
                    <option value="dropdown">dropdown</option>
                  </select>
                </td>
                <td>
                  <input
                    aria-label={t("forms.name")}
                    value={field.name}
                    onChange={(event) =>
                      setFields(
                        fields.map((candidate, position) =>
                          position === index ? { ...candidate, name: event.target.value } : candidate,
                        ),
                      )
                    }
                  />
                </td>
                {(["page", "x", "y", "w", "h"] as const).map((key) => (
                  <td key={key}>
                    <input
                      type="number"
                      aria-label={t(`forms.${key}`)}
                      value={field[key]}
                      onChange={(event) =>
                        setFields(
                          fields.map((candidate, position) =>
                            position === index ? { ...candidate, [key]: Number(event.target.value) } : candidate,
                          ),
                        )
                      }
                    />
                  </td>
                ))}
                <td>
                  <input
                    aria-label={t("forms.value")}
                    value={field.value}
                    onChange={(event) =>
                      setFields(
                        fields.map((candidate, position) =>
                          position === index ? { ...candidate, value: event.target.value } : candidate,
                        ),
                      )
                    }
                  />
                </td>
                <td>
                  <input
                    aria-label={t("forms.options")}
                    value={field.options.join(",")}
                    placeholder="A,B,C"
                    onChange={(event) =>
                      setFields(
                        fields.map((candidate, position) =>
                          position === index
                            ? {
                                ...candidate,
                                options: event.target.value
                                  .split(",")
                                  .map((option) => option.trim())
                                  .filter(Boolean),
                              }
                            : candidate,
                        ),
                      )
                    }
                  />
                </td>
                <td>
                  <button
                    type="button"
                    className="icon-btn"
                    onClick={() => setFields(fields.filter((_, position) => position !== index))}
                    aria-label={t("forms.removeField")}
                  >
                    <Trash2 size={13} />
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        <p className="muted">{t("forms.hint")}</p>
      </div>
    </Screen>
  );
}

// ---------------------------------------------------------------------------
// Paste helper used by tools that need raw file text
// ---------------------------------------------------------------------------

export async function readTextFile(path: string): Promise<string> {
  const bytes = await readFile(path);
  return new TextDecoder().decode(bytes);
}

export { ArrowLeft, Search };
