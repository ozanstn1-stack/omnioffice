// Context-menu deep links: the service worker opens the app with
// `app.html#url=<encoded link URL>` and this module turns that fragment into a
// real File the reader can open.
//
// Safety rules:
//  * only absolute http(s) URLs are accepted - `javascript:`, `data:`,
//    `file:` and malformed values are ignored entirely;
//  * the extension stays offline by default: the file is only fetched after
//    the user presses the button, and the host permission is requested for
//    that single origin (optional host permissions in the manifest);
//  * the download is size-capped and never sent anywhere else.

import { setPendingFiles } from "./pending";

export interface RemoteLink {
  url: string;
  host: string;
}

export interface RemoteLinkDeps {
  fetch?: typeof fetch;
  /** Requests the optional host permission for one origin (extension only). */
  requestPermission?: (origin: string) => Promise<boolean>;
}

/** Hard cap for a context-menu download (same order of magnitude as the app). */
export const MAX_REMOTE_BYTES = 512 * 1024 * 1024;

/**
 * Parses the `#url=` fragment produced by the background service worker.
 * Returns null for anything that is not an absolute http(s) URL.
 */
export function parseRemoteLink(hash: string): RemoteLink | null {
  const raw = hash.startsWith("#") ? hash.slice(1) : hash;
  if (!raw) return null;
  let value: string | null = null;
  try {
    value = new URLSearchParams(raw).get("url");
  } catch {
    return null;
  }
  if (!value) return null;
  let parsed: URL;
  try {
    parsed = new URL(value);
  } catch {
    return null;
  }
  if (parsed.protocol !== "http:" && parsed.protocol !== "https:") return null;
  if (!parsed.hostname) return null;
  return { url: parsed.toString(), host: parsed.host };
}

/** Derives a safe file name from the link; always ends in `.pdf`. */
export function fileNameForLink(url: URL | string): string {
  const parsed = typeof url === "string" ? new URL(url) : url;
  const last = parsed.pathname.split("/").filter(Boolean).pop() ?? "";
  let decoded = last;
  try {
    decoded = decodeURIComponent(last);
  } catch {
    // Keep the raw value when it is not valid percent-encoding.
  }
  const safe = decoded.replace(/[\\/:*?"<>|\u0000-\u001f]/g, "_").trim();
  if (!safe) return "remote-document.pdf";
  return safe.toLowerCase().endsWith(".pdf") ? safe : `${safe}.pdf`;
}

/**
 * Fetches the linked document and hands it to the pending-file registry.
 * `deps` exists so the browser self test can exercise the whole path with a
 * fake fetch and without network access.
 */
export async function openRemoteLink(link: RemoteLink, deps: RemoteLinkDeps = {}): Promise<File> {
  const target = new URL(link.url);
  if (deps.requestPermission) {
    const granted = await deps.requestPermission(`${target.protocol}//${target.host}/*`);
    if (!granted) {
      throw new Error("Permission to read the linked site was not granted.");
    }
  }
  const fetchImpl = deps.fetch ?? fetch;
  const response = await fetchImpl(link.url, { credentials: "omit", redirect: "follow" });
  if (!response.ok) {
    throw new Error(`The server answered HTTP ${response.status} for the linked document.`);
  }
  const declared = Number(response.headers.get("content-length") ?? "0");
  if (declared > MAX_REMOTE_BYTES) {
    throw new Error("The linked document is larger than the download limit.");
  }
  const buffer = await response.arrayBuffer();
  if (buffer.byteLength > MAX_REMOTE_BYTES) {
    throw new Error("The linked document is larger than the download limit.");
  }
  if (buffer.byteLength === 0) {
    throw new Error("The linked document is empty.");
  }
  const file = new File([buffer], fileNameForLink(target), { type: "application/pdf" });
  setPendingFiles([file]);
  return file;
}
