/**
 * Turns the batch detector's answer into scanner page rows (D4).
 *
 * One photo is one page unless the detector saw a two-page spread, in which
 * case the photo becomes two rows that keep the same path but carry their half
 * (0 left, 1 right). A row the user already corrected keeps its corners, so a
 * later detection never undoes a manual fix.
 */
import type { ScanBatchPage } from "../lib/scan-detect";
import type { ScannerPage } from "./scanner-page-list";

const cornersOf = (corners: number[][]): [number, number][] => corners.map(([x, y]) => [x, y] as [number, number]);

/** One placeholder row per new photo, shown while the detector runs. */
export const pendingScanPages = (paths: string[]): ScannerPage[] => paths.map((path) => ({ id: `${path}#0`, path }));

/** Applies the batch detection: a spread yields two rows, each its own page. */
export function mergeScanDetection(current: ScannerPage[], found: ScanBatchPage[]): ScannerPage[] {
  const byPath = new Map(found.map((item) => [item.path, item]));
  return current.flatMap((page) => {
    const item = byPath.get(page.path);
    // A page the user already corrected keeps its corners.
    if (!item || page.corners) return [page];
    if (item.pages.length >= 2) {
      return item.pages.map((corners, half) => ({
        id: `${page.path}#${half}`,
        path: page.path,
        corners: cornersOf(corners),
        auto: true,
        half,
      }));
    }
    const corners = item.pages[0];
    return corners ? [{ id: `${page.path}#0`, path: page.path, corners: cornersOf(corners), auto: true }] : [page];
  });
}
