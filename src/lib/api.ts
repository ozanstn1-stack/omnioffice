import type * as specta from "./generated/specta";
import type { SyncState, VaultScanRequest_Deserialize } from "./generated/specta";
import type { CommandResult } from "./typed-ipc";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { FileToken } from "./file-tokens";
import type {
  Annotation,
  AnnotationEditItem,
  AnnotationEditReport,
  AppInfo,
  BatesOptions,
  CompressEstimate,
  CompareOptions,
  CompareReport,
  CompressOptions,
  CropItem,
  DocumentInspection,
  EditableAnnotation,
  EngineStatus,
  HeaderFooterOptions,
  ImageItem,
  ImageToPdfOptions,
  NumberingOptions,
  NupOptions,
  OcrLanguage,
  OcrOptions,
  OperationEntry,
  OpResult,
  OutlineEntry,
  OutputSpec,
  PagePlanItem,
  PdfInfo,
  ProgressPayload,
  SearchResponse,
  ProtectOptions,
  RecentAdd,
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
import type { CertificateItem } from "./generated/CertificateItem";
import type { CertificateVerification } from "./generated/CertificateVerification";
import type { RedactionCertificate } from "./generated/RedactionCertificate";
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

/**
 * Typed command calls (tauri-specta, #316). The generated wrappers load on
 * first use: this module sits in the entry chunk, which cannot carry them.
 * `call` unwraps a command that returns a Result (it rejects with the
 * command's error, as `invoke` does); `callPlain` is for the others.
 */
type Commands = typeof import("./generated/specta").commands;
const typedCommands = () => import("./typed-ipc");
const call = <T>(run: (commands: Commands) => Promise<CommandResult<T, unknown>>): Promise<T> =>
  typedCommands().then(({ commands, unwrap }) => unwrap(run(commands)));
const callPlain = <T>(run: (commands: Commands) => Promise<T>): Promise<T> =>
  typedCommands().then(({ commands }) => run(commands));

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

// AI (DeepSeek) - the only network feature, opt-in with the user's own key.

export const aiGetSettings = () => callPlain((c) => c.aiGetSettings());
export const aiSaveSettings = (input: specta.AiSettingsInput_Deserialize) => call((c) => c.aiSaveSettings(input));
/** Removes the stored key of `provider` (the saved provider when omitted). */
export const aiClearKey = (provider?: string) => call((c) => c.aiClearKey(provider ?? null));
export const aiTestConnection = () => call((c) => c.aiTestConnection());
export const aiDocumentPreview = (path: string, pages: number[] | undefined, password?: string) =>
  call((c) => c.aiDocumentPreview(path, pages ?? null, password || null));
// The long AI jobs stay on `invokeTracked` (the Jobs screen re-runs them from
// the stored args); their request and result types are the generated ones.
export const aiSummarize = (request: specta.AiSummarizeRequest_Deserialize) =>
  invokeTracked<specta.AiTextResult>("ai_summarize", { request });
export const aiTranslate = (request: specta.AiTranslateRequest_Deserialize) =>
  invokeTracked<specta.AiTextResult>("ai_translate", { request });
export const aiAsk = (request: specta.AiAskRequest_Deserialize) =>
  invokeTracked<specta.AiTextResult>("ai_ask", { request });
export const aiChat = (request: specta.AiChatRequest_Deserialize) =>
  invokeTracked<specta.AiChatResult_Serialize>("ai_chat", { request });
export const aiChatSave = async (thread: specta.SavedChatThread_Deserialize) => {
  await call((c) => c.aiChatSave(thread));
};
export const aiChatList = () => call((c) => c.aiChatList());
export const aiChatLoad = (id: string) => call((c) => c.aiChatLoad(id));
export const aiChatDelete = async (id: string) => {
  await call((c) => c.aiChatDelete(id));
};
/** Deletes AI data: saved chats (all, or older than the retention) and the derived vault index. */
export const aiClearData = (olderThanDays?: number) => call((c) => c.aiClearData(olderThanDays ?? null));
export const aiExtractTable = (request: specta.AiExtractRequest_Deserialize) =>
  invokeTracked<specta.AiTableResult_Serialize>("ai_extract_table", { request });
export const aiBatchRun = (request: specta.AiBatchRequest_Deserialize) =>
  invokeTracked<specta.AiBatchResult_Serialize>("ai_batch_run", { request });
export const aiCleanupText = (request: specta.AiCleanupRequest_Deserialize) =>
  invokeTracked<specta.AiTextResult>("ai_cleanup_text", { request });
/**
 * In-editor AI action. Deliberately NOT `invokeTracked`: the retry store
 * persists the invoke args, which here hold the user's selected document text.
 */
export const aiEditText = (request: specta.AiEditRequest_Deserialize) => call((c) => c.aiEditText(request));
export const aiSuggestMetadata = (path: string, password: string | undefined, jobId: string, consent = true) => {
  const request: specta.AiMetadataRequest_Deserialize = { path, password: password || null, jobId, consent };
  return invokeTracked<specta.MetadataSuggestion>("ai_suggest_metadata", { request });
};
export const aiSaveOutput = (path: string, text: string, overwrite?: string) =>
  call((c) => c.aiSaveOutput(path, text, overwrite ?? null));
export const aiExamplePrompts = () => callPlain((c) => c.aiExamplePrompts());
export const aiModels = (provider?: string) => callPlain((c) => c.aiModels(provider ?? null));

/**
 * Live model list straight from the configured provider. The command takes no
 * arguments: it reads the saved provider, base URL and key itself, so the
 * settings must be saved before calling it. Never throws for a provider that
 * does not answer - `discovered` is false and `message` explains why.
 */
export type AiDiscoveredModels = specta.AiDiscoveredModels;

export const aiDiscoverModels = () => call((c) => c.aiDiscoverModels());

// ---------------------------------------------------------------------------
// AI library (saved results) and the operation log
// ---------------------------------------------------------------------------

export type SaveAiEntryRequest = specta.SaveAiEntryRequest_Deserialize;

export const aiLibrarySave = (request: SaveAiEntryRequest) => call((c) => c.aiLibrarySave(request));
export const aiLibraryList = () => call((c) => c.aiLibraryList());
export const aiLibraryText = (id: string) => call((c) => c.aiLibraryText(id));
export const aiLibraryDelete = (id: string, deleteFile = true) => call((c) => c.aiLibraryDelete(id, deleteFile));
export const aiLibraryClear = async (deleteFiles = true) => {
  await call((c) => c.aiLibraryClear(deleteFiles));
};
export const aiLibraryExport = (id: string, target: string) => call((c) => c.aiLibraryExport(id, target));
export const aiLibraryDefaultDir = () => callPlain((c) => c.aiLibraryDefaultDir());

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

export const onAiBatchProgress = (
  handler: (payload: { jobId: string; index: number; total: number; path: string; action: string }) => void,
): Promise<UnlistenFn> => listen("ai:batch_progress", (event) => handler(event.payload as never));

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

export const startupFiles = () => invoke<FileToken[]>("office_startup_files");

/** Drains the files later launches forwarded to this (single) instance. */
export const takeLaunchFiles = () => invoke<FileToken[]>("office_take_launch_files");

/** Fires when a later launch queued files; fetch them with `takeLaunchFiles`. */
export const onLaunchFilesQueued = (handler: () => void): Promise<UnlistenFn> =>
  listen("launch:files-queued", () => handler());

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

/** Real (editable) annotations written into the page's `/Annots` array. */
export const pdfAnnotateEditable = (
  input: string,
  output: OutputSpec,
  annotations: Annotation[],
  jobId: string,
  password?: string,
  keepSignatures = true,
) =>
  invokeTracked<OpResult>("pdf_annotate_editable", {
    request: { input, output, annotations, password, jobId, keepSignatures },
  });

/** Lists the document's annotations in display space (top-left origin). */
export const pdfListAnnotations = (path: string, password?: string) =>
  invoke<EditableAnnotation[]>("pdf_list_annotations", { path, password: password || null });

/** Moves, resizes, updates or deletes existing annotations by index. */
export const pdfEditAnnotations = (input: string, output: OutputSpec, edits: AnnotationEditItem[], password?: string) =>
  invoke<AnnotationEditReport>("pdf_edit_annotations", {
    request: { input, output, edits, password },
  });

// ---------------------------------------------------------------------------
// Signature / paraphe library
// ---------------------------------------------------------------------------

export interface SignatureEntry {
  name: string;
  imageBase64: string;
}

/** Stores a drawn signature/paraphe PNG locally under a name. */
export const signatureSave = (name: string, imageBase64: string) =>
  invoke<void>("signature_save", { name, imageBase64 });

/** Lists the locally saved signatures. */
export const signatureList = () => invoke<SignatureEntry[]>("signature_list");

/** Deletes a saved signature. */
export const signatureDelete = (name: string) => invoke<void>("signature_delete", { name });

// ---------------------------------------------------------------------------
// Batch recipes and watched folder
// ---------------------------------------------------------------------------

/**
 * Batch recipes. The step shapes are locked to the Rust enum by
 * `src/lib/generated/contract.test.ts` (`npm run types:gen` regenerates the
 * bindings). The option objects stay partial because the Rust structs carry
 * `#[serde(default)]`, so a step only names the fields it sets.
 */
export type RecipeStep =
  | { kind: "compress"; options: Partial<CompressOptions> }
  | { kind: "stamp"; headerFooter?: Partial<HeaderFooterOptions>; bates?: Partial<BatesOptions> }
  | { kind: "pdfa"; level: string }
  | { kind: "ocr"; options: Partial<OcrOptions> }
  | { kind: "redact"; areas?: RedactionArea[]; options?: Partial<RedactionOptions> };

export interface Recipe {
  id: string;
  name: string;
  steps: RecipeStep[];
}

export interface WatchedFolder {
  folder: string;
  outputDir: string;
  recipeId: string;
  password?: string | null;
}

export interface WatchedResult {
  input: string;
  output: string;
  ok: boolean;
  detail: string;
}

export const recipeSave = (recipe: Recipe) => invoke<void>("recipe_save", { recipe });
export const recipeList = () => invoke<Recipe[]>("recipe_list");
export const recipeDelete = (id: string) => invoke<void>("recipe_delete", { id });
export const recipeRun = (input: string, output: string, recipe: Recipe, password?: string) =>
  invoke<{ output: string; applied: string[] }>("recipe_run", {
    request: { input, output, recipe, password: password || null },
  });
export const watchedFolderGet = () => invoke<WatchedFolder>("watched_folder_get");
export const watchedFolderSet = (config: WatchedFolder) => invoke<void>("watched_folder_set", { config });
export const watchedFolderScan = (seen?: string[]) =>
  invoke<WatchedResult[]>("watched_folder_scan", { seen: seen ?? null });
export const watchedFolderStart = (intervalSeconds?: number) =>
  invoke<void>("watched_folder_start", { intervalSeconds: intervalSeconds ?? null });
export const watchedFolderStop = () => invoke<void>("watched_folder_stop");

/** Keyword/regex matches for redaction, next to the built-in detector. */
export const detectRedactionMatches = (
  path: string,
  keywords: string[],
  pattern: string | null,
  caseSensitive: boolean,
  pages: number[] | null,
  jobId: string,
  password?: string,
  auto = false,
) =>
  invokeTracked<RedactionMatch[]>("detect_redaction_matches", {
    request: { path, keywords, pattern, caseSensitive, pages, auto, password, jobId },
  });

/** Writes the document outline (bookmarks) as a new revision. */
export const pdfSetOutline = (
  input: string,
  output: OutputSpec,
  entries: OutlineEntry[],
  jobId: string,
  password?: string,
) =>
  invokeTracked<OpResult>("pdf_set_outline", {
    request: { input, output, entries, password, jobId },
  });

/** Header/footer text and Bates numbering stamps in one revision. */
export const stampPdf = (
  input: string,
  output: OutputSpec,
  headerFooter: HeaderFooterOptions | null,
  bates: BatesOptions | null,
  jobId: string,
  password?: string,
  keepSignatures = true,
) =>
  invokeTracked<OpResult>("stamp_pdf", {
    request: { input, output, headerFooter, bates, password, jobId, keepSignatures },
  });

/** N-up sheets and saddle-stitch booklets. */
export const nupPdf = (input: string, output: OutputSpec, options: NupOptions, jobId: string, password?: string) =>
  invokeTracked<OpResult>("nup_pdf", {
    request: { input, output, options, password, jobId },
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

/**
 * Builds the verifiable redaction certificate: digests of the input and the
 * certified output, the item list (page/category/rule - never a value) and,
 * when `tsaUrl` is set, an RFC 3161 timestamp over the certified digest.
 */
export const redactCertificateBuild = (request: {
  originalPath: string;
  redactedPath: string;
  items: CertificateItem[];
  tsaUrl?: string | null;
}) => invoke<RedactionCertificate>("redact_certificate_build", { request });

/** Embeds the certificate in the redacted PDF and returns its verification. */
export const redactCertificateEmbed = (path: string, certificate: RedactionCertificate) =>
  invoke<CertificateVerification>("redact_certificate_embed", { path, certificate });

/** Extracts and verifies the certificate a PDF carries, if any. */
export const redactCertificateVerify = (path: string) =>
  invoke<CertificateVerification>("redact_certificate_verify", { path });

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
export const addRecent = (entry: RecentAdd) => invoke<void>("add_recent", { entry });
export const clearRecent = () => invoke<void>("clear_recent");

/** Vault scan: tracked so a failed/interrupted scan can be retried from Jobs. */
// The request type is generated from the Rust struct (#316); the call stays
// on invokeTracked because the job tracker needs the command name.
export const vaultScan = <T>(request: VaultScanRequest_Deserialize) => invokeTracked<T>("vault_scan", { request });

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
export type SyncStateId = SyncState;
export type SyncResolutionId = "keep_local" | "keep_cloud" | "keep_both";

// The views and commands are generated from the Rust types (#316): the sync
// screen calls `commands.sync*` / `commands.oauth*` from `typed-ipc`.

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
