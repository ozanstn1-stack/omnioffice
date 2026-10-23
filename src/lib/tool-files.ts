/**
 * File-list helpers for tool sessions: the accepted categories, the dialog
 * extensions and the mapping from picked paths/tokens to `SelectedFile`.
 * Kept apart from `useTool` so the hook stays under its line budget and the
 * mapping is unit-testable.
 */
import { pickDocumentTokens, type FileToken } from "./file-tokens";
import { fileBaseName, isImage, isOfficeDocument, isPdf, officeDocumentExtensions } from "./format";
import type { SelectedFile } from "./types";

export type ToolAccept = "pdf" | "image" | "document" | "any";

/** True when a file name belongs to the tool's category. */
export function acceptsFile(accept: ToolAccept): (name: string) => boolean {
  return (name) => {
    if (accept === "pdf") return isPdf(name);
    if (accept === "image") return isImage(name);
    if (accept === "document") return isPdf(name) || isImage(name) || isOfficeDocument(name);
    return isPdf(name) || isImage(name);
  };
}

/** Extensions for the native dialog in the tool's category. */
export function extensionsFor(accept: ToolAccept): string[] {
  if (accept === "pdf") return ["pdf"];
  if (accept === "image") return ["jpg", "jpeg", "png", "webp", "bmp", "tif", "tiff"];
  if (accept === "document") {
    return [...new Set([...officeDocumentExtensions(), "pdf", "jpg", "jpeg", "png", "webp", "bmp", "tif", "tiff"])];
  }
  return ["pdf", "jpg", "jpeg", "png", "webp", "bmp", "tif", "tiff"];
}

/** Opens the Rust dialog and returns tokens (empty when cancelled). */
export async function pickDesktopFiles(accept: ToolAccept, multiple: boolean): Promise<FileToken[]> {
  return pickDocumentTokens(extensionsFor(accept), multiple).catch(() => []);
}

export function tokenEntries(refs: FileToken[], sizes: Array<number | null>): SelectedFile[] {
  return refs.map((ref, index) => ({
    path: ref.token,
    token: ref.token,
    name: ref.name,
    sizeBytes: sizes[index] ?? 0,
  }));
}

export function pathEntries(
  paths: string[],
  sizes: Array<number | null>,
  names?: Array<string | null>,
): SelectedFile[] {
  return paths.map((path, index) => ({
    path,
    name: names?.[index] ?? fileBaseName(path),
    sizeBytes: sizes[index] ?? 0,
  }));
}
