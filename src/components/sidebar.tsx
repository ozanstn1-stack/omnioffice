import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import {
  CalendarDays,
  ChevronDown,
  FileSpreadsheet,
  FileText,
  Layers,
  ListChecks,
  Lock,
  Moon,
  PanelLeft,
  Plus,
  Presentation,
  ScanLine,
  Search,
  Settings2,
  ShieldCheck,
  Sparkles,
  Sun,
} from "lucide-react";
import { BrandMark } from "./brand";
import { IconButton } from "./ui";
import { useT } from "../lib/i18n";
import { useSettings } from "../lib/store";
import { navSections, type NavSectionId } from "../lib/routes";
import { useOfficeTabs } from "../lib/office-store";
import type { ScreenId } from "../lib/nav";

/**
 * The 7.31 navigation: the ~55 routes are folded into a handful of hubs
 * (Office, Productivity, Scan & convert, PDF tools, Security, AI, System) that
 * open and close, instead of one long flat list. The hub that holds the
 * current screen is always open. Closed hubs keep their items in the DOM
 * (`hidden`), so keyboard shortcuts, the e2e flows and screen readers that
 * look an entry up by name still find it.
 */
export interface NavEntry {
  id: ScreenId;
  label: string;
  icon: ReactNode;
}

export interface NavHub {
  id: string;
  label?: string;
  icon?: ReactNode;
  items: NavEntry[];
}

/** Sections that have no hub of their own join the hub named here. */
const HUB_OF: Partial<Record<NavSectionId, string>> = { images: "convert", ocr: "convert" };

const HUB_META: Record<string, { labelKey: string; icon: ReactNode }> = {
  office: { labelKey: "nav.officeHub", icon: <FileText size={17} /> },
  productivity: { labelKey: "nav.productivity", icon: <CalendarDays size={17} /> },
  convert: { labelKey: "nav.scanConvert", icon: <ScanLine size={17} /> },
  pdfTools: { labelKey: "nav.pdfTools", icon: <Layers size={17} /> },
  security: { labelKey: "nav.security", icon: <ShieldCheck size={17} /> },
  ai: { labelKey: "home.groupAi", icon: <Sparkles size={17} /> },
  system: { labelKey: "nav.system", icon: <Settings2 size={17} /> },
};

/** Groups the enabled routes into hubs; `top` stays a flat, unlabeled list. */
export function buildNavHubs(t: (key: string) => string, icons: Record<ScreenId, ReactNode>): NavHub[] {
  const hubs: NavHub[] = [];
  for (const section of navSections()) {
    const hubId = HUB_OF[section.id as NavSectionId] ?? section.id;
    let hub = hubs.find((candidate) => candidate.id === hubId);
    if (!hub) {
      const meta = HUB_META[hubId];
      hub = { id: hubId, ...(meta ? { label: t(meta.labelKey), icon: meta.icon } : {}), items: [] };
      hubs.push(hub);
    }
    for (const route of section.routes)
      hub.items.push({ id: route.id, label: t(route.labelKey), icon: icons[route.id] });
  }
  return hubs;
}

function NavButton({
  entry,
  active,
  showLabel,
  onPick,
}: {
  entry: NavEntry;
  active: boolean;
  showLabel: boolean;
  onPick: (id: ScreenId) => void;
}) {
  return (
    <button
      type="button"
      className="nav-item"
      data-active={active}
      aria-current={active ? "page" : undefined}
      onClick={() => onPick(entry.id)}
      title={showLabel ? undefined : entry.label}
    >
      <span className="shrink-0">{entry.icon}</span>
      {showLabel ? <span className="truncate">{entry.label}</span> : null}
    </button>
  );
}

/** The hub list shared by the desktop sidebar and the phone drawer. */
export function NavTree({
  hubs,
  screen,
  onPick,
  showLabels,
}: {
  hubs: NavHub[];
  screen: ScreenId;
  onPick: (id: ScreenId) => void;
  showLabels: boolean;
}) {
  const t = useT();
  const activeHub = hubs.find((hub) => hub.items.some((item) => item.id === screen))?.id;
  // Hubs the person opened by hand. The hub holding the current screen is
  // open regardless, so navigating from the palette, a Home card or the
  // Android back button always reveals where you are.
  const [open, setOpen] = useState<ReadonlySet<string>>(() => new Set());

  // A compact sidebar has no room for hub headers: one flat icon column.
  if (!showLabels) {
    return (
      <>
        {hubs
          .flatMap((hub) => hub.items)
          .map((entry) => (
            <NavButton key={entry.id} entry={entry} active={entry.id === screen} showLabel={false} onPick={onPick} />
          ))}
      </>
    );
  }

  return (
    <>
      {hubs.map((hub) => {
        const items = hub.items.map((entry) => (
          <NavButton key={entry.id} entry={entry} active={entry.id === screen} showLabel onPick={onPick} />
        ));
        if (!hub.label) return <div key={hub.id}>{items}</div>;
        const expanded = open.has(hub.id) || hub.id === activeHub;
        const panelId = `nav-hub-${hub.id}`;
        return (
          <div key={hub.id} className="nav-hub">
            <button
              type="button"
              className="nav-hub-toggle"
              data-has-active={hub.id === activeHub}
              aria-expanded={expanded}
              aria-controls={panelId}
              title={hub.id === activeHub ? undefined : expanded ? t("nav.collapse") : t("nav.expand")}
              onClick={() =>
                setOpen((previous) => {
                  const next = new Set(previous);
                  if (next.has(hub.id)) next.delete(hub.id);
                  else next.add(hub.id);
                  return next;
                })
              }
            >
              <span className="shrink-0">{hub.icon}</span>
              <span className="truncate flex-1">{hub.label}</span>
              <ChevronDown size={15} className="nav-hub-chevron" aria-hidden />
            </button>
            <div id={panelId} className="nav-hub-items" hidden={!expanded}>
              {items}
            </div>
          </div>
        );
      })}
    </>
  );
}

/** "New" with its three blank documents; a template opens the gallery. */
export function NewMenu({ onNavigate }: { onNavigate: (id: ScreenId) => void }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const close = (event: MouseEvent) => {
      if (!ref.current?.contains(event.target as Node)) setOpen(false);
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", close);
    document.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("mousedown", close);
      document.removeEventListener("keydown", escape);
    };
  }, [open]);

  const create = (kind: "writer" | "calc" | "impress") => {
    useOfficeTabs.getState().create(kind);
    setOpen(false);
    onNavigate("office");
  };

  return (
    <div className="new-menu" ref={ref}>
      <button
        type="button"
        className="btn btn-primary new-menu-button"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
      >
        <Plus size={17} /> {t("nav.new")}
      </button>
      {open ? (
        <div className="new-menu-list" role="menu">
          <button type="button" role="menuitem" onClick={() => create("writer")}>
            <span className="doc-tile doc-tile-writer" aria-hidden>
              <FileText size={16} />
            </span>
            {t("home.newDocument")}
          </button>
          <button type="button" role="menuitem" onClick={() => create("calc")}>
            <span className="doc-tile doc-tile-calc" aria-hidden>
              <FileSpreadsheet size={16} />
            </span>
            {t("home.newSpreadsheet")}
          </button>
          <button type="button" role="menuitem" onClick={() => create("impress")}>
            <span className="doc-tile doc-tile-impress" aria-hidden>
              <Presentation size={16} />
            </span>
            {t("home.newPresentation")}
          </button>
          <button
            type="button"
            role="menuitem"
            onClick={() => {
              setOpen(false);
              onNavigate("templates");
            }}
          >
            <span className="doc-tile" aria-hidden>
              <Layers size={16} />
            </span>
            {t("home.fromTemplate")}
          </button>
        </div>
      ) : null}
    </div>
  );
}

/** The phone drawer's body: New, then the same hubs as the desktop sidebar. */
export function DrawerNav({
  icons,
  screen,
  onPick,
}: {
  icons: Record<ScreenId, ReactNode>;
  screen: ScreenId;
  onPick: (id: ScreenId) => void;
}) {
  const t = useT();
  return (
    <>
      <div className="px-3 pb-3">
        <NewMenu onNavigate={onPick} />
      </div>
      <nav className="flex-1 overflow-y-auto px-2 pb-4">
        <NavTree hubs={buildNavHubs(t, icons)} screen={screen} onPick={onPick} showLabels />
      </nav>
    </>
  );
}

export function DesktopSidebar({
  icons,
  screen,
  compact,
  version,
  backgroundJobs,
  isDark,
  onNavigate,
  onSearch,
  onToggleTheme,
  onToggleCompact,
}: {
  icons: Record<ScreenId, ReactNode>;
  screen: ScreenId;
  compact: boolean;
  version: string;
  backgroundJobs: number;
  isDark: boolean;
  onNavigate: (id: ScreenId) => void;
  onSearch: () => void;
  onToggleTheme: () => void;
  onToggleCompact: () => void;
}) {
  const t = useT();
  const hubs = buildNavHubs(t, icons);
  const pinnedIds = useSettings((state) => state.settings.pinnedTools);
  const pinned = useMemo(() => {
    const all = hubs.flatMap((hub) => hub.items);
    return pinnedIds.flatMap((id) => all.filter((entry) => entry.id === id)).slice(0, 5);
  }, [hubs, pinnedIds]);

  return (
    <aside className="sidebar flex flex-col shrink-0" style={{ width: compact ? 68 : 252 }}>
      <div className="flex items-center gap-2.5 px-4 pt-4 pb-3">
        <BrandMark size={32} className="shrink-0" />
        {!compact ? (
          <div className="min-w-0">
            <p className="font-bold text-[14px] leading-tight truncate">{t("app.name")}</p>
            <p className="text-[11px] muted truncate">v{version}</p>
          </div>
        ) : null}
      </div>

      {!compact ? (
        <div className="flex flex-col gap-2 px-3 pb-3">
          <NewMenu onNavigate={onNavigate} />
          <button type="button" className="sidebar-search" onClick={onSearch}>
            <Search size={15} aria-hidden />
            <span className="flex-1 truncate text-left">{t("nav.search")}</span>
            <kbd>Ctrl ⇧ F</kbd>
          </button>
        </div>
      ) : (
        <div className="flex flex-col items-center gap-1 pb-2">
          <IconButton label={t("nav.search")} onClick={onSearch}>
            <Search size={16} />
          </IconButton>
        </div>
      )}

      <nav className="flex-1 overflow-y-auto px-3 pb-3" aria-label={t("nav.bottom")}>
        <NavTree hubs={hubs} screen={screen} onPick={onNavigate} showLabels={!compact} />
        {!compact && pinned.length > 0 ? (
          <div>
            <p className="nav-group-label">{t("home.pinned")}</p>
            {pinned.map((entry) => (
              <NavButton key={entry.id} entry={entry} active={false} showLabel onPick={onNavigate} />
            ))}
          </div>
        ) : null}
      </nav>

      <div className="sidebar-foot">
        {!compact ? (
          <span className="privacy-pill">
            <Lock size={12} aria-hidden /> {t("nav.privacy")}
          </span>
        ) : null}
        <div className={`flex items-center gap-1 ${compact ? "flex-col" : ""}`}>
          <button
            type="button"
            className="icon-btn"
            title={t("nav.jobs")}
            aria-label={t("nav.jobs")}
            onClick={() => onNavigate("jobs")}
            style={{ position: "relative" }}
          >
            <ListChecks size={15} />
            {backgroundJobs > 0 ? <span className="jobs-dot">{backgroundJobs}</span> : null}
          </button>
          <IconButton label={t("settings.theme")} onClick={onToggleTheme}>
            {isDark ? <Sun size={15} /> : <Moon size={15} />}
          </IconButton>
          <IconButton label={t("nav.sidebar")} onClick={onToggleCompact}>
            <PanelLeft size={15} />
          </IconButton>
        </div>
      </div>
    </aside>
  );
}
