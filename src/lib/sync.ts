import type { SyncStateId } from "./api";

/**
 * Presentation helpers for the Sync screen. The backend owns all sync logic;
 * this file only maps states to badge tones and labels.
 */

export const SYNC_STATE_ORDER: SyncStateId[] = ["conflict", "cloud_ahead", "local_ahead", "local_only", "synced"];

export type SyncBadgeTone = "default" | "ok" | "warn" | "danger" | "accent";

/** Conflict is the loudest state; synced is the calmest. */
export function syncStateTone(state: SyncStateId | null): SyncBadgeTone {
  switch (state) {
    case "synced":
      return "ok";
    case "local_only":
      return "default";
    case "local_ahead":
      return "accent";
    case "cloud_ahead":
      return "warn";
    case "conflict":
      return "danger";
    default:
      return "default";
  }
}

/** True when the resolve panel should offer a decision. */
export function needsResolution(state: SyncStateId | null): boolean {
  return state === "conflict" || state === "cloud_ahead" || state === "local_ahead";
}

/** Recent-file filter: cloud sync tracks only native documents. */
export function isOswkPath(path: string): boolean {
  return path.toLowerCase().endsWith(".oswk");
}

export function shortHash(value: string | null | undefined, length = 8): string {
  if (!value) return "—";
  return value.length > length ? `${value.slice(0, length)}…` : value;
}
