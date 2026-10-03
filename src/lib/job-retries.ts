/**
 * Retry routing for background jobs.
 *
 * Every long-running api wrapper registers its job with a stable `kind` and
 * the exact invoke arguments as the persisted payload (see
 * `trackJobInvocation`). After a restart the Jobs screen can therefore re-run
 * the operation from `jobs.json` without the originating screen being mounted:
 * one handler per kind re-invokes the same command with the same arguments.
 *
 * Credential-looking fields are blanked before the payload is persisted - the
 * job store is plain JSON on disk. A retry of an encrypted document therefore
 * fails with `password_required` instead of leaking the password.
 */
import { invoke } from "@tauri-apps/api/core";
import { fileBaseName } from "./format";
import { registerJobRetryHandler, useJobs, type JobRecord } from "./jobs";

/** The long operations that can be re-run from their persisted payload. */
export const TRACKED_JOB_COMMANDS: Record<string, { kind: string; command: string }> = {
  merge_pdfs: { kind: "merge", command: "merge_pdfs" },
  split_pdf: { kind: "split", command: "split_pdf" },
  apply_page_plan: { kind: "organize", command: "apply_page_plan" },
  extract_pages: { kind: "extract", command: "extract_pages" },
  delete_pages: { kind: "delete-pages", command: "delete_pages" },
  rotate_pages: { kind: "rotate-pages", command: "rotate_pages" },
  compress_pdf: { kind: "compress", command: "compress_pdf" },
  ocr_pdf: { kind: "ocr", command: "ocr_pdf" },
  protect_pdf: { kind: "protect", command: "protect_pdf" },
  unlock_pdf: { kind: "unlock", command: "unlock_pdf" },
  pdf_to_images: { kind: "pdf-to-images", command: "pdf_to_images" },
  images_to_pdf: { kind: "images-to-pdf", command: "images_to_pdf" },
  resize_pages: { kind: "resize-pages", command: "resize_pages" },
  crop_pages: { kind: "crop-pages", command: "crop_pages" },
  edit_metadata: { kind: "metadata", command: "edit_metadata" },
  add_page_numbers: { kind: "page-numbers", command: "add_page_numbers" },
  watermark_pdf: { kind: "watermark", command: "watermark_pdf" },
  annotate_pdf: { kind: "annotate", command: "annotate_pdf" },
  redact_pdf: { kind: "redact", command: "redact_pdf" },
  compare_pdfs: { kind: "compare", command: "compare_pdfs" },
  sanitize_pdf: { kind: "sanitize", command: "sanitize_pdf" },
  flatten_pdf: { kind: "flatten", command: "flatten_pdf" },
  pdfa_convert: { kind: "pdfa", command: "pdfa_convert" },
  ai_summarize: { kind: "ai-summarize", command: "ai_summarize" },
  ai_translate: { kind: "ai-translate", command: "ai_translate" },
  ai_ask: { kind: "ai-ask", command: "ai_ask" },
  ai_cleanup_text: { kind: "ai-cleanup", command: "ai_cleanup_text" },
  ai_suggest_metadata: { kind: "ai-metadata", command: "ai_suggest_metadata" },
  vault_scan: { kind: "vault", command: "vault_scan" },
};

const COMMAND_BY_KIND = new Map(Object.values(TRACKED_JOB_COMMANDS).map((entry) => [entry.kind, entry.command]));

/** Keys whose values never reach `jobs.json` in clear text. */
const SECRET_KEY = /(password|secret|token|api[_-]?key|private[_-]?key)/i;

/** Deep copy of a retry payload with credential-looking fields blanked. */
export function sanitizeRetryPayload(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(sanitizeRetryPayload);
  if (value && typeof value === "object") {
    const output: Record<string, unknown> = {};
    for (const [key, entry] of Object.entries(value as Record<string, unknown>)) {
      output[key] = SECRET_KEY.test(key) ? null : sanitizeRetryPayload(entry);
    }
    return output;
  }
  return value;
}

/** First string that looks like a document path, for a human-readable title. */
function firstDocumentPath(value: unknown): string | null {
  if (typeof value === "string") {
    return /\.(pdf|oswk|docx|odt|xlsx|ods|pptx|odp|csv|rtf|txt|md|html|jpe?g|png|webp|bmp|tiff?)$/i.test(value)
      ? value
      : null;
  }
  if (Array.isArray(value)) {
    for (const entry of value) {
      const found = firstDocumentPath(entry);
      if (found) return found;
    }
    return null;
  }
  if (value && typeof value === "object") {
    const record = value as Record<string, unknown>;
    // Prefer the input/output keys so a nested password or id cannot win.
    for (const key of ["input", "inputs", "left", "path", "output", "outputDir"]) {
      const found = firstDocumentPath(record[key]);
      if (found) return found;
    }
    for (const entry of Object.values(record)) {
      const found = firstDocumentPath(entry);
      if (found) return found;
    }
  }
  return null;
}

/** Finds the job id a request carries (all tracked commands have one). */
function findJobId(value: unknown): string | null {
  if (Array.isArray(value)) {
    for (const entry of value) {
      const found = findJobId(entry);
      if (found) return found;
    }
    return null;
  }
  if (value && typeof value === "object") {
    const record = value as Record<string, unknown>;
    if (typeof record.jobId === "string" && record.jobId) return record.jobId;
    for (const entry of Object.values(record)) {
      const found = findJobId(entry);
      if (found) return found;
    }
  }
  return null;
}

/**
 * Registers a tracked invocation with the jobs store. Called by the api
 * wrappers right before `invoke`; a no-op for commands without a job.
 */
export function trackJobInvocation(command: string, args: Record<string, unknown>): void {
  const tracked = TRACKED_JOB_COMMANDS[command];
  if (!tracked) return;
  const jobId = findJobId(args);
  if (!jobId) return;
  const path = firstDocumentPath(args);
  const title = path ? `${fileBaseName(path)} · ${tracked.kind}` : tracked.kind;
  useJobs.getState().start({ id: jobId, kind: tracked.kind, title, payload: sanitizeRetryPayload(args) });
}

/**
 * Installs one retry handler per tracked kind. The app shell calls this once;
 * it returns the uninstall function for effect cleanup.
 */
export function installJobRetryHandlers(): () => void {
  const unregisters = [...COMMAND_BY_KIND.entries()].map(([kind, command]) =>
    registerJobRetryHandler(kind, async (record: JobRecord) => {
      const payload =
        record.payload && typeof record.payload === "object" ? (record.payload as Record<string, unknown>) : null;
      if (!payload) throw new Error(`job ${record.id} has no stored payload`);
      // Re-register first so the row leaves its terminal state and the UI
      // shows the re-run as running again.
      useJobs.getState().start({ id: record.id, kind: record.kind, title: record.title, payload });
      await invoke(command, payload);
    }),
  );
  return () => unregisters.forEach((unregister) => unregister());
}
