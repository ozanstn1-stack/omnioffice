/**
 * The page queue of the document scanner: the count, the filter choice and one
 * row per photo with its detected/corrected state, reorder and remove actions.
 *
 * Auto-detected pages carry the automatic badge; a page the user corrected by
 * hand says so instead, and a page with no corners at all is kept as it is.
 */
import { Crop } from "lucide-react";
import { Badge, Button } from "../components/ui";
import { fileBaseName } from "../lib/format";
import { useT } from "../lib/i18n";
import type { ScanFilter } from "../lib/scan-detect";

export interface ScannerPage {
  path: string;
  corners?: [number, number][];
  /** True when the corners came from the detector, not from the picker. */
  auto?: boolean;
}

const FILTERS: { value: ScanFilter; key: string }[] = [
  { value: "colour", key: "scanner.filterColour" },
  { value: "greyscale", key: "scanner.filterGrey" },
  { value: "blackWhite", key: "scanner.filterBlackWhite" },
];

export function ScannerPageList({
  pages,
  busy,
  detecting,
  filter,
  onFilter,
  onMove,
  onRemove,
  onAdjust,
  onClear,
}: {
  pages: ScannerPage[];
  busy: boolean;
  detecting: boolean;
  filter: ScanFilter;
  onFilter: (filter: ScanFilter) => void;
  onMove: (index: number, by: number) => void;
  onRemove: (path: string) => void;
  onAdjust: (path: string) => void;
  onClear: () => void;
}) {
  const t = useT();
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between gap-2 flex-wrap">
        <div className="flex items-center gap-2 flex-wrap">
          <span className="text-xs muted">{t("scanner.filter")}</span>
          {FILTERS.map((item) => (
            <Button
              key={item.value}
              size="sm"
              variant={filter === item.value ? "primary" : "ghost"}
              disabled={busy || detecting || !pages.length}
              onClick={() => onFilter(item.value)}
            >
              {t(item.key)}
            </Button>
          ))}
          {detecting ? <span className="text-xs muted">{t("scanner.detecting")}</span> : null}
        </div>
        {pages.length ? (
          <div className="flex items-center gap-2 text-xs muted">
            <span>{t("scanner.pageCount", { count: pages.length })}</span>
            <Button size="sm" variant="ghost" onClick={onClear}>
              {t("scanner.clearAll")}
            </Button>
          </div>
        ) : null}
      </div>
      {pages.length ? (
        <ul className="flex flex-col gap-1 text-sm">
          {pages.map((page, index) => (
            <li
              key={page.path}
              className="flex items-center justify-between gap-2 p-1.5 rounded bg-surface/50 border border-border/40"
            >
              <div className="flex items-center gap-2 min-w-0">
                <span className="muted font-mono text-xs">{index + 1}.</span>
                <span className="truncate">{fileBaseName(page.path)}</span>
                {page.auto ? <Badge tone="ok">{t("scanner.autoBadge")}</Badge> : null}
                {!page.auto && page.corners ? <Badge tone="accent">{t("scanner.cropped")}</Badge> : null}
              </div>
              <div className="flex items-center gap-1 shrink-0">
                <Button size="sm" variant="ghost" disabled={busy || detecting} onClick={() => onAdjust(page.path)}>
                  <Crop size={14} /> {t("scanner.adjust")}
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  aria-label={t("common.moveUp")}
                  disabled={index === 0 || busy || detecting}
                  onClick={() => onMove(index, -1)}
                >
                  ↑
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  aria-label={t("common.moveDown")}
                  disabled={index === pages.length - 1 || busy || detecting}
                  onClick={() => onMove(index, 1)}
                >
                  ↓
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  aria-label={t("common.remove")}
                  disabled={busy || detecting}
                  onClick={() => onRemove(page.path)}
                >
                  ×
                </Button>
              </div>
            </li>
          ))}
        </ul>
      ) : (
        <p className="muted text-sm">{t("scanner.empty")}</p>
      )}
    </div>
  );
}
