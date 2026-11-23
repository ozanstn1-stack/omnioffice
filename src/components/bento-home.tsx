import { ArrowRight, FolderOpen, Lock, ScanLine, Search, Sparkles } from "lucide-react";
import { Button } from "./ui";
import { JobsGlance } from "./jobs-glance";
import { recentKind, relativeTime } from "./recent-table";
import { useT } from "../lib/i18n";
import { useOfficeTabs } from "../lib/office-store";
import { pickDocumentTokens } from "../lib/file-tokens";
import { extensionsFor } from "../lib/tool-files";
import { isAndroid, pickAndroidFiles } from "../lib/mobile";
import type { Navigate } from "../lib/nav";
import type { RecentEntry } from "../lib/types";

const KIND_CLASS: Record<string, string> = {
  doc: "doc-tile-writer",
  writer: "doc-tile-writer",
  sheet: "doc-tile-calc",
  calc: "doc-tile-calc",
  slides: "doc-tile-impress",
  impress: "doc-tile-impress",
  pdf: "doc-tile-pdf",
  other: "",
};

const KIND_EXT: Record<string, string> = {
  writer: "DOCX",
  calc: "XLSX",
  impress: "PPTX",
};

const OMNIBAR_SHORTCUT = "Ctrl K";

export function BentoOmnibar({
  dragging,
  onSearch,
  onDropFiles,
}: {
  dragging: boolean;
  onSearch?: () => void;
  onDropFiles: (paths: string[]) => void;
}) {
  const t = useT();

  const pickFiles = async () => {
    if (isAndroid()) {
      const paths = await pickAndroidFiles({ multiple: true, accept: "any" }).catch(() => []);
      if (paths.length) onDropFiles(paths);
      return;
    }
    const refs = await pickDocumentTokens(extensionsFor("document"), true).catch(() => []);
    if (refs.length) onDropFiles(refs.map((ref) => ref.token));
  };

  return (
    <div className="bento-omnibar" data-over={dragging}>
      <button
        type="button"
        className="bento-omnibar-trigger"
        onClick={onSearch}
        aria-label={t("bento.omnibarPlaceholder")}
      >
        <span className="bento-omnibar-orb" aria-hidden>
          <Sparkles size={17} />
        </span>
        <span className="min-w-0 flex-1 text-left">
          <span className="bento-omnibar-title">{t("bento.omnibarPlaceholder")}</span>
          <span className="bento-omnibar-hint">{t("bento.omnibarHint")}</span>
        </span>
      </button>
      <div className="bento-omnibar-actions">
        <Button size="sm" variant="primary" icon={<FolderOpen size={14} />} onClick={() => void pickFiles()}>
          {t("bento.omnibarPick")}
        </Button>
        {!isAndroid() ? (
          <button type="button" className="bento-omnibar-kbd" onClick={onSearch} aria-label={t("nav.search")}>
            <Search size={12} aria-hidden />
            <kbd>{OMNIBAR_SHORTCUT}</kbd>
          </button>
        ) : null}
      </div>
    </div>
  );
}

export function BentoHeroGrid({
  recent,
  language,
  onOpenRecent,
  onNavigate,
}: {
  recent: RecentEntry[];
  language: string;
  onOpenRecent: (entry: RecentEntry) => void;
  onNavigate: Navigate;
}) {
  const t = useT();
  const officeTabs = useOfficeTabs((s) => s.tabs);
  const activeOffice = officeTabs[officeTabs.length - 1] ?? null;
  const latestRecent = recent[0] ?? null;

  const resumeKind = activeOffice ? activeOffice.kind : latestRecent ? recentKind(latestRecent.fileName) : null;
  const resumeExt = activeOffice
    ? (KIND_EXT[activeOffice.kind] ?? "DOC")
    : latestRecent
      ? (latestRecent.fileName.split(".").pop() ?? "FILE").slice(0, 4).toUpperCase()
      : "";
  const resumeTitle = activeOffice ? activeOffice.title : (latestRecent?.fileName ?? "");
  const resumeMeta = activeOffice
    ? t("nav.office")
    : latestRecent
      ? relativeTime(latestRecent.timestamp, language)
      : "";

  const openResume = () => {
    if (activeOffice) {
      useOfficeTabs.getState().activate(activeOffice.id);
      onNavigate("office");
      return;
    }
    if (latestRecent) onOpenRecent(latestRecent);
  };

  return (
    <div className="bento-hero-grid">
      <div className="bento-resume-card">
        {resumeTitle ? (
          <>
            <div className="bento-page-thumb" aria-hidden>
              <span className={`bento-page-bar ${KIND_CLASS[resumeKind ?? "other"]}`} />
              <span className="bento-page-lines">
                <span className="bento-page-line w-3/4" />
                <span className="bento-page-line w-full" />
                <span className="bento-page-line w-5/6" />
              </span>
              <span className={`bento-page-ext ${KIND_CLASS[resumeKind ?? "other"]}`}>{resumeExt}</span>
            </div>
            <div className="bento-resume-body">
              <span className="bento-resume-badge">{t("bento.resumeBadge")}</span>
              <h3 className="bento-resume-title">{resumeTitle}</h3>
              <p className="text-xs muted">{resumeMeta}</p>
              <div className="bento-resume-actions">
                <Button size="sm" variant="primary" icon={<ArrowRight size={14} />} onClick={openResume}>
                  {t("bento.resumeOpen")}
                </Button>
                <Button size="sm" variant="ghost" icon={<Sparkles size={14} />} onClick={() => onNavigate("ai")}>
                  {t("nav.assistant")}
                </Button>
              </div>
            </div>
          </>
        ) : (
          <div className="bento-resume-body">
            <span className="bento-resume-badge">{t("bento.resumeBadge")}</span>
            <h3 className="bento-resume-title">{t("bento.resumeEmptyTitle")}</h3>
            <p className="text-xs muted">{t("bento.resumeEmptyHint")}</p>
            <div className="bento-resume-actions">
              <Button
                size="sm"
                variant="primary"
                onClick={() => {
                  useOfficeTabs.getState().create("writer");
                  onNavigate("office");
                }}
              >
                {t("home.newDocument")}
              </Button>
              <Button size="sm" icon={<ScanLine size={14} />} onClick={() => onNavigate("scanner")}>
                {t("mobile.scanNow")}
              </Button>
            </div>
          </div>
        )}
      </div>

      <div className="bento-pulse-stack">
        <div className="bento-engine-card">
          <div className="flex items-center justify-between gap-2">
            <span className="flex items-center gap-2 font-semibold text-[13.5px]">
              <span className="pulse-dot" aria-hidden />
              {t("bento.enginePulse")}
            </span>
            <Lock size={14} style={{ color: "var(--ok)" }} aria-hidden />
          </div>
          <p className="text-xs muted">{t("bento.engineReady")}</p>
          <p className="text-[11.5px] muted">{t("home.supportedFormats")}</p>
        </div>
        <JobsGlance onNavigate={onNavigate} />
      </div>
    </div>
  );
}
