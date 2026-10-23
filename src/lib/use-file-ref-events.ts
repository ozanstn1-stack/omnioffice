/**
 * Token-based OS events: files dropped on the window and files this launch was
 * opened with. The Rust side registers the paths and emits only tokens, so the
 * React layer never handles a path. The listeners are registered once; the
 * latest callbacks are read through a ref at event time.
 */
import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { isOfficePath } from "./office-store";
import { useDrop } from "./store";
import type { FileToken } from "./file-tokens";
import { watchLaunchFiles } from "./launch-files";
import type { ScreenId } from "./nav";
import { rememberTokenRefs } from "./file-token-names";

export function useDropTokens(options: {
  screen: ScreenId;
  openDocument: (ref: FileToken) => void;
  setFiles: (files: string[]) => void;
  notify: (detail: string) => void;
}): void {
  const latest = useRef(options);
  useEffect(() => {
    latest.current = options;
  });
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<FileToken[]>("file-drop", (event) => {
      const refs = event.payload ?? [];
      if (!refs.length) return;
      rememberTokenRefs(refs);
      const current = latest.current;
      if (current.screen === "home") {
        if (refs.length === 1) {
          current.openDocument(refs[0]);
          return;
        }
        current.setFiles(refs.map((ref) => ref.token));
        current.notify(`${refs.length} files`);
        return;
      }
      const handler = useDrop.getState().handler;
      if (handler) handler(refs.map((ref) => ref.token));
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch(() => undefined);
    return () => unlisten?.();
  }, []);
}

export function useLaunchTokens(options: {
  openOffice: (tokens: string[]) => void;
  openPdfs: (tokens: string[]) => void;
}): void {
  const latest = useRef(options);
  useEffect(() => {
    latest.current = options;
  });
  useEffect(
    () =>
      watchLaunchFiles((refs) => {
        rememberTokenRefs(refs);
        const office = refs.filter((ref) => isOfficePath(ref.name));
        if (office.length > 0) latest.current.openOffice(office.map((ref) => ref.token));
        const pdfs = refs.filter((ref) => ref.name.toLowerCase().endsWith(".pdf"));
        if (pdfs.length > 0 && office.length === 0) latest.current.openPdfs(pdfs.map((ref) => ref.token));
      }),
    [],
  );
}
