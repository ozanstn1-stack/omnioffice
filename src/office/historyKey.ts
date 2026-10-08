/**
 * Version history is stored per document key on disk. A tab id is minted
 * every time a document is opened, so a history keyed by it is unreachable
 * after the file is reopened; the file path is the stable identity. The
 * backend only accepts [A-Za-z0-9_-] keys, so the path is hashed.
 */

/** FNV-1a, 64 bit, over the UTF-8 bytes: short, synchronous and stable. */
function fnv1a64(text: string): string {
  let hash = 0xcbf29ce484222325n;
  for (const byte of new TextEncoder().encode(text)) {
    hash ^= BigInt(byte);
    hash = BigInt.asUintN(64, hash * 0x100000001b3n);
  }
  return hash.toString(16).padStart(16, "0");
}

/** Windows paths ignore case and accept either separator; POSIX paths do neither. */
function normalizePath(path: string): string {
  if (/^[a-zA-Z]:[\\/]/.test(path) || path.startsWith("\\\\")) return path.replaceAll("\\", "/").toLowerCase();
  return path;
}

/** The history key of a document: its path when saved, else its tab id. */
export function historyKeyFor(tab: { id: string; path: string | null }): string {
  if (!tab.path) return tab.id;
  return `p-${fnv1a64(normalizePath(tab.path))}`;
}
