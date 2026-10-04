/**
 * Which screen opens a document the user picked or dropped.
 *
 * Home used to only fill the selection state, so choosing a file appeared to
 * do nothing. The routing is a pure function so it can be unit tested: office
 * documents go to the workspace, PDFs to the reader, images to image-to-PDF,
 * and anything else returns null (the caller hands it to the system viewer).
 */
import { isImage } from "./format";
import { isOfficePath } from "./office-store";
import type { ScreenId } from "./nav";

export interface OpenRoute {
  screen: ScreenId;
  /** Files the screen should open. */
  files?: string[];
  /** True when the workspace opens the file itself from the path. */
  office: boolean;
}

export function routeForPath(path: string): OpenRoute | null {
  if (!path) return null;
  if (isOfficePath(path)) {
    return { screen: "office", office: true };
  }
  if (path.toLowerCase().endsWith(".pdf")) {
    return { screen: "reader", files: [path], office: false };
  }
  if (isImage(path)) {
    return { screen: "imagesToPdf", files: [path], office: false };
  }
  return null;
}
