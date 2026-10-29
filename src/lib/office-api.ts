/**
 * The office commands. Their argument and result types are generated from
 * the Rust structs (tauri-specta, #316); the document models stay opaque
 * JSON (`specta.Value`) and the editors cast them with the helpers below.
 */
import type * as specta from "./generated/specta";
import { call, callPlain } from "./typed-call";
import type { Deck, OfficeKind, TextDocument, Workbook } from "./office-types";

/** `legacy`: a .doc/.ppt import, saved to a modern destination first;
 *  `token`: the write token for an in-place save (empty for legacy imports). */
// The model is opaque JSON here; the editors read it through the casts below.
export type OpenDocumentResult = Omit<specta.OpenDocument_Serialize, "kind" | "model"> & {
  kind: OfficeKind;
  model: unknown;
};

export type SaveDocumentResult = specta.SaveDocument;

export type CleanOptions = specta.CleanOptions_Deserialize;

export type CleanResult = specta.CleanResult_Serialize;

export type ConversionInfo = specta.ConversionInfo;

export type HistoryEntry = specta.HistoryEntry;

export type RecoveryEntry = specta.RecoveryEntry_Serialize;

export type PdfFormField = specta.PdfFormField_Serialize;

export const openDocument = (path: string) =>
  call((c) => c.officeOpenDocument(path)).then((document) => document as OpenDocumentResult);

export const saveDocument = (kind: OfficeKind, model: unknown, path: string) =>
  call((c) => c.officeSaveDocument(kind, model as specta.Value, path));

export const saveUnit = (kind: OfficeKind, title: string, model: unknown, path: string) =>
  call((c) => c.officeSaveUnit(kind, title, model as specta.Value, path));

export const exportPdf = (kind: OfficeKind, model: unknown, path: string) =>
  call((c) => c.officeExportPdf(kind, model as specta.Value, path));

/** Exports straight to PDF/A ("2b" by default); the report says whether the file validates. */
export const exportPdfa = (kind: OfficeKind, model: unknown, path: string, level = "2b") =>
  call((c) => c.officeExportPdfa(kind, model as specta.Value, path, level));

export const convertFile = (input: string, output: string, options?: specta.ConvertOptions_Deserialize) =>
  call((c) => c.officeConvert(input, output, options ?? null));

export const conversionTargets = (extension: string) => callPlain((c) => c.officeConversionTargets(extension));

export const cleanDocument = (path: string, options: CleanOptions) => call((c) => c.officeClean(path, options));

export const imageFootprint = (path: string) => call((c) => c.officeImageFootprint(path));

export interface FileFingerprint {
  exists: boolean;
  size: number;
  modifiedMs: number;
  sha256: string;
}

/** Fingerprints a local file so a save can detect an external change. */
export const fileFingerprint = (path: string) => call((c) => c.fileFingerprint(path));

export const storeLoad = <T>(key: string) => call((c) => c.storeLoad(key)).then((value) => value as T | null);

export const storeSave = async (key: string, value: unknown) => {
  await call((c) => c.storeSave(key, value as specta.Value));
};

export const storeClear = async (key: string) => {
  await call((c) => c.storeClear(key));
};

export const historyPush = (documentId: string, kind: string, title: string, model: unknown) =>
  call((c) => c.historyPush(documentId, kind, title, model as specta.Value));

export const historyList = (documentId: string) => call((c) => c.historyList(documentId));

export const historyLoad = (documentId: string, version: number): Promise<unknown> =>
  call((c) => c.historyLoad(documentId, version));

export const historyClear = async (documentId: string) => {
  await call((c) => c.historyClear(documentId));
};

export const recoverySave = async (
  documentId: string,
  kind: string,
  title: string,
  path: string | null,
  model: unknown,
) => {
  await call((c) => c.recoverySave(documentId, kind, title, path, model as specta.Value));
};

/** Autosave with images moved to the content-addressed blob store. */
export const recoverySaveBlobs = async (
  documentId: string,
  kind: string,
  title: string,
  path: string | null,
  model: unknown,
  blobs: Record<string, string>,
) => {
  await call((c) => c.recoverySaveBlobs(documentId, kind, title, path, model as specta.Value, blobs));
};

/** Deletes image blobs no recovery snapshot references any more. */
export const recoveryGcBlobs = () => call((c) => c.recoveryGcBlobs());

export const recoveryList = () => call((c) => c.recoveryList());

export const recoveryLoad = (documentId: string): Promise<unknown> => call((c) => c.recoveryLoad(documentId));

export const recoveryDiscard = async (documentId: string) => {
  await call((c) => c.recoveryDiscard(documentId));
};

export const recoveryDiscardAll = async () => {
  await call((c) => c.recoveryDiscardAll());
};

export const imagesToPdf = (request: specta.OfficeImagesToPdfRequest_Deserialize) =>
  call((c) => c.officeImagesToPdf(request));

export const pdfToImages = (request: specta.OfficePdfToImagesRequest_Deserialize) =>
  call((c) => c.officePdfToImages(request));

export const pdfToText = (input: string, output: string, password?: string) =>
  call((c) => c.officePdfToText({ input, output, password }));

export const pdfAddForm = (request: specta.PdfFormRequest_Deserialize) => call((c) => c.officePdfAddForm(request));

export const pdfListForm = (input: string) => call((c) => c.officePdfListForm(input));

/** Cast helpers so the editors stay type-safe after a generic open. */
export const asWriterModel = (model: unknown) => model as TextDocument;
export const asCalcModel = (model: unknown) => model as Workbook;
export const asDeckModel = (model: unknown) => model as Deck;

// ---------------------------------------------------------------------------
// e-Fatura / e-Arşiv (UBL-TR)
// ---------------------------------------------------------------------------

export type UblParty = specta.UblParty_Serialize;

export type UblTaxSubtotal = specta.UblTaxSubtotal_Serialize;

export type UblLine = specta.UblLine_Serialize;

export type UblInvoice = specta.UblInvoice_Serialize;

export type UblIssue = specta.UblIssue_Serialize;

export type InvoiceDocument = Omit<specta.InvoiceDocument_Serialize, "kind" | "model"> & {
  kind: OfficeKind;
  model: unknown;
};

export type InvoiceSummary = Omit<specta.InvoiceSummary, "workbook"> & { workbook: Workbook };

export type VatRate = specta.VatRate;

/** Opens a `.xml`/`.zip` UBL-TR invoice as a Writer document plus its fields. */
export const openInvoice = (path: string) =>
  call((c) => c.officeOpenInvoice(path)).then((invoice) => invoice as InvoiceDocument);

/** Every invoice in one `.xml`/`.zip` file (a zip can hold several). */
export const openInvoicePackage = (path: string) =>
  call((c) => c.officeOpenInvoicePackage(path)).then((invoices) => invoices as InvoiceDocument[]);

/** Reads every invoice in the selection and builds the VAT summary workbook. */
export const invoiceSummary = (paths: string[]) =>
  // The summary workbook is an opaque model on the Rust side.
  call((c) => c.officeInvoiceSummary(paths)).then((summary) => summary as unknown as InvoiceSummary);

/** The dated reference VAT-rate table. */
export const vatRates = () => callPlain((c) => c.officeVatRates());

/**
 * The GİB karekod (e-Fatura QR) payload and its reader result. Both types are
 * generated from the Rust structs (`npm run types:gen`), so a field rename on
 * the Rust side fails the TypeScript build instead of arriving as `undefined`.
 */
import type { InvoiceQr } from "./generated/InvoiceQr";
export type { GibQr } from "./generated/GibQr";
export type { InvoiceQr } from "./generated/InvoiceQr";

/** Reads the GİB karekod from a PDF (first pages, or a specific page). */
export const readInvoiceQr = (path: string, page?: number): Promise<InvoiceQr | null> =>
  call((c) => c.officeReadInvoiceQr(path, page ?? null));

// ---------------------------------------------------------------------------
// KEP .eml (read-only)
// ---------------------------------------------------------------------------

export type EmlHeader = specta.EmlHeader;

export type EmlAttachment = specta.EmlAttachment_Serialize;

export type EmlMessage = specta.EmlMessage_Serialize;

/** Opens a KEP `.eml` message read-only and extracts its attachments. */
export const openEml = (path: string) => call((c) => c.officeOpenEml(path));

// ---------------------------------------------------------------------------
// Document scanner (Android camera path)
// ---------------------------------------------------------------------------

export interface ScanPage {
  path: string;
  /** The four page corners picked on the upright photo, in its pixels; omitted to use the whole photo. */
  corners?: [number, number][];
}

export type ScanResult = specta.ScanResult;

/** Corrects the perspective of each captured page and assembles a multi-page PDF. */
export const scanPages = (pages: ScanPage[], output: string) => call((c) => c.officeScanPages({ pages, output }));

/** An upright, downscaled page photo for the corner picker; `width`/`height` are the full-size pixels. */
export type ScanPreview = specta.ScanPreview;

export const scanPreview = (path: string) => call((c) => c.officeScanPreview(path));

// ---------------------------------------------------------------------------
// Address mail merge
// ---------------------------------------------------------------------------

export type MailMergeError = specta.MailMergeError;

export type MailMergeReport = specta.MailMergeReport;

/** Fills a Writer template's {{alan}} placeholders from a CSV and writes one document per row. */
export const mailMerge = (request: {
  template: string;
  data: string;
  outputDir: string;
  format: "pdf" | "docx";
  nameField?: string;
}) => call((c) => c.officeMailMerge({ ...request, nameField: request.nameField || null }));

// ---------------------------------------------------------------------------
// Turkish spell checker (v5.3)
// ---------------------------------------------------------------------------

export type SpellCheckMisspelling = specta.SpellCheckMisspelling;

export type SpellCheckResponse = specta.SpellCheckResponse;

/** Checks Turkish text using the offline Zemberek dictionary + morphological analyzer. */
export const checkTurkishText = (text: string, suggestionsLimit = 4) =>
  callPlain((c) => c.turkishSpellcheck(text, suggestionsLimit));

/** Returns suggestions for a single misspelled word. */
export const getTurkishSuggestions = (word: string, limit = 5) => callPlain((c) => c.turkishSuggestions(word, limit));
