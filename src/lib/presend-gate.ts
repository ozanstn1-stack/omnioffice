/**
 * The share gate for C10: a PDF is checked (`presend.ts`) before it goes to
 * the share sheet, unless Settings turned the check off. Anything else is
 * shared straight away.
 *
 * `shareWithCheck` opens `<PresendDialog>` (lazy-mounted once in App) through
 * this store; the dialog runs the check and shares the file, a cleaned copy,
 * or nothing. This module stays small because the share buttons live in the
 * entry bundle: the share code itself is loaded on use.
 */
import { create } from "zustand";
import { isPdf } from "./format";
import { isAndroid, revealAnyFile } from "./mobile";
import { useSettings } from "./store";

export interface PresendRequest {
  /** A path or a file token. */
  path: string;
  /** Share-sheet title. */
  title?: string;
  /** The file's name when `path` is a token (Recent files). */
  name?: string;
}

interface PresendState {
  request: PresendRequest | null;
  open: (request: PresendRequest) => void;
  close: () => void;
}

export const usePresend = create<PresendState>((set) => ({
  request: null,
  open: (request) => set({ request }),
  close: () => set({ request: null }),
}));

/** Shares a file, checking a PDF first when the setting is on. */
export async function shareWithCheck(path: string, title?: string, name?: string): Promise<void> {
  if (!isPdf(name ?? path) || useSettings.getState().settings.presendCheck === false) {
    const { shareDocumentFile } = await import("./share");
    await shareDocumentFile(path, title);
    return;
  }
  usePresend.getState().open({ path, title, name });
}

/**
 * The Recent files button: Android shares the file (a PDF is checked first),
 * desktop opens its folder.
 */
export function shareRecent(token: string, fileName: string): Promise<void> {
  return isAndroid() ? shareWithCheck(token, undefined, fileName) : revealAnyFile(token);
}
