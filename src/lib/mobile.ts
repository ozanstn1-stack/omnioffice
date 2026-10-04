import { invoke } from "@tauri-apps/api/core";
import { appCacheDir, join } from "@tauri-apps/api/path";
import * as AndroidFs from "tauri-plugin-android-fs-api";
import { fileBaseName, uid } from "./format";

// ---------------------------------------------------------------------------
// Android file bridge
//
// The desktop UI is built around real filesystem paths: every Tauri command
// reads and writes plain files. Android does not hand out paths for files the
// user picks (the Storage Access Framework returns content:// URIs), and it
// does not let ordinary files be opened by other apps either. This module
// closes that gap:
//
//   * picked documents are copied into the app cache and handed to the UI as
//     ordinary paths, so every tool keeps working unchanged;
//   * finished documents are published to the public Downloads folder (or to
//     a destination the user picked) and the resulting content:// URI is used
//     for "open" and "share".
//
// Everything here is a no-op on desktop.
// ---------------------------------------------------------------------------

let androidCache: boolean | null = null;

/** True when running inside the Android WebView. */
export function isAndroid(): boolean {
  if (androidCache !== null) return androidCache;
  androidCache = typeof navigator !== "undefined" && /android/i.test(navigator.userAgent);
  return androidCache;
}

const publishedUris = new Map<string, AndroidFs.FsUri>();

/** Content URI of a document the app published (after open/share/save). */
export function publishedUri(path: string): string | undefined {
  return publishedUris.get(path)?.uri;
}

function sanitizeName(name: string): string {
  const cleaned = name.replace(/[\\/:*?"<>|]/g, "_").trim();
  return cleaned || "document.pdf";
}

function uniqueName(name: string, taken: Set<string>): string {
  if (!taken.has(name.toLowerCase())) {
    taken.add(name.toLowerCase());
    return name;
  }
  const dot = name.lastIndexOf(".");
  const stem = dot > 0 ? name.slice(0, dot) : name;
  const extension = dot > 0 ? name.slice(dot) : "";
  let index = 2;
  let candidate = `${stem} (${index})${extension}`;
  while (taken.has(candidate.toLowerCase())) {
    index += 1;
    candidate = `${stem} (${index})${extension}`;
  }
  taken.add(candidate.toLowerCase());
  return candidate;
}

export function mimeForName(name: string): string {
  const lower = name.toLowerCase();
  if (lower.endsWith(".pdf")) return "application/pdf";
  if (lower.endsWith(".png")) return "image/png";
  if (lower.endsWith(".jpg") || lower.endsWith(".jpeg")) return "image/jpeg";
  if (lower.endsWith(".webp")) return "image/webp";
  if (lower.endsWith(".bmp")) return "image/bmp";
  if (lower.endsWith(".tif") || lower.endsWith(".tiff")) return "image/tiff";
  if (lower.endsWith(".gif")) return "image/gif";
  if (lower.endsWith(".txt")) return "text/plain";
  if (lower.endsWith(".md")) return "text/markdown";
  return "application/octet-stream";
}

function mimeFilters(accept: "pdf" | "image" | "any" | "document"): string[] {
  if (accept === "pdf") return ["application/pdf"];
  if (accept === "image") return ["image/*"];
  // `any` is the Home picker and `document` the AI assistant: both accept the
  // office formats the open-with intent filter accepts.
  return [...OFFICE_MIME_TYPES, "image/*"];
}

/**
 * Office formats the Android shell accepts. Kept in sync with the intent
 * filters in AndroidManifest.xml and the whitelist in MainActivity.kt so an
 * open-with intent never produces a file the app cannot route.
 */
export const OFFICE_EXTENSIONS: readonly string[] = [
  "docx",
  "docm",
  "dotx",
  "doc",
  "dot",
  "odt",
  "rtf",
  "txt",
  "md",
  "html",
  "xlsx",
  "xlsm",
  "xls",
  "ods",
  "csv",
  "tsv",
  "pptx",
  "pptm",
  "ppt",
  "odp",
  "osed",
  "ospr",
  "osdt",
  "oswk",
];

/**
 * MIME types for {@link OFFICE_EXTENSIONS}. The last entry is a catch-all so
 * providers that report a generic type still offer the app. PDF is listed
 * explicitly because the vault imports PDFs through the same picker.
 */
const OFFICE_MIME_TYPES: readonly string[] = [
  "application/pdf",
  "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
  "application/vnd.oasis.opendocument.text",
  "application/rtf",
  "application/msword",
  "application/vnd.ms-powerpoint",
  "text/plain",
  "text/markdown",
  "text/html",
  "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
  "application/vnd.oasis.opendocument.spreadsheet",
  "text/csv",
  "text/tab-separated-values",
  "application/vnd.openxmlformats-officedocument.presentationml.presentation",
  "application/vnd.oasis.opendocument.presentation",
  "application/octet-stream",
];

/** Copies picked SAF URIs into a fresh cache directory and returns real paths. */
async function importPickedUris(picked: AndroidFs.FsUri[], fallbackName: string): Promise<string[]> {
  if (!picked.length) return [];

  const cache = await appCacheDir();
  const directory = await join(cache, "imports", uid("import"));
  await invoke("ensure_dir", { path: directory });

  const taken = new Set<string>();
  const paths: string[] = [];
  for (const uri of picked) {
    let name = fallbackName;
    try {
      name = sanitizeName(await AndroidFs.getName(uri));
    } catch {
      name = fallbackName;
    }
    const destination = await join(directory, uniqueName(name, taken));
    await AndroidFs.copyFile(uri, destination, { create: true });
    paths.push(destination);
  }
  return paths;
}

/** Picks documents through the system picker and imports them as local paths. */
export async function pickAndroidFiles(options: {
  multiple: boolean;
  accept: "pdf" | "image" | "any" | "document";
}): Promise<string[]> {
  const picked = await AndroidFs.showOpenFilePicker({
    multiple: options.multiple,
    mimeTypes: mimeFilters(options.accept),
    localOnly: true,
  });
  return importPickedUris(picked, "document.pdf");
}

/**
 * Picks documents with an explicit MIME allow-list. The picked files are
 * copied into the app cache and returned as ordinary paths (content:// URIs
 * never leave this module).
 */
export async function pickAndroidFilesWithMime(mimeTypes: string[], multiple = true): Promise<string[]> {
  const picked = await AndroidFs.showOpenFilePicker({
    multiple,
    mimeTypes,
    localOnly: true,
  });
  return importPickedUris(picked, "document");
}

/**
 * Picks office documents for the workspace. The office MIME list is offered
 * first and a catch-all wildcard acts as a fallback so providers that do not
 * describe their files precisely are still usable.
 */
export async function pickOfficeFiles(multiple = true): Promise<string[]> {
  return pickAndroidFilesWithMime([...OFFICE_MIME_TYPES, "*/*"], multiple);
}

/**
 * Picks a PKCS#12 certificate for the signature tools and imports it into the
 * app cache. Returns the local path plus the sanitized display name, or null
 * when the user cancels.
 */
export async function pickCertificatePfx(): Promise<{ path: string; name: string } | null> {
  const picked = await AndroidFs.showOpenFilePicker({
    multiple: false,
    mimeTypes: ["application/x-pkcs12", "*/*"],
    localOnly: true,
  });
  if (!picked.length) return null;
  const [path] = await importPickedUris(picked.slice(0, 1), "certificate.p12");
  if (!path) return null;
  return { path, name: fileBaseName(path) };
}

export interface AndroidTarget {
  uri: AndroidFs.FsUri;
  name: string;
}

/** Picks a destination file (SAF "save as") for a single output. */
export async function pickAndroidSaveTarget(defaultName: string, mimeType?: string): Promise<AndroidTarget | null> {
  const uri = await AndroidFs.showSaveFilePicker(defaultName, mimeType ?? mimeForName(defaultName));
  if (!uri) return null;
  const name = await AndroidFs.getName(uri).catch(() => defaultName);
  return { uri, name };
}

/** Picks a destination folder (SAF) for tools that produce several files. */
export async function pickAndroidFolder(): Promise<AndroidTarget | null> {
  const uri = await AndroidFs.showOpenDirPicker();
  if (!uri) return null;
  const name = await AndroidFs.getName(uri).catch(() => "folder");
  return { uri, name };
}

async function createPublicDownload(name: string, mimeType: string): Promise<AndroidFs.FsUri> {
  const taken = new Set<string>();
  let lastError: unknown = null;
  for (let attempt = 0; attempt < 25; attempt += 1) {
    const candidate = uniqueName(name, taken);
    try {
      return await AndroidFs.createNewPublicFile(
        AndroidFs.PublicGeneralPurposeDir.Download,
        `OmniOffice/${candidate}`,
        mimeType,
        { isPending: true },
      );
    } catch (error) {
      lastError = error;
    }
  }
  throw lastError ?? new Error("could not create a destination file");
}

async function createInDir(dirUri: AndroidFs.FsUri, name: string, mimeType: string): Promise<AndroidFs.FsUri> {
  const taken = new Set<string>();
  let lastError: unknown = null;
  for (let attempt = 0; attempt < 25; attempt += 1) {
    const candidate = uniqueName(name, taken);
    try {
      return await AndroidFs.createNewFile(dirUri, candidate, mimeType);
    } catch (error) {
      lastError = error;
    }
  }
  throw lastError ?? new Error("could not create a destination file");
}

export interface PublishTarget {
  /** A single file chosen with the save dialog. */
  file?: AndroidTarget | null;
  /** A folder chosen with the directory picker. */
  dir?: AndroidTarget | null;
}

/** A document that could not be published, with a human readable reason. */
export interface PublishFailure {
  path: string;
  error: string;
}

function describeError(error: unknown): string {
  if (error instanceof Error && error.message) return error.message;
  return String(error);
}

/**
 * Copies finished documents to a user-visible location and remembers the
 * resulting content URIs so they can be opened or shared afterwards.
 * Without an explicit target the documents land in
 * `Downloads/OmniOffice`.
 *
 * On Android 9 and older the app cannot write into public directories itself
 * (the storage permission bridge lives on the native side and is not compiled
 * in), so publishing falls back to the system save dialog, one document at a
 * time. Failures - including a cancelled dialog - are returned instead of
 * being logged, so the caller can show them.
 */
export async function publishOutputs(paths: string[], target?: PublishTarget): Promise<PublishFailure[]> {
  const failures: PublishFailure[] = [];
  if (!isAndroid() || !paths.length) return failures;

  const apiLevel = await AndroidFs.getAndroidApiLevel().catch(() => 29);
  const legacyStorage = apiLevel < 29;

  for (const path of paths) {
    const name = fileBaseName(path);
    const mimeType = mimeForName(name);
    try {
      let uri: AndroidFs.FsUri;
      if (target?.file) {
        uri = target.file.uri;
        await AndroidFs.copyFile(path, uri, { create: false });
      } else if (target?.dir) {
        uri = await createInDir(target.dir.uri, name, mimeType);
        await AndroidFs.copyFile(path, uri, { create: false });
      } else if (legacyStorage) {
        // API 24-28: ask the user where the file should go.
        const picked = await pickAndroidSaveTarget(name, mimeType);
        if (!picked) {
          failures.push({ path, error: "Save cancelled: Android 9 and older need a destination for every export." });
          continue;
        }
        uri = picked.uri;
        await AndroidFs.copyFile(path, uri, { create: false });
      } else {
        uri = await createPublicDownload(name, mimeType);
        await AndroidFs.copyFile(path, uri, { create: false });
        await AndroidFs.setPublicFilePending(uri, false).catch(() => undefined);
        await AndroidFs.scanPublicFile(uri).catch(() => undefined);
      }
      publishedUris.set(path, uri);
    } catch (error) {
      failures.push({ path, error: describeError(error) });
    }
  }
  return failures;
}

/**
 * Android replacement for the desktop "save" dialog: asks for a destination
 * and writes text straight into it. Returns false when the user cancels.
 */
export async function saveTextOnAndroid(text: string, defaultName: string): Promise<boolean> {
  const target = await pickAndroidSaveTarget(defaultName, mimeForName(defaultName));
  if (!target) return false;
  await AndroidFs.writeTextFile(target.uri, text);
  publishedUris.set(defaultName, target.uri);
  return true;
}

/**
 * Android replacement for the desktop "save" dialog: asks for a destination
 * and copies an existing document into it. Returns the chosen name or null.
 */
export async function saveFileOnAndroid(sourcePath: string, defaultName?: string): Promise<string | null> {
  const name = defaultName ?? fileBaseName(sourcePath);
  const target = await pickAndroidSaveTarget(name);
  if (!target) return null;
  await AndroidFs.copyFile(sourcePath, target.uri, { create: false });
  publishedUris.set(sourcePath, target.uri);
  return target.name;
}

/**
 * Copies an updated document over the destination that was chosen when it was
 * first published. Returns false when the document has no remembered
 * destination (first save) or the copy fails.
 */
export async function updatePublishedOutput(sourcePath: string): Promise<boolean> {
  const target = publishedUris.get(sourcePath);
  if (!target) return false;
  try {
    await AndroidFs.copyFile(sourcePath, target.uri, { create: false });
    return true;
  } catch {
    return false;
  }
}

/** Opens a document with the system viewer (Android) or default app. */
export async function openAnyFile(path: string): Promise<void> {
  if (!isAndroid()) {
    // Goes through the validated Rust command: the opener plugin permission is
    // not granted to the webview, so only document types the app produces can
    // reach the OS default handler.
    await invoke("open_document_file", { path });
    return;
  }
  let uri = publishedUris.get(path);
  if (!uri) {
    await publishOutputs([path]);
    uri = publishedUris.get(path);
  }
  if (uri) {
    await AndroidFs.showViewFileAppChooser(uri);
  }
}

/** Shares a document with another app (Android) or reveals it in the folder. */
export async function revealAnyFile(path: string): Promise<void> {
  if (!isAndroid()) {
    await invoke("reveal_document_file", { path });
    return;
  }
  let uri = publishedUris.get(path);
  if (!uri) {
    await publishOutputs([path]);
    uri = publishedUris.get(path);
  }
  if (uri) {
    await AndroidFs.showShareFileAppChooser(uri);
  }
}
