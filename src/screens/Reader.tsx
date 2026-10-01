import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import {
  BookOpen,
  ChevronLeft,
  ChevronRight,
  Copy,
  ExternalLink,
  Search,
  X,
  ZoomIn,
  ZoomOut,
} from "lucide-react";
import { openAnyFile } from "../lib/mobile";
import { Badge, Button, IconButton, Spinner, useDebounced } from "../components/ui";
import { DropZone, InfoStrip } from "../components/files";
import { Screen } from "../components/layout";
import { useT } from "../lib/i18n";
import { useTool } from "../lib/useTool";
import { pagePreview, pageText, searchDocument, toAppError } from "../lib/api";
import { clamp, fileBaseName, uid } from "../lib/format";
import { reportError, useJobProgress, useToasts } from "../lib/store";
import type { PageGeometry, SearchResponse, TextMatch } from "../lib/types";

const PT_TO_CSS = 96 / 72;

// ---------------------------------------------------------------------------
// Touch zoom maths (pure so jsdom can test them)
// ---------------------------------------------------------------------------

/** Clamps a reader zoom factor to the range the buttons and pinch produce. */
export function clampReaderZoom(value: number): number {
  if (!Number.isFinite(value)) return 1;
  return clamp(Number(value.toFixed(2)), 0.25, 4);
}

/** The zoom a two-finger pinch asks for: the distance ratio applied to the
 * zoom the gesture started with. */
export function pinchZoomValue(startZoom: number, startDistance: number, distance: number): number {
  if (!Number.isFinite(distance) || !Number.isFinite(startDistance) || startDistance <= 0) return startZoom;
  return clampReaderZoom(startZoom * (distance / startDistance));
}

/** Double-tap toggles between fit width and 200 %, like mobile PDF readers. */
export function doubleTapZoom(current: number | "fit"): number | "fit" {
  return current === 2 ? "fit" : 2;
}

interface ReaderPinch {
  distance: number;
  zoom: number;
}

/** Where a pinch midpoint sat in content coordinates, kept so the layout
 * effect can put the same content point back under the finger after the
 * debounced page width changes. */
interface ReaderFocal {
  contentX: number;
  contentY: number;
  screenX: number;
  screenY: number;
  width: number;
}

/** Copies text to the clipboard with a fallback for restricted webviews. */
async function copyText(value: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(value);
    return true;
  } catch {
    try {
      const area = document.createElement("textarea");
      area.value = value;
      area.style.position = "fixed";
      area.style.opacity = "0";
      document.body.appendChild(area);
      area.select();
      const ok = document.execCommand("copy");
      area.remove();
      return ok;
    } catch {
      return false;
    }
  }
}

function ReaderPage({
  path,
  page,
  geometry,
  width,
  password,
  cache,
  register,
  highlight,
}: {
  path: string;
  page: number;
  geometry: PageGeometry;
  width: number;
  password?: string;
  cache: Map<string, string>;
  register: (page: number, element: HTMLDivElement | null) => void;
  highlight?: string;
}) {
  const key = `${path}@${page}@${width}`;
  const [src, setSrc] = useState<string | null>(() => cache.get(key) ?? null);
  const [failed, setFailed] = useState(false);
  const elementRef = useRef<HTMLDivElement | null>(null);
  const requested = useRef(false);
  // A new page key means a different preview: reset the src/failed and the
  // once-only guard together with the key (derived during render).
  const [lastKey, setLastKey] = useState(key);
  if (lastKey !== key) {
    setLastKey(key);
    setSrc(cache.get(key) ?? null);
    setFailed(false);
  }

  useEffect(() => {
    const element = elementRef.current;
    if (!element) return;
    // A new key means a new thumbnail: reset the once-only guard in an effect
    // (effects may touch refs; render may not).
    requested.current = false;
    // Lazy rendering: pages are only rasterized when they come near the
    // viewport, which keeps memory flat for very large documents.
    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries.some((entry) => entry.isIntersecting)) return;
        if (requested.current) return;
        requested.current = true;
        void pagePreview(path, page, width, password, "jpeg", 86)
          .then((preview) => {
            cache.set(key, preview.dataUrl);
            setSrc(preview.dataUrl);
          })
          .catch(() => setFailed(true));
      },
      { rootMargin: "900px 0px" },
    );
    observer.observe(element);
    return () => {
      observer.disconnect();
      requested.current = false;
    };
  }, [cache, key, page, password, path, width]);

  const ratio = geometry.display_height_pt / Math.max(1, geometry.display_width_pt);
  const height = Math.round(width * ratio);

  return (
    <div
      ref={(node) => {
        elementRef.current = node;
        register(page, node);
      }}
      data-page={page}
      className="card overflow-hidden shrink-0"
      style={{ width, minHeight: height, background: "white" }}
    >
      {src ? (
        <img src={src} alt={`Page ${page}`} width={width} style={{ display: "block", width: "100%", height: "auto" }} draggable={false} />
      ) : failed ? (
        <div className="flex items-center justify-center text-sm muted" style={{ height }}>
          —
        </div>
      ) : (
        <div className="flex items-center justify-center" style={{ height }}>
          <Spinner size={22} />
        </div>
      )}
      {highlight ? (
        <div className="px-3 py-1.5 text-[12px] muted border-t" style={{ borderColor: "var(--border)", background: "var(--surface)" }}>
          {highlight}
        </div>
      ) : null}
    </div>
  );
}

export function Reader({ initialFiles, dragging }: { initialFiles?: string[]; dragging: boolean }) {
  const t = useT();
  const session = useTool({ suffix: "_reader", accept: "pdf", loadInfo: true, initialPaths: initialFiles });
  const pushToast = useToasts((s) => s.push);
  const scrollRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const pageRefs = useRef<Map<number, HTMLDivElement>>(new Map());
  // A plain memo value (not a ref) so rendering and effects share the persistent
  // cache map without reading a ref during render.
  const [imageCacheMap] = useState(() => new Map<string, string>());
  const currentPageRef = useRef(1);
  const framePending = useRef(false);
  const pointersRef = useRef(new Map<number, { x: number; y: number }>());
  const panRef = useRef<{ pointerId: number; startX: number; startY: number; scrollLeft: number } | null>(null);
  const pinchRef = useRef<ReaderPinch | null>(null);
  const pinchFocalRef = useRef<ReaderFocal | null>(null);
  const lastTapRef = useRef<{ x: number; y: number; time: number } | null>(null);
  const [containerWidth, setContainerWidth] = useState(900);
  const [zoom, setZoom] = useState<number | "fit">("fit");
  const [currentPage, setCurrentPage] = useState(1);
  const [query, setQuery] = useState("");
  const [searchResult, setSearchResult] = useState<SearchResponse | null>(null);
  const [searchJobId, setSearchJobId] = useState<string | null>(null);
  const [copiedPage, setCopiedPage] = useState<number | null>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);
  const searchProgress = useJobProgress(searchJobId ?? "");

  const debouncedZoom = useDebounced(zoom, 180);
  const geometries = session.info?.pageGeometries ?? [];
  const pageCount = geometries.length;

  const renderWidth = useMemo(() => {
    const available = Math.max(320, containerWidth - 72);
    if (debouncedZoom === "fit") return Math.round(available);
    const base = (geometries[0]?.display_width_pt ?? 595) * PT_TO_CSS;
    return Math.round(clamp(base * debouncedZoom, 200, 3200));
    // `geometries` is recreated each render from session.info; keying on the
    // first page width is what actually affects the fit-width math.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [containerWidth, debouncedZoom, geometries[0]?.display_width_pt]);

  // Track the reading area width for "fit width" and current page on scroll.
  useEffect(() => {
    const element = scrollRef.current;
    if (!element) return;
    const observer = new ResizeObserver((entries) => {
      setContainerWidth(entries[0]?.contentRect.width ?? 900);
    });
    observer.observe(element);
    // The initial measurement is a read of layout, not a state cascade.
     
    setContainerWidth(element.clientWidth || 900);
    return () => observer.disconnect();
  }, [session.primary]);

  const handleScroll = useCallback(() => {
    if (framePending.current) return;
    framePending.current = true;
    requestAnimationFrame(() => {
      framePending.current = false;
      const container = scrollRef.current;
      if (!container) return;
      const top = container.getBoundingClientRect().top + 12;
      let best = currentPageRef.current;
      let bestDistance = Number.POSITIVE_INFINITY;
      pageRefs.current.forEach((element, page) => {
        const distance = Math.abs(element.getBoundingClientRect().top - top);
        if (distance < bestDistance) {
          bestDistance = distance;
          best = page;
        }
      });
      if (best !== currentPageRef.current) {
        currentPageRef.current = best;
        setCurrentPage(best);
      }
    });
  }, []);

  // New document: reset the view state and caches. The reset is derived from
  // the document identity so no stale search/query paints for the new file.
  const docKey = session.primary?.path ?? "";
  const [lastDocKey, setLastDocKey] = useState(docKey);
  if (lastDocKey !== docKey) {
    setLastDocKey(docKey);
    setSearchResult(null);
    setQuery("");
    setCurrentPage(1);
    setZoom("fit");
  }

  // Scroll the reading area back to the top once per document (effects may
  // touch refs; render may not).
  useEffect(() => {
    const container = scrollRef.current;
    if (container) container.scrollTop = 0;
    imageCacheMap.clear();
    currentPageRef.current = 1;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [docKey]);

  const register = useCallback((page: number, element: HTMLDivElement | null) => {
    if (element) pageRefs.current.set(page, element);
    else pageRefs.current.delete(page);
  }, []);

  const scrollToPage = useCallback((page: number) => {
    const element = pageRefs.current.get(page);
    if (element) {
      element.scrollIntoView({ behavior: "smooth", block: "start" });
      currentPageRef.current = page;
      setCurrentPage(page);
    }
  }, []);

  const applyZoom = (next: number | "fit") => setZoom(next);
  const zoomBy = (factor: number) => {
    const current = typeof zoom === "number" ? zoom : 1;
    applyZoom(clampReaderZoom(current * factor));
  };

  // -------------------------------------------------------------------------
  // Touch pan / pinch / double-tap (the mouse keeps the stock experience)
  //
  // The scroller keeps `touch-action: pan-y`, so vertical swipes stay native
  // while horizontal drags are ours: the page is wider than the viewport only
  // when zoomed, and overflow-x stays hidden so desktop never sees a bar.
  // -------------------------------------------------------------------------

  const trackPinchFocal = (clientX: number, clientY: number) => {
    const content = contentRef.current;
    if (!content) return;
    const bounds = content.getBoundingClientRect();
    pinchFocalRef.current = {
      contentX: clientX - bounds.left,
      contentY: clientY - bounds.top,
      screenX: clientX,
      screenY: clientY,
      width: renderWidth,
    };
  };

  const handleReaderPointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    if (event.pointerType === "mouse") return;
    const container = scrollRef.current;
    if (!container) return;
    pointersRef.current.set(event.pointerId, { x: event.clientX, y: event.clientY });
    try {
      event.currentTarget.setPointerCapture?.(event.pointerId);
    } catch {
      // Pointer capture is unavailable (older webviews, jsdom).
    }
    if (pointersRef.current.size >= 2) {
      panRef.current = null;
      const [a, b] = [...pointersRef.current.values()];
      pinchRef.current = { distance: Math.max(1, Math.hypot(a.x - b.x, a.y - b.y)), zoom: typeof zoom === "number" ? zoom : 1 };
      trackPinchFocal((a.x + b.x) / 2, (a.y + b.y) / 2);
      return;
    }
    panRef.current = { pointerId: event.pointerId, startX: event.clientX, startY: event.clientY, scrollLeft: container.scrollLeft };
  };

  const handleReaderPointerMove = (event: React.PointerEvent<HTMLDivElement>) => {
    const points = pointersRef.current;
    if (!points.has(event.pointerId)) return;
    points.set(event.pointerId, { x: event.clientX, y: event.clientY });
    const pinch = pinchRef.current;
    if (pinch && points.size >= 2) {
      const [a, b] = [...points.values()];
      const midX = (a.x + b.x) / 2;
      const midY = (a.y + b.y) / 2;
      trackPinchFocal(midX, midY);
      setZoom(pinchZoomValue(pinch.zoom, pinch.distance, Math.hypot(a.x - b.x, a.y - b.y)));
      return;
    }
    const pan = panRef.current;
    const container = scrollRef.current;
    if (!pan || !container || pan.pointerId !== event.pointerId) return;
    // Horizontal drag pans; vertical stays with the browser's native scroll.
    container.scrollLeft = Math.max(0, pan.scrollLeft - (event.clientX - pan.startX));
  };

  const handleReaderPointerUp = (event: React.PointerEvent<HTMLDivElement>) => {
    const points = pointersRef.current;
    if (!points.has(event.pointerId)) return;
    points.delete(event.pointerId);
    try {
      event.currentTarget.releasePointerCapture?.(event.pointerId);
    } catch {
      // Not captured.
    }
    if (pinchRef.current) {
      if (points.size < 2) pinchRef.current = null;
      return;
    }
    const pan = panRef.current;
    if (!pan || pan.pointerId !== event.pointerId) return;
    panRef.current = null;
    if (Math.abs(event.clientX - pan.startX) + Math.abs(event.clientY - pan.startY) > 8) return;
    const now = Date.now();
    const previous = lastTapRef.current;
    if (previous && now - previous.time < 320 && Math.abs(previous.x - event.clientX) < 40 && Math.abs(previous.y - event.clientY) < 40) {
      lastTapRef.current = null;
      setZoom((current) => doubleTapZoom(current));
    } else {
      lastTapRef.current = { x: event.clientX, y: event.clientY, time: now };
    }
  };

  // Once the debounced page width has settled, put the content point that was
  // under the pinch midpoint back under the finger; measuring the real rect
  // keeps the centring of narrow pages out of the equation.
  useLayoutEffect(() => {
    const focal = pinchFocalRef.current;
    if (!focal) return;
    pinchFocalRef.current = null;
    const container = scrollRef.current;
    const content = contentRef.current;
    if (!container || !content) return;
    const bounds = content.getBoundingClientRect();
    const factor = renderWidth / Math.max(1, focal.width);
    container.scrollLeft = Math.max(0, container.scrollLeft + bounds.left + focal.contentX * factor - focal.screenX);
    container.scrollTop += bounds.top + focal.contentY * factor - focal.screenY;
  }, [renderWidth]);

  const runSearch = useCallback(async () => {
    const trimmed = query.trim();
    if (!session.primary || !trimmed) {
      setSearchResult(null);
      return;
    }
    const jobId = uid("search");
    setSearchJobId(jobId);
    try {
      const result = await searchDocument(session.primary.path, trimmed, false, 300, session.password || undefined, jobId);
      setSearchResult(result);
      if (result.matches.length) scrollToPage(result.matches[0].page);
    } catch (error) {
      const appError = toAppError(error);
      if (appError.code !== "cancelled") reportError(error, t);
      setSearchResult(null);
    } finally {
      setSearchJobId(null);
    }
  }, [query, scrollToPage, session.password, session.primary, t]);

  const copyPage = async () => {
    if (!session.primary) return;
    try {
      const text = await pageText(session.primary.path, currentPage, session.password || undefined);
      if (!text.trim()) {
        pushToast({ kind: "info", title: t("reader.noTextLayerShort") });
        return;
      }
      const ok = await copyText(text.trim());
      if (ok) {
        setCopiedPage(currentPage);
        pushToast({ kind: "success", title: t("reader.copied", { page: currentPage }) });
        setTimeout(() => setCopiedPage(null), 1800);
      }
    } catch (error) {
      reportError(error, t);
    }
  };

  // Reading keyboard shortcuts.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement;
      const typing = ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName);
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "f") {
        event.preventDefault();
        searchInputRef.current?.focus();
        searchInputRef.current?.select();
        return;
      }
      if ((event.ctrlKey || event.metaKey) && (event.key === "+" || event.key === "=")) {
        event.preventDefault();
        zoomBy(1.25);
        return;
      }
      if ((event.ctrlKey || event.metaKey) && event.key === "-") {
        event.preventDefault();
        zoomBy(0.8);
        return;
      }
      if ((event.ctrlKey || event.metaKey) && event.key === "0") {
        event.preventDefault();
        applyZoom("fit");
        return;
      }
      if (typing) return;
      if (event.key === "PageDown") {
        event.preventDefault();
        scrollRef.current?.scrollBy({ top: scrollRef.current.clientHeight * 0.9, behavior: "smooth" });
      } else if (event.key === "PageUp") {
        event.preventDefault();
        scrollRef.current?.scrollBy({ top: -scrollRef.current.clientHeight * 0.9, behavior: "smooth" });
      } else if (event.key === "Home") {
        event.preventDefault();
        scrollToPage(1);
      } else if (event.key === "End") {
        event.preventDefault();
        scrollToPage(pageCount || 1);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pageCount, scrollToPage, zoom]);

  const matchesByPage = useMemo(() => {
    const map = new Map<number, TextMatch[]>();
    for (const match of searchResult?.matches ?? []) {
      const list = map.get(match.page) ?? [];
      list.push(match);
      map.set(match.page, list);
    }
    return map;
  }, [searchResult]);

  if (!session.primary) {
    return (
      <Screen title={t("reader.title")} subtitle={t("reader.subtitle")}>
        <DropZone onPaths={(paths) => void session.addPaths(paths)} dragging={dragging} accept="pdf" />
      </Screen>
    );
  }

  const zoomLabel = typeof zoom === "number" ? `${Math.round(zoom * 100)}%` : t("reader.fitWidth");

  return (
    <div className="h-full flex min-h-0">
      <div className="flex-1 min-w-0 flex flex-col min-h-0">
        <div
          className="flex items-center gap-2 px-4 py-2.5 border-b shrink-0 flex-wrap"
          style={{ borderColor: "var(--border)", background: "var(--bg-soft)" }}
        >
          <BookOpen size={16} style={{ color: "var(--accent)" }} />
          <span className="font-semibold text-[13.5px] truncate max-w-[220px]" title={session.primary.path}>
            {fileBaseName(session.primary.path)}
          </span>
          <Badge tone="accent">
            {currentPage} / {pageCount}
          </Badge>

          <div className="flex items-center gap-1 ml-2">
            <IconButton label={t("common.zoomOut")} onClick={() => zoomBy(0.8)}>
              <ZoomOut size={15} />
            </IconButton>
            <button className="btn btn-sm" onClick={() => applyZoom("fit")} style={{ minWidth: 76 }}>
              {zoomLabel}
            </button>
            <IconButton label={t("common.zoomIn")} onClick={() => zoomBy(1.25)}>
              <ZoomIn size={15} />
            </IconButton>
          </div>

          <div className="flex items-center gap-1">
            <IconButton label={t("common.page")} onClick={() => scrollToPage(Math.max(1, currentPage - 1))} disabled={currentPage <= 1}>
              <ChevronLeft size={15} />
            </IconButton>
            <IconButton
              label={t("common.page")}
              onClick={() => scrollToPage(Math.min(pageCount, currentPage + 1))}
              disabled={currentPage >= pageCount}
            >
              <ChevronRight size={15} />
            </IconButton>
          </div>

          <div className="relative flex-1 min-w-[180px] max-w-[380px]">
            <Search size={14} className="absolute left-2.5 top-1/2 -translate-y-1/2 muted" />
            <input
              ref={searchInputRef}
              className="input input-sm pl-8 pr-8"
              placeholder={t("reader.searchPlaceholder")}
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") void runSearch();
                if (event.key === "Escape") {
                  setQuery("");
                  setSearchResult(null);
                }
              }}
            />
            {query ? (
              <button
                className="icon-btn absolute right-0 top-1/2 -translate-y-1/2"
                onClick={() => {
                  setQuery("");
                  setSearchResult(null);
                }}
                aria-label={t("common.clear")}
              >
                <X size={13} />
              </button>
            ) : null}
          </div>
          <Button size="sm" onClick={() => void runSearch()} disabled={!query.trim() || searchJobId !== null}>
            {t("reader.search")}
          </Button>
          <div className="flex items-center gap-1 ml-auto">
            <IconButton label={t("reader.copyText")} onClick={() => void copyPage()}>
              <Copy size={15} />
            </IconButton>
            <IconButton label={t("reader.openExternal")} onClick={() => void openAnyFile(session.primary!.path).catch(() => undefined)}>
              <ExternalLink size={15} />
            </IconButton>
          </div>
        </div>

        {searchJobId ? (
          <div className="px-4 py-1.5 text-xs muted shrink-0 flex items-center gap-2" style={{ background: "var(--surface-2)" }}>
            <Spinner size={12} /> {t("reader.searching")}
            {searchProgress?.total ? ` (${searchProgress.current} / ${searchProgress.total})` : ""}
          </div>
        ) : null}

        {searchResult ? (
          <div className="px-4 py-1.5 text-xs shrink-0 flex items-center gap-3" style={{ background: "var(--surface-2)" }}>
            <span className={searchResult.totalMatches ? "" : "muted"}>
              {searchResult.totalMatches
                ? t("reader.matches", { count: searchResult.totalMatches, pages: searchResult.pagesWithMatches })
                : t("reader.noMatches")}
            </span>
            {searchResult.truncated ? <span style={{ color: "var(--warn)" }}>{t("reader.truncated", { count: searchResult.totalMatches })}</span> : null}
          </div>
        ) : null}

        <div
          ref={scrollRef}
          onScroll={handleScroll}
          onPointerDown={handleReaderPointerDown}
          onPointerMove={handleReaderPointerMove}
          onPointerUp={handleReaderPointerUp}
          onPointerCancel={handleReaderPointerUp}
          className="reader-scroll flex-1 min-h-0 overflow-y-auto overflow-x-hidden touch-pan-y"
          style={{ background: "var(--bg)" }}
        >
          {/* `w-max` + `min-w-full` lets the pages overflow sideways (instead
              of centring out of reach) once a zoom makes them wider than the
              reading area, which is what horizontal panning scrolls. */}
          <div ref={contentRef} className="px-6 py-5 flex flex-col gap-5 items-center min-w-full w-max mx-auto">
            {session.info && !session.info.hasTextLayer ? (
              <div className="text-xs muted text-center max-w-[620px]">{t("reader.noTextLayer")}</div>
            ) : null}
            {geometries.map((geometry) => (
              <ReaderPage
                key={`${geometry.page}-${renderWidth}`}
                path={session.primary!.path}
                page={geometry.page}
                geometry={geometry}
                width={renderWidth}
                password={session.password || undefined}
                cache={imageCacheMap}
                register={register}
                highlight={
                  matchesByPage.get(geometry.page)?.[0]?.snippet
                    ? `${matchesByPage.get(geometry.page)!.length}× ${matchesByPage.get(geometry.page)![0].snippet}`
                    : undefined
                }
              />
            ))}
            <div className="text-center text-xs muted py-3">{t("reader.shortcutHint")}</div>
          </div>
        </div>
      </div>

      <aside
        className="w-[300px] shrink-0 border-l overflow-y-auto hidden xl:flex flex-col gap-3 p-3"
        style={{ borderColor: "var(--border)", background: "var(--bg-soft)" }}
      >
        <p className="text-[12px] font-bold uppercase tracking-wider muted">{t("reader.jumpTo")}</p>
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

        <p className="text-[12px] font-bold uppercase tracking-wider muted mt-2">{t("reader.search")}</p>
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
          {session.info ? (
            <div className="card-soft p-3">
              <InfoStrip info={session.info} error={session.infoError} />
            </div>
          ) : null}
          {copiedPage ? (
            <p className="text-xs mt-2" style={{ color: "var(--ok)" }}>
              {t("reader.copied", { page: copiedPage })}
            </p>
          ) : null}
        </div>
      </aside>
    </div>
  );
}
