/**
 * In-app trace of the Android open-with / share pipeline.
 *
 * Why: the reported failure ("sharing a document opens the app, but it stays on
 * Home") only reproduces on a real phone without adb. The Rust side and the
 * Kotlin shell already keep their own logs; this module keeps the frontend's
 * half - which drain delivered what, which route was chosen, why an open
 * failed - in a small in-memory ring. Settings combines the three into one
 * copyable report (see `components/open-with-diagnostics.tsx`).
 *
 * Nothing here may throw into the pipeline it observes: recording is best
 * effort, and the text builder always returns something readable.
 */
import packageJson from "../../package.json";

export interface OpenWithEvent {
  /** ISO-8601 timestamp of the event. */
  at: string;
  /** Short pipeline fact, e.g. `drain:2`, `route:office` or `open-failed:<message>`. */
  event: string;
}

/** Enough history for one reproduction without bloating the clipboard text. */
const MAX_EVENTS = 100;

/** One entry stays short; a stack trace or a long path must not fill the ring. */
const MAX_EVENT_CHARS = 400;

const events: OpenWithEvent[] = [];

/** Records one event; never throws and never breaks the caller's flow. */
export function recordOpenWith(event: string): void {
  try {
    const line = String(event ?? "")
      .replace(/[\r\n]+/g, " ")
      .slice(0, MAX_EVENT_CHARS);
    events.push({ at: new Date().toISOString(), event: line });
    if (events.length > MAX_EVENTS) events.splice(0, events.length - MAX_EVENTS);
  } catch {
    // Diagnostics must never break the open-with pipeline.
  }
}

/** Clears the ring (tests, and a fresh reproduction). */
export function resetOpenWithDiagnostics(): void {
  events.length = 0;
}

/** Renders the frontend section of the report; always returns readable text. */
export function openWithDiagnosticsText(): string {
  try {
    const version = (packageJson as { version?: string }).version;
    const platform = typeof navigator === "undefined" ? "unknown" : navigator.userAgent;
    const lines = [
      "== open-with: frontend ==",
      `version: ${version ? `v${version}` : "(unknown)"}`,
      `platform: ${platform || "(unknown)"}`,
      `events: ${events.length}/${MAX_EVENTS}`,
    ];
    if (events.length === 0) {
      lines.push("(none)");
    } else {
      for (const entry of events) lines.push(`${entry.at} ${entry.event}`);
    }
    return lines.join("\n");
  } catch {
    return "== open-with: frontend ==\n(note: the event ring could not be rendered)";
  }
}
