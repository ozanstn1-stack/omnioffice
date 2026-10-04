// Types mirrored from the Rust command layer (serde camelCase).

export interface AppError {
  code: string;
  message: string;
}

export interface EngineStatus {
  pdfium: boolean;
  qpdf: boolean;
  tesseract: boolean;
  tesseract_version: string | null;
  ocr_languages: string[];
}

export interface OcrLanguage {
  code: string;
  name: string;
}

export interface AppInfo {
  appVersion: string;
  coreVersion: string;
  platform: string;
  name: string;
}

export interface PageGeometry {
  page: number;
  width_pt: number;
  height_pt: number;
  display_width_pt: number;
  display_height_pt: number;
  rotation: number;
}

export interface PdfMetadata {
  title: string;
  author: string;
  subject: string;
  keywords: string;
  creator: string;
  producer: string;
  creation_date: string;
  mod_date: string;
}

export interface PdfInfo {
  path: string;
  fileName: string;
  fileSizeBytes: number;
  pageCount: number;
  pdfVersion: string;
  encrypted: boolean;
  hasTextLayer: boolean;
  metadata: PdfMetadata;
  pageGeometries: PageGeometry[];
  imageCount: number;
  title: string;
  author: string;
  producer: string;
  /** Document bookmarks; entries whose destination could not be resolved have page 0. */
  outline: OutlineEntry[];
}

export interface Thumbnail {
  dataUrl: string;
  width: number;
  height: number;
}

export type OverwriteMode = "error" | "replace" | "unique_name";

export interface OutputSpec {
  path: string;
  overwrite?: OverwriteMode;
}

export interface OpResult {
  path: string;
  pageCount?: number;
  originalBytes?: number;
  outputBytes?: number;
  reduction?: number;
  message?: string;
}

export interface ProgressPayload {
  jobId: string;
  stage: string;
  current: number;
  total: number;
  message?: string;
}

export interface RecentEntry {
  path: string;
  fileName: string;
  tool: string;
  timestamp: number;
}

export interface SplitPart {
  path: string;
  first_page: number;
  last_page: number;
}

export interface ImageOutput {
  path: string;
  page: number;
  width: number;
  height: number;
  bytes: number;
}

export interface CompressEstimate {
  original_bytes: number;
  estimated_bytes: number;
  page_count: number;
  reduction: number;
  method: string;
  sample_pages: number;
  accurate: boolean;
}

export interface CompressOptions {
  strategy: "lossless" | "raster";
  preset: "low" | "medium" | "high" | "custom";
  dpi: number;
  jpeg_quality: number;
  grayscale: boolean;
  remove_metadata: boolean;
}

export interface OcrPreprocess {
  auto_rotate: boolean;
  deskew: boolean;
  contrast: boolean;
  denoise: boolean;
  binarize: boolean;
  grayscale: boolean;
}

export interface OcrOptions {
  languages: string[];
  psm: number;
  dpi: number;
  output_mode: "searchable_pdf" | "text" | "markdown";
  pages: number[];
  preprocess: OcrPreprocess;
  skip_text_pages: boolean;
}

export type WatermarkPosition =
  "top_left" | "top_center" | "top_right" | "center" | "bottom_left" | "bottom_center" | "bottom_right";

export interface WatermarkOptions {
  kind: "text" | "image";
  text: string;
  font_size_pt: number;
  bold: boolean;
  color: string;
  opacity: number;
  rotation_deg: number;
  position: WatermarkPosition;
  margin_pt: number;
  tile: boolean;
  image_path: string | null;
  image_scale: number;
  pages: number[];
}

export interface NumberingOptions {
  position: WatermarkPosition;
  format: "n" | "page_n" | "n_of_total" | "page_n_of_total";
  start_number: number;
  font_size_pt: number;
  color: string;
  margin_pt: number;
  pages: number[];
  count_from_start: boolean;
}

export interface ResizeOptions {
  page_size: string;
  custom_width_pt: number;
  custom_height_pt: number;
  orientation: "auto" | "portrait" | "landscape";
  mode: "fit" | "stretch";
  pages: number[];
}

export interface CropItem {
  page: number;
  x: number;
  y: number;
  w: number;
  h: number;
}

export type AnnotationKind = "text" | "image" | "rect" | "highlight" | "line";

export interface Annotation {
  kind: AnnotationKind;
  page: number;
  x: number;
  y: number;
  w: number;
  h: number;
  text: string;
  font_size_pt: number;
  bold: boolean;
  color: string;
  opacity: number;
  image_path: string | null;
  line_width_pt: number;
  x2: number | null;
  y2: number | null;
}

export interface ImageToPdfOptions {
  page_size: string;
  custom_width_pt: number;
  custom_height_pt: number;
  orientation: "auto" | "portrait" | "landscape";
  fit: "fit" | "fill" | "actual";
  margin_pt: number;
  dpi: number;
  jpeg_quality: number;
}

export interface ImageItem {
  path: string;
  rotation_delta: number;
}

export interface PagePlanItem {
  source_page: number;
  rotation_delta: number;
}

export interface ProtectOptions {
  userPassword: string;
  ownerPassword: string;
  allowPrinting: boolean;
  allowCopying: boolean;
  allowEditing: boolean;
  allowCommenting: boolean;
}

export type SplitMode =
  | { mode: "ranges"; ranges: string[] }
  | { mode: "every_n"; n: number }
  | { mode: "individual" }
  | { mode: "at_pages"; pages: number[] };

export interface Settings {
  theme: "dark" | "light" | "system" | "midnight" | "paper";
  language: "en" | "tr";
  defaultOutputDir: string;
  defaultCompression: "low" | "medium" | "high";
  defaultImageDpi: number;
  ocrLanguages: string[];
  autoCleanupTemp: boolean;
  showRecentFiles: boolean;
  defaultExportFormat: "jpg" | "png";
  /** Automatically store every AI result in the library. */
  aiAutoSave: boolean;
  /** AI library folder; empty means Documents/PDF Swiss Army Knife AI. */
  aiLibraryDir: string;
  /** Keep an operation log (paths and sizes only). */
  keepOperationLog: boolean;
  /** Autosave interval for office documents: 0 = off. */
  autosaveSeconds: number;
  /** Keep local version history snapshots on save. */
  versionHistory: boolean;
  /** Default save format when creating a new Writer document. */
  defaultWriterFormat: "docx" | "odt";
  /** Default save format when creating a new Calc workbook. */
  defaultCalcFormat: "xlsx" | "ods";
  /** Default save format when creating a new Impress deck. */
  defaultImpressFormat: "pptx" | "odp";
  /** Show the unsupported-feature warnings after opening a document. */
  showImportWarnings: boolean;
}

export const DEFAULT_SETTINGS: Settings = {
  theme: "dark",
  language: "en",
  defaultOutputDir: "",
  defaultCompression: "medium",
  defaultImageDpi: 150,
  ocrLanguages: ["eng"],
  autoCleanupTemp: true,
  showRecentFiles: true,
  defaultExportFormat: "jpg",
  aiAutoSave: true,
  aiLibraryDir: "",
  keepOperationLog: true,
  autosaveSeconds: 30,
  versionHistory: true,
  defaultWriterFormat: "docx",
  defaultCalcFormat: "xlsx",
  defaultImpressFormat: "pptx",
  showImportWarnings: true,
};

export interface TextMatch {
  page: number;
  snippet: string;
  index_on_page: number;
}

export interface SearchResponse {
  matches: TextMatch[];
  pagesWithMatches: number;
  totalMatches: number;
  truncated: boolean;
}

// ---------------------------------------------------------------------------
// AI (DeepSeek) types
// ---------------------------------------------------------------------------

export type SummaryLength = "short" | "medium" | "detailed";
export type SummaryStyle = "paragraph" | "bullets" | "executive";

export interface SummaryOptions {
  language: string;
  length: SummaryLength;
  style: SummaryStyle;
  focus: string;
}

export interface TranslateOptions {
  /** The Rust struct is `#[serde(rename_all = "camelCase")]`. */
  targetLanguage: string;
  bilingual: boolean;
}

export type ReasoningEffort = "low" | "high" | "max";

export interface AiSettingsView {
  configured: boolean;
  keyStorage: "none" | "dpapi" | "plain" | string;
  maskedKey: string;
  baseUrl: string;
  model: string;
  temperature: number;
  maxTokens: number;
  thinking: boolean;
  reasoningEffort: ReasoningEffort | string;
  contextTokens: number;
  /** Hard API maximum for generated tokens (384K on DeepSeek V4). */
  maxOutputTokens: number;
}

export interface AiSettingsInput {
  apiKey?: string;
  baseUrl: string;
  model: string;
  temperature: number;
  maxTokens: number;
  thinking?: boolean;
  reasoningEffort?: ReasoningEffort;
  contextTokens?: number;
}

export interface AiTestResult {
  ok: boolean;
  message: string;
  model: string;
}

export interface AiPreview {
  pages: number;
  characters: number;
  sample: string;
  estimatedWords: number;
}

export interface AiTextResult {
  text: string;
  pages: number;
  characters: number;
  model: string;
  elapsedMs: number;
}

export interface AiMetadataSuggestion {
  title: string;
  author: string;
  subject: string;
  keywords: string[];
}

export interface AiModelOption {
  id: string;
  label: string;
  recommended: boolean;
}

export interface AiExamplePrompts {
  summarize: string[];
  ask: string[];
  translateTargets: string[];
}

export interface AiSummarizeRequest {
  path: string;
  options: SummaryOptions;
  pages?: number[];
  password?: string;
  jobId: string;
}

export interface AiTranslateRequest {
  path: string;
  options: TranslateOptions;
  pages?: number[];
  password?: string;
  jobId: string;
}

export interface AiAskRequest {
  path: string;
  question: string;
  password?: string;
  jobId: string;
}

export interface AiCleanupRequest {
  path: string;
  pages?: number[];
  password?: string;
  jobId: string;
}

export interface AiLibraryEntry {
  id: string;
  createdAt: number;
  kind: "summary" | "translate" | "ask" | "cleanup" | "metadata" | string;
  sourcePath: string;
  sourceName: string;
  model: string;
  pages: number;
  characters: number;
  options: string;
  filePath: string;
  preview: string;
  elapsedMs: number;
}

export interface OperationEntry {
  id: string;
  createdAt: number;
  operation: string;
  inputPath: string;
  outputPath: string;
  pageCount?: number;
  inputBytes?: number;
  outputBytes?: number;
  ok: boolean;
  detail?: string;
}

export interface SelectedFile {
  path: string;
  name: string;
  sizeBytes: number;
  pages?: number;
}

// ---------------------------------------------------------------------------
// Redaction, comparison, inspection
//
// These mirror the Rust structs one field at a time. Every one of those derives
// Serialize with `#[serde(rename_all = "camelCase")]`, so the wire format is
// camelCase; inventing a field here reads as `undefined` at runtime and, in one
// case, blanked the whole window. `crates/pdfcore/tests/dump_inspection.rs`
// prints the real payload for a real file to check this against.
// ---------------------------------------------------------------------------

/** How an image area under a redaction box is treated. */
export type ImageRedactionMode = "obscure" | "removePixels";

/** A rectangle in PDF page space: origin bottom-left, units of points. */
export interface RedactionArea {
  page: number;
  left: number;
  bottom: number;
  right: number;
  top: number;
}

export interface RedactionOptions {
  /** Fill colour as `#rrggbb`; the text is removed, not just covered. */
  fill: string;
  images: ImageRedactionMode;
  /** Slack added around matched text so glyph edges are covered too. */
  paddingPt: number;
  removeMetadata: boolean;
}

export interface RedactionMatch {
  page: number;
  text: string;
  left: number;
  bottom: number;
  right: number;
  top: number;
  /** "email" | "iban" | "card" | "phone" | "passport". */
  kind: string;
}

export interface CompareOptions {
  maxPages: number;
  /** Per-channel difference (0-255) below which pixels count as equal. */
  tolerance: number;
  dpi: number;
  visual: boolean;
  ignoreWhitespace: boolean;
  maxDifferences: number;
}

export interface TextDifference {
  page: number;
  kind: "added" | "removed" | "changed";
  left: string;
  right: string;
}

export interface VisualDifference {
  page: number;
  /** Fraction of differing pixels, 0-1. */
  difference: number;
  changedPixels: number;
  totalPixels: number;
  /** JPEG data URL of a side-by-side render with differences highlighted. */
  preview: string;
}

export interface CompareReport {
  leftPages: number;
  rightPages: number;
  removedPages: number[];
  addedPages: number[];
  textDifferences: TextDifference[];
  visualTruncated: boolean;
  visualDifferences: VisualDifference[];
  identical: boolean;
  warnings: string[];
}

export type Severity = "info" | "warning" | "error";

export interface InspectionFinding {
  severity: Severity;
  /** Stable identifier, e.g. `a11y.missing-title`. */
  code: string;
  /** Short title for the list. */
  title: string;
  /** What to do about it. */
  detail: string;
}

export interface FontInfo {
  name: string;
  subtype: string;
  /** True for a Type 0 composite font. */
  composite: boolean;
  /** True when the font program itself is in the file. */
  embedded: boolean;
  embeddedFormats: string[];
}

export interface ImageInfo {
  width: number;
  height: number;
  colorSpace: string;
  bitsPerComponent: number;
  /** "DCTDecode", "FlateDecode", "JPXDecode"... */
  filter: string;
  occurrences: number;
}

export interface ColorSpaceInfo {
  name: string;
  components: number;
  occurrences: number;
  deviceDependent: boolean;
}

export interface AnnotationInfo {
  page: number;
  subtype: string;
  /** The annotation's /Contents text, which often holds a note. */
  contents: string;
  /** True when the annotation is not displayed on the page. */
  hidden: boolean;
}

export interface FormFieldInfo {
  name: string;
  /** "Tx" (text), "Btn" (button), "Ch" (choice)... */
  kind: string;
  fieldType: string;
  readOnly: boolean;
  required: boolean;
  /** True when the field has no /TU tooltip or alternate name. */
  missingLabel: boolean;
  options: string[];
}

export interface OutlineEntry {
  title: string;
  /** Page the entry points at, 1-based; 0 when the destination is unresolved. */
  page: number;
  depth: number;
}

export interface DocumentInspection {
  path: string;
  fileSizeBytes: number;
  pdfVersion: string;
  pageCount: number;
  encrypted: boolean;
  linearized: boolean;
  objectCount: number;
  /** /StructTreeRoot is present. */
  hasStructTree: boolean;
  /** /MarkInfo is present. */
  marked: boolean;
  /** The structure tree itself parsed. */
  structTree: boolean;
  language: string;
  viewerPreferences: string;
  fonts: FontInfo[];
  images: ImageInfo[];
  colorSpaces: ColorSpaceInfo[];
  annotations: AnnotationInfo[];
  formFields: FormFieldInfo[];
  outline: OutlineEntry[];
  embeddedFiles: string[];
  hasJavascript: boolean;
  javascriptEntries: string[];
  hasOpenAction: boolean;
  hasAcroForm: boolean;
  attachmentCount: number;
  totalImagePixels: number;
  findings: InspectionFinding[];
  /** "pdf/ua" when the blocking checks pass, otherwise the first failure. */
  accessibilityConformance: string;
  titleOverride: string;
  subjectOverride: string;
  authorOverride: string;
  producer: string;
  creator: string;
}
