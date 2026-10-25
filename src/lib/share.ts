/**
 * "Share" for documents the app has open: WhatsApp, mail, Drive, Bluetooth...
 *
 * Android: the file is handed to the system share sheet straight from the app's
 * private storage (`share_document_file`, `ShareFilePlugin.kt`); when that
 * bridge refuses - the file lives somewhere the FileProvider does not expose -
 * the older route publishes a copy to `Downloads/OmniOffice` and shares that.
 * Desktop has no share sheet here, so the file is revealed in its folder.
 *
 * Office tabs are shared as a fresh file written from the current model, so
 * unsaved edits go out too and the receiver sees the document's own name.
 */
import { invoke } from "@tauri-apps/api/core";
import { appCacheDir, join } from "@tauri-apps/api/path";
import { fileBaseName, uid } from "./format";
import { isAndroid, mimeForName, revealAnyFile } from "./mobile";
import * as api from "./office-api";
import type { OfficeKind } from "./office-types";

/** Formats each editor can write and a receiver can open without OmniOffice. */
const SHAREABLE: Record<OfficeKind, readonly string[]> = {
  writer: ["docx", "odt", "rtf", "txt", "md", "html"],
  calc: ["xlsx", "ods", "csv"],
  impress: ["pptx", "odp"],
};

const DEFAULT_SHARE: Record<OfficeKind, string> = { writer: "docx", calc: "xlsx", impress: "pptx" };

/**
 * The format an office tab is shared in: the format it was opened from when
 * the editor can write it back (a `.odt` stays `.odt`), otherwise the common
 * one (`.doc`, `.xls` and `.oswk` go out as DOCX/XLSX/PPTX).
 */
export function shareExtensionFor(kind: OfficeKind, path: string | null | undefined): string {
  const match = /\.([a-z0-9]+)$/i.exec(path ?? "");
  const extension = match ? match[1].toLowerCase() : "";
  return SHAREABLE[kind].includes(extension) ? extension : DEFAULT_SHARE[kind];
}

/** A file name the receiver can read: the tab title without path characters. */
export function shareFileName(title: string, extension: string): string {
  const stem = (title || "").replace(/\.[a-z0-9]{2,5}$/i, "");
  const cleaned = stem.replace(/[\\/:*?"<>|]/g, "-").trim() || "Document";
  return `${cleaned}.${extension}`;
}

/**
 * Shares one document file. Resolves once the share sheet (Android) or the
 * folder (desktop) is open; rejects when neither route worked.
 */
export async function shareDocumentFile(path: string, title?: string): Promise<void> {
  if (!isAndroid()) {
    await revealAnyFile(path);
    return;
  }
  try {
    await invoke("share_document_file", { path, mimeType: mimeForName(fileBaseName(path)), title: title ?? null });
  } catch {
    // Outside the FileProvider roots (or the bridge is not registered): share
    // a published copy instead, the route the tool result panels use.
    await revealAnyFile(path);
  }
}

/** Where an office share copy is written: its own folder keeps the plain name. */
async function shareTarget(name: string): Promise<string> {
  const directory = await join(await appCacheDir(), "share", uid("share"));
  await invoke("ensure_dir", { path: directory });
  return join(directory, name);
}

/**
 * Writes the tab's current model to a share copy and shares it, as the
 * document itself or as a PDF. Returns the written path.
 */
export async function shareOfficeDocument(
  tab: { kind: OfficeKind; title: string; path?: string | null; model: unknown },
  as: "document" | "pdf",
  chooserTitle?: string,
): Promise<string> {
  const extension = as === "pdf" ? "pdf" : shareExtensionFor(tab.kind, tab.path);
  const target = await shareTarget(shareFileName(tab.title, extension));
  const result =
    as === "pdf"
      ? await api.exportPdf(tab.kind, tab.model, target)
      : await api.saveDocument(tab.kind, tab.model, target);
  await shareDocumentFile(result.path, chooserTitle);
  return result.path;
}
