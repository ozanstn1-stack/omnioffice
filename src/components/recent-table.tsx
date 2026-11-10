import { useState } from "react";
import { FileImage, FolderOpen, Share2 } from "lucide-react";
import { EmptyState, IconButton } from "./ui";
import { useT } from "../lib/i18n";
import { isPdf, isPresentationPath, isSpreadsheetPath } from "../lib/format";
import { isOfficePath, OFFICE_RECENT_TOOL } from "../lib/office-store";
import { isAndroid } from "../lib/mobile";
import { routeEnabled, SCREEN_IDS } from "../lib/routes";
import type { RecentEntry } from "../lib/types";
import type { ScreenId } from "../lib/nav";

export type RecentKind = "doc" | "sheet" | "slides" | "pdf" | "other";

/** The kind a recent file is filtered and coloured by. */
export function recentKind(name: string): RecentKind {
  if (isPdf(name)) return "pdf";
  if (isSpreadsheetPath(name)) return "sheet";
  if (isPresentationPath(name)) return "slides";
  if (isOfficePath(name)) return "doc";
  return "other";
}

const FILTERS: { kind: RecentKind | "all"; labelKey: string }[] = [
  { kind: "all", labelKey: "recent.filterAll" },
  { kind: "doc", labelKey: "recent.filterDocs" },
  { kind: "sheet", labelKey: "recent.filterSheets" },
  { kind: "slides", labelKey: "recent.filterSlides" },
  { kind: "pdf", labelKey: "recent.filterPdf" },
];

const TILE: Record<RecentKind, string> = {
  doc: "doc-tile-writer",
  sheet: "doc-tile-calc",
  slides: "doc-tile-impress",
  pdf: "doc-tile-pdf",
  other: "",
};

/** "12 min ago", "3 h ago", "yesterday", then the short date. */
export function relativeTime(timestampSeconds: number, language: string, now: number = Date.now()): string {
  const seconds = Math.round(now / 1000 - timestampSeconds);
  try {
    const rtf = new Intl.RelativeTimeFormat(language, { numeric: "auto", style: "short" });
    if (seconds < 60) return rtf.format(0, "second");
    if (seconds < 3600) return rtf.format(-Math.floor(seconds / 60), "minute");
    if (seconds < 86_400) return rtf.format(-Math.floor(seconds / 3600), "hour");
    if (seconds < 7 * 86_400) return rtf.format(-Math.floor(seconds / 86_400), "day");
    return new Intl.DateTimeFormat(language, { day: "numeric", month: "short" }).format(
      new Date(timestampSeconds * 1000),
    );
  } catch {
    return new Date(timestampSeconds * 1000).toLocaleDateString();
  }
}

/** The tool a file was last used with, as the sidebar names it. */
function toolLabel(tool: string, t: (key: string) => string): string {
  if (tool === OFFICE_RECENT_TOOL) return t("nav.office");
  return (SCREEN_IDS as readonly string[]).includes(tool) && routeEnabled(tool as ScreenId) ? t(`nav.${tool}`) : "";
}

/**
 * Home's recent files (7.32): a filterable table with the document kind's
 * colour, the tool it was last used with and a relative time. Opening and
 * sharing stay with the caller, which knows where each kind opens.
 */
export function RecentTable({
  entries,
  language,
  limit = 7,
  onOpen,
  onShare,
}: {
  entries: RecentEntry[];
  language: string;
  limit?: number;
  onOpen: (entry: RecentEntry) => void;
  onShare: (entry: RecentEntry) => void;
}) {
  const t = useT();
  const [filter, setFilter] = useState<RecentKind | "all">("all");
  const shown = entries.filter((entry) => filter === "all" || recentKind(entry.fileName) === filter).slice(0, limit);

  return (
    <div className="recent-table">
      <div className="recent-filters" role="group" aria-label={t("recent.filterLabel")}>
        {FILTERS.map((item) => (
          <button
            key={item.kind}
            type="button"
            className="chip"
            aria-pressed={filter === item.kind}
            onClick={() => setFilter(item.kind)}
          >
            {t(item.labelKey)}
          </button>
        ))}
      </div>
      {shown.length === 0 ? (
        <EmptyState
          icon={<FileImage size={22} />}
          title={entries.length === 0 ? t("home.recentEmpty") : t("recent.noneOfKind")}
          hint={entries.length === 0 ? t("home.recentEmptyHint") : undefined}
        />
      ) : (
        <ul className="recent-rows">
          {shown.map((entry) => {
            const kind = recentKind(entry.fileName);
            const ext = (entry.fileName.split(".").pop() ?? "").slice(0, 4).toUpperCase();
            const tool = toolLabel(entry.tool, t);
            return (
              <li key={entry.token} className="recent-row-item">
                <button type="button" className="recent-open" onClick={() => onOpen(entry)}>
                  <span className={`doc-tile recent-ext ${TILE[kind]}`} aria-hidden>
                    {ext || "—"}
                  </span>
                  <span className="recent-name">{entry.fileName}</span>
                  <span className="recent-tool">{tool}</span>
                  <span className="recent-when">{relativeTime(entry.timestamp, language)}</span>
                </button>
                <IconButton
                  label={isAndroid() ? t("common.share") : t("common.openFolder")}
                  onClick={() => onShare(entry)}
                >
                  {isAndroid() ? <Share2 size={15} /> : <FolderOpen size={15} />}
                </IconButton>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
