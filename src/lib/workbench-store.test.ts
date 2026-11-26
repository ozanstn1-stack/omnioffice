import { beforeEach, describe, expect, it } from "vitest";
import { applyDensity, HOME_TAB_ID, MAX_WORKBENCH_TABS, useWorkbench, WORKBENCH_PREFS_KEY } from "./workbench-store";
import type { ScreenId } from "./nav";

describe("workbench store (v8.2.0)", () => {
  beforeEach(() => {
    useWorkbench.getState().reset();
    delete document.documentElement.dataset.density;
  });

  it("keeps Home pinned and creates live tabs for visited screens", () => {
    useWorkbench.getState().syncScreen("merge", []);
    useWorkbench.getState().syncScreen("reader", ["/docs/report.pdf"]);
    const { tabs, activeTabId } = useWorkbench.getState();
    expect(tabs.map((tab) => tab.screen)).toEqual(["home", "merge", "reader"]);
    expect(tabs[2].title).toBe("report.pdf");
    expect(activeTabId).toBe(tabs[2].id);

    // Pinned Home cannot be closed.
    expect(useWorkbench.getState().closeTab(HOME_TAB_ID)).toBeNull();
    expect(useWorkbench.getState().tabs[0].id).toBe(HOME_TAB_ID);

    // Closing the active tab falls back to its neighbour.
    const fallback = useWorkbench.getState().closeTab(tabs[2].id);
    expect(fallback?.screen).toBe("merge");
    expect(useWorkbench.getState().tabs).toHaveLength(2);
  });

  it("caps open tabs at MAX_WORKBENCH_TABS by evicting the oldest non-pinned tab", () => {
    const screens: ScreenId[] = [
      "merge",
      "split",
      "compress",
      "ocr",
      "watermark",
      "annotate",
      "redact",
      "compare",
      "inspect",
      "metadata",
      "batch",
      "history",
      "settings",
    ];
    for (const screen of screens) useWorkbench.getState().syncScreen(screen, []);
    const { tabs } = useWorkbench.getState();
    expect(tabs).toHaveLength(MAX_WORKBENCH_TABS);
    expect(tabs[0].id).toBe(HOME_TAB_ID);
    expect(tabs[tabs.length - 1].screen).toBe("settings");
  });

  it("manages Split View state and clamps the split ratio", () => {
    useWorkbench.getState().toggleSplit("ai");
    expect(useWorkbench.getState().splitScreen).toBe("ai");
    useWorkbench.getState().setSplitRatio(0.95);
    expect(useWorkbench.getState().splitRatio).toBe(0.72);
    useWorkbench.getState().setSplitRatio(0.1);
    expect(useWorkbench.getState().splitRatio).toBe(0.28);
    useWorkbench.getState().toggleSplit();
    expect(useWorkbench.getState().splitScreen).toBeNull();
  });

  it("applies comfortable and compact density attributes on documentElement", () => {
    applyDensity("compact");
    expect(document.documentElement.dataset.density).toBe("compact");
    applyDensity("comfortable");
    expect(document.documentElement.dataset.density).toBe("comfortable");
  });

  it("persists layout preferences without file paths and closes other tabs cleanly", () => {
    useWorkbench.getState().syncScreen("merge", ["/secret/a.pdf"]);
    useWorkbench.getState().syncScreen("reader", ["/secret/b.pdf"]);
    useWorkbench.getState().setSplitRatio(0.41);
    useWorkbench.getState().openSplit("notes", ["/secret/b.pdf"]);

    const saved = window.localStorage.getItem(WORKBENCH_PREFS_KEY) ?? "";
    expect(saved).toContain('"splitRatio":0.41');
    expect(saved).toContain('"lastSplitScreen":"notes"');
    expect(saved).not.toContain("/secret/");

    const cycled = useWorkbench.getState().cycleTab(1);
    expect(cycled?.screen).toBe("home");

    const readerTab = useWorkbench.getState().tabs.find((tab) => tab.screen === "reader")!;
    useWorkbench.getState().closeOtherTabs(readerTab.id);
    expect(useWorkbench.getState().tabs.map((tab) => tab.screen)).toEqual(["home", "reader"]);
  });

  it("forwards active document files when toggling Split View open", () => {
    useWorkbench.getState().toggleSplit("ai", ["/docs/contract.pdf"]);
    expect(useWorkbench.getState().splitScreen).toBe("ai");
    expect(useWorkbench.getState().splitFiles).toEqual(["/docs/contract.pdf"]);
  });
});
