/**
 * The i18n key for one API-key storage tier.
 *
 * The Rust layer reports how it stored the key: `dpapi` (Windows), `keystore`
 * (Android), `keyring` (the OS credential vault, E1/4), `plain` (the fallback
 * with owner-only file permissions) or `none` when nothing is stored.
 */
const STORAGE_LABELS: Record<string, string> = {
  dpapi: "ai.keySecure",
  keystore: "ai.keyKeystore",
  keyring: "ai.keyKeyring",
  plain: "ai.keyPlain",
};

/** The i18n key describing a `keyStorage` value; `ai.keyMissing` otherwise. */
export function storageLabelKey(kind: string): string {
  return STORAGE_LABELS[kind] ?? "ai.keyMissing";
}
