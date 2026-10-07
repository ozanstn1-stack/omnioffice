import { describe, expect, it, vi } from "vitest";
import { MAX_PREVIEW_RASTER_WIDTH } from "./format";
import {
  MAX_TILE_EDGE,
  MAX_TILE_SCALE,
  TileRequestQueue,
  rememberTile,
  sameTiles,
  tileAt,
  tileGrid,
  tileKey,
  tileScale,
  tilesInView,
  visibleLayerRect,
} from "./tiles";

// A4 in points, the reader's page width at 200 % zoom (CSS px).
const A4 = { width: 595.28, height: 841.89 };
const AT_200 = Math.round(A4.width * (96 / 72) * 2);

describe("tile grid", () => {
  it("is off while the full-page preview covers the display", () => {
    // Fit width on a laptop, and 200 % at 1x: both stay under the raster cap.
    expect(tileGrid(A4.width, A4.height, 900, 2)).toBeNull();
    expect(tileGrid(A4.width, A4.height, AT_200, 1)).toBeNull();
    expect(tileGrid(A4.width, A4.height, MAX_PREVIEW_RASTER_WIDTH / 2, 2)).toBeNull();
    // Degenerate geometry never tiles.
    expect(tileGrid(0, A4.height, 4000, 2)).toBeNull();
    expect(tileGrid(A4.width, A4.height, 0, 2)).toBeNull();
  });

  it("tiles once the wanted raster passes the preview cap", () => {
    const grid = tileGrid(A4.width, A4.height, AT_200, 2)!;
    expect(grid).not.toBeNull();
    // Never below the display density, at most one bucket step above it.
    const wanted = (AT_200 * 2) / A4.width;
    expect(grid.scale).toBeGreaterThanOrEqual(wanted);
    expect(grid.scale).toBeLessThan(wanted * 2 ** (1 / 8) + 1e-9);
    expect(grid.width).toBe(Math.round(A4.width * grid.scale));
    expect(grid.height).toBe(Math.round(A4.height * grid.scale));
    expect(grid.width).toBeGreaterThan(MAX_PREVIEW_RASTER_WIDTH);
    // 512 CSS px at 2x is 1024 output px.
    expect(grid.tileSize).toBe(1024);
    expect(grid.columns).toBe(Math.ceil(grid.width / 1024));
    expect(grid.rows).toBe(Math.ceil(grid.height / 1024));
  });

  it("keeps tiles within the backend bounds at the maximum zoom", () => {
    // 400 % at a 2.5x phone: the most the reader can ask for.
    const grid = tileGrid(A4.width, A4.height, Math.round(A4.width * (96 / 72) * 4), 2.5)!;
    expect(grid.scale).toBeLessThanOrEqual(MAX_TILE_SCALE);
    expect(grid.tileSize).toBeLessThanOrEqual(MAX_TILE_EDGE);
    for (let column = 0; column < grid.columns; column += 1) {
      const tile = tileAt(grid, column, grid.rows - 1);
      expect(tile.width).toBeGreaterThan(0);
      expect(tile.width).toBeLessThanOrEqual(MAX_TILE_EDGE);
      expect(tile.x + tile.width).toBeLessThanOrEqual(grid.width);
      expect(tile.y + tile.height).toBe(grid.height);
    }
  });

  it("does not tile a page the scale cap would render blurrier than the preview", () => {
    // A narrow page laid out at the first page's 400 % width.
    expect(tileGrid(100, 100, 3174, 2.5)).toBeNull();
  });

  it("snaps scales up to shared buckets and caps them", () => {
    expect(tileScale(4)).toBe(4);
    expect(tileScale(4.01)).toBeCloseTo(4 * 2 ** (1 / 8), 10);
    // Nearby zooms land in the same bucket, so their tiles are shared.
    expect(tileScale(5.4)).toBe(tileScale(5.5));
    expect(tileScale(500)).toBe(MAX_TILE_SCALE);
    expect(tileScale(Number.NaN)).toBe(MAX_TILE_SCALE);
  });

  it("keys tiles by page, zoom bucket and position", () => {
    const grid = tileGrid(A4.width, A4.height, AT_200, 2)!;
    const resized = tileGrid(A4.width, A4.height, AT_200 + 12, 2)!;
    const tile = tileAt(grid, 1, 2);
    // A slightly different layout width reuses the same bucket and keys.
    expect(tileKey(3, resized, tileAt(resized, 1, 2))).toBe(tileKey(3, grid, tile));
    expect(tileKey(4, grid, tile)).not.toBe(tileKey(3, grid, tile));
    expect(tileKey(3, grid, tileAt(grid, 2, 1))).not.toBe(tileKey(3, grid, tile));
    const zoomed = tileGrid(A4.width, A4.height, AT_200 * 1.5, 2)!;
    expect(tileKey(3, zoomed, tileAt(zoomed, 1, 2))).not.toBe(tileKey(3, grid, tile));
  });
});

describe("tiles in the viewport", () => {
  const grid = tileGrid(A4.width, A4.height, AT_200, 2)!;
  const layout = AT_200;

  it("measures the visible part of the page in layout pixels", () => {
    const layer = { left: 100, top: -500, right: 100 + layout, bottom: 2000 };
    const viewport = { left: 0, top: 0, right: 1200, bottom: 800 };
    expect(visibleLayerRect(layer, viewport, layout, 0)).toEqual({ left: 0, top: 500, right: 1100, bottom: 1300 });
    // The margin grows the viewport but never past the page.
    expect(visibleLayerRect(layer, viewport, layout, 100)).toEqual({ left: 0, top: 400, right: 1200, bottom: 1400 });
    // Off screen (beyond the margin): nothing.
    expect(visibleLayerRect({ ...layer, top: 1200, bottom: 3000 }, viewport, layout, 100)).toBeNull();
    // A pinch transform shows the layer at twice its layout size.
    const pinched = { left: 0, top: 0, right: layout * 2, bottom: 5000 };
    expect(visibleLayerRect(pinched, viewport, layout, 0)).toEqual({ left: 0, top: 0, right: 600, bottom: 400 });
  });

  it("returns only the tiles that intersect the view, centre first", () => {
    const pxPerCss = grid.width / layout;
    // A view covering exactly tile (1, 1).
    const size = grid.tileSize / pxPerCss;
    const tiles = tilesInView(
      grid,
      { left: size + 1, top: size + 1, right: 2 * size - 1, bottom: 2 * size - 1 },
      layout,
    );
    expect(tiles.map((tile) => [tile.column, tile.row])).toEqual([[1, 1]]);

    const wide = tilesInView(grid, { left: 0, top: 0, right: layout, bottom: size * 2.5 }, layout);
    expect(wide).toHaveLength(grid.columns * 3);
    // The tiles nearest to the centre of the view come first.
    const centre = wide[0];
    expect(centre.row).toBe(1);
    expect(wide.at(-1)!.row).not.toBe(1);
    for (const tile of wide) {
      expect(tile.x).toBe(tile.column * grid.tileSize);
      expect(tile.x + tile.width).toBeLessThanOrEqual(grid.width);
    }
  });

  it("returns nothing for an empty or degenerate view", () => {
    expect(tilesInView(grid, { left: 10, top: 10, right: 10, bottom: 400 }, layout)).toEqual([]);
    expect(tilesInView(grid, { left: 0, top: 0, right: 100, bottom: 100 }, 0)).toEqual([]);
    // A view past the page edge is clipped to it.
    const past = tilesInView(grid, { left: layout - 10, top: 0, right: layout + 500, bottom: 10 }, layout);
    expect(past.map((tile) => tile.column)).toEqual([grid.columns - 1]);
  });

  it("compares tile sets regardless of order", () => {
    const a = [tileAt(grid, 0, 0), tileAt(grid, 1, 0)];
    expect(sameTiles(a, [...a].reverse())).toBe(true);
    expect(sameTiles(a, [tileAt(grid, 0, 0)])).toBe(false);
    expect(sameTiles(a, [tileAt(grid, 0, 0), tileAt(grid, 0, 1)])).toBe(false);
  });
});

describe("tile cache and request queue", () => {
  it("evicts the least recently used tiles beyond the cap", () => {
    const cache = new Map<string, string>();
    for (let index = 0; index < 5; index += 1) rememberTile(cache, `t${index}`, `src${index}`, 3);
    expect([...cache.keys()]).toEqual(["t2", "t3", "t4"]);
    rememberTile(cache, "t2", "again", 3);
    rememberTile(cache, "t5", "src5", 3);
    expect([...cache.keys()]).toEqual(["t4", "t2", "t5"]);
  });

  it("runs at most `limit` renders and starts the next when one finishes", async () => {
    const queue = new TileRequestQueue<string>(2);
    const resolvers: Array<(value: string) => void> = [];
    const load = vi.fn(() => new Promise<string>((resolve) => resolvers.push(resolve)));
    const requests = ["a", "b", "c"].map((key) => queue.request(key, load));
    await vi.waitFor(() => expect(load).toHaveBeenCalledTimes(2));
    expect(queue.inFlight).toBe(2);
    expect(queue.pending).toBe(1);
    resolvers[0]("tile-a");
    await expect(requests[0].promise).resolves.toBe("tile-a");
    await vi.waitFor(() => expect(load).toHaveBeenCalledTimes(3));
    resolvers[1]("tile-b");
    resolvers[2]("tile-c");
    await expect(requests[2].promise).resolves.toBe("tile-c");
  });

  it("drops a cancelled request before it reaches the backend", async () => {
    const queue = new TileRequestQueue<string>(1);
    let release: ((value: string) => void) | undefined;
    const first = vi.fn(() => new Promise<string>((resolve) => (release = resolve)));
    const second = vi.fn(async () => "never");
    queue.request("a", first);
    const stale = queue.request("b", second);
    stale.cancel();
    stale.cancel(); // idempotent
    expect(queue.pending).toBe(0);
    await vi.waitFor(() => expect(release).toBeDefined());
    release!("a");
    await vi.waitFor(() => expect(queue.inFlight).toBe(0));
    expect(second).not.toHaveBeenCalled();
  });

  it("shares one render between requests for the same tile", async () => {
    const queue = new TileRequestQueue<string>(2);
    const load = vi.fn(async () => "tile");
    const a = queue.request("same", load);
    const b = queue.request("same", load);
    // One subscriber leaving keeps the render for the other.
    a.cancel();
    await expect(b.promise).resolves.toBe("tile");
    expect(load).toHaveBeenCalledTimes(1);
  });

  it("lets a running render finish and passes failures to the requester", async () => {
    const queue = new TileRequestQueue<string>(1);
    const failing = queue.request("bad", async () => {
      throw new Error("render failed");
    });
    await expect(failing.promise).rejects.toThrow("render failed");
    let release: ((value: string) => void) | undefined;
    const running = queue.request("slow", () => new Promise<string>((resolve) => (release = resolve)));
    await vi.waitFor(() => expect(release).toBeDefined());
    expect(queue.inFlight).toBe(1);
    // Cancelling a started render cannot abort the IPC call; it completes.
    running.cancel();
    release!("late");
    await expect(running.promise).resolves.toBe("late");
    await vi.waitFor(() => expect(queue.inFlight).toBe(0));
  });
});
