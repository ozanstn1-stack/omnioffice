import React, { useRef, type ReactNode } from "react";
import { ArrowLeft, ArrowRight, ArrowUpLeft, Columns2, Plus, Search, X } from "lucide-react";
import { IconButton } from "./ui";
import { useT } from "../lib/i18n";
import { routeEnabled, routeFor } from "../lib/routes";
import { useWorkbench, type WorkbenchTab } from "../lib/workbench-store";
import type { Navigate, ScreenId } from "../lib/nav";

const QUICK_SHORTCUT = "Ctrl K";
const SPLIT_CHOICES: readonly ScreenId[] = ["ai", "reader", "office", "notes", "tools"];

export function WorkbenchTopBar({
  icons,
  screen,
  onNavigate,
  onActivateTab,
  onCloseTab,
  onPalette,
}: {
  icons: Record<ScreenId, ReactNode>;
  screen: ScreenId;
  onNavigate: Navigate;
  onActivateTab: (tab: WorkbenchTab) => void;
  onCloseTab: (id: string) => void;
  onPalette: () => void;
}) {
  const t = useT();
  const tabs = useWorkbench((s) => s.tabs);
  const activeTabId = useWorkbench((s) => s.activeTabId);
  const splitScreen = useWorkbench((s) => s.splitScreen);
  const toggleSplit = useWorkbench((s) => s.toggleSplit);
  const splitActive = splitScreen !== null && routeEnabled(splitScreen);

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
          return (
            <div
              key={tab.id}
              role="tab"
              tabIndex={active ? 0 : -1}
              aria-selected={active}
              className={`workbench-tab${active ? " is-active" : ""}`}
              onClick={() => onActivateTab(tab)}
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
                }
              }}
            >
              <span className="workbench-tab-icon" aria-hidden>
                {icons[tab.screen]}
              </span>
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
          onClick={() => toggleSplit(screen === "ai" ? "reader" : "ai")}
        >
          <Columns2 size={15} aria-hidden />
          <span>{t("workbench.splitOpen")}</span>
        </button>
      </div>
    </header>
  );
}

export function WorkbenchSplitHost({
  dragging,
  onNavigate,
  renderScreen,
  children,
}: {
  dragging: boolean;
  onNavigate: Navigate;
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
            aria-label={t("workbench.splitOpen")}
            onMouseDown={startResize}
            onKeyDown={(event) => {
              if (event.key === "ArrowLeft") {
                event.preventDefault();
                setSplitRatio(splitRatio - 0.04);
              } else if (event.key === "ArrowRight") {
                event.preventDefault();
                setSplitRatio(splitRatio + 0.04);
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
                    onClick={() => openSplit(choice, choice === splitScreen ? splitFiles : [])}
                  >
                    {t(routeFor(choice).labelKey)}
                  </button>
                ))}
              </div>
              <div className="flex items-center gap-1 shrink-0">
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
