/**
 * Lists a PDF under Home's "Recent" when the reader opens it.
 *
 * Only tool results, office documents and files handed over by another
 * Android app were recorded; a PDF opened in the reader from a dialog, a drop
 * or Home left no trace, so Recent kept showing old tool outputs instead of
 * what the user had just read. The entry is added once the document has
 * loaded, so a file that failed to open is not listed.
 */
import { useEffect, useRef } from "react";
import { useRecent } from "../../lib/store";

/** Recent-files tag for PDFs opened in the reader. */
export const READER_RECENT_TOOL = "_reader";

export function readerRecentEntry(path: string, name: string, now: number = Date.now()) {
  return { path, fileName: name, tool: READER_RECENT_TOOL, timestamp: Math.floor(now / 1000) };
}

/**
 * Records the open document once per open. `path` may be a capability token:
 * the backend resolves it to the real path before storing it.
 */
export function useRememberReaderDocument(path: string | undefined, name: string | undefined, loaded: boolean) {
  const recorded = useRef<string | null>(null);
  useEffect(() => {
    if (!path || !loaded || recorded.current === path) return;
    recorded.current = path;
    void useRecent.getState().add(readerRecentEntry(path, name || path));
  }, [loaded, name, path]);
}
