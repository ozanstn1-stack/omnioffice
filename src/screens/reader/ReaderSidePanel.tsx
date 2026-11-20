/**
 * The reader's side panel: page jump, bookmarks, search hits and file info, or
 * the "ask this document" AI chat. Wide windows show it beside the pages; on
 * narrow windows, phones and in focus mode it opens as an overlay on demand.
 */
import { lazy, Suspense, useState, type ComponentProps } from "react";
import { BookOpen, Sparkles, X } from "lucide-react";
import { IconButton, Spinner } from "../../components/ui";
import { InfoStrip } from "../../components/files";
import { useT } from "../../lib/i18n";
import { clamp } from "../../lib/format";
import type { Navigate } from "../../lib/nav";
import type { SearchResponse } from "../../lib/types";

// The AI chat is its own chunk: most reading sessions never open it, and the
// Reader route has a 12 KB gzip budget (scripts/check-bundle-budget.mjs).
const ReaderAiPanel = lazy(() => import("./ReaderAiPanel").then((module) => ({ default: module.ReaderAiPanel })));

export type ReaderSideTab = "info" | "ai";

export function ReaderSidePanel({
  tab,
  onTab,
  open,
  onClose,
  focus,
  docId,
  path,
  password,
  pageCount,
  currentPage,
  scrollToPage,
  onNavigate,
  info,
  infoError,
  searchResult,
  copiedPage,
}: {
  tab: ReaderSideTab;
  onTab: (tab: ReaderSideTab) => void;
  /** Opened on demand (overlay below `xl`, and in focus mode). */
  open: boolean;
  onClose: () => void;
  focus: boolean;
  docId: string;
  path: string;
  password?: string;
  pageCount: number;
  currentPage: number;
  scrollToPage: (page: number) => void;
  onNavigate?: Navigate;
  info: ComponentProps<typeof InfoStrip>["info"];
  infoError: ComponentProps<typeof InfoStrip>["error"];
  searchResult: SearchResponse | null;
  copiedPage: number | null;
}) {
  const t = useT();
  // Mounted on first use, then kept mounted so the conversation survives.
  const [aiUsed, setAiUsed] = useState(tab === "ai");
  if (tab === "ai" && !aiUsed) setAiUsed(true);
  return (
    <aside
      className={`shrink-0 border-l overflow-y-auto flex-col gap-3 p-3 ${
        open
          ? "flex absolute right-0 top-0 bottom-0 z-30 shadow-xl xl:static xl:shadow-none"
          : focus
            ? "hidden"
            : "hidden xl:flex"
      }`}
      style={{
        borderColor: "var(--border)",
        background: "var(--bg-soft)",
        width: tab === "ai" ? "min(360px, 100%)" : "min(300px, 100%)",
      }}
    >
      <div className="flex items-center gap-1" role="tablist">
        <button
          type="button"
          role="tab"
          aria-selected={tab === "info"}
          className={`btn btn-sm flex-1${tab === "info" ? " btn-primary" : ""}`}
          onClick={() => onTab("info")}
        >
          <BookOpen size={13} /> {t("reader.panelInfo")}
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={tab === "ai"}
          className={`btn btn-sm flex-1${tab === "ai" ? " btn-primary" : ""}`}
          onClick={() => onTab("ai")}
        >
          <Sparkles size={13} /> {t("reader.aiAssistant")}
        </button>
        {open ? (
          <IconButton label={t("common.close")} onClick={() => onClose()}>
            <X size={14} />
          </IconButton>
        ) : null}
      </div>

      {aiUsed ? (
        <div className={tab === "ai" ? "flex flex-col flex-1 min-h-0" : "hidden"}>
          <Suspense fallback={<Spinner size={14} />}>
            <ReaderAiPanel
              key={docId}
              path={path}
              password={password}
              pageCount={pageCount}
              onJumpToPage={scrollToPage}
              onNavigate={onNavigate}
            />
          </Suspense>
        </div>
      ) : null}
      {tab === "ai" ? null : (
        <>
          <p className="group-label">{t("reader.jumpTo")}</p>
          <div className="flex items-center gap-2">
            <input
              className="input input-sm"
              type="number"
              min={1}
              max={Math.max(1, pageCount)}
              value={currentPage}
              onChange={(event) => {
                const next = clamp(Number(event.target.value) || 1, 1, Math.max(1, pageCount));
                scrollToPage(next);
              }}
            />
            <span className="text-xs muted shrink-0">/ {pageCount}</span>
          </div>

          <p className="group-label mt-2">{t("reader.bookmarks")}</p>
          {info?.outline?.length ? (
            <div className="flex flex-col gap-0.5 max-h-[260px] overflow-y-auto">
              {info.outline.map((entry, index) => (
                <button
                  key={`${entry.title}-${index}`}
                  type="button"
                  className="text-left text-[12.5px] truncate px-2 py-1 rounded hover:bg-[var(--surface-2)] disabled:opacity-50"
                  style={{ paddingLeft: 8 + Math.min(entry.depth, 8) * 12 }}
                  disabled={entry.page < 1}
                  title={entry.title}
                  onClick={() => {
                    if (entry.page > 0) scrollToPage(entry.page);
                  }}
                >
                  {entry.title || t("reader.untitledBookmark")}
                  {entry.page > 0 ? <span className="muted text-xs"> · {entry.page}</span> : null}
                </button>
              ))}
            </div>
          ) : (
            <p className="text-xs muted">{t("reader.noBookmarks")}</p>
          )}

          <p className="group-label mt-2">{t("reader.search")}</p>
          {searchResult ? (
            searchResult.matches.length ? (
              <div className="flex flex-col gap-1.5">
                {searchResult.matches.slice(0, 80).map((match, index) => (
                  <button
                    key={`${match.page}-${index}`}
                    className="text-left card-soft px-2.5 py-2 text-[12.5px] hover:border-[var(--accent)]"
                    onClick={() => scrollToPage(match.page)}
                    title={match.snippet}
                  >
                    <span className="badge badge-accent mr-2">{match.page}</span>
                    <span className="muted">{match.snippet || t("common.page")}</span>
                  </button>
                ))}
              </div>
            ) : (
              <p className="text-xs muted">{t("reader.noMatches")}</p>
            )
          ) : (
            <p className="text-xs muted">{t("reader.searchPlaceholder")}</p>
          )}

          <div className="mt-auto pt-3">
            {info || infoError ? (
              <div className="card-soft p-3">
                <InfoStrip info={info} error={infoError} />
              </div>
            ) : null}
            {copiedPage ? (
              <p className="text-xs mt-2" style={{ color: "var(--ok)" }}>
                {t("reader.copied", { page: copiedPage })}
              </p>
            ) : null}
          </div>
        </>
      )}
    </aside>
  );
}
