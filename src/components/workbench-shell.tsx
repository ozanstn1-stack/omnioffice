import { useMemo, type ReactNode } from "react";
import {
  BookOpen,
  FileText,
  Home as HomeIcon,
  LayoutGrid,
  ListChecks,
  Lock,
  Moon,
  PanelLeftClose,
  PanelLeftOpen,
  Search,
  Settings as SettingsIcon,
  Shapes,
  Sparkles,
  Sun,
  X,
} from "lucide-react";
import { BrandMark } from "./brand";
import * as Sidebar from "./sidebar";
import { WorkbenchSplitHost, WorkbenchTopBar } from "./workbench-panes";
import { IconButton } from "./ui";
import { useT } from "../lib/i18n";
import { useOfficeTabs } from "../lib/office-store";
import { useSettings } from "../lib/store";
import { useWorkbench, type WorkbenchTab } from "../lib/workbench-store";
import type { Navigate, ScreenId } from "../lib/nav";

const QUICK_SHORTCUT = "Ctrl K";

const RAIL_WORLDS: readonly { id: ScreenId; labelKey: string; icon: ReactNode }[] = [
  { id: "home", labelKey: "nav.home", icon: <HomeIcon size={18} /> },
  { id: "office", labelKey: "nav.office", icon: <LayoutGrid size={18} /> },
  { id: "reader", labelKey: "nav.reader", icon: <BookOpen size={18} /> },
  { id: "tools", labelKey: "nav.tools", icon: <Shapes size={18} /> },
  { id: "ai", labelKey: "nav.assistant", icon: <Sparkles size={18} /> },
];

const KIND_BADGE: Record<string, { letter: string; cls: string }> = {
  writer: { letter: "W", cls: "rail-doc-writer" },
  calc: { letter: "X", cls: "rail-doc-calc" },
  impress: { letter: "P", cls: "rail-doc-impress" },
};

export interface WorkbenchShellProps {
  icons: Record<ScreenId, ReactNode>;
  screen: ScreenId;
  version: string;
  backgroundJobs: number;
  isDark: boolean;
  readerFocus: boolean;
  dragging: boolean;
  onNavigate: Navigate;
  onActivateTab: (tab: WorkbenchTab) => void;
  onCloseTab: (id: string) => void;
  onCloseOtherTabs?: (id: string) => void;
  onCloseTabsToRight?: (id: string) => void;
  onSwapSplit?: () => void;
  onSearch: () => void;
  onPalette: () => void;
  onToggleTheme: () => void;
  renderScreen: (id: ScreenId, filesOverride?: string[]) => ReactNode;
  children: ReactNode;
}

export function WorkbenchShell({
  icons,
  screen,
  version,
  backgroundJobs,
  isDark,
  readerFocus,
  dragging,
  onNavigate,
  onActivateTab,
  onCloseTab,
  onCloseOtherTabs,
  onCloseTabsToRight,
  onSwapSplit,
  onSearch,
  onPalette,
  onToggleTheme,
  renderScreen,
  children,
}: WorkbenchShellProps) {
  const t = useT();
  const tabsCount = useWorkbench((s) => s.tabs.length);
  const railExpanded = useWorkbench((s) => s.railExpanded);
  const toggleRailExpanded = useWorkbench((s) => s.toggleRailExpanded);
  const setRailExpanded = useWorkbench((s) => s.setRailExpanded);

  const officeTabs = useOfficeTabs((s) => s.tabs);
  const activateOfficeTab = useOfficeTabs((s) => s.activate);
  const density = useSettings((s) => s.settings.uiDensity ?? "comfortable");
  const updateSettings = useSettings((s) => s.update);
  const pinnedIds = useSettings((s) => s.settings.pinnedTools);

  const safeSidebar = useMemo(() => {
    try {
      return {
        build: typeof Sidebar.buildNavHubs === "function" ? Sidebar.buildNavHubs : null,
        NavTree: typeof Sidebar.NavTree === "function" ? Sidebar.NavTree : null,
        NewMenu: typeof Sidebar.NewMenu === "function" ? Sidebar.NewMenu : null,
      };
    } catch {
      return { build: null, NavTree: null, NewMenu: null };
    }
  }, []);

  const hubs = useMemo(() => (safeSidebar.build ? safeSidebar.build(t, icons) : []), [icons, safeSidebar, t]);
  const pinnedEntries = useMemo(() => {
    const all = hubs.flatMap((hub) => hub.items);
    return pinnedIds.flatMap((id) => all.filter((entry) => entry.id === id)).slice(0, 6);
  }, [hubs, pinnedIds]);

  const pickFromHub = (id: ScreenId) => {
    onNavigate(id);
    setRailExpanded(false);
  };
  const nextDensity = density === "compact" ? "comfortable" : "compact";

  return (
    <div className="workbench-root">
      <div className="workbench-body">
        {readerFocus ? null : (
          <>
            <aside className="workbench-rail" data-testid="desktop-sidebar" aria-label={t("nav.bottom")}>
              <button
                type="button"
                className="workbench-brand-btn"
                title={t("app.name")}
                aria-label={t("nav.home")}
                onClick={() => onNavigate("home")}
              >
                <BrandMark size={32} />
              </button>

              <button
                type="button"
                className="rail-btn"
                data-active={railExpanded}
                title={railExpanded ? t("workbench.railCollapse") : t("workbench.railToggle")}
                aria-label={railExpanded ? t("workbench.railCollapse") : t("workbench.railToggle")}
                aria-expanded={railExpanded}
                onClick={toggleRailExpanded}
              >
                {railExpanded ? <PanelLeftClose size={17} /> : <PanelLeftOpen size={17} />}
              </button>

              <div className="rail-divider" aria-hidden />

              <nav className="rail-worlds" aria-label={t("nav.bottom")}>
                {RAIL_WORLDS.map((world) => {
                  const active = world.id === screen;
                  const badge = world.id === "office" && officeTabs.length > 0 ? officeTabs.length : 0;
                  return (
                    <button
                      key={world.id}
                      type="button"
                      className="rail-btn"
                      data-active={active}
                      aria-current={active ? "page" : undefined}
                      title={t(world.labelKey)}
                      aria-label={t(world.labelKey)}
                      onClick={() => onNavigate(world.id)}
                    >
                      {world.icon}
                      {badge > 0 ? <span className="rail-badge">{badge}</span> : null}
                    </button>
                  );
                })}
              </nav>

              {officeTabs.length > 0 ? (
                <>
                  <div className="rail-divider" aria-hidden />
                  <div className="rail-open-docs">
                    {officeTabs.slice(0, 4).map((doc) => {
                      const badge = KIND_BADGE[doc.kind] ?? KIND_BADGE.writer;
                      return (
                        <button
                          key={doc.id}
                          type="button"
                          className={`rail-doc-dot ${badge.cls}`}
                          title={doc.title}
                          aria-label={doc.title}
                          onClick={() => {
                            activateOfficeTab(doc.id);
                            onNavigate("office");
                          }}
                        >
                          {badge.letter}
                          {doc.dirty ? <span className="rail-dirty-dot" aria-hidden /> : null}
                        </button>
                      );
                    })}
                  </div>
                </>
              ) : null}

              <div className="spacer" />

              <div className="rail-foot">
                <button
                  type="button"
                  className="rail-btn"
                  title={t("nav.jobs")}
                  aria-label={t("nav.jobs")}
                  onClick={() => onNavigate("jobs")}
                >
                  <ListChecks size={16} />
                  {backgroundJobs > 0 ? <span className="rail-badge">{backgroundJobs}</span> : null}
                </button>
                <button
                  type="button"
                  className="rail-btn"
                  title={t("settings.theme")}
                  aria-label={t("settings.theme")}
                  onClick={onToggleTheme}
                >
                  {isDark ? <Sun size={16} /> : <Moon size={16} />}
                </button>
                <button
                  type="button"
                  className="rail-btn"
                  data-active={screen === "settings"}
                  title={t("nav.settings")}
                  aria-label={t("nav.settings")}
                  onClick={() => onNavigate("settings")}
                >
                  <SettingsIcon size={16} />
                </button>
              </div>
            </aside>

            <aside className="workbench-hubs-panel fade-in" aria-label={t("workbench.allHubs")} hidden={!railExpanded}>
              <div className="flex flex-col gap-2 px-3 pt-3 pb-2">
                <div className="flex items-center justify-between gap-2">
                  <span className="font-bold text-[13.5px] truncate">{t("app.name")}</span>
                  <IconButton label={t("workbench.railCollapse")} onClick={() => setRailExpanded(false)}>
                    <X size={15} />
                  </IconButton>
                </div>
                {safeSidebar.NewMenu ? <safeSidebar.NewMenu onNavigate={pickFromHub} /> : null}
                <button type="button" className="sidebar-search" onClick={onSearch}>
                  <Search size={14} aria-hidden />
                  <span className="flex-1 truncate text-left">{t("nav.search")}</span>
                  <kbd>{QUICK_SHORTCUT}</kbd>
                </button>
              </div>
              <nav className="flex-1 overflow-y-auto px-2.5 pb-3" aria-label={t("workbench.allHubs")}>
                {safeSidebar.NavTree ? (
                  <safeSidebar.NavTree hubs={hubs} screen={screen} onPick={pickFromHub} showLabels />
                ) : null}
                {pinnedEntries.length > 0 ? (
                  <div>
                    <p className="nav-group-label">{t("home.pinned")}</p>
                    {pinnedEntries.map((entry) => (
                      <button key={entry.id} type="button" className="nav-item" onClick={() => pickFromHub(entry.id)}>
                        <span className="shrink-0">{entry.icon}</span>
                        <span className="truncate">{entry.label}</span>
                      </button>
                    ))}
                  </div>
                ) : null}
              </nav>
            </aside>
          </>
        )}

        <div className="workbench-stage">
          {readerFocus ? null : (
            <WorkbenchTopBar
              icons={icons}
              screen={screen}
              onNavigate={onNavigate}
              onActivateTab={onActivateTab}
              onCloseTab={onCloseTab}
              onCloseOtherTabs={onCloseOtherTabs}
              onCloseTabsToRight={onCloseTabsToRight}
              onPalette={onPalette}
            />
          )}

          <WorkbenchSplitHost
            dragging={dragging}
            onNavigate={onNavigate}
            onSwapSplit={onSwapSplit}
            renderScreen={renderScreen}
          >
            {children}
          </WorkbenchSplitHost>

          {readerFocus ? null : (
            <footer className="workbench-statusbar">
              <div className="flex items-center gap-3 min-w-0">
                <span className="status-local">
                  <Lock size={11} aria-hidden /> {t("nav.privacy")}
                </span>
                <span className="muted">{t("workbench.tabsCount", { count: tabsCount })}</span>
                {officeTabs.length > 0 ? (
                  <button type="button" className="status-link" onClick={() => onNavigate("office")}>
                    <FileText size={11} aria-hidden />
                    {t("workbench.openDocs", { count: officeTabs.length })}
                  </button>
                ) : null}
                {backgroundJobs > 0 ? (
                  <button type="button" className="status-link" onClick={() => onNavigate("jobs")}>
                    <ListChecks size={11} aria-hidden /> {t("jobs.runningCount", { count: backgroundJobs })}
                  </button>
                ) : null}
              </div>
              <div className="flex items-center gap-2 shrink-0">
                <button
                  type="button"
                  className="status-pill"
                  onClick={() => void updateSettings({ uiDensity: nextDensity })}
                  title={t("settings.density")}
                >
                  {density === "compact" ? t("settings.densityCompact") : t("settings.densityComfortable")}
                </button>
                <span className="muted tabular-nums">v{version}</span>
              </div>
            </footer>
          )}
        </div>
      </div>
    </div>
  );
}
