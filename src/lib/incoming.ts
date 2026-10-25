/**
 * Android open-with tokens.
 *
 * `MainActivity` copies a shared document into the cache and queues its
 * absolute path, but a refused document is queued as `__error:<reason>:<name>`
 * instead (the reasons come from `IncomingFiles.CopyResult`). This module turns
 * that token back into structured data so the UI can explain what happened
 * instead of showing one generic message.
 */

/** The prefix the Android shell uses for a refused-document token. */
const ERROR_PREFIX = "__error:";

/**
 * Every reason the Android shell can queue. `NO_NAME` and `UNSUPPORTED` are
 * legacy: the current copy pipeline decides from content, but a token queued
 * before the app updated can still be drained, so both stay supported.
 */
export const INCOMING_ERROR_REASONS = [
  "NO_NAME",
  "UNSUPPORTED",
  "NO_STREAM",
  "TOO_LARGE",
  "FAILED",
  "NO_ACCESS",
  "UNKNOWN_TYPE",
] as const;

export type IncomingErrorReason = (typeof INCOMING_ERROR_REASONS)[number];

export interface IncomingError {
  reason: IncomingErrorReason;
  /** The provider's display name; null when the shell could not resolve one. */
  name: string | null;
}

/** True when [value] is one of the reasons the shell is allowed to queue. */
export function isIncomingErrorReason(value: string): value is IncomingErrorReason {
  return (INCOMING_ERROR_REASONS as readonly string[]).includes(value);
}

/**
 * Parses one queued token, or returns null for anything that is not a refused
 * document (a real path, an `__action:*` shortcut, an unknown reason).
 *
 * Only the first two colons are separators: sanitised provider names never
 * contain one, but a name from an older build may, and it is display-only.
 */
export function parseIncomingError(token: string): IncomingError | null {
  if (!token.startsWith(ERROR_PREFIX)) return null;
  const rest = token.slice(ERROR_PREFIX.length);
  const separator = rest.indexOf(":");
  const reason = separator < 0 ? rest : rest.slice(0, separator);
  if (!isIncomingErrorReason(reason)) return null;
  const name = separator < 0 ? null : rest.slice(separator + 1) || null;
  return { reason, name };
}

/** The first refused-document token in a drained queue, if there is one. */
export function firstIncomingError(tokens: string[]): IncomingError | null {
  for (const token of tokens) {
    const parsed = parseIncomingError(token);
    if (parsed) return parsed;
  }
  return null;
}

/**
 * The i18n key that explains one reason. The four groups are deliberate:
 * "too large", "could not read" (stream/access/copy), "type not recognised"
 * (the new content-sniffing rejection) and "unsupported" (legacy reasons).
 */
export function incomingErrorKey(reason: IncomingErrorReason): string {
  switch (reason) {
    case "TOO_LARGE":
      return "errors.incomingTooLarge";
    case "NO_STREAM":
    case "NO_ACCESS":
    case "FAILED":
      return "errors.incomingUnreadable";
    case "UNKNOWN_TYPE":
      return "errors.incomingUnknownType";
    default:
      return "errors.unsupported";
  }
}
