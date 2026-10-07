/**
 * Runs a user's regular expression over the document text in a throwaway
 * worker before the editor uses it on the main thread.
 *
 * A pattern with catastrophic backtracking (`(\p{L}+\s?)+;` on an ordinary
 * paragraph) never returns, and on the main thread that freezes the WebView
 * with the unsaved document in it. A worker can be terminated: the probe
 * scans every paragraph with the same pattern and gives up after a time
 * limit. Once it finishes, the editor's own matching and replacing on the
 * same document and pattern are known to finish too.
 */

/** How long a pattern may take over the whole document. */
export const PROBE_TIMEOUT_MS = 1500;

export type ProbeStatus = "ok" | "slow";

export interface RegexProbe {
  promise: Promise<ProbeStatus>;
  cancel: () => void;
}

const PROBE_SOURCE = `self.onmessage = (event) => {
  const { source, flags, texts } = event.data;
  const pattern = new RegExp(source, flags);
  for (const text of texts) {
    for (const match of text.matchAll(pattern)) void match;
  }
  self.postMessage("ok");
};`;

/** False where workers do not exist (tests); the caller then matches directly. */
export function canProbeRegex(): boolean {
  return typeof Worker !== "undefined" && typeof Blob !== "undefined" && typeof URL.createObjectURL === "function";
}

export function probeRegex(pattern: RegExp, texts: string[], timeoutMs = PROBE_TIMEOUT_MS): RegexProbe {
  if (!canProbeRegex()) return { promise: Promise.resolve("ok"), cancel: () => undefined };
  const url = URL.createObjectURL(new Blob([PROBE_SOURCE], { type: "text/javascript" }));
  const worker = new Worker(url);
  let resolveProbe: (status: ProbeStatus) => void = () => undefined;
  const promise = new Promise<ProbeStatus>((resolve) => {
    resolveProbe = resolve;
  });
  const timer = setTimeout(() => settle("slow"), timeoutMs);
  const stop = () => {
    clearTimeout(timer);
    worker.terminate();
    URL.revokeObjectURL(url);
  };
  const settle = (status: ProbeStatus) => {
    stop();
    resolveProbe(status);
  };
  worker.onmessage = () => settle("ok");
  // A worker that cannot run the pattern is no proof it is fast.
  worker.onerror = () => settle("slow");
  worker.postMessage({ source: pattern.source, flags: pattern.flags, texts });
  return { promise, cancel: stop };
}
