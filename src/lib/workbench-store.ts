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

let nextTabSeq = 1;

function makeTabId(screen: ScreenId): string {
  return `tab-${screen}-${nextTabSeq++}`;
}

function tabTitleFromFiles(files: string[]): string | undefined {
  if (files.length === 0) return undefined;
  const first = fileBaseName(files[0]);
  return files.length === 1 ? first : `${first} (+${files.length - 1})`;
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
  railExpanded: boolean;
  syncScreen: (screen: ScreenId, files: string[]) => void;
  activateTab: (id: string) => WorkbenchTab | null;
  closeTab: (id: string) => WorkbenchTab | null;
  openSplit: (screen: ScreenId, files?: string[]) => void;
  closeSplit: () => void;
  toggleSplit: (defaultScreen?: ScreenId) => void;
  setSplitRatio: (ratio: number) => void;
  toggleRailExpanded: () => void;
  setRailExpanded: (expanded: boolean) => void;
  reset: () => void;
}

const INITIAL_TABS: WorkbenchTab[] = [{ id: HOME_TAB_ID, screen: "home", files: [], pinned: true }];

export const useWorkbench = create<WorkbenchState>((set, get) => ({
  tabs: INITIAL_TABS,
  activeTabId: HOME_TAB_ID,
  splitScreen: null,
  splitFiles: [],
  splitRatio: 0.52,
  railExpanded: false,

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

  openSplit: (screen, files = []) => set({ splitScreen: screen, splitFiles: [...files] }),

  closeSplit: () => set({ splitScreen: null, splitFiles: [] }),

  toggleSplit: (defaultScreen = "ai") => {
    const { splitScreen } = get();
    if (splitScreen) {
      set({ splitScreen: null, splitFiles: [] });
    } else {
      set({ splitScreen: defaultScreen, splitFiles: [] });
    }
  },

  setSplitRatio: (ratio) => {
    const clamped = Math.min(0.72, Math.max(0.28, Number(ratio.toFixed(3))));
    set({ splitRatio: clamped });
  },

  toggleRailExpanded: () => set((state) => ({ railExpanded: !state.railExpanded })),

  setRailExpanded: (railExpanded) => set({ railExpanded }),

  reset: () =>
    set({
      tabs: INITIAL_TABS,
      activeTabId: HOME_TAB_ID,
      splitScreen: null,
      splitFiles: [],
      splitRatio: 0.52,
      railExpanded: false,
    }),
}));
