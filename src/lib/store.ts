import { create } from "zustand";
import {
  addRecent as apiAddRecent,
  clearRecent as apiClearRecent,
  loadRecent,
  loadSettings,
  ocrLanguages,
  onProgress,
  saveSettings,
  engineStatus,
  toAppError,
} from "./api";
import {
  DEFAULT_SETTINGS,
  type AppError,
  type EngineStatus,
  type OcrLanguage,
  type ProgressPayload,
  type RecentEntry,
  type Settings,
} from "./types";

interface SettingsState {
  settings: Settings;
  loaded: boolean;
  engine: EngineStatus | null;
  languages: OcrLanguage[];
  init: () => Promise<void>;
  update: (patch: Partial<Settings>) => Promise<void>;
}

export const useSettings = create<SettingsState>((set, get) => ({
  settings: DEFAULT_SETTINGS,
  loaded: false,
  engine: null,
  languages: [],
  init: async () => {
    try {
      const [stored, engine, languages] = await Promise.all([
        loadSettings().catch(() => ({})),
        engineStatus().catch(() => null),
        ocrLanguages().catch(() => []),
      ]);
      // A settings file written by an older version means this is not a
      // first run, so upgraders do not get the welcome card.
      const upgraded = Object.keys(stored).length > 0;
      const settings: Settings = { ...DEFAULT_SETTINGS, onboardingDone: upgraded, ...stored };
      set({ settings, engine, languages, loaded: true });
      applyTheme(settings.theme);
      void import("./api").then(({ logFrontend }) => logFrontend("info", `engines: ${JSON.stringify(engine)}`));
    } catch {
      set({ loaded: true });
    }
  },
  update: async (patch) => {
    const next = { ...get().settings, ...patch };
    set({ settings: next });
    if (patch.theme) applyTheme(next.theme);
    await saveSettings(next).catch(() => undefined);
  },
}));

export function applyTheme(theme: Settings["theme"]) {
  const root = document.documentElement;
  const prefersDark =
    theme === "system"
      ? window.matchMedia("(prefers-color-scheme: dark)").matches
      : theme === "dark" || theme === "midnight";
  root.classList.toggle("dark", prefersDark);
  root.dataset.theme = theme === "system" ? (prefersDark ? "dark" : "light") : theme;
}

// ---------------------------------------------------------------------------
// Toasts
// ---------------------------------------------------------------------------

export interface Toast {
  id: number;
  kind: "info" | "success" | "error";
  title: string;
  detail?: string;
}

interface ToastState {
  toasts: Toast[];
  push: (toast: Omit<Toast, "id">) => void;
  dismiss: (id: number) => void;
}

let toastCounter = 1;

export const useToasts = create<ToastState>((set, get) => ({
  toasts: [],
  push: (toast) => {
    const id = toastCounter++;
    set({ toasts: [...get().toasts, { ...toast, id }] });
    if (toast.kind !== "error") {
      setTimeout(() => get().dismiss(id), 4200);
    } else {
      setTimeout(() => get().dismiss(id), 9000);
    }
  },
  dismiss: (id) => set({ toasts: get().toasts.filter((t) => t.id !== id) }),
}));

// ---------------------------------------------------------------------------
// Jobs / progress
// ---------------------------------------------------------------------------

interface JobsState {
  progress: Record<string, ProgressPayload>;
  attach: () => Promise<() => void>;
}

export const useJobs = create<JobsState>((set, get) => ({
  progress: {},
  attach: async () => {
    const unlisten = await onProgress((payload) => {
      set({ progress: { ...get().progress, [payload.jobId]: payload } });
    });
    return unlisten;
  },
}));

/** Progress of a single job (reading mode search). */
export function useJobProgress(jobId: string): ProgressPayload | null {
  return useJobs((state) => (jobId ? (state.progress[jobId] ?? null) : null));
}

// ---------------------------------------------------------------------------
// Recent files (paths + timestamps only)
// ---------------------------------------------------------------------------

interface RecentState {
  entries: RecentEntry[];
  refresh: () => Promise<void>;
  add: (entry: RecentEntry) => Promise<void>;
  clear: () => Promise<void>;
}

export const useRecent = create<RecentState>((set) => ({
  entries: [],
  refresh: async () => {
    const entries = await loadRecent().catch(() => []);
    set({ entries });
  },
  add: async (entry) => {
    await apiAddRecent(entry).catch(() => undefined);
    const entries = await loadRecent().catch(() => []);
    set({ entries });
  },
  clear: async () => {
    await apiClearRecent().catch(() => undefined);
    set({ entries: [] });
  },
}));

// ---------------------------------------------------------------------------
// Drop routing: the active screen registers a handler for OS file drops.
// ---------------------------------------------------------------------------

interface DropState {
  handler: ((paths: string[]) => void) | null;
  setHandler: (handler: ((paths: string[]) => void) | null) => void;
}

export const useDrop = create<DropState>((set) => ({
  handler: null,
  setHandler: (handler) => set({ handler }),
}));

// ---------------------------------------------------------------------------
// Development launch context (screenshot/validation automation only)
// ---------------------------------------------------------------------------

interface DevState {
  startScreen: string | null;
  newTab: string | null;
  files: string[] | null;
  autoRun: boolean;
  tab: string | null;
  set: (context: {
    startScreen: string | null;
    newTab?: string | null;
    files: string[] | null;
    autoRun: boolean;
    tab?: string | null;
  }) => void;
}

export const useDev = create<DevState>((set) => ({
  startScreen: null,
  newTab: null,
  files: null,
  autoRun: false,
  tab: null,
  set: (context) => set({ ...context, newTab: context.newTab ?? null, tab: context.tab ?? null }),
}));

// ---------------------------------------------------------------------------
// Password prompting (shared modal)
// ---------------------------------------------------------------------------

interface PasswordState {
  request: {
    path: string;
    resolve: (password: string | null) => void;
  } | null;
  ask: (path: string) => Promise<string | null>;
  answer: (password: string | null) => void;
}

export const usePasswordPrompt = create<PasswordState>((set, get) => ({
  request: null,
  ask: (path) =>
    new Promise<string | null>((resolve) => {
      set({ request: { path, resolve } });
    }),
  answer: (password) => {
    const current = get().request;
    if (current) {
      current.resolve(password);
      set({ request: null });
    }
  },
}));

// ---------------------------------------------------------------------------
// Overwrite confirmation (global dialog, used by every tool session)
// ---------------------------------------------------------------------------

interface OverwriteState {
  prompt: { fileName: string; resolve: (mode: "error" | "replace" | "unique_name" | null) => void } | null;
  ask: (fileName: string) => Promise<"error" | "replace" | "unique_name" | null>;
  answer: (mode: "error" | "replace" | "unique_name" | null) => void;
}

export const useOverwritePrompt = create<OverwriteState>((set, get) => ({
  prompt: null,
  ask: (fileName) =>
    new Promise((resolve) => {
      set({ prompt: { fileName, resolve } });
    }),
  answer: (mode) => {
    const current = get().prompt;
    if (current) {
      current.resolve(mode);
      set({ prompt: null });
    }
  },
}));

/**
 * The user-facing text for any thrown value: the localized message for the
 * error's code when one exists, otherwise the technical detail. Screens that
 * show an error inline (per-file results, launch failures) should use this
 * instead of reaching into `String(error)` themselves.
 */
export function errorMessage(
  error: unknown,
  t: (key: string, params?: Record<string, string | number>) => string,
): string {
  const appError: AppError = toAppError(error);
  const key = `errors.${appError.code}`;
  const localized = t(key);
  return localized === key ? appError.message : localized;
}

/** Reports an error to the user with a localized, friendly message. */
export function reportError(error: unknown, t: (key: string, params?: Record<string, string | number>) => string) {
  const appError: AppError = toAppError(error);
  // Diagnostics: error code + message only (never document content).
  void import("./api").then(({ logFrontend }) => logFrontend("app-error", `${appError.code}: ${appError.message}`));
  useToasts.getState().push({
    kind: "error",
    title: t("errors.title"),
    detail: errorMessage(error, t),
  });
}
