import { useEffect, useMemo, useState } from "react";
import {
  Archive,
  BookOpen,
  Bot,
  CalendarDays,
  ClipboardList,
  Combine,
  Database,
  Eraser,
  FileDiff,
  FileImage,
  FileSearch,
  FileSpreadsheet,
  FileText,
  FolderClock,
  FolderOpen,
  Images,
  KeyRound,
  Pin,
  LayoutGrid,
  LayoutTemplate,
  Layers,
  Library,
  ListChecks,
  Lock,
  Minimize2,
  NotebookPen,
  PenTool,
  Presentation,
  Puzzle,
  RefreshCw,
  Repeat,
  Scissors,
  Search,
  Share2,
  ShieldCheck,
  Sparkles,
  Stamp,
  Type,
  Wand2,
} from "lucide-react";
import { Button, Card, EmptyState, IconButton } from "../components/ui";
import { DropZone } from "../components/files";
import { Screen } from "../components/layout";
import { BrandMark } from "../components/brand";
import { UpdateBanner } from "../components/update-banner";
import { WelcomeCard } from "../components/welcome-card";
import { ProfessionDesk, ProfessionPicker } from "../components/profession-desk";
import { deskTasks, isProfession, type ProfessionChoice } from "../lib/desks";
import { useT } from "../lib/i18n";
import { useRecent, useSettings, useToasts } from "../lib/store";
import { isOfficePath, openOfficePath } from "../lib/office-store";
import { fileBaseName, formatDate, isPdf, isPresentationPath, isSpreadsheetPath } from "../lib/format";
import { foldSearch } from "../lib/search-fold";
import { recommendTools } from "../lib/recommend";
import { isAndroid, openAnyFile } from "../lib/mobile";
import { shareRecent } from "../lib/presend-gate";
import type { Navigate, ScreenId } from "../lib/nav";
import type { RecentEntry } from "../lib/types";
import { routeEnabled } from "../lib/routes";

type ToolGroup = "pdf" | "office" | "tools" | "ai";

export interface ToolCardSpec {
  id: ScreenId;
  titleKey: string;
  descKey?: string;
  icon: React.ReactNode;
  group: ToolGroup;
  accent?: boolean;
}

const GROUPS: { key: ToolGroup; labelKey: string }[] = [
  { key: "office", labelKey: "home.groupOffice" },
  { key: "pdf", labelKey: "home.groupPdf" },
  { key: "tools", labelKey: "home.groupTools" },
  { key: "ai", labelKey: "home.groupAi" },
];

const CARDS: ToolCardSpec[] = [
  {
    id: "reader",
    titleKey: "nav.reader",
    descKey: "reader.subtitle",
    icon: <BookOpen size={20} />,
    group: "pdf",
    accent: true,
  },
  {
    id: "merge",
    titleKey: "nav.merge",
    descKey: "merge.subtitle",
    icon: <Combine size={20} />,
    group: "pdf",
    accent: true,
  },
  { id: "organize", titleKey: "nav.organize", descKey: "organize.subtitle", icon: <Layers size={20} />, group: "pdf" },
  { id: "split", titleKey: "nav.split", descKey: "split.subtitle", icon: <Scissors size={20} />, group: "pdf" },
  {
    id: "compress",
    titleKey: "nav.compress",
    descKey: "compress.subtitle",
    icon: <Minimize2 size={20} />,
    group: "pdf",
    accent: true,
  },
  {
    id: "ocr",
    titleKey: "nav.ocr",
    descKey: "ocr.subtitle",
    icon: <FileSearch size={20} />,
    group: "pdf",
    accent: true,
  },
  {
    id: "pdfToImages",
    titleKey: "nav.pdfToImages",
    descKey: "convert.pdfToImagesSubtitle",
    icon: <FileImage size={20} />,
    group: "pdf",
    accent: true,
  },
  {
    id: "imagesToPdf",
    titleKey: "nav.imagesToPdf",
    descKey: "convert.imagesToPdfSubtitle",
    icon: <Images size={20} />,
    group: "pdf",
    accent: true,
  },
  {
    id: "watermark",
    titleKey: "nav.watermark",
    descKey: "watermark.subtitle",
    icon: <Stamp size={20} />,
    group: "pdf",
    accent: true,
  },
  { id: "annotate", titleKey: "nav.annotate", descKey: "annotate.subtitle", icon: <Type size={20} />, group: "pdf" },
  { id: "redact", titleKey: "nav.redact", descKey: "redact.subtitle", icon: <Eraser size={20} />, group: "pdf" },
  { id: "compare", titleKey: "nav.compare", descKey: "compare.subtitle", icon: <FileDiff size={20} />, group: "pdf" },
  { id: "inspect", titleKey: "nav.inspect", descKey: "inspect.subtitle", icon: <Search size={20} />, group: "pdf" },
  { id: "verify", titleKey: "nav.verify", descKey: "verify.subtitle", icon: <ShieldCheck size={20} />, group: "pdf" },
  { id: "metadata", titleKey: "nav.metadata", descKey: "metadata.subtitle", icon: <Puzzle size={20} />, group: "pdf" },
  {
    id: "pageTools",
    titleKey: "nav.pageTools",
    descKey: "pageTools.resizeSubtitle",
    icon: <Wand2 size={20} />,
    group: "pdf",
  },
  {
    id: "protect",
    titleKey: "nav.protect",
    descKey: "security.protectSubtitle",
    icon: <Lock size={20} />,
    group: "pdf",
    accent: true,
  },
  { id: "pdfStudio", titleKey: "nav.pdfStudio", icon: <ShieldCheck size={20} />, group: "pdf" },
  { id: "pdfForms", titleKey: "nav.pdfForms", icon: <ClipboardList size={20} />, group: "pdf" },
  { id: "batch", titleKey: "nav.batch", descKey: "batch.subtitle", icon: <Archive size={20} />, group: "pdf" },
  {
    id: "office",
    titleKey: "nav.office",
    descKey: "office.subtitle",
    icon: <LayoutGrid size={20} />,
    group: "office",
    accent: true,
  },
  {
    id: "documents",
    titleKey: "nav.documents",
    descKey: "office.documentsSubtitle",
    icon: <FileText size={20} />,
    group: "office",
    accent: true,
  },
  {
    id: "spreadsheets",
    titleKey: "nav.spreadsheets",
    descKey: "office.spreadsheetsSubtitle",
    icon: <FileSpreadsheet size={20} />,
    group: "office",
    accent: true,
  },
  {
    id: "presentations",
    titleKey: "nav.presentations",
    descKey: "office.presentationsSubtitle",
    icon: <Presentation size={20} />,
    group: "office",
    accent: true,
  },
  {
    id: "templates",
    titleKey: "nav.templates",
    descKey: "templates.subtitle",
    icon: <LayoutTemplate size={20} />,
    group: "office",
  },
  {
    id: "converter",
    titleKey: "nav.converter",
    descKey: "converter.subtitle",
    icon: <Repeat size={20} />,
    group: "office",
  },
  {
    id: "cleaner",
    titleKey: "nav.cleaner",
    descKey: "cleaner.subtitle",
    icon: <Sparkles size={20} />,
    group: "office",
  },
  { id: "notes", titleKey: "nav.notes", descKey: "notes.subtitle", icon: <NotebookPen size={20} />, group: "tools" },
  {
    id: "planner",
    titleKey: "nav.planner",
    descKey: "planner.subtitle",
    icon: <CalendarDays size={20} />,
    group: "tools",
  },
  { id: "data", titleKey: "nav.data", descKey: "data.subtitle", icon: <Database size={20} />, group: "tools" },
  { id: "draw", titleKey: "nav.draw", descKey: "draw.subtitle", icon: <PenTool size={20} />, group: "tools" },
  { id: "vault", titleKey: "nav.vault", descKey: "vault.subtitle", icon: <Library size={20} />, group: "tools" },
  {
    id: "history",
    titleKey: "nav.history",
    descKey: "history.subtitle",
    icon: <FolderClock size={20} />,
    group: "tools",
  },
  { id: "jobs", titleKey: "nav.jobs", descKey: "jobs.subtitle", icon: <ListChecks size={20} />, group: "tools" },
  { id: "sync", titleKey: "nav.sync", descKey: "sync.subtitle", icon: <RefreshCw size={20} />, group: "tools" },
  { id: "plugins", titleKey: "nav.plugins", icon: <Puzzle size={20} />, group: "tools" },
  { id: "compat", titleKey: "nav.compat", descKey: "compat.subtitle", icon: <ShieldCheck size={20} />, group: "tools" },
  { id: "ai", titleKey: "nav.ai", descKey: "ai.subtitle", icon: <Bot size={20} />, group: "ai", accent: true },
  { id: "aiLibrary", titleKey: "nav.aiLibrary", icon: <Library size={20} />, group: "ai" },
];

const QUICK_ACTIONS: { id: ScreenId; icon: typeof Combine }[] = [
  { id: "reader", icon: BookOpen },
  { id: "merge", icon: Combine },
  { id: "organize", icon: Layers },
  { id: "split", icon: Scissors },
  { id: "compress", icon: Minimize2 },
  { id: "ocr", icon: FileSearch },
  { id: "watermark", icon: Stamp },
  { id: "protect", icon: Lock },
];

/** Icon lookup so a recommended tool renders with the card's own icon. */
const ICON_BY_ID: Map<ScreenId, React.ReactNode> = new Map(CARDS.map((card) => [card.id, card.icon]));

function fold(text: string): string {
  return foldSearch(text);
}

export function Home({
  onNavigate,
  onDropFiles,
  dragging,
  onFileList,
}: {
  onNavigate: Navigate;
  onDropFiles: (paths: string[]) => void;
  dragging: boolean;
  onFileList: (paths: string[]) => void;
}) {
  const t = useT();
  const recent = useRecent((s) => s.entries);
  const refreshRecent = useRecent((s) => s.refresh);
  const settings = useSettings((s) => s.settings);
  const settingsLoaded = useSettings((s) => s.loaded);
  const updateSettings = useSettings((s) => s.update);
  const pushToast = useToasts((s) => s.push);
  const [suggestion, setSuggestion] = useState<string[] | null>(null);
  const [query, setQuery] = useState("");

  // The profession desk leads Home; until the first-run question is answered
  // (or while "Change desk" is open) the picker takes its place.
  const profession = settings.profession ?? "";
  const desk = useMemo(() => deskTasks(profession), [profession]);
  const [choosing, setChoosing] = useState(false);
  const [showAllTools, setShowAllTools] = useState(false);
  const asking = settingsLoaded && (profession === "" || choosing);
  const catalogueCollapsed = desk.length > 0 && !showAllTools;
  const chooseProfession = (choice: ProfessionChoice) => {
    setChoosing(false);
    setShowAllTools(false);
    void updateSettings({ profession: choice });
  };

  const pinned = useMemo(() => settings.pinnedTools ?? [], [settings.pinnedTools]);
  const pinnedCards = useMemo(() => CARDS.filter((card) => pinned.includes(card.id)), [pinned]);
  const togglePin = (id: ScreenId) => {
    const next = pinned.includes(id) ? pinned.filter((entry) => entry !== id) : [...pinned, id];
    void updateSettings({ pinnedTools: next });
  };

  useEffect(() => {
    void refreshRecent();
  }, [refreshRecent]);

  // Cards follow the same route manifest: a flag-gated route is not offered.
  // While a desk is shown the full catalogue sits behind "All tools" (search
  // still covers every tool).
  const cards = useMemo(() => CARDS.filter((card) => routeEnabled(card.id)), []);

  const needle = fold(query.trim());
  const matches = useMemo(() => {
    if (!needle) return cards;
    return cards.filter((card) => {
      const haystack = fold(`${t(card.titleKey)} ${card.descKey ? t(card.descKey) : ""} ${card.id}`);
      return haystack.includes(needle);
    });
  }, [cards, needle, t]);

  const grouped = useMemo(
    () =>
      GROUPS.map((group) => ({ ...group, items: matches.filter((card) => card.group === group.key) })).filter(
        (group) => group.items.length > 0,
      ),
    [matches],
  );

  // PDFs open in the reader and office documents in their editor; anything
  // else (images, archives, outputs of other tools) goes to the system viewer.
  const openRecent = async (entry: RecentEntry) => {
    if (isPdf(entry.fileName)) {
      onFileList([entry.token]);
      onNavigate("reader");
      return;
    }
    if (isOfficePath(entry.fileName)) {
      onNavigate("office");
      const result = await openOfficePath(entry.token);
      if (!result.ok) pushToast({ kind: "error", title: t("errors.title"), detail: result.error });
      return;
    }
    await openAnyFile(entry.token).catch(() => undefined);
  };

  const suggestionLabel = suggestion
    ? suggestion.length === 1
      ? fileBaseName(suggestion[0])
      : `${suggestion.length} ${t("common.selected")}`
    : "";

  return (
    <Screen
      title={
        <span className="flex items-center gap-2.5">
          <BrandMark size={30} className="shrink-0" />
          <span className="brand-gradient">{t("app.name")}</span>
        </span>
      }
      subtitle={t("app.tagline")}
    >
      <UpdateBanner />
      {asking ? <ProfessionPicker onChoose={chooseProfession} /> : null}
      {profession !== "" ? <WelcomeCard onNavigate={onNavigate} /> : null}
      {!asking && isProfession(profession) && desk.length > 0 ? (
        <ProfessionDesk
          profession={profession}
          tasks={desk}
          onNavigate={onNavigate}
          onChange={() => setChoosing(true)}
        />
      ) : null}
      <DropZone
        onPaths={(paths) => {
          onDropFiles(paths);
          if (paths.length > 0) setSuggestion(paths);
        }}
        title={isAndroid() ? t("common.selectFiles") : t("home.dropTitle")}
        hint={isAndroid() ? t("common.tapHint") : t("home.dropHint")}
        dragging={dragging}
      />

      {suggestion && suggestion.length > 0 ? (
        <Card className="p-4 fade-in">
          <div className="flex items-center gap-2 mb-3">
            <Sparkles size={16} style={{ color: "var(--accent)" }} />
            <p className="text-[13.5px]">
              <strong>{suggestionLabel}</strong> — {t("home.quickActions")}
            </p>
          </div>
          <div className="flex flex-wrap gap-2">
            {(() => {
              const recommended = recommendTools(suggestion);
              const ids = recommended.length ? recommended : QUICK_ACTIONS.map((action) => action.id);
              return ids.map((id) => (
                <Button
                  key={id}
                  size="sm"
                  icon={(ICON_BY_ID.get(id) ?? <Sparkles size={14} />) as React.ReactElement}
                  onClick={() => onNavigate(id, { files: suggestion })}
                >
                  {t(`nav.${id}`)}
                </Button>
              ));
            })()}
            <Button
              size="sm"
              variant="ghost"
              icon={<KeyRound size={14} />}
              onClick={() => onNavigate("info", { files: suggestion })}
            >
              {t("nav.info")}
            </Button>
          </div>
        </Card>
      ) : null}

      <section>
        <div className="flex items-center gap-3 mb-3">
          <h2 className="text-[13px] font-bold uppercase tracking-wider muted shrink-0">{t("home.quickActions")}</h2>
          <div className="relative flex-1 max-w-md">
            <Search size={14} className="absolute left-2.5 top-1/2 -translate-y-1/2 muted pointer-events-none" />
            <input
              className="input input-sm input-lead-icon"
              value={query}
              aria-label={t("home.searchTools")}
              placeholder={t("home.searchTools")}
              onChange={(event) => setQuery(event.target.value)}
            />
          </div>
        </div>

        {matches.length === 0 ? (
          <Card>
            <EmptyState icon={<Search size={22} />} title={t("home.noToolResults")} />
          </Card>
        ) : query.trim() ? (
          <div className="grid gap-3" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(230px, 1fr))" }}>
            {matches.map((card) => (
              <ToolCard key={card.id} card={card} onOpen={() => onNavigate(card.id)} t={t} />
            ))}
          </div>
        ) : (
          <div className="flex flex-col gap-5">
            {pinnedCards.length ? (
              <div>
                <h3 className="text-[11px] font-bold uppercase tracking-wider muted mb-2">{t("home.pinned")}</h3>
                <div className="grid gap-3" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(230px, 1fr))" }}>
                  {pinnedCards.map((card) => (
                    <ToolCard
                      key={card.id}
                      card={card}
                      onOpen={() => onNavigate(card.id)}
                      pinned
                      onTogglePin={() => togglePin(card.id)}
                      t={t}
                    />
                  ))}
                </div>
              </div>
            ) : null}
            {catalogueCollapsed ? (
              <div>
                <Button size="sm" icon={<LayoutGrid size={14} />} onClick={() => setShowAllTools(true)}>
                  {t("desk.allTools", { count: cards.length })}
                </Button>
              </div>
            ) : null}
            {(catalogueCollapsed ? [] : grouped).map((group) => (
              <div key={group.key}>
                <h3 className="text-[11px] font-bold uppercase tracking-wider muted mb-2">{t(group.labelKey)}</h3>
                <div className="grid gap-3" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(230px, 1fr))" }}>
                  {group.items.map((card) => (
                    <ToolCard
                      key={card.id}
                      card={card}
                      onOpen={() => onNavigate(card.id)}
                      pinned={pinned.includes(card.id)}
                      onTogglePin={() => togglePin(card.id)}
                      t={t}
                    />
                  ))}
                </div>
              </div>
            ))}
          </div>
        )}
      </section>

      {settings.showRecentFiles ? (
        <section>
          <div className="flex items-center justify-between mb-3">
            <h2 className="text-[13px] font-bold uppercase tracking-wider muted">{t("home.recent")}</h2>
            <Button size="sm" variant="ghost" onClick={() => onNavigate("history")}>
              {t("home.viewAll")}
            </Button>
          </div>
          {recent.length === 0 ? (
            <Card>
              <EmptyState
                icon={<FileImage size={22} />}
                title={t("home.recentEmpty")}
                hint={t("home.recentEmptyHint")}
              />
            </Card>
          ) : (
            <Card className="p-2">
              <div className="flex flex-col">
                {recent.slice(0, 6).map((entry) => (
                  <div
                    key={entry.token}
                    className="flex items-center gap-3 px-2.5 py-2 rounded-lg hover:bg-[var(--surface-2)]"
                  >
                    <div
                      className="w-8 h-8 rounded-lg flex items-center justify-center shrink-0"
                      style={{ background: "var(--accent-weak)", color: "var(--accent)" }}
                    >
                      <RecentIcon name={entry.fileName} />
                    </div>
                    <div className="min-w-0 flex-1">
                      <p className="truncate text-[13.5px] font-medium">{entry.fileName}</p>
                    </div>
                    <span className="text-xs muted shrink-0">{formatDate(entry.timestamp)}</span>
                    <div className="flex gap-1 shrink-0">
                      <Button size="sm" variant="ghost" onClick={() => void openRecent(entry)}>
                        {t("common.open")}
                      </Button>
                      <IconButton
                        label={isAndroid() ? t("common.share") : t("common.openFolder")}
                        onClick={() => void shareRecent(entry.token, entry.fileName).catch(() => undefined)}
                      >
                        {isAndroid() ? <Share2 size={14} /> : <FolderOpen size={14} />}
                      </IconButton>
                    </div>
                  </div>
                ))}
              </div>
            </Card>
          )}
        </section>
      ) : null}

      <Card soft className="p-4 flex items-start gap-3">
        <Lock size={16} style={{ color: "var(--ok)", marginTop: 2 }} />
        <div>
          <p className="font-semibold text-[13px]">{t("nav.privacy")}</p>
          <p className="text-xs muted mt-1">
            {t("home.privacyNote")} {t("home.supportedFormats")}
          </p>
        </div>
      </Card>
    </Screen>
  );
}

function RecentIcon({ name }: { name: string }) {
  if (isSpreadsheetPath(name)) return <FileSpreadsheet size={15} />;
  if (isPresentationPath(name)) return <Presentation size={15} />;
  if (isOfficePath(name) || isPdf(name)) return <FileText size={15} />;
  return <FileImage size={15} />;
}

function ToolCard({
  card,
  onOpen,
  pinned = false,
  onTogglePin,
  t,
}: {
  card: ToolCardSpec;
  onOpen: () => void;
  pinned?: boolean;
  onTogglePin?: () => void;
  t: (key: string) => string;
}) {
  return (
    <div style={{ position: "relative" }}>
      <button className="tool-card" onClick={onOpen}>
        <span
          className="tool-icon"
          style={card.accent ? { background: "var(--accent)", color: "var(--accent-text)" } : undefined}
        >
          {card.icon}
        </span>
        <span>
          <span className="block font-semibold text-[14.5px]">{t(card.titleKey)}</span>
          {card.descKey ? <span className="block text-xs muted mt-1 leading-relaxed">{t(card.descKey)}</span> : null}
        </span>
      </button>
      {onTogglePin ? (
        <button
          type="button"
          className="icon-btn"
          style={{ position: "absolute", top: 6, right: 6 }}
          aria-label={pinned ? t("home.unpinTool") : t("home.pinTool")}
          title={pinned ? t("home.unpinTool") : t("home.pinTool")}
          onClick={(event) => {
            event.stopPropagation();
            onTogglePin();
          }}
        >
          <Pin size={13} fill={pinned ? "currentColor" : "none"} />
        </button>
      ) : null}
    </div>
  );
}
