import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  ExternalLink,
  FileText,
  FolderOpen,
  FolderPlus,
  HardDrive,
  RefreshCw,
  Search,
  ShieldCheck,
  Trash2,
  Upload,
  X,
} from "lucide-react";
import { Badge, Button, Card, Checkbox, EmptyState, Field, IconButton, Modal, NumberInput, Spinner, TextInput, Toggle } from "../components/ui";
import { OptionCard, Screen, TwoColumn } from "../components/layout";
import { useT } from "../lib/i18n";
import { errorMessage, useToasts } from "../lib/store";
import { cancelJob, onProgress, toAppError, vaultScan } from "../lib/api";
import { formatBytes, formatDate } from "../lib/format";
import { isAndroid, openAnyFile, pickOfficeFiles } from "../lib/mobile";
import type { ProgressPayload } from "../lib/types";

export interface VaultConfig {
  folders: string[];
  includePdf: boolean;
  includeOffice: boolean;
  maxFileMb: number;
  updatedAt: string;
}

export interface VaultStatus {
  indexed: number;
  folders: number;
  indexBytes: number;
  lastScan: string | null;
  scanning: boolean;
  warnings: string[];
  /** Platform capability: folder scanning (desktop only). Optional so an
   *  older backend without the field still renders. */
  canScanFolders?: boolean;
  /** Platform capability: importing picked documents into app storage. */
  canImportFiles?: boolean;
}

export interface VaultImportFile {
  source: string;
  path: string;
  name: string;
  size: number;
  sha256: string;
}

export interface VaultImportSkip {
  source: string;
  reason: string;
}

export interface VaultImportReport {
  imported: VaultImportFile[];
  skipped: VaultImportSkip[];
}

export interface VaultScanRequest {
  folders: string[];
  rescan: boolean;
  maxFiles: number;
}

export interface VaultSearchRequest {
  query: string;
  exact: boolean;
  fuzzy: boolean;
  phrase: boolean;
  extensions: string[];
  modifiedAfter: string | null;
  modifiedBefore: string | null;
  folder: string | null;
  limit: number;
  offset: number;
}

export interface VaultSearchHit {
  documentId: string;
  path: string;
  fileName: string;
  extension: string;
  size: number;
  modified: string;
  score: number;
  matchLabel: string;
  snippet: string;
  matchedTerms: string[];
}

export interface VaultSearchResponse {
  hits: VaultSearchHit[];
  total: number;
  tookMs: number;
  indexMissing: boolean;
}

const DEFAULT_CONFIG: VaultConfig = {
  folders: [],
  includePdf: true,
  includeOffice: true,
  maxFileMb: 25,
  updatedAt: "",
};

function Row({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="flex items-baseline justify-between gap-3 py-1 border-b last:border-0" style={{ borderColor: "var(--border)" }}>
      <span className="text-xs muted">{label}</span>
      <span className="text-xs text-right break-words" style={{ color: "var(--text-1)" }}>
        {value}
      </span>
    </div>
  );
}

function Snippet({ text }: { text: string }) {
  if (!text) return null;
  const parts = text.split(/(<<[^<>]*>>)/g).filter(Boolean);
  return (
    <span className="block text-[13px] leading-snug" style={{ color: "var(--text-1)" }}>
      {parts.map((part, index) =>
        part.startsWith("<<") && part.endsWith(">>") ? (
          <mark key={index} style={{ background: "var(--accent-weak)", color: "var(--accent)", borderRadius: 3, padding: "0 2px" }}>
            {part.slice(2, -2)}
          </mark>
        ) : (
          <span key={index}>{part}</span>
        ),
      )}
    </span>
  );
}

function HitRow({ hit, active, onSelect }: { hit: VaultSearchHit; active: boolean; onSelect: (hit: VaultSearchHit) => void }) {
  const t = useT();
  return (
    <button
      type="button"
      onClick={() => onSelect(hit)}
      className="card-soft text-left flex flex-col gap-1 px-3 py-2.5"
      style={active ? { outline: "2px solid var(--accent)" } : undefined}
    >
      <span className="flex w-full items-center gap-2 flex-wrap">
        <span className="font-medium text-[13px] truncate" style={{ color: "var(--text-1)" }}>
          {hit.fileName}
        </span>
        <Badge>{hit.extension.toUpperCase()}</Badge>
        <Badge tone="accent">{hit.matchLabel}</Badge>
        <span className="text-xs muted ml-auto">{t("vault.score", { score: Math.round(hit.score) })}</span>
      </span>
      <span className="block w-full text-xs muted truncate" title={hit.path}>
        {hit.path}
      </span>
      <span className="block text-xs muted">
        {formatBytes(hit.size)} · {formatDate(hit.modified)}
      </span>
      <Snippet text={hit.snippet} />
    </button>
  );
}

export function Vault() {
  const t = useT();
  // Android cannot browse folders: the user imports documents instead and the
  // backend indexes the app-private copies. `status` carries the platform
  // capabilities; the local probe is the fallback for an older backend.
  const android = isAndroid();
  const pushToast = useToasts((state) => state.push);
  const [config, setConfig] = useState<VaultConfig>(DEFAULT_CONFIG);
  const [status, setStatus] = useState<VaultStatus | null>(null);
  const [statusError, setStatusError] = useState<string | null>(null);
  const [configError, setConfigError] = useState<string | null>(null);
  const [scanError, setScanError] = useState<string | null>(null);
  const [scanning, setScanning] = useState(false);
  const [progress, setProgress] = useState<ProgressPayload | null>(null);
  const [importing, setImporting] = useState(false);
  const [clearOpen, setClearOpen] = useState(false);
  const [clearing, setClearing] = useState(false);
  const [deleteImports, setDeleteImports] = useState(false);

  const [query, setQuery] = useState("");
  const [exact, setExact] = useState(false);
  const [phrase, setPhrase] = useState(false);
  const [fuzzy, setFuzzy] = useState(false);
  const [extensions, setExtensions] = useState("");
  const [modifiedAfter, setModifiedAfter] = useState("");
  const [modifiedBefore, setModifiedBefore] = useState("");
  const [folderFilter, setFolderFilter] = useState("");
  const [limit, setLimit] = useState(50);
  const [searching, setSearching] = useState(false);
  const [result, setResult] = useState<VaultSearchResponse | null>(null);
  const [searchError, setSearchError] = useState<string | null>(null);

  const [hit, setHit] = useState<VaultSearchHit | null>(null);
  const [preview, setPreview] = useState("");
  const [previewLoading, setPreviewLoading] = useState(false);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const previewId = useRef("");

  const busy = scanning || Boolean(status?.scanning);
  const percent = progress && progress.total > 0 ? Math.min(100, Math.round((progress.current / progress.total) * 100)) : null;
  // The import flow is Android's replacement for folder selection; the
  // backend confirms both capabilities, the local probe keeps an older
  // backend usable.
  const canScanFolders = status?.canScanFolders ?? !android;
  const showImport = android && (status?.canImportFiles ?? true);

  const refreshStatus = useCallback(async () => {
    try {
      const next = await invoke<VaultStatus>("vault_status");
      setStatus(next);
      setStatusError(null);
    } catch (err) {
      setStatusError(errorMessage(err, t));
    }
  }, [t]);

  useEffect(() => {
    // Initial status load; the loader owns the state it sets.
    // eslint-disable-next-line react-hooks/set-state-in-effect
    void refreshStatus();
  }, [refreshStatus]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void onProgress((payload) => {
      if (payload.jobId === "vault-scan") setProgress(payload);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  const saveConfig = useCallback(
    async (next: VaultConfig) => {
      setConfig(next);
      try {
        const saved = await invoke<VaultConfig>("vault_configure", { config: next });
        setConfig(saved);
        setConfigError(null);
        await refreshStatus();
      } catch (err) {
        setConfigError(errorMessage(err, t));
      }
    },
    [refreshStatus, t],
  );

  const addFolder = async () => {
    // Android has no browsable folder picker; the button is hidden there, and
    // this guard keeps the handler honest if it is ever wired elsewhere.
    if (!canScanFolders) return;
    const picked = await openDialog({ directory: true, multiple: false, title: t("vault.addFolder") }).catch(() => null);
    if (!picked) return;
    const path = String(Array.isArray(picked) ? picked[0] : picked);
    if (!path || config.folders.some((folder) => folder.toLowerCase() === path.toLowerCase())) return;
    await saveConfig({ ...config, folders: [...config.folders, path] });
  };

  const removeFolder = (folder: string) => {
    void saveConfig({ ...config, folders: config.folders.filter((entry) => entry !== folder) });
  };

  const startScan = async () => {
    setScanning(true);
    setScanError(null);
    setProgress(null);
    try {
      const request: VaultScanRequest = { folders: config.folders, rescan: false, maxFiles: 20000 };
      const next = await vaultScan<VaultStatus>(request);
      setStatus(next);
      setStatusError(null);
    } catch (err) {
      if (toAppError(err).code !== "cancelled") {
        setScanError(errorMessage(err, t));
      }
      await refreshStatus();
    } finally {
      setScanning(false);
      setProgress(null);
    }
  };

  const cancelScan = () => {
    void cancelJob("vault-scan").catch(() => undefined);
  };

  /** Forgets the index/cache; optionally deletes the imported copies too. */
  const clearVault = async () => {
    setClearing(true);
    try {
      const next = await invoke<VaultStatus>("vault_clear", { deleteImports });
      setStatus(next);
      setStatusError(null);
      setResult(null);
      setHit(null);
      setPreview("");
      setClearOpen(false);
      pushToast({ kind: "success", title: t("vault.clearDone") });
    } catch (err) {
      pushToast({ kind: "error", title: t("errors.title"), detail: errorMessage(err, t) });
    } finally {
      setClearing(false);
    }
  };

  /**
   * Android ingestion: the system picker returns documents (PDF and office
   * types), the backend copies them into the app-private vault and a rescan
   * indexes those copies. The picked cache paths are only an intermediate
   * hop - the vault copies are what persists and is searchable. Never scans
   * anything the user did not pick.
   */
  const importDocuments = async () => {
    setImporting(true);
    setScanError(null);
    try {
      const picked = await pickOfficeFiles(true);
      if (!picked.length) return;
      const report = await invoke<VaultImportReport>("vault_import_files", { paths: picked });
      await startScan();
      if (report.imported.length) {
        pushToast({
          kind: "success",
          title: t("vault.importDone", { count: report.imported.length }),
          detail: report.skipped.length ? t("vault.importSkipped", { count: report.skipped.length }) : undefined,
        });
      } else {
        pushToast({
          kind: "info",
          title: t("vault.importNone"),
          detail: report.skipped.map((entry) => entry.reason).join("; ") || undefined,
        });
      }
    } catch (err) {
      setScanError(errorMessage(err, t));
    } finally {
      setImporting(false);
    }
  };

  const runSearch = async (offset: number) => {
    const trimmed = query.trim();
    if (!trimmed) {
      setResult(null);
      setSearchError(null);
      return;
    }
    setSearching(true);
    setSearchError(null);
    try {
      const request: VaultSearchRequest = {
        query: trimmed,
        exact,
        fuzzy,
        phrase,
        extensions: extensions.split(/[\s,;]+/).map((value) => value.trim()).filter(Boolean),
        modifiedAfter: modifiedAfter || null,
        modifiedBefore: modifiedBefore || null,
        folder: folderFilter.trim() || null,
        limit,
        offset,
      };
      const response = await invoke<VaultSearchResponse>("vault_search", { request });
      setResult(response);
    } catch (err) {
      setSearchError(errorMessage(err, t));
    } finally {
      setSearching(false);
    }
  };

  const selectHit = async (next: VaultSearchHit) => {
    previewId.current = next.documentId;
    setHit(next);
    setPreview("");
    setPreviewError(null);
    setPreviewLoading(true);
    try {
      const text = await invoke<string>("vault_document_text", { id: next.documentId });
      if (previewId.current === next.documentId) setPreview(text);
    } catch (err) {
      if (previewId.current === next.documentId) setPreviewError(errorMessage(err, t));
    } finally {
      if (previewId.current === next.documentId) setPreviewLoading(false);
    }
  };

  const openFile = async (path: string) => {
    try {
      await openAnyFile(path);
    } catch (err) {
      setPreviewError(errorMessage(err, t));
    }
  };

  return (
    <Screen
      title={t("vault.title")}
      subtitle={t("vault.subtitle")}
      actions={
        <Badge tone="ok">
          <span className="flex items-center gap-1">
            <ShieldCheck size={12} /> {t("nav.privacy")}
          </span>
        </Badge>
      }
    >
      <TwoColumn
        main={
          <>
            <Card soft className="p-4 flex items-start gap-3">
              <ShieldCheck size={16} className="shrink-0" style={{ color: "var(--ok)", marginTop: 2 }} />
              <div>
                <p className="font-semibold text-[13.5px]">{t("vault.privacyTitle")}</p>
                <p className="text-xs muted mt-1">{t("vault.privacyBody")}</p>
              </div>
            </Card>

            <OptionCard
              title={t("vault.foldersTitle")}
              action={
                canScanFolders ? (
                  <Button size="sm" variant="ghost" icon={<FolderPlus size={14} />} onClick={() => void addFolder()}>
                    {t("vault.addFolder")}
                  </Button>
                ) : null
              }
            >
              {android ? (
                <>
                  <p className="text-xs muted">{t("vault.androidImportNote")}</p>
                  {showImport ? (
                    <Button
                      variant="primary"
                      size="lg"
                      className="w-full"
                      icon={importing ? <Spinner size={15} /> : <Upload size={15} />}
                      onClick={() => void importDocuments()}
                      disabled={importing || busy}
                    >
                      {importing ? t("vault.importing") : t("vault.importDocuments")}
                    </Button>
                  ) : null}
                  {status ? (
                    <div className="card-soft flex items-center gap-2 px-3 py-2">
                      <HardDrive size={15} className="muted shrink-0" />
                      <span className="text-xs truncate flex-1">{t("vault.importedRootName")}</span>
                      <Badge>{t("vault.appPrivate")}</Badge>
                    </div>
                  ) : null}
                </>
              ) : null}

              {config.folders.length ? (
                <div className="flex flex-col gap-2">
                  {config.folders.map((folder) => (
                    <div key={folder} className="card-soft flex items-center gap-2 px-3 py-2">
                      <FolderOpen size={15} className="muted shrink-0" />
                      <span className="text-xs truncate flex-1" title={folder}>
                        {folder}
                      </span>
                      <IconButton label={t("vault.removeFolder")} onClick={() => removeFolder(folder)}>
                        <X size={14} />
                      </IconButton>
                    </div>
                  ))}
                </div>
              ) : !android ? (
                <EmptyState
                  icon={<FolderPlus size={22} />}
                  title={t("vault.noFolders")}
                  hint={t("vault.noFoldersHint")}
                  action={
                    <Button size="sm" icon={<FolderPlus size={14} />} onClick={() => void addFolder()}>
                      {t("vault.addFolder")}
                    </Button>
                  }
                />
              ) : null}

              {!android && status && status.folders > config.folders.length ? (
                <p className="text-xs" style={{ color: "var(--warn)" }}>
                  {t("vault.savedFoldersHint", { count: status.folders })}
                </p>
              ) : null}

              <div className="flex flex-col gap-1 border-t pt-3" style={{ borderColor: "var(--border)" }}>
                <Toggle
                  checked={config.includePdf}
                  onChange={(value) => void saveConfig({ ...config, includePdf: value })}
                  label={t("vault.includePdf")}
                />
                <Toggle
                  checked={config.includeOffice}
                  onChange={(value) => void saveConfig({ ...config, includeOffice: value })}
                  label={t("vault.includeOffice")}
                />
                <Field label={t("vault.maxFileMb")} className="max-w-[180px]">
                  <NumberInput
                    value={config.maxFileMb}
                    min={1}
                    max={2048}
                    onChange={(value) => void saveConfig({ ...config, maxFileMb: value })}
                    suffix="MB"
                  />
                </Field>
              </div>
              {configError ? (
                <p className="text-xs" style={{ color: "var(--danger)" }}>
                  {configError}
                </p>
              ) : null}
              {status ? (
                <div className="flex items-start justify-between gap-3 border-t pt-3" style={{ borderColor: "var(--border)" }}>
                  <div>
                    <p className="text-[13px] font-medium">{t("vault.clearTitle")}</p>
                    <p className="text-xs muted">{t("vault.clearHint")}</p>
                  </div>
                  <Button
                    variant="danger"
                    size="sm"
                    icon={<Trash2 size={14} />}
                    onClick={() => setClearOpen(true)}
                    disabled={busy || clearing}
                  >
                    {t("vault.clearAction")}
                  </Button>
                </div>
              ) : null}
            </OptionCard>

            <OptionCard title={t("vault.searchTitle")}>
              <form
                className="flex flex-col gap-3"
                onSubmit={(event) => {
                  event.preventDefault();
                  void runSearch(0);
                }}
              >
                <div className="flex items-end gap-2">
                  <Field label={t("vault.query")} className="flex-1">
                    <TextInput
                      value={query}
                      onChange={(event) => setQuery(event.target.value)}
                      placeholder={t("vault.queryPlaceholder")}
                      spellCheck={false}
                    />
                  </Field>
                  <Button
                    type="submit"
                    variant="primary"
                    icon={searching ? <Spinner size={14} /> : <Search size={15} />}
                    disabled={searching || !query.trim()}
                  >
                    {searching ? t("vault.searching") : t("vault.searchButton")}
                  </Button>
                </div>

                <div className="flex flex-wrap items-center gap-x-5 gap-y-1">
                  <Checkbox checked={exact} onChange={setExact} label={t("vault.exact")} />
                  <Checkbox checked={phrase} onChange={setPhrase} label={t("vault.phrase")} />
                  <Checkbox checked={fuzzy} onChange={setFuzzy} label={t("vault.fuzzy")} />
                </div>

                <div className="grid gap-3" style={{ gridTemplateColumns: "repeat(auto-fit, minmax(190px, 1fr))" }}>
                  <Field label={t("vault.extensions")} hint={t("vault.extensionsHint")}>
                    <TextInput
                      value={extensions}
                      onChange={(event) => setExtensions(event.target.value)}
                      placeholder="pdf, docx, md"
                      spellCheck={false}
                    />
                  </Field>
                  <Field label={t("vault.modifiedAfter")}>
                    <TextInput type="date" value={modifiedAfter} onChange={(event) => setModifiedAfter(event.target.value)} />
                  </Field>
                  <Field label={t("vault.modifiedBefore")}>
                    <TextInput type="date" value={modifiedBefore} onChange={(event) => setModifiedBefore(event.target.value)} />
                  </Field>
                  <Field label={t("vault.folderFilter")}>
                    <TextInput
                      value={folderFilter}
                      onChange={(event) => setFolderFilter(event.target.value)}
                      placeholder={android ? t("vault.folderFilterAndroid") : "C:\\Documents"}
                      spellCheck={false}
                    />
                  </Field>
                  <Field label={t("vault.limit")}>
                    <NumberInput value={limit} min={1} max={200} onChange={setLimit} />
                  </Field>
                </div>
              </form>
            </OptionCard>

            {searchError ? (
              <Card className="p-4">
                <p className="text-xs" style={{ color: "var(--danger)" }}>
                  {searchError}
                </p>
              </Card>
            ) : null}

            {searching ? (
              <Card className="p-6 flex items-center justify-center gap-2 text-sm muted">
                <Spinner size={18} /> {t("vault.searching")}
              </Card>
            ) : result ? (
              result.indexMissing ? (
                <EmptyState
                  icon={<HardDrive size={22} />}
                  title={t("vault.indexMissing")}
                  hint={t("vault.indexMissingHint")}
                  action={
                    <Button size="sm" variant="primary" onClick={() => void startScan()} disabled={busy}>
                      {t("vault.indexNow")}
                    </Button>
                  }
                />
              ) : result.hits.length ? (
                <Card className="p-4 flex flex-col gap-3">
                  <div className="flex items-center gap-2 flex-wrap">
                    <Badge tone="accent">{t("vault.resultCount", { count: result.total })}</Badge>
                    <span className="text-xs muted">{t("vault.tookMs", { ms: result.tookMs })}</span>
                  </div>
                  <div className="flex flex-col gap-2">
                    {result.hits.map((entry) => (
                      <HitRow key={entry.documentId} hit={entry} active={hit?.documentId === entry.documentId} onSelect={(next) => void selectHit(next)} />
                    ))}
                  </div>
                </Card>
              ) : (
                <EmptyState icon={<Search size={22} />} title={t("vault.noResults")} />
              )
            ) : !searchError ? (
              <EmptyState icon={<Search size={22} />} title={t("vault.searchEmpty")} hint={t("vault.searchEmptyHint")} />
            ) : null}

            {hit ? (
              <Card className="p-4 flex flex-col gap-3">
                <div className="flex items-center gap-2 flex-wrap">
                  <FileText size={15} className="muted" />
                  <span className="font-semibold text-[13.5px] truncate">{hit.fileName}</span>
                  <Badge>{hit.extension.toUpperCase()}</Badge>
                  <div className="ml-auto flex items-center gap-2">
                    <Button size="sm" icon={<ExternalLink size={14} />} onClick={() => void openFile(hit.path)}>
                      {t("common.openFile")}
                    </Button>
                    <IconButton label={t("common.close")} onClick={() => setHit(null)}>
                      <X size={15} />
                    </IconButton>
                  </div>
                </div>
                {previewLoading ? (
                  <div className="flex items-center gap-2 text-xs muted">
                    <Spinner size={14} /> {t("vault.previewLoading")}
                  </div>
                ) : previewError ? (
                  <p className="text-xs" style={{ color: "var(--danger)" }}>
                    {previewError}
                  </p>
                ) : preview ? (
                  <pre
                    className="text-xs whitespace-pre-wrap break-words max-h-[420px] overflow-auto p-3 rounded-lg"
                    style={{ background: "var(--surface-2)", color: "var(--text-1)" }}
                  >
                    {preview}
                  </pre>
                ) : (
                  <p className="text-xs muted">{t("vault.previewEmpty")}</p>
                )}
              </Card>
            ) : null}
          </>
        }
        side={
          <>
            <Card className="p-4 flex flex-col gap-3">
              <Button
                variant="primary"
                size="lg"
                icon={busy ? <Spinner size={15} /> : <RefreshCw size={15} />}
                onClick={() => void startScan()}
                disabled={busy}
              >
                {busy ? t("vault.indexing") : t("vault.indexNow")}
              </Button>
              {busy ? (
                <Button variant="ghost" size="lg" onClick={cancelScan}>
                  {t("common.cancel")}
                </Button>
              ) : null}
              {scanning && progress ? (
                <div className="flex flex-col gap-1.5">
                  <div className="flex items-center justify-between text-xs muted tabular-nums">
                    <span>
                      {progress.current} / {progress.total}
                    </span>
                    {percent !== null ? <span>{percent}%</span> : null}
                  </div>
                  <div className="progress-track">
                    <div className="progress-fill" style={{ width: `${percent ?? 8}%` }} />
                  </div>
                </div>
              ) : null}
              {scanError ? (
                <p className="text-xs" style={{ color: "var(--danger)" }}>
                  {scanError}
                </p>
              ) : null}
            </Card>

            <Card className="p-4 flex flex-col gap-3">
              <div className="flex items-center gap-2">
                <HardDrive size={16} className="muted" />
                <h3 className="font-semibold text-[13.5px]">{t("vault.statusTitle")}</h3>
                {busy ? (
                  <Badge tone="accent">
                    <span className="flex items-center gap-1">
                      <Spinner size={11} /> {t("vault.scanning")}
                    </span>
                  </Badge>
                ) : null}
              </div>
              {status ? (
                <div className="flex flex-col">
                  <Row label={t("vault.indexed")} value={status.indexed.toLocaleString()} />
                  <Row label={t("vault.folderCount")} value={status.folders} />
                  <Row label={t("vault.indexSize")} value={formatBytes(status.indexBytes)} />
                  <Row
                    label={t("vault.lastScan")}
                    value={status.lastScan ? formatDate(status.lastScan) : t("vault.neverScanned")}
                  />
                </div>
              ) : statusError ? (
                <p className="text-xs" style={{ color: "var(--danger)" }}>
                  {statusError}
                </p>
              ) : (
                <div className="flex items-center gap-2 text-xs muted">
                  <Spinner size={14} /> {t("common.loading")}
                </div>
              )}
              {status?.warnings.length ? (
                <div className="flex flex-col gap-1">
                  {status.warnings.map((warning, index) => (
                    <p key={index} className="text-xs flex items-start gap-1.5" style={{ color: "var(--warn)" }}>
                      <AlertTriangle size={12} className="shrink-0" style={{ marginTop: 2 }} /> {warning}
                    </p>
                  ))}
                </div>
              ) : null}
            </Card>

            <Card soft className="p-4 flex items-start gap-2 text-xs muted">
              <ShieldCheck size={14} className="shrink-0" style={{ marginTop: 2 }} />
              {t("vault.localNote")}
            </Card>
          </>
        }
      />

      {clearOpen ? (
        <Modal
          title={t("vault.clearConfirmTitle")}
          onClose={() => (!clearing ? setClearOpen(false) : undefined)}
          footer={
            <>
              <Button variant="ghost" onClick={() => setClearOpen(false)} disabled={clearing}>
                {t("common.cancel")}
              </Button>
              <Button variant="danger" icon={clearing ? <Spinner size={14} /> : <Trash2 size={14} />} onClick={() => void clearVault()} disabled={clearing}>
                {t("vault.clearAction")}
              </Button>
            </>
          }
        >
          <p className="text-[13px] muted">{t("vault.clearConfirmBody")}</p>
          {android ? (
            <label className="checkbox mt-3">
              <input type="checkbox" checked={deleteImports} onChange={(event) => setDeleteImports(event.target.checked)} />
              <span>{t("vault.deleteImports")}</span>
            </label>
          ) : null}
        </Modal>
      ) : null}
    </Screen>
  );
}
