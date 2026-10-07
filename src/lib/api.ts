import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AiAskRequest,
  AiCleanupRequest,
  AiEditRequest,
  AiEditResult,
  AiExamplePrompts,
  AiMetadataSuggestion,
  AiLibraryEntry,
  AiModelOption,
  AiPreview,
  AiSettingsInput,
  AiSettingsView,
  AiSummarizeRequest,
  AiTestResult,
  AiTextResult,
  AiTranslateRequest,
  Annotation,
  AppInfo,
  CompressEstimate,
  CompareOptions,
  CompareReport,
  CompressOptions,
  CropItem,
  DocumentInspection,
  EngineStatus,
  ImageItem,
  ImageToPdfOptions,
  NumberingOptions,
  OcrLanguage,
  OcrOptions,
  OperationEntry,
  OpResult,
  OutputSpec,
  PagePlanItem,
  PdfInfo,
  ProgressPayload,
  SearchResponse,
  ProtectOptions,
  RecentEntry,
  RedactionArea,
  UpdateInfo,
  RedactionMatch,
  RedactionOptions,
  Settings,
  SplitMode,
  Thumbnail,
  WatermarkOptions,
} from "./types";
import { trackJobInvocation } from "./job-retries";

/**
 * Every long operation goes through here: the retry metadata (kind + the exact
 * invoke args) is persisted before the work starts, so the Jobs screen can
 * re-run it after a restart. A no-op for commands without a job.
 */
function invokeTracked<T>(command: string, args: Record<string, unknown>): Promise<T> {
  trackJobInvocation(command, args);
  return invoke<T>(command, args);
}

export { invokeTracked };

/** Normalizes any thrown value into a friendly {code, message} pair. */
export function toAppError(error: unknown): { code: string; message: string } {
  if (typeof error === "string") {
    return { code: "internal", message: error };
  }
  if (error && typeof error === "object" && "code" in error && "message" in error) {
    const e = error as { code: string; message: string };
    return { code: String(e.code), message: String(e.message) };
  }
  if (error instanceof Error) {
    return { code: "internal", message: error.message };
  }
  return { code: "internal", message: String(error) };
}

// ---------------------------------------------------------------------------
// System
// ---------------------------------------------------------------------------

export const appInfo = () => invoke<AppInfo>("app_info");
export const updateCheck = () => invoke<UpdateInfo>("update_check");
export const updateOpen = (url: string) => invoke<void>("update_open", { url });
export const diagnosticsReport = () => invoke<string>("diagnostics_report");
export const engineStatus = () => invoke<EngineStatus>("engine_status");
export const ocrLanguages = () => invoke<OcrLanguage[]>("ocr_languages");
export const cancelJob = (jobId: string) => invoke<void>("cancel_job", { jobId });

export const onProgress = (handler: (payload: ProgressPayload) => void): Promise<UnlistenFn> =>
  listen<ProgressPayload>("job:progress", (event) => handler(event.payload));

// ---------------------------------------------------------------------------
// AI (DeepSeek) - the only network feature, opt-in with the user's own key
// ---------------------------------------------------------------------------

export const aiGetSettings = () => invoke<AiSettingsView>("ai_get_settings");
export const aiSaveSettings = (input: AiSettingsInput) => invoke<AiSettingsView>("ai_save_settings", { input });
/** Removes the stored key of `provider` (the saved provider when omitted). */
export const aiClearKey = (provider?: string) => invoke<AiSettingsView>("ai_clear_key", { provider: provider ?? null });
export const aiTestConnection = () => invoke<AiTestResult>("ai_test_connection");
export const aiDocumentPreview = (path: string, pages: number[] | undefined, password?: string) =>
  invoke<AiPreview>("ai_document_preview", { path, pages: pages ?? null, password: password || null });
export const aiSummarize = (request: AiSummarizeRequest) => invokeTracked<AiTextResult>("ai_summarize", { request });
export const aiTranslate = (request: AiTranslateRequest) => invokeTracked<AiTextResult>("ai_translate", { request });
export const aiAsk = (request: AiAskRequest) => invokeTracked<AiTextResult>("ai_ask", { request });
export const aiCleanupText = (request: AiCleanupRequest) => invokeTracked<AiTextResult>("ai_cleanup_text", { request });
/**
 * In-editor AI action. Deliberately NOT `invokeTracked`: the retry store
 * persists the invoke args, which here hold the user's selected document text.
 */
export const aiEditText = (request: AiEditRequest) => invoke<AiEditResult>("ai_edit_text", { request });
export const aiSuggestMetadata = (path: string, password: string | undefined, jobId: string) =>
  invokeTracked<AiMetadataSuggestion>("ai_suggest_metadata", { request: { path, password: password || null, jobId } });
export const aiSaveOutput = (path: string, text: string, overwrite?: string) =>
  invoke<string>("ai_save_output", { path, text, overwrite: overwrite ?? null });
export const aiExamplePrompts = () => invoke<AiExamplePrompts>("ai_example_prompts");
export const aiModels = (provider?: string) => invoke<AiModelOption[]>("ai_models", { provider: provider ?? null });

// ---------------------------------------------------------------------------
// AI library (saved results) and the operation log
// ---------------------------------------------------------------------------

export interface SaveAiEntryRequest {
  kind: string;
  sourcePath: string;
  sourceName: string;
  model: string;
  pages: number;
  characters: number;
  options: string;
  text: string;
  elapsedMs: number;
  directory?: string;
}

export const aiLibrarySave = (request: SaveAiEntryRequest) => invoke<AiLibraryEntry>("ai_library_save", { request });
export const aiLibraryList = () => invoke<AiLibraryEntry[]>("ai_library_list");
export const aiLibraryText = (id: string) => invoke<string>("ai_library_text", { id });
export const aiLibraryDelete = (id: string, deleteFile = true) =>
  invoke<AiLibraryEntry[]>("ai_library_delete", { id, deleteFile });
export const aiLibraryClear = (deleteFiles = true) => invoke<void>("ai_library_clear", { deleteFiles });
export const aiLibraryExport = (id: string, target: string) => invoke<string>("ai_library_export", { id, target });
export const aiLibraryDefaultDir = () => invoke<string>("ai_library_default_dir");

export const logOperation = (entry: {
  operation: string;
  inputPath: string;
  outputPath?: string;
  pageCount?: number;
  inputBytes?: number;
  outputBytes?: number;
  ok?: boolean;
  detail?: string;
}) => invoke<void>("log_operation", { entry }).catch(() => undefined);
export const loadOperations = () => invoke<OperationEntry[]>("load_operations");
export const clearOperations = () => invoke<void>("clear_operations");

export const onAiChunk = (
  handler: (payload: { jobId: string; delta: string; kind: "content" | "reasoning" }) => void,
): Promise<UnlistenFn> =>
  listen<{ jobId: string; delta: string; kind: "content" | "reasoning" }>("ai:chunk", (event) =>
    handler(event.payload),
  );

export const onAiProgress = (
  handler: (payload: { jobId: string; stage: string; current: number; total: number }) => void,
): Promise<UnlistenFn> => listen("ai:progress", (event) => handler(event.payload as never));

// ---------------------------------------------------------------------------
// Inspection
// ---------------------------------------------------------------------------

export const pdfInfo = (path: string, password?: string) =>
  invoke<PdfInfo>("pdf_info", { path, password: password || null });

export const pageThumbnail = (path: string, page: number, maxWidth = 200, password?: string) =>
  invoke<Thumbnail>("page_thumbnail", { path, page, maxWidth, password: password || null });

export const pagePreview = (
  path: string,
  page: number,
  maxWidth = 1100,
  password?: string,
  format: "png" | "jpeg" = "png",
  quality = 86,
) => invoke<Thumbnail>("page_preview", { path, page, maxWidth, password: password || null, format, quality });

/** One tile of a page rendered at `scale` output pixels per PDF point; the
 *  region is in output pixels (reader high-zoom overlay). */
export const pageTile = (
  path: string,
  page: number,
  scale: number,
  region: { x: number; y: number; width: number; height: number },
  password?: string,
  format: "png" | "jpeg" = "jpeg",
  quality = 90,
) =>
  invoke<Thumbnail>("page_tile", {
    path,
    page,
    scale,
    x: region.x,
    y: region.y,
    width: region.width,
    height: region.height,
    password: password || null,
    format,
    quality,
  });

export const pageText = (path: string, page: number, password?: string) =>
  invoke<string>("page_text", { path, page, password: password || null });

export const searchDocument = (
  path: string,
  query: string,
  matchCase: boolean,
  maxResults: number,
  password: string | undefined,
  jobId: string,
) =>
  invoke<SearchResponse>("search_document", {
    path,
    query,
    matchCase,
    maxResults,
    password: password ?? null,
    jobId,
  });

export const checkPassword = (path: string, password: string) => invoke<boolean>("check_password", { path, password });

export const outputExists = (path: string) => invoke<boolean>("output_exists", { path });

export const suggestOutput = (input: string, suffix: string) => invoke<string>("suggest_output", { input, suffix });

export const fileSizes = (paths: string[]) => invoke<(number | null)[]>("file_sizes", { paths });

export const logFrontend = (level: string, message: string) =>
  invoke<void>("log_frontend", { level, message }).catch(() => undefined);

export const devLaunchContext = () =>
  invoke<{
    startScreen: string | null;
    newTab: string | null;
    files: string[] | null;
    autoRun: boolean;
    tab: string | null;
  }>("dev_launch_context");

export const startupFiles = () => invoke<string[]>("office_startup_files");

// ---------------------------------------------------------------------------
// Operations
// ---------------------------------------------------------------------------

export const mergePdfs = (inputs: string[], output: OutputSpec, preserveMetadata: boolean, jobId: string) =>
  invokeTracked<OpResult>("merge_pdfs", { request: { inputs, output, preserveMetadata, jobId } });

interface PagesPayload {
  input: string;
  pages?: number[];
  selection?: string;
  degrees?: number;
  output: OutputSpec;
  password?: string;
  jobId: string;
}

export const extractPages = (payload: PagesPayload) => invokeTracked<OpResult>("extract_pages", { request: payload });

export const deletePages = (payload: PagesPayload) => invokeTracked<OpResult>("delete_pages", { request: payload });

export const rotatePages = (payload: PagesPayload) => invokeTracked<OpResult>("rotate_pages", { request: payload });

export const applyPagePlan = (
  input: string,
  plan: PagePlanItem[],
  output: OutputSpec,
  jobId: string,
  password?: string,
) => invokeTracked<OpResult>("apply_page_plan", { request: { input, plan, output, password, jobId } });

export const splitPdf = (
  input: string,
  mode: SplitMode,
  outputDir: string,
  overwrite: OutputSpec["overwrite"],
  jobId: string,
  password?: string,
) =>
  invokeTracked<{ parts: { path: string; first_page: number; last_page: number }[]; outputDir: string }>("split_pdf", {
    request: { input, mode, outputDir, overwrite, password, jobId },
  });

export const estimateCompression = (input: string, options: CompressOptions, password?: string) =>
  invoke<CompressEstimate>("estimate_compression", { input, options, password: password || null });

export const compressPdf = (
  input: string,
  output: OutputSpec,
  options: CompressOptions,
  jobId: string,
  password?: string,
) => invokeTracked<OpResult>("compress_pdf", { request: { input, output, options, password, jobId } });

export const ocrPdf = (input: string, output: OutputSpec, options: OcrOptions, jobId: string, password?: string) =>
  invokeTracked<OpResult>("ocr_pdf", { request: { input, output, options, password, jobId } });

export const protectPdf = (
  input: string,
  output: OutputSpec,
  options: ProtectOptions,
  jobId: string,
  password?: string,
) =>
  invokeTracked<OpResult>("protect_pdf", {
    request: { input, output, ...options, password, jobId },
  });

export const unlockPdf = (input: string, output: OutputSpec, password: string, jobId: string) =>
  invokeTracked<OpResult>("unlock_pdf", { request: { input, output, password, jobId } });

export const pdfToImages = (request: {
  input: string;
  outputDir: string;
  format: "jpeg" | "png";
  dpi: number;
  jpegQuality: number;
  grayscale: boolean;
  namePrefix: string;
  pages: number[];
  overwrite?: OutputSpec["overwrite"];
  password?: string;
  jobId: string;
}) =>
  invokeTracked<{
    files: { path: string; page: number; width: number; height: number; bytes: number }[];
    totalBytes: number;
    dpi: number;
    format: string;
  }>("pdf_to_images", { request });

export const imagesToPdf = (items: ImageItem[], output: OutputSpec, options: ImageToPdfOptions, jobId: string) =>
  invokeTracked<OpResult>("images_to_pdf", { request: { items, output, options, jobId } });

export const resizePages = (
  input: string,
  output: OutputSpec,
  options: {
    page_size: string;
    custom_width_pt: number;
    custom_height_pt: number;
    orientation: string;
    mode: string;
    pages: number[];
  },
  jobId: string,
  password?: string,
) => invokeTracked<OpResult>("resize_pages", { request: { input, output, options, password, jobId } });

export const cropPages = (input: string, output: OutputSpec, crops: CropItem[], jobId: string, password?: string) =>
  invokeTracked<OpResult>("crop_pages", { request: { input, output, crops, password, jobId } });

export const editMetadata = (
  input: string,
  output: OutputSpec,
  metadata: {
    title: string;
    author: string;
    subject: string;
    keywords: string;
    creator: string;
    producer: string;
    creation_date: string;
    mod_date: string;
  },
  remove: boolean,
  jobId: string,
  password?: string,
  /**
   * Keep existing signatures by appending the change as a new revision
   * (default). Removal cannot be expressed that way, so it always rewrites.
   */
  keepSignatures = true,
) =>
  invokeTracked<OpResult>("edit_metadata", {
    request: { input, output, metadata, remove, password, jobId, keepSignatures },
  });

export const addPageNumbers = (
  input: string,
  output: OutputSpec,
  options: NumberingOptions,
  jobId: string,
  password?: string,
  /** Keep existing signatures by appending the numbers as a new revision. */
  keepSignatures = true,
) =>
  invokeTracked<OpResult>("add_page_numbers", {
    request: { input, output, options, password, jobId, keepSignatures },
  });

export const watermarkPdf = (
  input: string,
  output: OutputSpec,
  options: WatermarkOptions,
  jobId: string,
  password?: string,
  /** Keep existing signatures by appending the watermark as a new revision. */
  keepSignatures = true,
) =>
  invokeTracked<OpResult>("watermark_pdf", {
    request: { input, output, options, password, jobId, keepSignatures },
  });

export const annotatePdf = (
  input: string,
  output: OutputSpec,
  annotations: Annotation[],
  jobId: string,
  password?: string,
  /** Keep existing signatures by appending the stamp as a new revision. */
  keepSignatures = true,
) =>
  invokeTracked<OpResult>("annotate_pdf", {
    request: { input, output, annotations, password, jobId, keepSignatures },
  });

export const redactPdf = (
  input: string,
  output: OutputSpec,
  areas: RedactionArea[],
  options: RedactionOptions,
  jobId: string,
  password?: string,
) => invokeTracked<OpResult>("redact_pdf", { request: { input, output, areas, options, password, jobId } });

export const detectSensitiveText = (path: string, page: number, password?: string) =>
  invoke<RedactionMatch[]>("detect_sensitive_text", { path, page, password: password || null });

export const comparePdfs = (
  left: string,
  right: string,
  options: CompareOptions,
  jobId: string,
  leftPassword?: string,
  rightPassword?: string,
) =>
  invokeTracked<CompareReport>("compare_pdfs", {
    request: {
      left,
      right,
      leftPassword,
      rightPassword,
      options,
      jobId,
    },
  });

export const inspectDocument = (path: string, password?: string) =>
  invoke<DocumentInspection>("inspect_document", { path, password: password || null });

// ---------------------------------------------------------------------------
// Settings / recent
// ---------------------------------------------------------------------------

export const loadSettings = () => invoke<Partial<Settings>>("load_settings");
export const saveSettings = (settings: Settings) => invoke<void>("save_settings", { settings });
export const loadRecent = () => invoke<RecentEntry[]>("load_recent");
export const addRecent = (entry: RecentEntry) => invoke<void>("add_recent", { entry });
export const clearRecent = () => invoke<void>("clear_recent");

/** Vault scan: tracked so a failed/interrupted scan can be retried from Jobs. */
export const vaultScan = <T>(request: { folders: string[]; rescan: boolean; maxFiles: number }) =>
  invokeTracked<T>("vault_scan", { request });

// ---------------------------------------------------------------------------
// Digital signatures - real CMS/PKCS#7 detached signatures (SHA-256)
//
// The Rust side never claims a signature is trusted: `trust` is always
// "unknown" because there is no system trust store. Verification is performed
// locally against the embedded certificate; the optional online revocation
// check (`pdfVerifySignaturesOnline`) is reported separately in `revocation`.
// ---------------------------------------------------------------------------

export interface SignatureCertificateInfo {
  subject: string;
  issuer: string;
  serialHex: string;
  notBefore: string;
  notAfter: string;
  expired: boolean;
  isCa: boolean;
  sha256Fingerprint: string;
}

export type RevocationStatus = "not_checked" | "good" | "revoked" | "unknown" | "error";

/**
 * Result of the optional online revocation check of the signer certificate.
 * Separate from `trust`: a certificate that is not revoked is still not trusted.
 */
export interface RevocationInfo {
  status: RevocationStatus;
  source?: "ocsp" | "crl" | null;
  /** RFC 3339 time the check ran. */
  checkedAt?: string | null;
  /** The responder or CRL URL that answered. */
  url?: string | null;
  /** RFC 3339 revocation time when `status` is "revoked". */
  revokedAt?: string | null;
  /** RFC 5280 CRLReason name, e.g. "keyCompromise". */
  reason?: string | null;
  /** Why the status is unknown or an error. */
  detail?: string | null;
}

export interface SignatureInfo {
  fieldName: string;
  subFilter: string;
  coversWholeDocument: boolean;
  /**
   * Bytes were appended after this signature's revision (a counter-signature,
   * validation data, any incremental update). The signature is still valid for
   * the revision it signed - it is simply not the last revision any more.
   */
  supersededByLaterRevision: boolean;
  modifiedAfterSigning: boolean;
  digestMatches: boolean;
  signatureValid: boolean;
  chain: SignatureCertificateInfo[];
  chainLinked: boolean;
  selfSignedChain: boolean;
  signer: SignatureCertificateInfo;
  signingTime: string | null;
  /** RFC 3161 timestamp token `genTime`, when the signature carries one. */
  timestamp?: string | null;
  algorithm: string;
  trust: string;
  /** Absent in reports from older builds; treat as "not_checked". */
  revocation?: RevocationInfo;
  notes: string[];
}

export interface SignatureReport {
  signatures: SignatureInfo[];
  /**
   * Problems with the document itself rather than with one signature - for
   * example a file that could not be parsed. An empty `signatures` list alone
   * reads like "this PDF is unsigned".
   */
  warnings: string[];
}

export interface SignOptionsInput {
  page?: number;
  /** [x1, y1, x2, y2] in PDF points, bottom-left origin. */
  rect?: [number, number, number, number] | null;
  reason?: string;
  location?: string;
  contact?: string;
  appearance?: boolean;
  signerName?: string | null;
}

export interface SignResult {
  output: string;
  signature: SignatureInfo;
}

export interface SigningCertificateSummary {
  index: number;
  subject: string;
  issuer: string;
  serialHex: string;
  notBefore: string;
  notAfter: string;
  expired: boolean;
  hasPrivateKey: boolean;
  sha256Fingerprint: string;
}

export const pdfVerifySignatures = (path: string) => invoke<SignatureReport>("pdf_verify_signatures", { path });

/** Verifies, then asks each signer certificate's CA about revocation. Needs the `onlineRevocationCheck` setting. */
export const pdfVerifySignaturesOnline = (path: string) =>
  invoke<SignatureReport>("pdf_verify_signatures_online", { path });

/** What the validation-data archiving wrote into the document. */
export interface LtvReport {
  signatures: number;
  certificates: number;
  vriKeys: string[];
  warnings: string[];
}

/**
 * Archives the validation data of every signature (the offline half of PAdES
 * B-LT): the certificate chains are written into a /DSS dictionary as an
 * incremental update, so every existing signature keeps covering exactly what
 * it signed.
 */
export const pdfArchiveValidationData = (input: string, output?: string) =>
  invoke<LtvReport>("pdf_archive_validation_data", { input, output: output ?? null });

export const pdfSign = (payload: {
  input: string;
  output: string;
  pfxPath?: string | null;
  pfxPassword?: string | null;
  certIndex?: number | null;
  options: SignOptionsInput;
  /** Optional RFC 3161 timestamp authority URL; a failure fails the signing. */
  tsaUrl?: string | null;
}) => invoke<SignResult>("pdf_sign", payload);

export const pdfListSigningCertificates = () => invoke<SigningCertificateSummary[]>("pdf_list_signing_certificates");

// ---------------------------------------------------------------------------
// Cloud sync (.oswk over WebDAV; off by default, everything explicit)
// ---------------------------------------------------------------------------

export type SyncProviderId = "webdav" | "onedrive" | "google-drive";
export type SyncStateId = "local_only" | "synced" | "local_ahead" | "cloud_ahead" | "conflict";
export type SyncResolutionId = "keep_local" | "keep_cloud" | "keep_both";

export interface SyncConfigView {
  enabled: boolean;
  provider: SyncProviderId;
  url: string;
  username: string;
  /** Explicit opt-in for plain HTTP, honored for loopback servers only. */
  allowInsecureHttp: boolean;
  remoteDir: string;
  hasPassword: boolean;
  passwordStorage: "dpapi" | "keystore" | "plain" | "none";
}

export interface SyncSaveInput {
  enabled: boolean;
  provider: SyncProviderId;
  url: string;
  username: string;
  allowInsecureHttp: boolean;
  /** null keeps the stored password, "" clears it, any value replaces it. */
  password: string | null;
  remoteDir: string;
}

export interface SyncTestResult {
  server: string;
  remoteDir: string;
  remoteDirExists: boolean;
  message: string;
}

export interface SyncStatusView {
  file: string;
  localPath: string;
  remotePath: string;
  state: SyncStateId;
  tracked: boolean;
  localSize: number;
  remoteSize: number | null;
  localSha256: string;
  cloudSha256: string | null;
  remoteEtag: string | null;
  baseEtag: string | null;
  baseSha256: string | null;
  localRevision: number;
  lastSyncedAt: string | null;
  updatedAt: string | null;
  note: string | null;
}

export interface SyncListEntry {
  name: string;
  size: number;
  etag: string | null;
  modified: string | null;
}

export interface SyncProviderInfo {
  id: string;
  available: boolean;
  note: string;
}

export interface SyncCapabilities {
  maxTransferBytes: number;
  backgroundSync: boolean;
  autoMerge: boolean;
  providers: SyncProviderInfo[];
}

export const syncGetConfig = () => invoke<SyncConfigView>("sync_get_config");
export const syncSaveConfig = (input: SyncSaveInput) => invoke<SyncConfigView>("sync_save_config", { input });
export const syncTestConnection = () => invoke<SyncTestResult>("sync_test_connection");
export const syncStatus = (localPath: string) => invoke<SyncStatusView>("sync_status", { localPath });
export const syncUpload = (localPath: string) => invoke<SyncStatusView>("sync_upload", { localPath });
export const syncDownload = (remoteName: string, localPath: string) =>
  invoke<SyncStatusView>("sync_download", { remoteName, localPath });
export const syncList = () => invoke<SyncListEntry[]>("sync_list");
export const syncResolve = (localPath: string, resolution: SyncResolutionId) =>
  invoke<SyncStatusView>("sync_resolve", { localPath, resolution });
export const syncForget = (localPath: string) => invoke<void>("sync_forget", { localPath });
export const syncCapabilities = () => invoke<SyncCapabilities>("sync_capabilities");

export interface OAuthProviderStatus {
  provider: string;
  configured: boolean;
  connected: boolean;
  account: string;
  clientId: string;
  tenant: string;
  /** `keychain` (OS vault) or `file` (app secret store fallback). */
  store: string;
}

export const oauthStatus = () => invoke<OAuthProviderStatus[]>("oauth_status");
export const oauthSaveClient = (provider: string, clientId: string, clientSecret: string, tenant: string) =>
  invoke<OAuthProviderStatus[]>("oauth_save_client", { provider, clientId, clientSecret, tenant });
export const oauthConnect = (provider: string) => invoke<OAuthProviderStatus[]>("oauth_connect", { provider });
export const oauthDisconnect = (provider: string) => invoke<OAuthProviderStatus[]>("oauth_disconnect", { provider });

// ---------------------------------------------------------------------------
// AcroForm fields and PDF Studio page objects (V3.1)
//
// Every value below is a real PDF object read or written by
// `pdfcore::forms`: fill writes `/V` and rebuilds widget appearances, object
// edits rewrite the annotation `/Rect` (plus appearance matrix for rotation)
// or the image placement matrix in the content stream. The commands never
// execute PDF JavaScript, and validation reports only what is provable from
// the file; scripted formats come back as "scripted_format" warnings.
// ---------------------------------------------------------------------------

export interface FieldOption {
  value: string;
  label: string;
}

export type FormFieldType = "text" | "checkbox" | "radio" | "pushbutton" | "choice" | "signature" | "unknown";

export interface FormFieldInfo {
  name: string;
  fieldType: FormFieldType | string;
  flags: number;
  required: boolean;
  readOnly: boolean;
  value: string;
  values: string[];
  defaultValue: string;
  tooltip: string | null;
  maxLength: number | null;
  multiline: boolean;
  password: boolean;
  comb: boolean;
  combo: boolean;
  editable: boolean;
  multiSelect: boolean;
  options: FieldOption[];
  page: number | null;
  rect: [number, number, number, number] | null;
  tabOrder: number | null;
  widgetCount: number;
  hasScript: boolean;
}

export interface FieldValue {
  name: string;
  value: string;
  values?: string[];
}

export interface FillReport {
  filled: number;
  skipped: string[];
  warnings: string[];
}

export interface FieldIssue {
  field: string;
  code: string;
  severity: "error" | "warning" | string;
  message: string;
}

export interface PageObjectInfo {
  page: number;
  index: number;
  /** `annotation` | `widget` | `image`. */
  kind: string;
  subtype: string;
  /** `"object generation"` for indirect objects; null for inline ones. */
  id: string | null;
  /** Page-space box, bottom-left origin. */
  rect: [number, number, number, number];
  matrix: [number, number, number, number, number, number] | null;
  resourceName: string | null;
  fieldName: string | null;
  contents: string;
  flags: number;
  hidden: boolean;
  tabOrder: number | null;
}

export type ObjectEditAction =
  | { action: "move"; dx: number; dy: number }
  | { action: "resize"; rect: [number, number, number, number] }
  | { action: "delete" }
  | { action: "rotate"; degrees: number };

export type ObjectEdit = { page: number; index: number } & ObjectEditAction;

export interface EditReport {
  edited: number;
  deleted: number;
  warnings: string[];
}

export const pdfListFormFields = (path: string, password?: string) =>
  invoke<FormFieldInfo[]>("pdf_list_form_fields", { path, password: password || null });

export const pdfFillForm = (request: {
  input: string;
  output: OutputSpec;
  values: FieldValue[];
  password?: string | null;
}) => invoke<FillReport>("pdf_fill_form", { request });

export const pdfValidateForm = (path: string, values: FieldValue[], password?: string) =>
  invoke<FieldIssue[]>("pdf_validate_form", { path, values, password: password || null });

export const pdfListObjects = (path: string, password?: string) =>
  invoke<PageObjectInfo[]>("pdf_list_objects", { path, password: password || null });

export const pdfEditObjects = (request: {
  input: string;
  output: OutputSpec;
  edits: ObjectEdit[];
  password?: string | null;
}) => invoke<EditReport>("pdf_edit_objects", { request });

/** One text-showing operation on a page (V3.6 content editing). */
export interface TextRunInfo {
  page: number;
  index: number;
  text: string;
  font: string | null;
  fontSizePt: number;
  x: number;
  y: number;
  widthPt: number;
  renderMode: number;
  editable: boolean;
  note: string | null;
}

export interface TextRunEdit {
  page: number;
  index: number;
  text: string;
}

export interface TextEditReport {
  edited: number;
  warnings: string[];
}

export const pdfListTextRuns = (path: string, password?: string) =>
  invoke<TextRunInfo[]>("pdf_list_text_runs", { path, password: password || null });

/** Replaces run text; the output keeps the original bytes as a new revision. */
export const pdfEditTextRuns = (request: {
  input: string;
  output: OutputSpec;
  edits: TextRunEdit[];
  password?: string | null;
}) => invoke<TextEditReport>("pdf_edit_text_runs", { request });
