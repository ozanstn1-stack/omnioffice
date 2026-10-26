/**
 * What the editors' status line shows for a document's location.
 *
 * Desktop shows the full path, the user picked it. On Android the path is the
 * app's private copy (`/data/user/0/<app>/cache/incoming/<uuid>/name.docx` for
 * a document opened from WhatsApp), which means nothing to the user and filled
 * the status line on a phone, so only the file name is shown there.
 */
import { fileBaseName } from "../lib/format";
import { isAndroid } from "../lib/mobile";

/** The status-line label for `path`, or null for an unsaved document. */
export function documentLocationLabel(path: string | null | undefined): string | null {
  if (!path) return null;
  return isAndroid() ? fileBaseName(path) : path;
}
