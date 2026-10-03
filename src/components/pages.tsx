import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ChevronLeft, ChevronRight, RotateCw } from "lucide-react";
import { pageThumbnail, pagePreview } from "../lib/api";
import { IconButton, Spinner } from "./ui";
import { useT } from "../lib/i18n";
import { clamp, previewRasterWidth } from "../lib/format";

// ---------------------------------------------------------------------------
// Lazy thumbnail grid with selection and pointer-based reordering
// ---------------------------------------------------------------------------

export interface PageItem {
  id: string;
  sourcePage: number;
  rotationDelta: number;
}

function Thumb({
  path,
  page,
  size,
  password,
  refreshedAt,
}: {
  path: string;
  page: number;
  size: number;
  password?: string;
  refreshedAt: number;
}) {
  const [src, setSrc] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const requested = useRef(false);
  // A refresh (or a new path/page) is a different thumbnail: the reset is
  // derived from the identity below instead of a setState effect.
  const identity = `${path}:${page}:${refreshedAt}`;
  const [lastIdentity, setLastIdentity] = useState(identity);
  if (lastIdentity !== identity) {
    // Render-phase reset of derived state: React re-runs the render with the
    // fresh values before committing (no extra DOM pass, no cascade).
    setLastIdentity(identity);
    setSrc(null);
    setFailed(false);
  }

  useEffect(() => {
    const element = containerRef.current;
    if (!element || requested.current) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting) && !requested.current) {
          requested.current = true;
          void pageThumbnail(path, page, size, password)
            .then((thumb) => setSrc(thumb.dataUrl))
            .catch(() => setFailed(true));
        }
      },
      { rootMargin: "320px" },
    );
    observer.observe(element);
    return () => {
      observer.disconnect();
      // A new identity means a new thumbnail: clear the once-only guard here
      // (effects may touch refs, render may not).
      requested.current = false;
    };
  }, [path, page, size, password, identity]);

  return (
    <div
      ref={containerRef}
      className="w-full h-full flex items-center justify-center overflow-hidden"
      style={{ background: "var(--surface-2)" }}
    >
      {src ? (
        <img src={src} alt={`Page ${page}`} className="w-full h-full object-contain" draggable={false} />
      ) : failed ? (
        <span className="text-xs muted">—</span>
      ) : (
        <Spinner size={18} />
      )}
    </div>
  );
}

export function ThumbGrid({
  path,
  pages,
  selected,
  onSelectionChange,
  onReorder,
  password,
  thumbWidth = 190,
  refreshedAt = 0,
  overlayFor,
  onPageClick,
  onPageDoubleClick,
}: {
  path: string;
  pages: PageItem[];
  selected: Set<number>;
  onSelectionChange: (next: Set<number>) => void;
  onReorder?: (from: number, to: number) => void;
  password?: string;
  thumbWidth?: number;
  refreshedAt?: number;
  overlayFor?: (index: number) => React.ReactNode;
  onPageClick?: (index: number, event: React.MouseEvent) => void;
  onPageDoubleClick?: (index: number) => void;
}) {
  const t = useT();
  // State (not a ref) so the render can honestly reflect the drag highlight.
  const [dragFromIndex, setDragFromIndex] = useState<number | null>(null);
  const dragFrom = useRef<number | null>(null);
  const [overIndex, setOverIndex] = useState<number | null>(null);
  const [shiftAnchor, setShiftAnchor] = useState<number | null>(null);

  const handleClick = useCallback(
    (index: number, event: React.MouseEvent) => {
      const next = new Set(selected);
      if (event.shiftKey && shiftAnchor !== null) {
        const [a, b] = [Math.min(shiftAnchor, index), Math.max(shiftAnchor, index)];
        for (let i = a; i <= b; i += 1) next.add(i);
      } else if (event.ctrlKey || event.metaKey) {
        if (next.has(index)) next.delete(index);
        else next.add(index);
        setShiftAnchor(index);
      } else {
        if (next.size === 1 && next.has(index)) next.clear();
        else {
          next.clear();
          next.add(index);
        }
        setShiftAnchor(index);
      }
      onSelectionChange(next);
      onPageClick?.(index, event);
    },
    [onPageClick, onSelectionChange, selected, shiftAnchor],
  );

  const handlePointerDown = (index: number) => (event: React.PointerEvent) => {
    if (!onReorder) return;
    if (event.button !== 0) return;
    if ((event.target as HTMLElement).closest("button")) return;
    dragFrom.current = index;
    setDragFromIndex(index);
    setOverIndex(index);
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  };

  const handlePointerMove = (event: React.PointerEvent) => {
    if (dragFrom.current === null) return;
    const elements = document.elementsFromPoint(event.clientX, event.clientY);
    const target = elements.find(
      (element) => element instanceof HTMLElement && element.dataset.pageIndex !== undefined,
    ) as HTMLElement | undefined;
    if (target) setOverIndex(Number(target.dataset.pageIndex));
  };

  const handlePointerUp = () => {
    if (dragFrom.current !== null && overIndex !== null && dragFrom.current !== overIndex) {
      onReorder?.(dragFrom.current, overIndex);
    }
    dragFrom.current = null;
    setDragFromIndex(null);
    setOverIndex(null);
  };

  const columns = useMemo(
    () => `repeat(auto-fill, minmax(${thumbWidth}px, 1fr))`,
    [thumbWidth],
  );

  return (
    <div
      className="grid gap-3"
      style={{ gridTemplateColumns: columns, touchAction: "none" }}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
      onPointerCancel={handlePointerUp}
    >
      {pages.map((page, index) => (
        <div key={page.id} className="flex flex-col gap-1.5">
          <div
            data-page-index={index}
            className="thumb"
            data-selected={selected.has(index)}
            data-dragging={dragFromIndex === index}
            data-droptarget={overIndex === index && dragFromIndex !== null && dragFromIndex !== index}
            style={{ aspectRatio: "1 / 1.3" }}
            onPointerDown={handlePointerDown(index)}
            onClick={(event) => handleClick(index, event)}
            onDoubleClick={() => onPageDoubleClick?.(index)}
            role="button"
            tabIndex={0}
            aria-label={`${t("common.page")} ${index + 1}`}
            onKeyDown={(event) => {
              if (event.key === " " || event.key === "Enter") {
                event.preventDefault();
                handleClick(index, event as unknown as React.MouseEvent);
              }
            }}
            title={page.rotationDelta ? `${t("common.rotation")}: ${page.rotationDelta}°` : undefined}
          >
            <Thumb path={path} page={page.sourcePage} size={thumbWidth * 2} password={password} refreshedAt={refreshedAt} />
            {overlayFor ? <div className="absolute inset-0 pointer-events-none">{overlayFor(index)}</div> : null}
            <div className="absolute top-1.5 left-1.5 flex items-center gap-1">
              <span
                className="text-[11px] font-semibold px-1.5 py-0.5 rounded-md"
                style={{ background: "rgb(0 0 0 / 0.55)", color: "white" }}
              >
                {index + 1}
              </span>
              {page.sourcePage !== index + 1 ? (
                <span
                  className="text-[10px] px-1.5 py-0.5 rounded-md"
                  style={{ background: "rgb(0 0 0 / 0.4)", color: "white" }}
                  title={`Source page ${page.sourcePage}`}
                >
                  ←{page.sourcePage}
                </span>
              ) : null}
              {page.rotationDelta ? (
                <span
                  className="text-[10px] px-1.5 py-0.5 rounded-md flex items-center gap-0.5"
                  style={{ background: "rgb(0 0 0 / 0.45)", color: "white" }}
                >
                  <RotateCw size={9} />
                  {page.rotationDelta}°
                </span>
              ) : null}
            </div>
          </div>
        </div>
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Single page preview with optional overlay interactions (crop / annotate)
// ---------------------------------------------------------------------------

/** Normalized rectangle (0..1) relative to the rendered page image. */
export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export function PageCanvas({
  path,
  page,
  password,
  maxWidth = 1100,
  overlay: overlayContent,
  onDragRect,
  onPointClick,
  dragging: externalDragging = false,
  cropRect,
  style,
}: {
  path: string;
  page: number;
  password?: string;
  maxWidth?: number;
  overlay?: React.ReactNode;
  onDragRect?: (rect: Rect) => void;
  onPointClick?: (x: number, y: number) => void;
  dragging?: boolean;
  cropRect?: Rect | null;
  style?: React.CSSProperties;
}) {
  const [image, setImage] = useState<{ dataUrl: string; width: number; height: number } | null>(null);
  const [error, setError] = useState(false);
  const [rect, setRect] = useState<Rect | null>(null);
  const [dragStart, setDragStart] = useState<{ x: number; y: number } | null>(null);
  const [point, setPoint] = useState<{ x: number; y: number } | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  // The raster is requested in physical pixels for the CSS width so text stays
  // sharp on high-density screens; the page still lays out at `maxWidth`.
  const rasterWidth = previewRasterWidth(maxWidth);
  // A different page/width is a different preview: reset in render instead of
  // an effect, so the old image never paints against the new geometry.
  const identity = `${path}:${page}:${rasterWidth}:${password ?? ""}`;
  const [lastIdentity, setLastIdentity] = useState(identity);
  if (lastIdentity !== identity) {
    setLastIdentity(identity);
    setImage(null);
    setError(false);
    setRect(null);
  }

  useEffect(() => {
    void pagePreview(path, page, rasterWidth, password)
      .then((preview) => setImage({ dataUrl: preview.dataUrl, width: preview.width, height: preview.height }))
      .catch(() => setError(true));
  }, [path, page, rasterWidth, password]);

  const toNatural = useCallback(
    (event: { clientX: number; clientY: number }) => {
      const element = containerRef.current?.querySelector("img");
      if (!element || !image) return null;
      const bounds = element.getBoundingClientRect();
      const scaleX = image.width / bounds.width;
      const scaleY = image.height / bounds.height;
      return {
        x: clamp((event.clientX - bounds.left) * scaleX, 0, image.width),
        y: clamp((event.clientY - bounds.top) * scaleY, 0, image.height),
      };
    },
    [image],
  );

  const handlePointerDown = (event: React.PointerEvent) => {
    if (externalDragging) {
      const position = toNatural(event);
      if (position && image) {
        setPoint(position);
        onPointClick?.(position.x / image.width, position.y / image.height);
      }
      return;
    }
    const position = toNatural(event);
    if (!position) return;
    setDragStart(position);
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  };

  const handlePointerMove = (event: React.PointerEvent) => {
    if (externalDragging) {
      const position = toNatural(event);
      if (position) setPoint(position);
      return;
    }
    if (!dragStart) return;
    const position = toNatural(event);
    if (!position) return;
    setRect({
      x: Math.min(dragStart.x, position.x),
      y: Math.min(dragStart.y, position.y),
      w: Math.abs(position.x - dragStart.x),
      h: Math.abs(position.y - dragStart.y),
    });
  };

  const handlePointerUp = () => {
    if (rect && onDragRect && image) {
      onDragRect({
        x: rect.x / image.width,
        y: rect.y / image.height,
        w: rect.w / image.width,
        h: rect.h / image.height,
      });
    }
    setDragStart(null);
  };

  const activeRect = rect ?? cropRect ?? null;

  return (
    <div
      className="canvas-wrap"
      ref={containerRef}
      style={{ cursor: externalDragging ? "crosshair" : onDragRect ? "crosshair" : "default", ...style }}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
    >
      {image ? (
        <>
          <img src={image.dataUrl} alt={`Page ${page}`} draggable={false} style={{ background: "white" }} />
          {activeRect ? (
            <div
              className="canvas-overlay"
              style={{
                position: "absolute",
                left: `${(activeRect.x / image.width) * 100}%`,
                top: `${(activeRect.y / image.height) * 100}%`,
                width: `${(activeRect.w / image.width) * 100}%`,
                height: `${(activeRect.h / image.height) * 100}%`,
                border: "2px solid var(--accent)",
                background: "color-mix(in srgb, var(--accent) 18%, transparent)",
                pointerEvents: "none",
              }}
            />
          ) : null}
          {externalDragging && point ? (
            <div
              style={{
                position: "absolute",
                left: `${(point.x / image.width) * 100}%`,
                top: `${(point.y / image.height) * 100}%`,
                width: 14,
                height: 14,
                marginLeft: -7,
                marginTop: -7,
                borderRadius: "50%",
                border: "2px solid var(--accent)",
                background: "white",
                pointerEvents: "none",
              }}
            />
          ) : null}
          {overlayContent}
        </>
      ) : error ? (
        <div className="flex items-center justify-center py-20 muted text-sm">Preview unavailable</div>
      ) : (
        <div className="flex items-center justify-center py-20">
          <Spinner size={22} />
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Simple pager
// ---------------------------------------------------------------------------

export function Pager({
  page,
  pageCount,
  onChange,
}: {
  page: number;
  pageCount: number;
  onChange: (page: number) => void;
}) {
  return (
    <div className="flex items-center gap-1.5">
      <IconButton label="Previous" onClick={() => onChange(Math.max(1, page - 1))} disabled={page <= 1}>
        <ChevronLeft size={15} />
      </IconButton>
      <span className="text-xs tabular-nums muted min-w-[70px] text-center">
        {page} / {pageCount}
      </span>
      <IconButton label="Next" onClick={() => onChange(Math.min(pageCount, page + 1))} disabled={page >= pageCount}>
        <ChevronRight size={15} />
      </IconButton>
    </div>
  );
}
