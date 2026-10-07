import { clamp, previewRasterWidth } from "./format";

/**
 * Tile maths for the reader's high-zoom overlay.
 *
 * A page preview is one bitmap capped at MAX_PREVIEW_RASTER_WIDTH, so past
 * that width the webview upscales it and text blurs. The reader then keeps the
 * preview as a base layer and lays sharp tiles (`page_tile`) over the part of
 * the page that is on screen. Everything here is pure so jsdom can test it.
 */

/** CSS pixels per PDF point at 100 % zoom. */
const PT_TO_CSS = 96 / 72;

/** Highest tile scale (output pixels per PDF point) the backend renders: the
 *  reader's 400 % zoom at the 2.5 device pixel ratio cap. Mirrors
 *  `MAX_TILE_SCALE` in src-tauri/src/commands.rs. */
export const MAX_TILE_SCALE = 4 * PT_TO_CSS * 2.5;

/** Largest tile edge the backend renders, in output pixels. */
export const MAX_TILE_EDGE = 1024;

/** Tiles cover about this many CSS pixels per side (capped by MAX_TILE_EDGE). */
export const TILE_CSS_SIZE = 512;

/** Screen pixels around the viewport whose tiles are fetched ahead of a scroll. */
export const TILE_VIEW_MARGIN = 256;

/** Tile scales snap up to steps of 2^(1/8) (about 9 %), so a small zoom change
 *  or a fit-width resize reuses cached tiles; the browser only ever scales a
 *  tile down, which keeps it sharp. */
export const TILE_SCALE_STEPS_PER_OCTAVE = 8;

/** Tiles kept per document (data URLs), least recently used evicted first. */
export const MAX_TILE_CACHE_ENTRIES = 64;

/** Tile renders sent to the backend at once; the rest wait in the queue, where
 *  a scroll or zoom can still cancel them. */
export const MAX_TILE_REQUESTS = 3;

export interface TileGrid {
  /** Output pixels per PDF point the tiles are rendered at. */
  scale: number;
  /** The whole page at `scale`, in output pixels. */
  width: number;
  height: number;
  /** Tile edge in output pixels (the last row and column may be smaller). */
  tileSize: number;
  columns: number;
  rows: number;
}

export interface TileSpec {
  column: number;
  row: number;
  /** Region of the page raster, in output pixels. */
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface TileRect {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

/** The bucketed tile scale for a wanted scale (output px per point): rounded
 *  up to the next step so tiles never undershoot the display density. */
export function tileScale(wanted: number): number {
  if (!Number.isFinite(wanted) || wanted <= 0) return MAX_TILE_SCALE;
  // The epsilon keeps an exact step from rounding up to the next one.
  const step = Math.ceil(Math.log2(wanted) * TILE_SCALE_STEPS_PER_OCTAVE - 1e-9);
  return Math.min(2 ** (step / TILE_SCALE_STEPS_PER_OCTAVE), MAX_TILE_SCALE);
}

/**
 * The tile grid for a page laid out `cssWidth` CSS pixels wide, or null when
 * the full-page preview already covers the display (normal zoom: no tiles).
 */
export function tileGrid(widthPt: number, heightPt: number, cssWidth: number, ratio: number): TileGrid | null {
  if (!(widthPt > 0) || !(heightPt > 0) || !(cssWidth > 0) || !(ratio > 0)) return null;
  const wanted = Math.round(cssWidth * ratio);
  const base = previewRasterWidth(cssWidth, ratio);
  if (wanted <= base) return null;
  const scale = tileScale(wanted / widthPt);
  const width = Math.max(1, Math.round(widthPt * scale));
  // Capped at MAX_TILE_SCALE a tile may not beat the preview (a narrow page
  // laid out at the first page's width): the base layer alone is then best.
  if (width <= base) return null;
  const height = Math.max(1, Math.round(heightPt * scale));
  // The tile size depends only on the pixel ratio, never on the layout width,
  // so a cached tile key always names the same region.
  const tileSize = Math.round(clamp(TILE_CSS_SIZE * ratio, 256, MAX_TILE_EDGE));
  return { scale, width, height, tileSize, columns: Math.ceil(width / tileSize), rows: Math.ceil(height / tileSize) };
}

/**
 * The part of a page's tile layer inside the viewport (grown by `margin`), in
 * the layer's own layout pixels, or null when it is off screen. `layer` and
 * `viewport` are client rects; `layoutWidth` is the layer's untransformed
 * width, so a pinch transform in progress does not skew the result.
 */
export function visibleLayerRect(
  layer: TileRect,
  viewport: TileRect,
  layoutWidth: number,
  margin: number = TILE_VIEW_MARGIN,
): TileRect | null {
  const shownWidth = layer.right - layer.left;
  if (!(shownWidth > 0) || !(layoutWidth > 0)) return null;
  const left = Math.max(layer.left, viewport.left - margin);
  const right = Math.min(layer.right, viewport.right + margin);
  const top = Math.max(layer.top, viewport.top - margin);
  const bottom = Math.min(layer.bottom, viewport.bottom + margin);
  if (right <= left || bottom <= top) return null;
  const factor = layoutWidth / shownWidth;
  return {
    left: (left - layer.left) * factor,
    top: (top - layer.top) * factor,
    right: (right - layer.left) * factor,
    bottom: (bottom - layer.top) * factor,
  };
}

/** The tile at a grid position, clipped to the page. */
export function tileAt(grid: TileGrid, column: number, row: number): TileSpec {
  const x = column * grid.tileSize;
  const y = row * grid.tileSize;
  return {
    column,
    row,
    x,
    y,
    width: Math.min(grid.tileSize, grid.width - x),
    height: Math.min(grid.tileSize, grid.height - y),
  };
}

/**
 * Tiles intersecting `view` (layer layout pixels, see visibleLayerRect) for a
 * layer `layoutWidth` CSS pixels wide, nearest to the view's centre first so
 * the middle of the screen sharpens before its edges.
 */
export function tilesInView(grid: TileGrid, view: TileRect, layoutWidth: number): TileSpec[] {
  if (!(layoutWidth > 0)) return [];
  const pxPerCss = grid.width / layoutWidth;
  const x0 = clamp(view.left * pxPerCss, 0, grid.width);
  const x1 = clamp(view.right * pxPerCss, 0, grid.width);
  const y0 = clamp(view.top * pxPerCss, 0, grid.height);
  const y1 = clamp(view.bottom * pxPerCss, 0, grid.height);
  if (x1 <= x0 || y1 <= y0) return [];
  const firstColumn = Math.floor(x0 / grid.tileSize);
  const lastColumn = Math.min(grid.columns - 1, Math.ceil(x1 / grid.tileSize) - 1);
  const firstRow = Math.floor(y0 / grid.tileSize);
  const lastRow = Math.min(grid.rows - 1, Math.ceil(y1 / grid.tileSize) - 1);
  const tiles: TileSpec[] = [];
  for (let row = firstRow; row <= lastRow; row += 1) {
    for (let column = firstColumn; column <= lastColumn; column += 1) tiles.push(tileAt(grid, column, row));
  }
  const centreX = (x0 + x1) / 2;
  const centreY = (y0 + y1) / 2;
  const distance = (tile: TileSpec) =>
    Math.hypot(tile.x + tile.width / 2 - centreX, tile.y + tile.height / 2 - centreY);
  return tiles.sort((a, b) => distance(a) - distance(b));
}

/** True when both lists name the same tiles (order aside). */
export function sameTiles(a: readonly TileSpec[], b: readonly TileSpec[]): boolean {
  if (a.length !== b.length) return false;
  const names = new Set(a.map((tile) => `${tile.column}:${tile.row}`));
  return b.every((tile) => names.has(`${tile.column}:${tile.row}`));
}

/** Cache key of a tile: page, zoom bucket, tile size and grid position. */
export function tileKey(page: number, grid: TileGrid, tile: TileSpec): string {
  return `${page}:${grid.scale.toFixed(4)}:${grid.tileSize}:${tile.column}:${tile.row}`;
}

/** Stores a tile and evicts the least recently used ones beyond the cap. */
export function rememberTile<T>(
  cache: Map<string, T>,
  key: string,
  entry: T,
  limit: number = MAX_TILE_CACHE_ENTRIES,
): void {
  // Re-insert so Map iteration order tracks recency.
  cache.delete(key);
  cache.set(key, entry);
  while (cache.size > limit) {
    const oldest = cache.keys().next().value;
    if (oldest === undefined) break;
    cache.delete(oldest);
  }
}

export interface TileRequest<T> {
  /** Settles with the tile; never settles once the request was cancelled
   *  before the render started. */
  promise: Promise<T>;
  cancel: () => void;
}

interface TileJob<T> {
  key: string;
  load: () => Promise<T>;
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (reason: unknown) => void;
  subscribers: number;
  started: boolean;
}

/**
 * Bounded queue for tile renders. At most `limit` renders run at once; the
 * same key is never rendered twice concurrently, and a request cancelled
 * before its render started is dropped without reaching the backend. A render
 * already running cannot be stopped (IPC has no abort), so its subscribers
 * simply ignore the result.
 */
export class TileRequestQueue<T> {
  private readonly jobs = new Map<string, TileJob<T>>();
  private readonly waiting: TileJob<T>[] = [];
  private running = 0;

  constructor(private readonly limit: number = MAX_TILE_REQUESTS) {}

  /** Renders waiting for a free slot. */
  get pending(): number {
    return this.waiting.length;
  }

  /** Renders currently running. */
  get inFlight(): number {
    return this.running;
  }

  request(key: string, load: () => Promise<T>): TileRequest<T> {
    let job = this.jobs.get(key);
    if (!job) {
      let resolve!: (value: T) => void;
      let reject!: (reason: unknown) => void;
      const promise = new Promise<T>((onResolve, onReject) => {
        resolve = onResolve;
        reject = onReject;
      });
      job = { key, load, promise, resolve, reject, subscribers: 0, started: false };
      this.jobs.set(key, job);
      this.waiting.push(job);
    }
    const current = job;
    current.subscribers += 1;
    let active = true;
    this.pump();
    return {
      promise: current.promise,
      cancel: () => {
        if (!active) return;
        active = false;
        current.subscribers -= 1;
        if (current.subscribers > 0 || current.started) return;
        const index = this.waiting.indexOf(current);
        if (index >= 0) this.waiting.splice(index, 1);
        this.jobs.delete(current.key);
      },
    };
  }

  private pump(): void {
    while (this.running < this.limit && this.waiting.length) {
      const job = this.waiting.shift()!;
      job.started = true;
      this.running += 1;
      void Promise.resolve()
        .then(job.load)
        .then(job.resolve, job.reject)
        .finally(() => {
          this.running -= 1;
          if (this.jobs.get(job.key) === job) this.jobs.delete(job.key);
          this.pump();
        });
    }
  }
}
