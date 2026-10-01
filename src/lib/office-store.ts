/**
 * Workspace state for the office modules: open document tabs, autosave /
 * recovery bookkeeping and the local productivity stores (Notes, Planner,
 * Data sheets, favourites). Every store persists through the Rust store
 * commands, so data lives in the app data directory and never in the cloud.
 */
import { create } from "zustand";
import type { Deck, OfficeKind, TextDocument, Workbook } from "./office-types";
import { newDeck, newTextDocument, newWorkbook, uid } from "./office-types";
export type { Deck, Slide, SlideObject, TextDocument, Workbook } from "./office-types";
import * as api from "./office-api";
import { toAppError } from "./api";

export type OfficeModel = TextDocument | Workbook | Deck;

export interface OfficeTab {
  id: string;
  kind: OfficeKind;
  title: string;
  path: string | null;
  model: OfficeModel;
  dirty: boolean;
  warnings: string[];
  lastSavedAt: string | null;
  openedAt: string;
  /**
   * SHA-256 of the file when it was opened or last saved. A save compares the
   * on-disk hash against this so an external change is never overwritten
   * silently; null means "never fingerprinted" (a brand-new unit).
   */
  fingerprint?: string | null;
}

interface OfficeTabsState {
  tabs: OfficeTab[];
  activeId: string | null;
  create: (kind: OfficeKind, title?: string, model?: OfficeModel) => string;
  open: (tab: Omit<OfficeTab, "id" | "openedAt" | "dirty" | "lastSavedAt">) => string;
  activate: (id: string) => void;
  close: (id: string) => void;
  closeOthers: (id: string) => void;
  patch: (id: string, patch: Partial<Omit<OfficeTab, "id">>) => void;
  /** Mutates the model and marks the tab dirty. */
  edit: (id: string, mutate: (model: OfficeModel) => OfficeModel) => void;
  setDirty: (id: string, dirty: boolean) => void;
  markSaved: (id: string, path: string, fingerprint?: string | null) => void;
  /** Records the hash captured when a file was opened. */
  setFingerprint: (id: string, fingerprint: string | null) => void;
}

export const useOfficeTabs = create<OfficeTabsState>((set, get) => ({
  tabs: [],
  activeId: null,
  create: (kind, title, model) => {
    const id = uid();
    const tab: OfficeTab = {
      id,
      kind,
      title: title ?? (kind === "writer" ? "Untitled document" : kind === "calc" ? "Untitled spreadsheet" : "Untitled presentation"),
      path: null,
      model: (model ?? defaultModelFor(kind)) as OfficeModel,
      dirty: false,
      warnings: [],
      lastSavedAt: new Date().toISOString(),
      openedAt: new Date().toISOString(),
    };
    set({ tabs: [...get().tabs, tab], activeId: id });
    return id;
  },
  open: (tab) => {
    const existing = get().tabs.find((candidate) => candidate.path && candidate.path === tab.path);
    if (existing) {
      set({ activeId: existing.id });
      return existing.id;
    }
    const id = uid();
    const entry: OfficeTab = {
      ...tab,
      id,
      dirty: false,
      lastSavedAt: new Date().toISOString(),
      openedAt: new Date().toISOString(),
    };
    set({ tabs: [...get().tabs, entry], activeId: id });
    return id;
  },
  activate: (id) => set({ activeId: id }),
  close: (id) => {
    const tabs = get().tabs.filter((tab) => tab.id !== id);
    const activeId = get().activeId === id ? tabs[tabs.length - 1]?.id ?? null : get().activeId;
    set({ tabs, activeId });
  },
  closeOthers: (id) => {
    const tab = get().tabs.find((candidate) => candidate.id === id);
    if (tab) set({ tabs: [tab], activeId: id });
  },
  patch: (id, patch) => set({ tabs: get().tabs.map((tab) => (tab.id === id ? { ...tab, ...patch } : tab)) }),
  edit: (id, mutate) =>
    set({
      tabs: get().tabs.map((tab) => (tab.id === id ? { ...tab, model: mutate(tab.model), dirty: true } : tab)),
    }),
  setDirty: (id, dirty) => set({ tabs: get().tabs.map((tab) => (tab.id === id ? { ...tab, dirty } : tab)) }),
  markSaved: (id, path, fingerprint) =>
    set({
      tabs: get().tabs.map((tab) =>
        tab.id === id
          ? {
              ...tab,
              path,
              dirty: false,
              lastSavedAt: new Date().toISOString(),
              ...(fingerprint !== undefined ? { fingerprint } : {}),
            }
          : tab,
      ),
    }),
  setFingerprint: (id, fingerprint) =>
    set({ tabs: get().tabs.map((tab) => (tab.id === id ? { ...tab, fingerprint } : tab)) }),
}));

function defaultModelFor(kind: OfficeKind): OfficeModel {
  if (kind === "calc") return newWorkbook();
  if (kind === "impress") return newDeck();
  return newTextDocument();
}

// ---------------------------------------------------------------------------
// Autosave / recovery
// ---------------------------------------------------------------------------

export type RecoveryRecord = api.RecoveryEntry;

interface RecoveryState {
  records: RecoveryRecord[];
  busy: boolean;
  refresh: () => Promise<RecoveryRecord[]>;
  save: (tab: OfficeTab) => Promise<void>;
  discard: (documentId: string) => Promise<void>;
  restore: (documentId: string) => Promise<{ kind: OfficeKind; model: unknown } | null>;
}

export const useRecovery = create<RecoveryState>((set) => ({
  records: [],
  busy: false,
  refresh: async () => {
    try {
      const list = await api.recoveryList();
      set({ records: list });
      return list;
    } catch {
      set({ records: [] });
      return [];
    }
  },
  save: async (tab) => {
    if (!tab.dirty) return;
    try {
      await api.recoverySave(tab.id, tab.kind, tab.title, tab.path, tab.model);
    } catch {
      // Autosave failures are silent by design; the undo history in the editor
      // and the previous save remain valid.
    }
  },
  discard: async (documentId) => {
    await api.recoveryDiscard(documentId).catch(() => undefined);
    const list = await api.recoveryList().catch(() => []);
    set({ records: list });
  },
  restore: async (documentId) => {
    try {
      const record = (await api.recoveryList()).find((entry) => entry.documentId === documentId);
      const model = await api.recoveryLoad(documentId);
      if (model === null || model === undefined) return null;
      return { kind: (record?.kind as OfficeKind) ?? "writer", model };
    } catch {
      return null;
    }
  },
}));

// ---------------------------------------------------------------------------
// Favourites / pinned office files
// ---------------------------------------------------------------------------

export interface FavoriteEntry {
  path: string;
  kind: OfficeKind | "pdf" | "image" | "other";
  pinned: boolean;
  favorite: boolean;
  addedAt: string;
}

interface FavoritesState {
  entries: FavoriteEntry[];
  loaded: boolean;
  load: () => Promise<void>;
  toggle: (path: string, kind: FavoriteEntry["kind"], field: "pinned" | "favorite") => Promise<void>;
  isPinned: (path: string) => boolean;
  isFavorite: (path: string) => boolean;
}

const FAVORITES_KEY = "office-favorites";

export const useFavorites = create<FavoritesState>((set, get) => ({
  entries: [],
  loaded: false,
  load: async () => {
    const stored = await api.storeLoad<FavoriteEntry[]>(FAVORITES_KEY).catch(() => null);
    set({ entries: stored ?? [], loaded: true });
  },
  toggle: async (path, kind, field) => {
    const existing = get().entries.find((entry) => entry.path === path);
    let entries: FavoriteEntry[];
    if (existing) {
      entries = get().entries.map((entry) => (entry.path === path ? { ...entry, [field]: !entry[field] } : entry));
    } else {
      entries = [...get().entries, { path, kind, pinned: field === "pinned", favorite: field === "favorite", addedAt: new Date().toISOString() }];
    }
    set({ entries });
    await api.storeSave(FAVORITES_KEY, entries).catch(() => undefined);
  },
  isPinned: (path) => get().entries.some((entry) => entry.path === path && entry.pinned),
  isFavorite: (path) => get().entries.some((entry) => entry.path === path && entry.favorite),
}));

// ---------------------------------------------------------------------------
// Notes
// ---------------------------------------------------------------------------

export interface NoteFolder {
  id: string;
  name: string;
  color: string | null;
}

export interface Note {
  id: string;
  title: string;
  body: string;
  folderId: string | null;
  tags: string[];
  pinned: boolean;
  favorite: boolean;
  archived: boolean;
  color: string | null;
  createdAt: string;
  updatedAt: string;
}

interface NotesState {
  notes: Note[];
  folders: NoteFolder[];
  loaded: boolean;
  load: () => Promise<void>;
  create: (partial?: Partial<Note>) => Note;
  update: (id: string, patch: Partial<Note>) => void;
  remove: (id: string) => void;
  addFolder: (name: string) => void;
  removeFolder: (id: string) => void;
}

const NOTES_KEY = "office-notes";

function persistNotes(notes: Note[], folders: NoteFolder[]) {
  void api.storeSave(NOTES_KEY, { notes, folders }).catch(() => undefined);
}

export const useNotes = create<NotesState>((set, get) => ({
  notes: [],
  folders: [],
  loaded: false,
  load: async () => {
    const stored = await api
      .storeLoad<{ notes: Note[]; folders: NoteFolder[] }>(NOTES_KEY)
      .catch(() => null);
    set({ notes: stored?.notes ?? [], folders: stored?.folders ?? [], loaded: true });
  },
  create: (partial) => {
    const now = new Date().toISOString();
    const note: Note = {
      id: uid(),
      title: partial?.title ?? "New note",
      body: partial?.body ?? "",
      folderId: partial?.folderId ?? null,
      tags: partial?.tags ?? [],
      pinned: false,
      favorite: false,
      archived: false,
      color: partial?.color ?? null,
      createdAt: now,
      updatedAt: now,
    };
    const notes = [note, ...get().notes];
    set({ notes });
    persistNotes(notes, get().folders);
    return note;
  },
  update: (id, patch) => {
    const notes = get().notes.map((note) => (note.id === id ? { ...note, ...patch, updatedAt: new Date().toISOString() } : note));
    set({ notes });
    persistNotes(notes, get().folders);
  },
  remove: (id) => {
    const notes = get().notes.filter((note) => note.id !== id);
    set({ notes });
    persistNotes(notes, get().folders);
  },
  addFolder: (name) => {
    const folders = [...get().folders, { id: uid(), name, color: null }];
    set({ folders });
    persistNotes(get().notes, folders);
  },
  removeFolder: (id) => {
    const folders = get().folders.filter((folder) => folder.id !== id);
    const notes = get().notes.map((note) => (note.folderId === id ? { ...note, folderId: null } : note));
    set({ folders, notes });
    persistNotes(notes, folders);
  },
}));

// ---------------------------------------------------------------------------
// Planner
// ---------------------------------------------------------------------------

export interface PlannerTask {
  id: string;
  title: string;
  date: string;
  done: boolean;
  priority: "low" | "normal" | "high";
  note: string;
  createdAt: string;
}

interface PlannerState {
  tasks: PlannerTask[];
  loaded: boolean;
  load: () => Promise<void>;
  add: (date: string, title: string) => void;
  update: (id: string, patch: Partial<PlannerTask>) => void;
  remove: (id: string) => void;
}

const PLANNER_KEY = "office-planner";

export const usePlanner = create<PlannerState>((set, get) => ({
  tasks: [],
  loaded: false,
  load: async () => {
    const stored = await api.storeLoad<PlannerTask[]>(PLANNER_KEY).catch(() => null);
    set({ tasks: stored ?? [], loaded: true });
  },
  add: (date, title) => {
    const tasks = [...get().tasks, { id: uid(), title, date, done: false, priority: "normal" as const, note: "", createdAt: new Date().toISOString() }];
    set({ tasks });
    void api.storeSave(PLANNER_KEY, tasks).catch(() => undefined);
  },
  update: (id, patch) => {
    const tasks = get().tasks.map((task) => (task.id === id ? { ...task, ...patch } : task));
    set({ tasks });
    void api.storeSave(PLANNER_KEY, tasks).catch(() => undefined);
  },
  remove: (id) => {
    const tasks = get().tasks.filter((task) => task.id !== id);
    set({ tasks });
    void api.storeSave(PLANNER_KEY, tasks).catch(() => undefined);
  },
}));

// ---------------------------------------------------------------------------
// Simple data tables
// ---------------------------------------------------------------------------

export interface DataSheet {
  id: string;
  name: string;
  columns: string[];
  rows: string[][];
  updatedAt: string;
}

interface DataState {
  sheets: DataSheet[];
  loaded: boolean;
  load: () => Promise<void>;
  create: (name?: string) => DataSheet;
  update: (id: string, patch: Partial<DataSheet>) => void;
  remove: (id: string) => void;
}

const DATA_KEY = "office-data";

export const useDataSheets = create<DataState>((set, get) => ({
  sheets: [],
  loaded: false,
  load: async () => {
    const stored = await api.storeLoad<DataSheet[]>(DATA_KEY).catch(() => null);
    set({ sheets: stored ?? [], loaded: true });
  },
  create: (name) => {
    const sheet: DataSheet = {
      id: uid(),
      name: name ?? `Table ${get().sheets.length + 1}`,
      columns: ["Name", "Email", "Phone", "Date", "Category", "Status"],
      rows: [["", "", "", "", "", ""]],
      updatedAt: new Date().toISOString(),
    };
    const sheets = [...get().sheets, sheet];
    set({ sheets });
    void api.storeSave(DATA_KEY, sheets).catch(() => undefined);
    return sheet;
  },
  update: (id, patch) => {
    const sheets = get().sheets.map((sheet) => (sheet.id === id ? { ...sheet, ...patch, updatedAt: new Date().toISOString() } : sheet));
    set({ sheets });
    void api.storeSave(DATA_KEY, sheets).catch(() => undefined);
  },
  remove: (id) => {
    const sheets = get().sheets.filter((sheet) => sheet.id !== id);
    set({ sheets });
    void api.storeSave(DATA_KEY, sheets).catch(() => undefined);
  },
}));

// ---------------------------------------------------------------------------
// Draw documents
// ---------------------------------------------------------------------------

export interface DrawDocument {
  id: string;
  name: string;
  width: number;
  height: number;
  elements: DrawElement[];
  updatedAt: string;
}

export interface DrawElement {
  id: string;
  kind: "rect" | "ellipse" | "line" | "arrow" | "text" | "path";
  x: number;
  y: number;
  w: number;
  h: number;
  points?: Array<{ x: number; y: number }>;
  text?: string;
  fill?: string | null;
  stroke?: string;
  strokeWidth?: number;
}

interface DrawState {
  documents: DrawDocument[];
  loaded: boolean;
  load: () => Promise<void>;
  save: (document: DrawDocument) => void;
  remove: (id: string) => void;
}

const DRAW_KEY = "office-draw";

export const useDraw = create<DrawState>((set, get) => ({
  documents: [],
  loaded: false,
  load: async () => {
    const stored = await api.storeLoad<DrawDocument[]>(DRAW_KEY).catch(() => null);
    set({ documents: stored ?? [], loaded: true });
  },
  save: (document) => {
    const existing = get().documents.find((candidate) => candidate.id === document.id);
    const documents = existing
      ? get().documents.map((candidate) => (candidate.id === document.id ? document : candidate))
      : [...get().documents, document];
    set({ documents });
    void api.storeSave(DRAW_KEY, documents).catch(() => undefined);
  },
  remove: (id) => {
    const documents = get().documents.filter((candidate) => candidate.id !== id);
    set({ documents });
    void api.storeSave(DRAW_KEY, documents).catch(() => undefined);
  },
}));

// ---------------------------------------------------------------------------
// Opening files into the workspace (shared by Home, the launcher, toolbars,
// drag & drop, file associations and the command line)
// ---------------------------------------------------------------------------

export const OFFICE_EXTENSIONS = ["docx", "docm", "dotx", "odt", "rtf", "txt", "md", "markdown", "html", "htm", "xlsx", "xlsm", "xls", "ods", "csv", "tsv", "pptx", "pptm", "odp", "oswk"];

export function isOfficePath(path: string): boolean {
  const extension = (path.split(".").pop() ?? "").toLowerCase();
  return OFFICE_EXTENSIONS.includes(extension);
}

export function isSpreadsheetPath(path: string): boolean {
  return ["xlsx", "xlsm", "xls", "ods", "csv", "tsv"].includes((path.split(".").pop() ?? "").toLowerCase());
}

export function isPresentationPath(path: string): boolean {
  return ["pptx", "pptm", "odp"].includes((path.split(".").pop() ?? "").toLowerCase());
}

export interface OpenPathResult {
  ok: boolean;
  kind?: OfficeKind;
  error?: string;
}

/** Opens a document file and adds it as an editor tab. */
export async function openOfficePath(path: string): Promise<OpenPathResult> {
  try {
    const result = await api.openDocument(path);
    const id = useOfficeTabs.getState().open({
      kind: result.kind,
      title: result.title || path.split(/[\\/]/).pop() || "Document",
      path: result.path,
      model: result.model as OfficeModel,
      warnings: result.warnings,
    });
    // Capture the file's hash now so a later save can tell whether another
    // program changed it (a missing fingerprint simply skips the check).
    try {
      const fingerprint = await api.fileFingerprint(result.path);
      useOfficeTabs.getState().setFingerprint(id, fingerprint.exists ? fingerprint.sha256 : null);
    } catch {
      useOfficeTabs.getState().setFingerprint(id, null);
    }
    return { ok: true, kind: result.kind };
  } catch (error) {
    return { ok: false, error: toAppError(error).message };
  }
}