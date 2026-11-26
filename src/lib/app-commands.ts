import { registerCommand, unregisterCommand } from "./commands";
import type { Navigate, ScreenId } from "./nav";
import { enabledRoutes } from "./routes";
import { useWorkbench } from "./workbench-store";

interface GlobalCommandDeps {
  compactNav: boolean;
  screen: ScreenId;
  navigate: Navigate;
  pickFiles: () => void;
  openPalette: () => void;
  openSearch: () => void;
}

/** Registers view navigation, global search/palette, and desktop workbench commands. */
export function registerAppCommands({
  compactNav,
  screen,
  navigate,
  pickFiles,
  openPalette,
  openSearch,
}: GlobalCommandDeps): () => void {
  for (const view of enabledRoutes()) {
    registerCommand({
      id: `view.${view.id}`,
      titleKey: view.labelKey,
      category: view.category,
      keybinding: view.keybinding,
      enabled: () => true,
      execute: () => navigate(view.id),
    });
  }
  registerCommand({
    id: "file.open",
    titleKey: "common.openFile",
    category: "file",
    keybinding: "Ctrl+O",
    enabled: () => true,
    execute: () => pickFiles(),
  });
  registerCommand({
    id: "app.commandPalette",
    titleKey: "palette.open",
    category: "view",
    keybinding: "Ctrl+Shift+P",
    enabled: () => true,
    execute: openPalette,
  });
  registerCommand({
    id: "app.globalSearch",
    titleKey: "palette.search",
    category: "view",
    keybinding: "Ctrl+Shift+F",
    enabled: () => true,
    execute: openSearch,
  });
  registerCommand({
    id: "app.quickOmnibar",
    titleKey: "workbench.commandHint",
    category: "view",
    keybinding: "Ctrl+K",
    enabled: () => true,
    execute: openSearch,
  });
  registerCommand({
    id: "workbench.toggleSplit",
    titleKey: "workbench.toggleSplitCmd",
    category: "view",
    keybinding: "Ctrl+\\",
    enabled: () => !compactNav,
    execute: () => {
      const wb = useWorkbench.getState();
      const active = wb.tabs.find((tab) => tab.id === wb.activeTabId);
      wb.toggleSplit(screen === "ai" ? "reader" : undefined, active?.files ?? []);
    },
  });
  registerCommand({
    id: "workbench.nextTab",
    titleKey: "workbench.nextTabCmd",
    category: "view",
    keybinding: "Ctrl+Shift+]",
    enabled: () => !compactNav,
    execute: () => {
      const next = useWorkbench.getState().cycleTab(1);
      if (next) navigate(next.screen, { files: next.files });
    },
  });
  registerCommand({
    id: "workbench.prevTab",
    titleKey: "workbench.prevTabCmd",
    category: "view",
    keybinding: "Ctrl+Shift+[",
    enabled: () => !compactNav,
    execute: () => {
      const prev = useWorkbench.getState().cycleTab(-1);
      if (prev) navigate(prev.screen, { files: prev.files });
    },
  });

  const ids = [
    ...enabledRoutes().map((view) => `view.${view.id}`),
    "file.open",
    "app.commandPalette",
    "app.globalSearch",
    "app.quickOmnibar",
    "workbench.toggleSplit",
    "workbench.nextTab",
    "workbench.prevTab",
  ];
  return () => ids.forEach((id) => unregisterCommand(id));
}
