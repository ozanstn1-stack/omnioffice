/**
 * Where a chat citation should take the user.
 *
 * A citation carries the source document's path and its location inside the
 * document (`page 3`, `paragraph 12`, `Sheet1!A1:B7`, `slide 4`). A PDF opens
 * in the Reader at the cited page; an office document opens in the workspace
 * (a paragraph/sheet/slide anchor is not supported yet, so only the file is
 * opened). When a citation has no path (an older saved thread) the current
 * document is used.
 */

export interface CitationTarget {
  kind: "pdf" | "office";
  path: string;
  /** The page to open; 0 when the citation is not page-anchored. */
  page: number;
}

/** `page 3` -> 3; anything else -> 0. */
export function parseLocationPage(location?: string): number {
  if (!location) return 0;
  const match = /^page\s+(\d+)/i.exec(location.trim());
  return match ? Number(match[1]) : 0;
}

export function resolveCitationTarget(
  path?: string,
  page = 0,
  location?: string,
  fallbackPath?: string,
): CitationTarget | null {
  const target = (path ?? "").trim() || (fallbackPath ?? "").trim();
  if (!target) return null;
  const resolvedPage = page > 0 ? page : parseLocationPage(location);
  return { kind: /\.pdf$/i.test(target) ? "pdf" : "office", path: target, page: resolvedPage };
}
