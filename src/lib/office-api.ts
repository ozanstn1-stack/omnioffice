/** Typed wrappers for the office commands exposed by the Rust backend. */
import { invoke } from "@tauri-apps/api/core";
import type { Deck, OfficeKind, TextDocument, Workbook } from "./office-types";

export interface OpenDocumentResult {
  kind: OfficeKind;
  title: string;
  path: string;
  model: unknown;
  warnings: string[];
  /** Legacy binary (.doc/.ppt): the tab has no save path, so the first save
   *  asks for a modern destination. */
  legacy?: boolean;
}

export interface SaveDocumentResult {
  path: string;
  warnings: string[];
}

export interface CleanOptions {
  removeMetadata: boolean;
  removeComments: boolean;
  optimizeImages: boolean;
  imageMaxPixels: number;
  imageQuality: number;
}

export interface CleanResult {
  bytesBefore: number;
  bytesAfter: number;
  actions: string[];
  warnings: string[];
}

export interface ConversionInfo {
  input: string;
  output: string;
  converted: boolean;
  warnings: string[];
}

export interface HistoryEntry {
  version: number;
  savedAt: string;
  title: string;
  kind: string;
  size: number;
}

export interface RecoveryEntry {
  documentId: string;
  kind: string;
  title: string;
}

export interface PdfFormField {
  kind: string;
  name: string;
  page: number;
  x: number;
  y: number;
  w: number;
  h: number;
  value: string;
  options: string[];
  fontSize: number | null;
  required: boolean;
}

export const openDocument = (path: string) => invoke<OpenDocumentResult>("office_open_document", { path });

export const saveDocument = (kind: OfficeKind, model: unknown, path: string) =>
  invoke<SaveDocumentResult>("office_save_document", { kind, model, path });

export const saveUnit = (kind: OfficeKind, title: string, model: unknown, path: string) =>
  invoke<SaveDocumentResult>("office_save_unit", { kind, title, model, path });

export const exportPdf = (kind: OfficeKind, model: unknown, path: string) =>
  invoke<SaveDocumentResult>("office_export_pdf", { kind, model, path });

export const convertFile = (input: string, output: string, options?: Record<string, unknown>) =>
  invoke<ConversionInfo>("office_convert", { input, output, options: options ?? null });

export const conversionTargets = (extension: string) => invoke<string[]>("office_conversion_targets", { extension });

export const cleanDocument = (path: string, options: CleanOptions) =>
  invoke<CleanResult>("office_clean", { path, options });

export const imageFootprint = (path: string) => invoke<number>("office_image_footprint", { path });

export interface FileFingerprint {
  exists: boolean;
  size: number;
  modifiedMs: number;
  sha256: string;
}

/** Fingerprints a local file so a save can detect an external change. */
export const fileFingerprint = (path: string) => invoke<FileFingerprint>("file_fingerprint", { path });

export const storeLoad = <T>(key: string) => invoke<T | null>("store_load", { key });

export const storeSave = (key: string, value: unknown) => invoke<void>("store_save", { key, value });

export const storeClear = (key: string) => invoke<void>("store_clear", { key });

export const historyPush = (documentId: string, kind: string, title: string, model: unknown) =>
  invoke<HistoryEntry>("history_push", { documentId, kind, title, model });

export const historyList = (documentId: string) => invoke<HistoryEntry[]>("history_list", { documentId });

export const historyLoad = (documentId: string, version: number) =>
  invoke<unknown>("history_load", { documentId, version });

export const historyClear = (documentId: string) => invoke<void>("history_clear", { documentId });

export const recoverySave = (documentId: string, kind: string, title: string, path: string | null, model: unknown) =>
  invoke<void>("recovery_save", { documentId, kind, title, path, model });

export const recoveryList = () => invoke<RecoveryEntry[]>("recovery_list");

export const recoveryLoad = (documentId: string) => invoke<unknown>("recovery_load", { documentId });

export const recoveryDiscard = (documentId: string) => invoke<void>("recovery_discard", { documentId });

export const recoveryDiscardAll = () => invoke<void>("recovery_discard_all");

export const imagesToPdf = (request: Record<string, unknown>) => invoke<string>("office_images_to_pdf", { request });

export const pdfToImages = (request: Record<string, unknown>) => invoke<string[]>("office_pdf_to_images", { request });

export const pdfToText = (input: string, output: string, password?: string) =>
  invoke<string>("office_pdf_to_text", { request: { input, output, password } });

export const pdfAddForm = (request: Record<string, unknown>) => invoke<string>("office_pdf_add_form", { request });

export const pdfListForm = (input: string) => invoke<PdfFormField[]>("office_pdf_list_form", { input });

/** Cast helpers so the editors stay type-safe after a generic open. */
export const asWriterModel = (model: unknown) => model as TextDocument;
export const asCalcModel = (model: unknown) => model as Workbook;
export const asDeckModel = (model: unknown) => model as Deck;
