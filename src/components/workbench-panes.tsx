import React, { useRef, useState, type ReactNode } from "react";
import {
  ArrowLeft,
  ArrowLeftRight,
  ArrowRight,
  ArrowUpLeft,
  Columns2,
  ListX,
  Pin,
  Plus,
  Search,
  X,
} from "lucide-react";
import { IconButton } from "./ui";
import { WorkbenchTabMenu } from "./workbench-tab-menu";
import { useT } from "../lib/i18n";
import { routeEnabled, routeFor } from "../lib/routes";
import { HOME_TAB_ID, useWorkbench, type WorkbenchTab } from "../lib/workbench-store";
import type { Navigate, ScreenId } from "../lib/nav";

const QUICK_SHORTCUT = "Ctrl K";
const SPLIT_CHOICES: readonly ScreenId[] = [
  "ai",
  "reader",
  "office",
  "notes",
  "compare",
  "vault",
  "pdfStudio",
  "tools",
];

export function WorkbenchTopBar({
  icons,
  screen,
  onNavigate,
  onActivateTab,
  onCloseTab,
  onCloseOtherTabs,
  onCloseTabsToRight,
  onPalette,
}: {
  icons: Record<ScreenId, ReactNode>;
  screen: ScreenId;
  onNavigate: Navigate;
  onActivateTab: (tab: WorkbenchTab) => void;
  onCloseTab: (id: string) => void;
  onCloseOtherTabs?: (id: string) => void;
  onCloseTabsToRight?: (id: string) => void;
  onPalette: () => void;
}) {
  const t = useT();
  const tabs = useWorkbench((s) => s.tabs);
  const activeTabId = useWorkbench((s) => s.activeTabId);
  const splitScreen = useWorkbench((s) => s.splitScreen);
  const toggleSplit = useWorkbench((s) => s.toggleSplit);
  const openSplit = useWorkbench((s) => s.openSplit);
  const togglePinTab = useWorkbench((s) => s.togglePinTab);
  const reorderTab = useWorkbench((s) => s.reorderTab);
  const [dragSourceId, setDragSourceId] = useState<string | null>(null);
  const [dragOverId, setDragOverId] = useState<string | null>(null);
  const [menuState, setMenuState] = useState<{ tabId: string; x: number; y: number } | null>(null);
  const splitActive = splitScreen !== null && routeEnabled(splitScreen);
  const activeTab = tabs.find((tab) => tab.id === activeTabId);
  const menuTab = menuState ? (tabs.find((tab) => tab.id === menuState.tabId) ?? null) : null;
  const menuIndex = menuTab ? tabs.findIndex((tab) => tab.id === menuTab.id) : -1;

  const tabLabel = (tab: WorkbenchTab) => {
    if (tab.title) return tab.title;
    return routeEnabled(tab.screen) ? t(routeFor(tab.screen).labelKey) : t("app.name");
  };

  return (
    <header className="workbench-topbar">
      <div className="workbench-nav-arrows">
        <IconButton label={t("workbench.back")} onClick={() => window.history.back()}>
          <ArrowLeft size={15} />
        </IconButton>
        <IconButton label={t("workbench.forward")} onClick={() => window.history.forward()}>
          <ArrowRight size={15} />
        </IconButton>
      </div>

      <div className="workbench-tab-strip" role="tablist" aria-label={t("workbench.tabsCount", { count: tabs.length })}>
        {tabs.map((tab) => {
          const active = tab.id === activeTabId;
          const userPinned = Boolean(tab.pinned) && tab.id !== HOME_TAB_ID;
          const dropTarget = dragOverId === tab.id && dragSourceId !== tab.id;
          return (
            <div
              key={tab.id}
              role="tab"
              tabIndex={active ? 0 : -1}
              aria-selected={active}
              draggable={tab.id !== HOME_TAB_ID}
              className={`workbench-tab${active ? " is-active" : ""}${userPinned ? " is-pinned" : ""}${dropTarget ? " is-drop-target" : ""}`}
              onClick={() => onActivateTab(tab)}
              onContextMenu={(event) => {
                event.preventDefault();
                setMenuState({ tabId: tab.id, x: event.clientX, y: event.clientY });
              }}
              onDragStart={(event) => {
                if (tab.id === HOME_TAB_ID) return;
                setDragSourceId(tab.id);
                event.dataTransfer?.setData("text/plain", tab.id);
              }}
              onDragOver={(event) => {
                if (!dragSourceId || tab.id === HOME_TAB_ID || dragSourceId === tab.id) return;
                event.preventDefault();
                setDragOverId(tab.id);
              }}
              onDragLeave={() => {
                if (dragOverId === tab.id) setDragOverId(null);
              }}
              onDrop={(event) => {
                event.preventDefault();
                const sourceId = dragSourceId ?? event.dataTransfer?.getData("text/plain") ?? "";
                if (sourceId && tab.id !== HOME_TAB_ID) reorderTab(sourceId, tab.id);
                setDragSourceId(null);
                setDragOverId(null);
              }}
              onDragEnd={() => {
                setDragSourceId(null);
                setDragOverId(null);
              }}
              onMouseDown={(event) => {
                if (event.button === 1 && !tab.pinned) {
                  event.preventDefault();
                  onCloseTab(tab.id);
                }
              }}
              onKeyDown={(event) => {
                if (event.key === "Enter" || event.key === " ") {
                  event.preventDefault();
                  onActivateTab(tab);
                } else if (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) {
                  event.preventDefault();
                  const rect = event.currentTarget.getBoundingClientRect();
                  setMenuState({ tabId: tab.id, x: Math.round(rect.left), y: Math.round(rect.bottom + 4) });
                }
              }}
            >
              <span className="workbench-tab-icon" aria-hidden>
                {icons[tab.screen]}
              </span>
              {userPinned ? <Pin size={11} className="workbench-tab-pin" aria-hidden /> : null}
              <span className="workbench-tab-label">{tabLabel(tab)}</span>
              {!tab.pinned ? (
                <button
                  type="button"
                  className="workbench-tab-close"
                  aria-label={t("workbench.closeTab")}
                  onClick={(event) => {
                    event.stopPropagation();
                    onCloseTab(tab.id);
                  }}
                >
                  <X size={12} />
                </button>
              ) : null}
            </div>
          );
        })}
        <button
          type="button"
          className="workbench-tab-add"
          title={t("workbench.newTab")}
          aria-label={t("workbench.newTab")}
          onClick={() => onNavigate("tools")}
        >
          <Plus size={14} />
        </button>
        {tabs.length > 2 && onCloseOtherTabs ? (
          <IconButton label={t("workbench.closeOthers")} onClick={() => onCloseOtherTabs(activeTabId)}>
            <ListX size={14} />
          </IconButton>
        ) : null}
      </div>

      <div className="workbench-topbar-actions">
        <button type="button" className="workbench-cmd-pill" onClick={onPalette} title={t("workbench.commandHint")}>
          <Search size={13} aria-hidden />
          <span>{t("nav.search")}</span>
          <kbd>{QUICK_SHORTCUT}</kbd>
        </button>
        <button
          type="button"
          className={`workbench-split-btn${splitActive ? " is-active" : ""}`}
          aria-pressed={splitActive}
          title={splitActive ? t("workbench.splitClose") : t("workbench.splitOpen")}
          onClick={() => toggleSplit(screen === "ai" ? "reader" : "ai", activeTab?.files ?? [])}
        >
          <Columns2 size={15} aria-hidden />
          <span>{t("workbench.splitOpen")}</span>
        </button>
      </div>

      {menuTab ? (
        <WorkbenchTabMenu
          tab={menuTab}
          x={menuState!.x}
          y={menuState!.y}
          canOpenSplit={routeEnabled(menuTab.screen) && menuTab.screen !== "home"}
          hasClosableRight={Boolean(onCloseTabsToRight) && tabs.slice(menuIndex + 1).some((entry) => !entry.pinned)}
          hasClosableOthers={
            Boolean(onCloseOtherTabs) && tabs.some((entry) => entry.id !== menuTab.id && !entry.pinned)
          }
          onCloseMenu={() => setMenuState(null)}
          onTogglePin={togglePinTab}
          onOpenInSplit={(target) => openSplit(target.screen, target.files)}
          onCloseToRight={(id) => onCloseTabsToRight?.(id)}
          onCloseOthers={(id) => onCloseOtherTabs?.(id)}
          onCloseTab={onCloseTab}
        />
      ) : null}
    </header>
  );
}

export function WorkbenchSplitHost({
  dragging,
  onNavigate,
  onSwapSplit,
  renderScreen,
  children,
}: {
  dragging: boolean;
  onNavigate: Navigate;
  onSwapSplit?: () => void;
  renderScreen: (id: ScreenId, filesOverride?: string[]) => ReactNode;
  children: ReactNode;
}) {
  const t = useT();
  const splitScreen = useWorkbench((s) => s.splitScreen);
  const splitFiles = useWorkbench((s) => s.splitFiles);
  const splitRatio = useWorkbench((s) => s.splitRatio);
  const openSplit = useWorkbench((s) => s.openSplit);
  const closeSplit = useWorkbench((s) => s.closeSplit);
  const setSplitRatio = useWorkbench((s) => s.setSplitRatio);
  const resetSplitRatio = useWorkbench((s) => s.resetSplitRatio);
  const splitContainerRef = useRef<HTMLDivElement>(null);
  const splitActive = splitScreen !== null && routeEnabled(splitScreen);

  const startResize = (event: React.MouseEvent<HTMLElement>) => {
    event.preventDefault();
    const container = splitContainerRef.current;
    if (!container) return;
    const rect = container.getBoundingClientRect();
    const onMove = (moveEvent: MouseEvent) => {
      setSplitRatio((moveEvent.clientX - rect.left) / Math.max(1, rect.width));
    };
    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  };

  return (
    <div
      ref={splitContainerRef}
      className={`workbench-split${splitActive ? " is-split" : ""}`}
      style={
        splitActive
          ? {
              gridTemplateColumns: `minmax(320px, ${(splitRatio * 100).toFixed(1)}fr) 6px minmax(300px, ${((1 - splitRatio) * 100).toFixed(1)}fr)`,
            }
          : undefined
      }
    >
      <main className="workbench-pane workbench-pane-main relative">
        {dragging ? (
          <div className="workbench-drop-overlay">
            <p className="font-semibold">{t("common.dropHere")}</p>
          </div>
        ) : null}
        {children}
      </main>

      {splitActive && splitScreen ? (
        <>
          <button
            type="button"
            className="workbench-splitter"
            aria-label={t("workbench.resetSplit")}
            title={t("workbench.resetSplit")}
            onMouseDown={startResize}
            onDoubleClick={() => resetSplitRatio()}
            onKeyDown={(event) => {
              if (event.key === "ArrowLeft") {
                event.preventDefault();
                setSplitRatio(splitRatio - 0.04);
              } else if (event.key === "ArrowRight") {
                event.preventDefault();
                setSplitRatio(splitRatio + 0.04);
              } else if (event.key === "0" || event.key === "Home") {
                event.preventDefault();
                resetSplitRatio();
              }
            }}
          />
          <section className="workbench-pane workbench-pane-side" aria-label={t("workbench.splitPick")}>
            <div className="workbench-split-head">
              <div className="workbench-split-choices">
                {SPLIT_CHOICES.filter((id) => routeEnabled(id)).map((choice) => (
                  <button
                    key={choice}
                    type="button"
                    className={`workbench-split-chip${splitScreen === choice ? " is-active" : ""}`}
                    onClick={() => openSplit(choice, splitFiles)}
                  >
                    {t(routeFor(choice).labelKey)}
                  </button>
                ))}
              </div>
              <div className="flex items-center gap-1 shrink-0">
                {onSwapSplit ? (
                  <IconButton label={t("workbench.swapSplit")} onClick={onSwapSplit}>
                    <ArrowLeftRight size={14} />
                  </IconButton>
                ) : null}
                <IconButton
                  label={t("workbench.splitPromote")}
                  onClick={() => {
                    onNavigate(splitScreen, { files: splitFiles });
                    closeSplit();
                  }}
                >
                  <ArrowUpLeft size={14} />
                </IconButton>
                <IconButton label={t("workbench.splitClose")} onClick={closeSplit}>
                  <X size={14} />
                </IconButton>
              </div>
            </div>
            <div className="workbench-split-body">{renderScreen(splitScreen, splitFiles)}</div>
          </section>
        </>
      ) : null}
    </div>
  );
}
