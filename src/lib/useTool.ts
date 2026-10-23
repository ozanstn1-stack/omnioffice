import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { cancelJob, fileSizes, logOperation, pdfInfo, toAppError } from "./api";
import { displayNames, releaseFileToken, type FileToken } from "./file-tokens";
import { acceptsFile, pathEntries, pickDesktopFiles, tokenEntries } from "./tool-files";
import { useOutputTarget } from "./use-output-target";
import { fileBaseName, isPdf, uid } from "./format";
import { isAndroid, pickAndroidFiles, publishOutputs, type PublishTarget } from "./mobile";
import {
  reportError,
  useDev,
  useDrop,
  useJobs,
  useOverwritePrompt,
  usePasswordPrompt,
  useRecent,
  useSettings,
} from "./store";
import { useT } from "./i18n";
import type { OpResult, OutputSpec, OverwriteMode, PdfInfo, ProgressPayload, SelectedFile } from "./types";

export interface ToolOptions {
  /** Suffix for the default output file name, e.g. "_merged". */
  suffix: string;
  /** `document` accepts PDFs, images and every office format (AI assistant). */
  accept?: "pdf" | "image" | "any" | "document";
  multiple?: boolean;
  /** Load pdf info (pages, metadata) for the primary file. */
  loadInfo?: boolean;
  /** Route OS file drops to this tool while mounted. */
  dropEnabled?: boolean;
  /** Files handed over by another screen (e.g. Home quick actions). */
  initialPaths?: string[];
  /** Tools that write several files (split, image export) pick an overwrite policy up front. */
  multiOutput?: boolean;
}

export interface OverwritePrompt {
  fileName: string;
  resolve: (mode: OverwriteMode | null) => void;
}

export interface ToolSession {
  files: SelectedFile[];
  primary: SelectedFile | null;
  info: PdfInfo | null;
  infoError: string | null;
  password: string;
  setPassword: (value: string) => void;
  needsPassword: boolean;
  addPaths: (paths: string[]) => Promise<void>;
  pickFiles: () => Promise<void>;
  removeFile: (index: number) => void;
  moveFile: (from: number, to: number) => void;
  clearFiles: () => void;
  outputPath: string;
  setOutputPath: (value: string) => void;
  /** Save target from the Rust dialog: token for the command, path for display. */
  setOutputTarget: (target: { token: string; path: string }) => void;
  outputDir: string;
  setOutputDir: (value: string) => void;
  overwrite: OverwriteMode;
  setOverwrite: (mode: OverwriteMode) => void;
  outputSpec: (overwrite?: OverwriteMode, path?: string) => OutputSpec;
  jobId: string;
  running: boolean;
  progress: ProgressPayload | null;
  result: OpResult | null;
  setResult: (result: OpResult | null) => void;
  error: { code: string; message: string } | null;
  run: (task: (jobId: string, overwrite: OverwriteMode) => Promise<OpResult | void>) => Promise<void>;
  cancel: () => void;
  resetResult: () => void;

  isMultiOutput: boolean;
  setIsMultiOutput: (value: boolean) => void;
  /** Development automation hook (no-op in normal use). */
  registerAutoRun: (handler: () => void) => void;
  reloadInfo: () => Promise<void>;

  /** Android only: user-chosen export destination (SAF save dialog/picker). */
  androidTarget: PublishTarget | null;
  setAndroidTarget: (target: PublishTarget | null) => void;
  /** Android only: copies finished documents to the public Downloads folder. */
  publish: (paths: string[]) => Promise<void>;
}

export function useTool(options: ToolOptions): ToolSession {
  const {
    suffix,
    accept = "pdf",
    multiple = false,
    loadInfo = true,
    dropEnabled = true,
    initialPaths,
    multiOutput = false,
  } = options;
  const t = useT();
  const settings = useSettings((s) => s.settings);
  const addRecentEntry = useRecent((s) => s.add);
  const progressMap = useJobs((s) => s.progress);
  const setDropHandler = useDrop((s) => s.setHandler);
  const askPassword = usePasswordPrompt((s) => s.ask);

  const [files, setFiles] = useState<SelectedFile[]>([]);
  const filesRef = useRef<SelectedFile[]>([]);
  const [info, setInfo] = useState<PdfInfo | null>(null);
  const [infoError, setInfoError] = useState<string | null>(null);
  const [password, setPassword] = useState("");
  const [needsPassword, setNeedsPassword] = useState(false);
  // Multi-output tools show the overwrite choice up front; "create new" is the
  // displayed default, so the state must match it (the old "error" default made
  // a second export fail with "a file with this name already exists").
  const [overwrite, setOverwrite] = useState<OverwriteMode>(multiOutput ? "unique_name" : "error");
  const [jobId] = useState(() => uid("job"));
  const [running, setRunning] = useState(false);
  const [result, setResult] = useState<OpResult | null>(null);
  const [error, setError] = useState<{ code: string; message: string } | null>(null);

  const [isMultiOutput, setIsMultiOutput] = useState(multiOutput);
  const [androidTarget, setAndroidTarget] = useState<PublishTarget | null>(null);
  const androidTargetRef = useRef<PublishTarget | null>(null);
  const taskRef = useRef<((jobId: string, overwrite: OverwriteMode) => Promise<OpResult | void>) | null>(null);

  // Keep the refs in sync in an effect (writing refs during render is unsafe
  // under concurrent rendering) instead of assigning them in the render body.
  useEffect(() => {
    androidTargetRef.current = androidTarget;
  }, [androidTarget]);

  const primary = files.length ? files[0] : null;
  const progress = progressMap[jobId] ?? null;

  useEffect(() => {
    filesRef.current = files;
  }, [files]);

  const accepted = useMemo(() => acceptsFile(accept), [accept]);

  const addPaths = useCallback(
    async (paths: string[]) => {
      const filtered = paths.filter(accepted);
      if (!filtered.length) {
        if (paths.length) setError({ code: "unsupported", message: t("errors.unsupported") });
        return;
      }
      const sizes = await fileSizes(filtered).catch(() => filtered.map(() => null));
      const entries = pathEntries(filtered, sizes, await displayNames(filtered).catch(() => filtered.map(() => null)));
      setResult(null);
      setError(null);
      setFiles((previous) => (multiple ? [...previous, ...entries] : entries.slice(0, 1)));
    },
    [accepted, multiple, t],
  );

  const addTokens = useCallback(
    async (refs: FileToken[]) => {
      const acceptedRefs = refs.filter((ref) => accepted(ref.name));
      if (!acceptedRefs.length) {
        if (refs.length) setError({ code: "unsupported", message: t("errors.unsupported") });
        return;
      }
      const sizes = await fileSizes(acceptedRefs.map((ref) => ref.token)).catch(() => acceptedRefs.map(() => null));
      const entries = tokenEntries(acceptedRefs, sizes);
      setResult(null);
      setError(null);
      setFiles((previous) => (multiple ? [...previous, ...entries] : entries.slice(0, 1)));
    },
    [accepted, multiple, t],
  );

  const pickFiles = useCallback(async () => {
    if (isAndroid()) {
      const paths = await pickAndroidFiles({ multiple, accept }).catch(() => []);
      if (!paths.length) return;
      await addPaths(paths);
      return;
    }
    const refs = await pickDesktopFiles(accept, multiple);
    if (refs.length) await addTokens(refs);
  }, [accept, addPaths, addTokens, multiple]);

  const removeFile = useCallback((index: number) => {
    const file = filesRef.current[index];
    if (file?.token) void releaseFileToken(file.token);
    setFiles((previous) => previous.filter((_, i) => i !== index));
    setResult(null);
  }, []);

  const moveFile = useCallback((from: number, to: number) => {
    setFiles((previous) => {
      if (from === to || from < 0 || to < 0 || from >= previous.length || to >= previous.length) return previous;
      const next = [...previous];
      const [item] = next.splice(from, 1);
      next.splice(to, 0, item);
      return next;
    });
  }, []);

  const clearFiles = useCallback(() => {
    setFiles([]);
    setInfo(null);
    setResult(null);
    setError(null);
    setPassword("");
    setNeedsPassword(false);
  }, []);

  // Load document info for the primary file (with password prompting).
  const loadInfoForRef = useRef<(file: SelectedFile, secret?: string) => Promise<void>>(async () => undefined);
  const loadInfoFor = useCallback(
    async (file: SelectedFile, secret?: string) => {
      if (!isPdf(file.path)) {
        setInfo(null);
        setInfoError(null);
        return;
      }
      try {
        const loaded = await pdfInfo(file.path, secret);
        setInfo(loaded);
        setInfoError(null);
        setNeedsPassword(false);
        if (secret) setPassword(secret);
      } catch (err) {
        const appError = toAppError(err);
        if (appError.code === "password_required") {
          setNeedsPassword(true);
          setInfo(null);
          const entered = await askPassword(file.path, file.name);
          if (entered) {
            await loadInfoForRef.current(file, entered);
          }
          return;
        }
        setInfo(null);
        setInfoError(appError.message);
      }
    },
    [askPassword],
  );

  // Indirection so the retry-with-password path does not reference the not-yet
  // declared `loadInfoFor` binding.
  useEffect(() => {
    loadInfoForRef.current = loadInfoFor;
  }, [loadInfoFor]);

  const reloadInfo = useCallback(async () => {
    if (primary) await loadInfoFor(primary, password || undefined);
  }, [loadInfoFor, password, primary]);

  useEffect(() => {
    if (!loadInfo) return;
    if (primary) {
      // eslint-disable-next-line react-hooks/set-state-in-effect -- the loader sets info from an async response
      void loadInfoFor(primary, password || undefined);
    } else {
      setInfo(null);
    }
    // The load is keyed on the file path; including loadInfoFor/password here
    // would refetch on every password keystroke.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [primary?.path, loadInfo]);

  const { outputPath, outputToken, outputDir, setOutputPath, setOutputDir, setOutputTarget } = useOutputTarget({
    input: primary?.path ?? null,
    suffix,
    accept,
    isMultiOutput,
    defaultOutputDir: settings.defaultOutputDir,
  });

  // Files handed over from another screen.
  const initialised = useRef(false);
  useEffect(() => {
    if (initialised.current) return;
    if (initialPaths && initialPaths.length) {
      initialised.current = true;
      // eslint-disable-next-line react-hooks/set-state-in-effect -- addPaths owns the file/info state
      void addPaths(initialPaths);
    }
  }, [addPaths, initialPaths]);

  // OS drop routing.
  useEffect(() => {
    if (!dropEnabled) return;
    setDropHandler((paths: string[]) => {
      void addPaths(paths);
    });
    return () => setDropHandler(null);
  }, [addPaths, dropEnabled, setDropHandler]);

  // Development automation: run the primary action once the file is ready.
  const autoRunHandler = useRef<(() => void) | null>(null);
  const autoRunDone = useRef(false);
  const devAutoRun = useDev((s) => s.autoRun);
  const registerAutoRun = useCallback((handler: () => void) => {
    autoRunHandler.current = handler;
  }, []);
  useEffect(() => {
    if (!devAutoRun || autoRunDone.current) return;
    if (!primary) return;
    if (loadInfo && !info) return;
    if (!autoRunHandler.current) return;
    const timer = setTimeout(() => {
      if (autoRunDone.current) return;
      autoRunDone.current = true;
      autoRunHandler.current?.();
    }, 1200);
    return () => clearTimeout(timer);
  }, [devAutoRun, info, loadInfo, primary, running]);

  // Android: copy finished documents to a visible location (Downloads by
  // default, or the destination the user picked) so they can be opened and
  // shared. No-op on desktop.
  const publish = useCallback(async (paths: string[]) => {
    if (!isAndroid() || !paths.length) return;
    await publishOutputs(paths, androidTargetRef.current ?? undefined);
  }, []);

  const run = useCallback(
    async (task: (jobId: string, overwrite: OverwriteMode) => Promise<OpResult | void>) => {
      taskRef.current = task;

      const execute = async (mode: OverwriteMode): Promise<void> => {
        setRunning(true);
        setError(null);
        setResult(null);
        try {
          const outcome = await task(jobId, mode);
          if (outcome) {
            if (outcome.path) {
              await publish([outcome.path]);
            }
            setResult(outcome);
            // Persistent operation log (paths and sizes only - never content).
            // Read the setting from the store outside the render cycle so a
            // toggle takes effect for the very next operation; when it is off
            // no `log_operation` call is made at all.
            if (useSettings.getState().settings.keepOperationLog) {
              void logOperation({
                operation: suffix.replace(/^_/, ""),
                inputPath: filesRef.current.length ? filesRef.current[0].path : outcome.path,
                outputPath: outcome.path,
                pageCount: outcome.pageCount,
                inputBytes: outcome.originalBytes,
                outputBytes: outcome.outputBytes,
                ok: true,
                detail: outcome.message,
              });
            }
            if (outcome.path) {
              void addRecentEntry({
                path: outcome.path,
                fileName: fileBaseName(outcome.path),
                tool: suffix,
                timestamp: Math.floor(Date.now() / 1000),
              });
            }
          }
        } catch (err) {
          const appError = toAppError(err);
          if (appError.code === "output_exists") {
            const match = /already exists:\s*(.*)$/i.exec(appError.message);
            const targetName = match?.[1] ? fileBaseName(match[1].trim()) : fileBaseName(outputPath) || "output";
            const choice = await useOverwritePrompt.getState().ask(targetName);
            if (choice && choice !== "error") {
              setOverwrite(choice);
              return execute(choice);
            }
          } else if (appError.code !== "cancelled") {
            setError(appError);
            reportError(err, t);
          }
        } finally {
          setRunning(false);
        }
      };

      await execute(overwrite);
    },
    [addRecentEntry, jobId, outputPath, overwrite, publish, suffix, t],
  );

  const cancel = useCallback(() => {
    void cancelJob(jobId).catch(() => undefined);
  }, [jobId]);

  const outputSpec = useCallback(
    (mode?: OverwriteMode, path?: string): OutputSpec => ({
      // Single-output tools send the capability token; multi-output folders
      // (and Android staging) keep the path.
      path: path ?? outputToken ?? outputPath,
      overwrite: mode ?? overwrite,
    }),
    [outputPath, outputToken, overwrite],
  );

  const resetResult = useCallback(() => setResult(null), []);

  return {
    files,
    primary,
    info,
    infoError,
    password,
    setPassword,
    needsPassword,
    addPaths,
    pickFiles,
    removeFile,
    moveFile,
    clearFiles,
    outputPath,
    setOutputPath,
    setOutputTarget,
    outputDir,
    setOutputDir,
    overwrite,
    setOverwrite,
    outputSpec,
    jobId,
    running,
    progress,
    result,
    setResult,
    error,
    run,
    cancel,
    resetResult,
    isMultiOutput,
    setIsMultiOutput,
    registerAutoRun,
    reloadInfo,
    androidTarget,
    setAndroidTarget,
    publish,
  };
}

export function useProgressPercent(progress: ProgressPayload | null): number | null {
  // `progress` changes identity every event; the percentage is a pure function
  // of the two counters, so keying on them preserves (and narrows) the memo.
  const current = progress?.current ?? 0;
  const total = progress?.total ?? 0;
  return useMemo(() => {
    if (!total) return null;
    return Math.min(100, Math.round((current / total) * 100));
  }, [current, total]);
}
