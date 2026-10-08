/**
 * Follows a hyperlink that leaves the document. The webview itself is never
 * navigated and has no opener permission; the address goes to the native
 * `open_external_link` command, which applies the same allow-list again
 * (http, https and mailto only) before it hands the address to the system.
 */
import { invoke } from "@tauri-apps/api/core";
import { isExternalLink, safeLinkTarget } from "./links";

/** True when the system took the link; false for a refused address or a failed open. */
export async function openExternalLink(target: string): Promise<boolean> {
  const url = safeLinkTarget(target);
  if (!url || !isExternalLink(url)) return false;
  try {
    await invoke("open_external_link", { url });
    return true;
  } catch {
    return false;
  }
}
