import type * as specta from "./generated/specta";
import type { SyncState, VaultScanRequest_Deserialize } from "./generated/specta";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { FileToken } from "./file-tokens";
import type {
  Annotation,
  AnnotationEditItem,
  BatesOptions,
  CompareOptions,
  CompareReport,
  CompressOptions,
  CropItem,
  EditableAnnotation,
  HeaderFooterOptions,
  ImageItem,
  ImageToPdfOptions,
  NumberingOptions,
  NupOptions,
  OcrOptions,
  OpResult,
  OutlineEntry,
  OutputSpec,
  PagePlanItem,
  ProgressPayload,
  ProtectOptions,
  RecentAdd,
  RedactionArea,
  RedactionMatch,
  RedactionOptions,
  Settings,
  SplitMode,
  WatermarkOptions,
} from "./types";
// The redaction certificate as Rust sends it (sizes are JS numbers; the
// older ts-rs files typed them as bigint, which the JSON never carries).
type CertificateItem = specta.CertificateItem;
type RedactionCertificate = specta.RedactionCertificate_Serialize;
import { trackJobInvocation } from "./job-retries";
import { asJson, call, callPlain } from "./typed-call";

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

export const appInfo = () => callPlain((c) => c.appInfo());
export const updateCheck = () => call((c) => c.updateCheck());
export const updateOpen = (url: string) => call((c) => c.updateOpen(url));
export const diagnosticsReport = () => callPlain((c) => c.diagnosticsReport());
export const engineStatus = () => callPlain((c) => c.engineStatus());
export const ocrLanguages = () => callPlain((c) => c.ocrLanguages());
export const cancelJob = (jobId: string) => callPlain((c) => c.cancelJob(jobId));

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

export const logOperation = (entry: specta.OperationLogRequest_Deserialize) =>
  call((c) => c.logOperation(entry)).catch(() => undefined);
export const loadOperations = () => callPlain((c) => c.loadOperations());
export const clearOperations = () => call((c) => c.clearOperations());

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

export const pdfInfo = (path: string, password?: string) => call((c) => c.pdfInfo(path, password || null));

export const pageThumbnail = (path: string, page: number, maxWidth = 200, password?: string) =>
  call((c) => c.pageThumbnail(path, page, maxWidth, password || null));

export const pagePreview = (
  path: string,
  page: number,
  maxWidth = 1100,
  password?: string,
  format: "png" | "jpeg" = "png",
  quality = 86,
) => call((c) => c.pagePreview(path, page, maxWidth, password || null, format, quality));

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
  call((c) =>
    c.pageTile(path, page, scale, region.x, region.y, region.width, region.height, password || null, format, quality),
  );

export const pageText = (path: string, page: number, password?: string) =>
  call((c) => c.pageText(path, page, password || null));

export const searchDocument = (
  path: string,
  query: string,
  matchCase: boolean,
  maxResults: number,
  password: string | undefined,
  jobId: string,
) => call((c) => c.searchDocument(path, query, matchCase, maxResults, password ?? null, jobId));

export const checkPassword = (path: string, password: string) => call((c) => c.checkPassword(path, password));

export const outputExists = (path: string) => callPlain((c) => c.outputExists(path));

export const fileSizes = (paths: string[]) => callPlain((c) => c.fileSizes(paths));

export const logFrontend = (level: string, message: string) =>
  callPlain((c) => c.logFrontend(level, message)).catch(() => undefined);

/** The dev launch arguments (opaque JSON on the Rust side). */
export interface DevLaunchContext {
  startScreen: string | null;
  newTab: string | null;
  files: string[] | null;
  autoRun: boolean;
  tab: string | null;
}

export const devLaunchContext = () =>
  callPlain((c) => c.devLaunchContext()).then((context) => context as unknown as DevLaunchContext);

export const startupFiles = (): Promise<FileToken[]> => callPlain((c) => c.officeStartupFiles());

/** Drains the files later launches forwarded to this (single) instance. */
export const takeLaunchFiles = () => callPlain((c) => c.officeTakeLaunchFiles());

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
  call((c) => c.estimateCompression(input, options, password || null));

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
  call((c) => c.pdfListAnnotations(path, password || null)).then(
    // Rust names the kind with a plain string; these are the kinds it writes.
    (annotations) => annotations as EditableAnnotation[],
  );

/** Moves, resizes, updates or deletes existing annotations by index. */
export const pdfEditAnnotations = (input: string, output: OutputSpec, edits: AnnotationEditItem[], password?: string) =>
  call((c) => c.pdfEditAnnotations({ input, output, edits, password }));

// ---------------------------------------------------------------------------
// Signature / paraphe library
// ---------------------------------------------------------------------------

export interface SignatureEntry {
  name: string;
  imageBase64: string;
}

/** Stores a drawn signature/paraphe PNG locally under a name. */
export const signatureSave = (name: string, imageBase64: string) => call((c) => c.signatureSave(name, imageBase64));

/** Lists the locally saved signatures. */
export const signatureList = () => call((c) => c.signatureList());

/** Deletes a saved signature. */
export const signatureDelete = (name: string) => call((c) => c.signatureDelete(name));

// ---------------------------------------------------------------------------
// Batch recipes and watched folder
// ---------------------------------------------------------------------------

/**
 * Batch recipes, generated from the Rust enum (#316). The input shape is the
 * one Rust reads: the option structs carry `#[serde(default)]`, so a step only
 * names the fields it sets; a saved recipe Rust returns fits it as well.
 */
export type RecipeStep = specta.RecipeStep_Deserialize;
export type Recipe = specta.Recipe_Deserialize;
export type WatchedFolder = specta.WatchedFolder_Deserialize;
export type WatchedResult = specta.WatchedResult;

export const recipeSave = (recipe: Recipe) => call((c) => c.recipeSave(recipe));
export const recipeList = () => call((c) => c.recipeList());
export const recipeDelete = (id: string) => call((c) => c.recipeDelete(id));
export const recipeRun = (input: string, output: string, recipe: Recipe, password?: string) =>
  call((c) => c.recipeRun({ input, output, recipe, password: password || null }));
export const watchedFolderGet = () => call((c) => c.watchedFolderGet());
export const watchedFolderSet = (config: WatchedFolder) => call((c) => c.watchedFolderSet(config));
export const watchedFolderScan = (seen?: string[]) => call((c) => c.watchedFolderScan(seen ?? null));
export const watchedFolderStart = (intervalSeconds?: number) =>
  call((c) => c.watchedFolderStart(intervalSeconds ?? null));
export const watchedFolderStop = () => call((c) => c.watchedFolderStop());

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
  call((c) => c.detectSensitiveText(path, page, password || null));

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
}) => call((c) => c.redactCertificateBuild(request));

/** Embeds the certificate in the redacted PDF and returns its verification. */
export const redactCertificateEmbed = (path: string, certificate: RedactionCertificate) =>
  call((c) => c.redactCertificateEmbed(path, certificate));

/** Extracts and verifies the certificate a PDF carries, if any. */
export const redactCertificateVerify = (path: string) => call((c) => c.redactCertificateVerify(path));

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
  call((c) => c.inspectDocument(path, password || null));

// ---------------------------------------------------------------------------
// Settings / recent
// ---------------------------------------------------------------------------

// Settings are stored as opaque JSON on the Rust side.
export const loadSettings = () =>
  call((c) => c.loadSettings()).then((settings) => settings as unknown as Partial<Settings>);
export const saveSettings = (settings: Settings) => call((c) => c.saveSettings(asJson(settings)));
export const loadRecent = () => call((c) => c.loadRecent());
export const addRecent = (entry: RecentAdd) => call((c) => c.addRecent(entry));
export const clearRecent = () => call((c) => c.clearRecent());

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

export const pdfVerifySignatures = (path: string) => call((c) => c.pdfVerifySignatures(path));

/** Verifies, then asks each signer certificate's CA about revocation. Needs the `onlineRevocationCheck` setting. */
export const pdfVerifySignaturesOnline = (path: string) => call((c) => c.pdfVerifySignaturesOnline(path));

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
  call((c) => c.pdfArchiveValidationData(input, output ?? null));

export const pdfSign = (payload: {
  input: string;
  output: string;
  pfxPath?: string | null;
  pfxPassword?: string | null;
  certIndex?: number | null;
  options: SignOptionsInput;
  /** Optional RFC 3161 timestamp authority URL; a failure fails the signing. */
  tsaUrl?: string | null;
}) =>
  call((c) =>
    c.pdfSign(
      payload.input,
      payload.output,
      payload.pfxPath ?? null,
      payload.pfxPassword ?? null,
      payload.certIndex ?? null,
      payload.options,
      payload.tsaUrl ?? null,
    ),
  );

export const pdfListSigningCertificates = () => call((c) => c.pdfListSigningCertificates());

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
  call((c) => c.pdfListFormFields(path, password || null));

export const pdfFillForm = (request: {
  input: string;
  output: OutputSpec;
  values: FieldValue[];
  password?: string | null;
}) => call((c) => c.pdfFillForm(request));

export const pdfValidateForm = (path: string, values: FieldValue[], password?: string) =>
  call((c) => c.pdfValidateForm(path, values, password || null));

export const pdfListObjects = (path: string, password?: string) =>
  call((c) => c.pdfListObjects(path, password || null));

export const pdfEditObjects = (request: {
  input: string;
  output: OutputSpec;
  edits: ObjectEdit[];
  password?: string | null;
}) => call((c) => c.pdfEditObjects(request));

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
  call((c) => c.pdfListTextRuns(path, password || null));

/** Replaces run text; the output keeps the original bytes as a new revision. */
export const pdfEditTextRuns = (request: {
  input: string;
  output: OutputSpec;
  edits: TextRunEdit[];
  password?: string | null;
}) => call((c) => c.pdfEditTextRuns(request));
