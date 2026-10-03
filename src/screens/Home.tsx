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
  ShieldCheck,
  Sparkles,
  Stamp,
  Type,
  Wand2,
} from "lucide-react";
import { Button, Card, EmptyState, IconButton } from "../components/ui";
import { DropZone } from "../components/files";
import { Screen } from "../components/layout";
import { useT } from "../lib/i18n";
import { useRecent, useSettings } from "../lib/store";
import { fileBaseName, formatDate, isPdf } from "../lib/format";
import { isAndroid, openAnyFile, revealAnyFile } from "../lib/mobile";
import type { Navigate, ScreenId } from "../lib/nav";
import { fileStem } from "../lib/format";

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
  { key: "pdf", labelKey: "home.groupPdf" },
  { key: "office", labelKey: "home.groupOffice" },
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
  { id: "office", titleKey: "nav.office", icon: <LayoutGrid size={20} />, group: "office", accent: true },
  { id: "documents", titleKey: "nav.documents", icon: <FileText size={20} />, group: "office" },
  { id: "spreadsheets", titleKey: "nav.spreadsheets", icon: <FileSpreadsheet size={20} />, group: "office" },
  { id: "presentations", titleKey: "nav.presentations", icon: <Presentation size={20} />, group: "office" },
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

function fold(text: string): string {
  return text.toLowerCase();
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
  const [suggestion, setSuggestion] = useState<string[] | null>(null);
  const [query, setQuery] = useState("");

  useEffect(() => {
    void refreshRecent();
  }, [refreshRecent]);

  const cards = useMemo(() => CARDS, []);

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

  const suggestionLabel = suggestion
    ? suggestion.length === 1
      ? fileBaseName(suggestion[0])
      : `${suggestion.length} ${t("common.selected")}`
    : "";

  return (
    <Screen title={t("app.name")} subtitle={t("app.tagline")}>
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
            {QUICK_ACTIONS.map(({ id, icon: IconComponent }) => (
              <Button
                key={id}
                size="sm"
                icon={<IconComponent size={14} />}
                onClick={() => onNavigate(id, { files: suggestion })}
              >
                {t(`nav.${id}`)}
              </Button>
            ))}
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
              className="input input-sm pl-8"
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
            {grouped.map((group) => (
              <div key={group.key}>
                <h3 className="text-[11px] font-bold uppercase tracking-wider muted mb-2">{t(group.labelKey)}</h3>
                <div className="grid gap-3" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(230px, 1fr))" }}>
                  {group.items.map((card) => (
                    <ToolCard key={card.id} card={card} onOpen={() => onNavigate(card.id)} t={t} />
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
                    key={entry.path}
                    className="flex items-center gap-3 px-2.5 py-2 rounded-lg hover:bg-[var(--surface-2)]"
                  >
                    <div
                      className="w-8 h-8 rounded-lg flex items-center justify-center shrink-0"
                      style={{ background: "var(--accent-weak)", color: "var(--accent)" }}
                    >
                      <FileImage size={15} />
                    </div>
                    <div className="min-w-0 flex-1">
                      <p className="truncate text-[13.5px] font-medium">{entry.fileName || fileStem(entry.path)}</p>
                      <p className="text-xs muted truncate">{entry.path}</p>
                    </div>
                    <span className="text-xs muted shrink-0">{formatDate(entry.timestamp)}</span>
                    <div className="flex gap-1 shrink-0">
                      <Button
                        size="sm"
                        variant="ghost"
                        onClick={() => {
                          if (isPdf(entry.path)) {
                            onFileList([entry.path]);
                            onNavigate("reader");
                          } else {
                            void openAnyFile(entry.path).catch(() => undefined);
                          }
                        }}
                      >
                        {t("common.open")}
                      </Button>
                      <IconButton
                        label={t("common.openFolder")}
                        onClick={() => void revealAnyFile(entry.path).catch(() => undefined)}
                      >
                        <FolderOpen size={14} />
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

function ToolCard({ card, onOpen, t }: { card: ToolCardSpec; onOpen: () => void; t: (key: string) => string }) {
  return (
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
  );
}
