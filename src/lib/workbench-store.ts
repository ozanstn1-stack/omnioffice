import { create } from "zustand";
import type { ScreenId } from "./nav";
import { fileBaseName } from "./format";

export type UiDensity = "comfortable" | "compact";

export interface WorkbenchTab {
  id: string;
  screen: ScreenId;
  files: string[];
  title?: string;
  pinned?: boolean;
}

export const HOME_TAB_ID = "tab-home";
export const MAX_WORKBENCH_TABS = 12;
export const WORKBENCH_PREFS_KEY = "omnioffice.workbench.prefs.v1";

interface WorkbenchPrefs {
  splitRatio?: number;
  railExpanded?: boolean;
  lastSplitScreen?: ScreenId;
}

let nextTabSeq = 1;

function makeTabId(screen: ScreenId): string {
  return `tab-${screen}-${nextTabSeq++}`;
}

function tabTitleFromFiles(files: string[]): string | undefined {
  if (files.length === 0) return undefined;
  const first = fileBaseName(files[0]);
  return files.length === 1 ? first : `${first} (+${files.length - 1})`;
}

function clampRatio(ratio: number): number {
  return Math.min(0.72, Math.max(0.28, Number(ratio.toFixed(3))));
}

function readPrefs(): WorkbenchPrefs {
  if (typeof window === "undefined" || typeof window.localStorage === "undefined") return {};
  try {
    const raw = window.localStorage.getItem(WORKBENCH_PREFS_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as WorkbenchPrefs;
    return {
      splitRatio: typeof parsed.splitRatio === "number" ? clampRatio(parsed.splitRatio) : undefined,
      railExpanded: typeof parsed.railExpanded === "boolean" ? parsed.railExpanded : undefined,
      lastSplitScreen: typeof parsed.lastSplitScreen === "string" ? parsed.lastSplitScreen : undefined,
    };
  } catch {
    return {};
  }
}

function writePrefs(patch: WorkbenchPrefs): void {
  if (typeof window === "undefined" || typeof window.localStorage === "undefined") return;
  try {
    const current = readPrefs();
    window.localStorage.setItem(WORKBENCH_PREFS_KEY, JSON.stringify({ ...current, ...patch }));
  } catch {
    // Ignore storage quota or private-mode failures.
  }
}

export function clearWorkbenchPrefs(): void {
  if (typeof window === "undefined" || typeof window.localStorage === "undefined") return;
  try {
    window.localStorage.removeItem(WORKBENCH_PREFS_KEY);
  } catch {
    // Ignore storage errors.
  }
}

/** Publishes the chosen UI density on the document root for CSS rules. */
export function applyDensity(density: UiDensity | undefined): void {
  if (typeof document === "undefined") return;
  document.documentElement.dataset.density = density === "compact" ? "compact" : "comfortable";
}

interface WorkbenchState {
  tabs: WorkbenchTab[];
  activeTabId: string;
  splitScreen: ScreenId | null;
  splitFiles: string[];
  splitRatio: number;
  lastSplitScreen: ScreenId;
  railExpanded: boolean;
  syncScreen: (screen: ScreenId, files: string[]) => void;
  activateTab: (id: string) => WorkbenchTab | null;
  closeTab: (id: string) => WorkbenchTab | null;
  closeOtherTabs: (id: string) => WorkbenchTab | null;
  cycleTab: (delta: 1 | -1) => WorkbenchTab | null;
  openSplit: (screen: ScreenId, files?: string[]) => void;
  closeSplit: () => void;
  toggleSplit: (defaultScreen?: ScreenId, files?: string[]) => void;
  setSplitRatio: (ratio: number) => void;
  toggleRailExpanded: () => void;
  setRailExpanded: (expanded: boolean) => void;
  reset: () => void;
}

const INITIAL_TABS: WorkbenchTab[] = [{ id: HOME_TAB_ID, screen: "home", files: [], pinned: true }];
const initialPrefs = readPrefs();

export const useWorkbench = create<WorkbenchState>((set, get) => ({
  tabs: INITIAL_TABS,
  activeTabId: HOME_TAB_ID,
  splitScreen: null,
  splitFiles: [],
  splitRatio: initialPrefs.splitRatio ?? 0.52,
  lastSplitScreen: initialPrefs.lastSplitScreen ?? "ai",
  railExpanded: initialPrefs.railExpanded ?? false,

  syncScreen: (screen, files) => {
    const { tabs } = get();
    if (screen === "home") {
      const nextTabs = tabs.map((tab) => (tab.id === HOME_TAB_ID ? { ...tab, files: [...files] } : tab));
      set({ tabs: nextTabs, activeTabId: HOME_TAB_ID });
      return;
    }

    const firstFile = files[0] ?? "";
    const existing = tabs.find((tab) => {
      if (tab.screen !== screen) return false;
      if (screen === "reader" && firstFile) return (tab.files[0] ?? "") === firstFile;
      return true;
    });

    if (existing) {
      const title = tabTitleFromFiles(files) ?? existing.title;
      const nextTabs = tabs.map((tab) => (tab.id === existing.id ? { ...tab, files: [...files], title } : tab));
      set({ tabs: nextTabs, activeTabId: existing.id });
      return;
    }

    const created: WorkbenchTab = {
      id: makeTabId(screen),
      screen,
      files: [...files],
      title: tabTitleFromFiles(files),
    };
    const nextTabs = [...tabs, created];
    while (nextTabs.length > MAX_WORKBENCH_TABS) {
      const evictIndex = nextTabs.findIndex((tab) => !tab.pinned && tab.id !== created.id);
      if (evictIndex < 0) break;
      nextTabs.splice(evictIndex, 1);
    }
    set({ tabs: nextTabs, activeTabId: created.id });
  },

  activateTab: (id) => {
    const target = get().tabs.find((tab) => tab.id === id) ?? null;
    if (!target) return null;
    set({ activeTabId: target.id });
    return target;
  },

  closeTab: (id) => {
    const { tabs, activeTabId } = get();
    const index = tabs.findIndex((tab) => tab.id === id);
    if (index < 0 || tabs[index].pinned) return null;
    const nextTabs = tabs.filter((tab) => tab.id !== id);
    if (nextTabs.length === 0) {
      set({ tabs: INITIAL_TABS, activeTabId: HOME_TAB_ID });
      return INITIAL_TABS[0];
    }
    if (activeTabId !== id) {
      set({ tabs: nextTabs });
      return null;
    }
    const fallback = nextTabs[Math.max(0, index - 1)] ?? nextTabs[0];
    set({ tabs: nextTabs, activeTabId: fallback.id });
    return fallback;
  },

  closeOtherTabs: (id) => {
    const { tabs } = get();
    const target = tabs.find((tab) => tab.id === id) ?? tabs[0];
    if (!target) return null;
    const nextTabs = tabs.filter((tab) => tab.pinned || tab.id === target.id);
    set({ tabs: nextTabs, activeTabId: target.id });
    return target;
  },

  cycleTab: (delta) => {
    const { tabs, activeTabId } = get();
    if (tabs.length <= 1) return tabs[0] ?? null;
    const currentIndex = Math.max(
      0,
      tabs.findIndex((tab) => tab.id === activeTabId),
    );
    const nextIndex = (currentIndex + delta + tabs.length) % tabs.length;
    const target = tabs[nextIndex] ?? null;
    if (target) set({ activeTabId: target.id });
    return target;
  },

  openSplit: (screen, files = []) => {
    writePrefs({ lastSplitScreen: screen });
    set({ splitScreen: screen, splitFiles: [...files], lastSplitScreen: screen });
  },

  closeSplit: () => set({ splitScreen: null, splitFiles: [] }),

  toggleSplit: (defaultScreen, files = []) => {
    const { splitScreen, lastSplitScreen } = get();
    if (splitScreen) {
      set({ splitScreen: null, splitFiles: [] });
    } else {
      const target = defaultScreen ?? lastSplitScreen ?? "ai";
      writePrefs({ lastSplitScreen: target });
      set({ splitScreen: target, splitFiles: [...files], lastSplitScreen: target });
    }
  },

  setSplitRatio: (ratio) => {
    const clamped = clampRatio(ratio);
    writePrefs({ splitRatio: clamped });
    set({ splitRatio: clamped });
  },

  toggleRailExpanded: () =>
    set((state) => {
      const next = !state.railExpanded;
      writePrefs({ railExpanded: next });
      return { railExpanded: next };
    }),

  setRailExpanded: (railExpanded) => {
    writePrefs({ railExpanded });
    set({ railExpanded });
  },

  reset: () => {
    clearWorkbenchPrefs();
    set({
      tabs: INITIAL_TABS,
      activeTabId: HOME_TAB_ID,
      splitScreen: null,
      splitFiles: [],
      splitRatio: 0.52,
      lastSplitScreen: "ai",
      railExpanded: false,
    });
  },
}));
