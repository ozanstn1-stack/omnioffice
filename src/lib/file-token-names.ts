/**
 * Display names for capability tokens. The Rust dialog/drop/launch events and
 * the recent store return the name with the token, but many screens still
 * render `fileBaseName(value)`; this map lets those call sites show the real
 * name instead of the opaque token without threading a new prop everywhere.
 * It is a display cache only - never a security boundary.
 */
const names = new Map<string, string>();

export function rememberTokenName(token: string, name: string): void {
  if (token && name) names.set(token, name);
}

export function tokenName(token: string): string | undefined {
  return names.get(token);
}

export function rememberTokenRefs(refs: Array<{ token: string; name: string }>): void {
  for (const ref of refs) rememberTokenName(ref.token, ref.name);
}

/**
 * Recent entries are opaque tokens: remember their names so a tool can tell a
 * PDF from the rest (the reader used to drop a recent PDF as "unsupported").
 */
export function rememberRecentNames<T extends { token: string; fileName: string }>(entries: T[]): T[] {
  if (Array.isArray(entries)) for (const entry of entries) rememberTokenName(entry.token, entry.fileName);
  return entries;
}
