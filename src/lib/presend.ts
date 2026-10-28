/**
 * The pre-send check's work (C10), loaded with the dialog: read the file into
 * findings (`presend-check.ts`) and write a cleaned copy for sending. The
 * gate the share buttons call is `presend-gate.ts`.
 */
import { invoke } from "@tauri-apps/api/core";
import { appCacheDir, join } from "@tauri-apps/api/path";
import { inspectDocument, pageText, pdfVerifySignatures } from "./api";
import { fileBaseName, uid } from "./format";
import { presendFindings, SEND_SANITIZE_OPTIONS, type PresendFinding } from "./presend-check";

/** Pages read for personal data; a long file is judged by its first pages. */
export const PRESEND_TEXT_PAGES = 20;

/** Reads the file and returns its findings, worst first. */
export async function runPresendCheck(path: string): Promise<PresendFinding[]> {
  const [inspection, signatures, pdfa] = await Promise.all([
    inspectDocument(path),
    pdfVerifySignatures(path).catch(() => ({ signatures: [], warnings: [] })),
    invoke<{ valid: boolean }>("pdfa_validate", { request: { input: path, level: "2b" } }).catch(() => null),
  ]);
  const texts: string[] = [];
  for (let page = 1; page <= Math.min(inspection.pageCount, PRESEND_TEXT_PAGES); page++) {
    texts.push(await pageText(path, page).catch(() => ""));
  }
  return presendFindings({
    inspection,
    signatures: signatures.signatures,
    text: texts.join("\n"),
    pdfa: pdfa ? { valid: pdfa.valid } : null,
  });
}

/**
 * Writes a sanitized copy under the cache's share folder (the FileProvider
 * root on Android) with the original name, and returns its path.
 */
export async function cleanCopyForSending(path: string, name?: string): Promise<string> {
  const directory = await join(await appCacheDir(), "share", uid("clean"));
  await invoke("ensure_dir", { path: directory });
  const output = await join(directory, name ?? fileBaseName(path));
  await invoke("sanitize_pdf", {
    request: { input: path, output: { path: output, overwrite: "replace" }, options: SEND_SANITIZE_OPTIONS },
  });
  return output;
}
