/**
 * On-device page detection for the scanner (C12).
 *
 * `office_scan_detect` runs the Canny/Hough pipeline in Rust on a downscaled
 * copy of the photo and returns the page corners in full-size pixels, so they
 * feed straight into the corner picker or straight into `office_scan_pages`.
 * Nothing here calls the network: the detector is the local `pdfcore`
 * pipeline, and a miss simply leaves the manual picker in charge.
 */
import { call } from "./typed-call";

/** How a corrected page is rendered (generated from the Rust enum). */
export type ScanFilter = import("./generated/specta").ScanFilter;

/** One page as `office_scan_pages` takes it. */
export interface ScanPageRequest {
  path: string;
  /** The four corners in upright full-size pixels; omitted to use the whole photo. */
  corners?: [number, number][];
  /** Colour, greyscale or black-and-white output. */
  filter?: ScanFilter;
}

/** What the detector found for one photo (generated from the Rust struct). */
export type ScanDetection = import("./generated/specta").ScanDetection;

/** Finds the page's corners on the device. */
export const scanDetect = (path: string): Promise<ScanDetection> => call((c) => c.officeScanDetect(path));

/** Flattens the detected pages and assembles the PDF. */
export const scanPages = (pages: ScanPageRequest[], output: string) =>
  call((c) => c.officeScanPages({ pages, output }));

/** The corners to keep, or `undefined` when the detector saw no page. */
export function detectedCorners(detection: ScanDetection | null | undefined): [number, number][] | undefined {
  if (!detection || !detection.detected || detection.corners.length !== 4) return undefined;
  return detection.corners.map(([x, y]) => [x, y] as [number, number]);
}
