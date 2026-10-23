import { onLaunchFilesQueued, startupFiles, takeLaunchFiles } from "./api";
import type { FileToken } from "./file-tokens";

/**
 * Delivers every document the OS handed to the app: the files on the command
 * line of this launch plus the ones later launches forwarded to this window
 * (single instance, see src-tauri/src/launch.rs).
 *
 * Forwarded files wait in a Rust-side queue because selecting several files in
 * Explorer starts one process per file before this window has registered its
 * listener. The queue is drained when the event arrives and once more right
 * after listening starts, so nothing queued earlier is missed. Returns the
 * cleanup function.
 */
export function watchLaunchFiles(deliver: (refs: FileToken[]) => void): () => void {
  let cancelled = false;
  let unlisten: (() => void) | undefined;
  const hand = (refs: FileToken[]) => {
    if (!cancelled && refs.length > 0) deliver(refs);
  };
  const drain = () => {
    void takeLaunchFiles()
      .then(hand)
      .catch(() => undefined);
  };
  void startupFiles()
    .then(hand)
    .catch(() => undefined);
  void onLaunchFilesQueued(drain)
    .then((stop) => {
      if (cancelled) {
        stop();
        return;
      }
      unlisten = stop;
      drain();
    })
    .catch(() => undefined);
  return () => {
    cancelled = true;
    unlisten?.();
  };
}
