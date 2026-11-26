import { useEffect, useState, type ReactNode } from "react";
import { ArrowLeft, Moon, Plus, ScanLine, Search, Sun } from "lucide-react";
import { BrandMark } from "./brand";
import { BottomNav } from "./bottom-nav";
import { MobileToolsSheet } from "./mobile-tools-sheet";
import { MobileTabGrid } from "./mobile-tab-grid";
import { IconButton } from "./ui";
import { useT } from "../lib/i18n";
import { useKeyboardOpen } from "../lib/viewport";
import { useWorkbench, type WorkbenchTab } from "../lib/workbench-store";
import type { Navigate, ScreenId } from "../lib/nav";

const ROOT_SCREENS = new Set<ScreenId>(["home", "office", "tools", "ai"]);

export interface MobileShellProps {
  icons: Record<ScreenId, ReactNode>;
  screen: ScreenId;
  activeLabel: string;
  isDark: boolean;
  readerImmersive: boolean;
  sheetOpen: boolean;
  onOpenSheet: () => void;
  onCloseSheet: () => void;
  tabsOpen?: boolean;
  onOpenTabs?: () => void;
  onCloseTabs?: () => void;
  onNavigate: Navigate;
  onActivateTab: (tab: WorkbenchTab) => void;
  onCloseTab: (id: string) => void;
  onSearch: () => void;
  onToggleTheme: () => void;
  children: ReactNode;
}

export function MobileShell({
  icons,
  screen,
  activeLabel,
  isDark,
  readerImmersive,
  sheetOpen,
  onOpenSheet,
  onCloseSheet,
  tabsOpen: controlledTabsOpen,
  onOpenTabs,
  onCloseTabs,
  onNavigate,
  onActivateTab,
  onCloseTab,
  onSearch,
  onToggleTheme,
  children,
}: MobileShellProps) {
  const t = useT();
  const keyboardOpen = useKeyboardOpen();
  const tabs = useWorkbench((s) => s.tabs);
  const activeTabId = useWorkbench((s) => s.activeTabId);
  const [uncontrolledTabsOpen, setUncontrolledTabsOpen] = useState(false);

  const tabsOpen = controlledTabsOpen ?? uncontrolledTabsOpen;
  const openTabs = () => {
    if (onOpenTabs) onOpenTabs();
    else setUncontrolledTabsOpen(true);
  };
  const closeTabs = () => {
    if (onCloseTabs) onCloseTabs();
    else setUncontrolledTabsOpen(false);
  };

  const isRoot = ROOT_SCREENS.has(screen);
  const showThumbOmnibar = !keyboardOpen && !readerImmersive && (screen === "home" || screen === "tools");

  useEffect(() => {
    if (typeof document === "undefined") return undefined;
    if (showThumbOmnibar) document.documentElement.dataset.mobileDock = "open";
    else delete document.documentElement.dataset.mobileDock;
    return () => {
      delete document.documentElement.dataset.mobileDock;
    };
  }, [showThumbOmnibar]);

  const pickFromSheet = (id: ScreenId) => {
    onNavigate(id);
    onCloseSheet();
  };

  return (
    <div className="mobile-shell flex flex-col h-full" style={{ background: "var(--bg)" }}>
      <header className={`mobile-bar${readerImmersive ? " hidden" : ""}`}>
        {isRoot ? (
          <BrandMark size={26} className="shrink-0" />
        ) : (
          <IconButton label={t("workbench.back")} onClick={() => window.history.back()}>
            <ArrowLeft size={18} />
          </IconButton>
        )}
        <p className="font-bold text-[16px] truncate flex-1">{activeLabel}</p>
        {screen === "home" || screen === "tools" ? (
          <button type="button" className="mobile-scan-pill" onClick={() => onNavigate("scanner")}>
            <ScanLine size={14} aria-hidden />
            <span>{t("mobile.scanNow")}</span>
          </button>
        ) : null}
        <button
          type="button"
          className="mobile-tabs-btn"
          aria-label={t("mobile.tabsButton", { count: tabs.length })}
          title={t("mobile.tabsButton", { count: tabs.length })}
          onClick={openTabs}
        >
          {tabs.length}
        </button>
        <IconButton label={t("settings.theme")} onClick={onToggleTheme}>
          {isDark ? <Sun size={16} /> : <Moon size={16} />}
        </IconButton>
      </header>

      <main className={`flex-1 min-w-0 relative overflow-hidden${readerImmersive ? " immersive-main" : ""}`}>
        {children}
      </main>

      {showThumbOmnibar ? (
        <div className="mobile-thumb-dock">
          <button type="button" className="mobile-thumb-search" onClick={onSearch}>
            <Search size={16} aria-hidden />
            <span className="truncate">{t("mobile.thumbSearch")}</span>
          </button>
          <button
            type="button"
            className="mobile-thumb-fab"
            aria-label={t("mobile.quickCreate")}
            title={t("mobile.quickCreate")}
            onClick={onOpenSheet}
          >
            <Plus size={18} aria-hidden />
          </button>
        </div>
      ) : null}

      {readerImmersive ? null : <BottomNav screen={screen} onNavigate={onNavigate} onMore={onOpenSheet} />}

      <MobileToolsSheet open={sheetOpen} icons={icons} screen={screen} onClose={onCloseSheet} onPick={pickFromSheet} />
      <MobileTabGrid
        open={tabsOpen}
        tabs={tabs}
        activeTabId={activeTabId}
        icons={icons}
        onClose={closeTabs}
        onActivate={onActivateTab}
        onCloseTab={onCloseTab}
        onNewTab={() => onNavigate("tools")}
      />
    </div>
  );
}
