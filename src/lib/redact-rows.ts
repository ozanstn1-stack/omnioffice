/**
 * Pure helpers for the redaction screen: keyword splitting and the mapping
 * between detector matches and the boxes drawn on the page. They live apart
 * from the screen so the JSX file stays under its line budget.
 */
import type { RedactionArea, RedactionMatch } from "./types";

/** A box the user drew, in page space, with a stable id for removal. */
export interface DrawnBox extends RedactionArea {
  id: string;
  /** Set for boxes the detector proposed, so they can be labelled. */
  kind?: string;
}

/** A detected match plus the id that links its checkbox to its box. */
export interface MatchRow {
  id: string;
  match: RedactionMatch;
}

/** Comma/semicolon/newline separated keywords: trimmed, order kept, deduped. */
export function splitKeywords(raw: string): string[] {
  const seen = new Set<string>();
  const keywords: string[] = [];
  for (const part of raw.split(/[,;\n]/)) {
    const keyword = part.trim();
    if (!keyword || seen.has(keyword)) continue;
    seen.add(keyword);
    keywords.push(keyword);
  }
  return keywords;
}

/** The row id is page-scoped so a match keeps its identity off the shown page. */
export function matchRow(source: "auto" | "pattern", match: RedactionMatch, index: number): MatchRow {
  return { id: `${source}-${match.page}-${index}`, match };
}

export function matchBox(row: MatchRow): DrawnBox {
  return {
    id: row.id,
    page: row.match.page,
    left: row.match.left,
    bottom: row.match.bottom,
    right: row.match.right,
    top: row.match.top,
    kind: row.match.kind,
  };
}

export function kindLabel(t: (key: string) => string, kind?: string): string {
  if (!kind) return t("redact.manual");
  const key = `redact.kind.${kind}`;
  const label = t(key);
  return label === key ? kind : label;
}
