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

/** Autosave with images moved to the content-addressed blob store. */
export const recoverySaveBlobs = (
  documentId: string,
  kind: string,
  title: string,
  path: string | null,
  model: unknown,
  blobs: Record<string, string>,
) => invoke<void>("recovery_save_blobs", { documentId, kind, title, path, model, blobs });

/** Deletes image blobs no recovery snapshot references any more. */
export const recoveryGcBlobs = () => invoke<number>("recovery_gc_blobs");

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

// ---------------------------------------------------------------------------
// e-Fatura / e-Arşiv (UBL-TR)
// ---------------------------------------------------------------------------

export interface UblParty {
  name: string;
  taxId: string;
  taxIdScheme: string;
  taxOffice: string;
  address: string;
  city: string;
}

export interface UblTaxSubtotal {
  rate: number;
  taxable: number;
  tax: number;
  name: string;
  exemptionReason: string;
}

export interface UblLine {
  id: string;
  name: string;
  quantity: number;
  unit: string;
  unitPrice: number;
  lineTotal: number;
  vatRate: number;
  vatAmount: number;
  withholdingRate: number;
  withholdingAmount: number;
}

export interface UblInvoice {
  number: string;
  ettn: string;
  issueDate: string;
  issueTime: string;
  typeCode: string;
  profile: string;
  currency: string;
  supplier: UblParty;
  customer: UblParty;
  lines: UblLine[];
  taxSubtotals: UblTaxSubtotal[];
  withholdingSubtotals: UblTaxSubtotal[];
  lineExtensionAmount: number;
  taxExclusiveAmount: number;
  taxInclusiveAmount: number;
  allowanceTotal: number;
  taxAmount: number;
  withholdingAmount: number;
  payableAmount: number;
  notes: string[];
}

export interface UblIssue {
  field: string;
  expected: number;
  found: number;
  message: string;
}

export interface InvoiceDocument {
  kind: OfficeKind;
  title: string;
  model: unknown;
  invoice: UblInvoice;
  warnings: string[];
  issues: UblIssue[];
}

export interface InvoiceSummary {
  workbook: Workbook;
  warnings: string[];
  count: number;
}

export interface VatRate {
  rate: number;
  label: string;
  since: string;
  until: string;
}

/** Opens a `.xml`/`.zip` UBL-TR invoice as a Writer document plus its fields. */
export const openInvoice = (path: string) => invoke<InvoiceDocument>("office_open_invoice", { path });

/** Reads every invoice in the selection and builds the VAT summary workbook. */
export const invoiceSummary = (paths: string[]) => invoke<InvoiceSummary>("office_invoice_summary", { paths });

/** The dated reference VAT-rate table. */
export const vatRates = () => invoke<VatRate[]>("office_vat_rates");

/** The GİB karekod (e-Fatura QR) payload. */
export interface GibQr {
  vknTckn: string;
  avknTckn: string;
  senaryo: string;
  tip: string;
  tarih: string;
  no: string;
  ettn: string;
  paraBirimi: string;
  malHizmetToplam: string;
  vergiDahil: string;
  odenecek: string;
  kdvMatrah: Record<string, string>;
  hesaplananKdv: Record<string, string>;
}

export interface InvoiceQr {
  payload: GibQr;
  raw: string;
  page: number;
  /** The payload's sender VKN/TCKN passed its checksum. */
  taxIdValid: boolean;
  warnings: string[];
}

/** Reads the GİB karekod from a PDF (first pages, or a specific page). */
export const readInvoiceQr = (path: string, page?: number) =>
  invoke<InvoiceQr | null>("office_read_invoice_qr", { path, page: page ?? null });

// ---------------------------------------------------------------------------
// KEP .eml (read-only)
// ---------------------------------------------------------------------------

export interface EmlHeader {
  name: string;
  value: string;
}

export interface EmlAttachment {
  name: string;
  contentType: string;
  size: number;
  /** Where the attachment was extracted, when it was written to disk. */
  path?: string;
  dataBase64?: string;
}

export interface EmlMessage {
  subject: string;
  from: string;
  to: string;
  cc: string;
  date: string;
  messageId: string;
  headers: EmlHeader[];
  bodyText: string;
  bodyHtml: string;
  attachments: EmlAttachment[];
  warnings: string[];
}

/** Opens a KEP `.eml` message read-only and extracts its attachments. */
export const openEml = (path: string) => invoke<EmlMessage>("office_open_eml", { path });

// ---------------------------------------------------------------------------
// Document scanner (Android camera path)
// ---------------------------------------------------------------------------

export interface ScanPage {
  path: string;
  /** The four page corners the user tapped, in source pixels; omitted to use the image as it is. */
  corners?: [number, number][];
}

export interface ScanResult {
  output: string;
  pages: number;
  corrected: number;
  warnings: string[];
}

/** Corrects the perspective of each captured page and assembles a multi-page PDF. */
export const scanPages = (pages: ScanPage[], output: string) =>
  invoke<ScanResult>("office_scan_pages", { request: { pages, output } });

// ---------------------------------------------------------------------------
// Address mail merge
// ---------------------------------------------------------------------------

export interface MailMergeError {
  row: number;
  message: string;
}

export interface MailMergeReport {
  produced: string[];
  errors: MailMergeError[];
}

/** Fills a Writer template's {{alan}} placeholders from a CSV and writes one document per row. */
export const mailMerge = (request: {
  template: string;
  data: string;
  outputDir: string;
  format: "pdf" | "docx";
  nameField?: string;
}) => invoke<MailMergeReport>("office_mail_merge", { request: { ...request, nameField: request.nameField || null } });
