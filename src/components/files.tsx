import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  ArrowLeft,
  ArrowRight,
  CheckCircle2,
  FileImage,
  FileText,
  FolderOpen,
  Info,
  Loader2,
  Lock,
  Plus,
  X,
  XCircle,
} from "lucide-react";
import { Badge, Button, Card, IconButton, Modal, Spinner, TextInput } from "./ui";
import { useT } from "../lib/i18n";
import { formatBytes, formatPoints, fileBaseName } from "../lib/format";
import { useOverwritePrompt, usePasswordPrompt, useToasts } from "../lib/store";
import { useProgressPercent, type ToolSession } from "../lib/useTool";
import type { OpResult, PdfInfo, SelectedFile } from "../lib/types";
import { checkPassword } from "../lib/api";
import {
  isAndroid,
  openAnyFile,
  pickAndroidFiles,
  pickAndroidFolder,
  pickAndroidSaveTarget,
  revealAnyFile,
} from "../lib/mobile";

// ---------------------------------------------------------------------------
// Drop zone
// ---------------------------------------------------------------------------

export function DropZone({
  onPaths,
  title,
  hint,
  dragging,
  compact = false,
  multiple = false,
  accept = "any",
}: {
  onPaths: (paths: string[]) => void;
  title?: string;
  hint?: string;
  dragging: boolean;
  compact?: boolean;
  multiple?: boolean;
  accept?: "pdf" | "image" | "any";
}) {
  const t = useT();
  const filters =
    accept === "pdf"
      ? [{ name: "PDF", extensions: ["pdf"] }]
      : accept === "image"
        ? [{ name: "Images", extensions: ["jpg", "jpeg", "png", "webp", "bmp", "tif", "tiff"] }]
        : [
            {
              name: "Documents & images",
              extensions: [
                "pdf",
                "docx",
                "docm",
                "dotx",
                "odt",
                "rtf",
                "txt",
                "md",
                "html",
                "xlsx",
                "xlsm",
                "xls",
                "ods",
                "csv",
                "tsv",
                "pptx",
                "pptm",
                "odp",
                "oswk",
                "jpg",
                "jpeg",
                "png",
                "webp",
                "bmp",
                "tif",
                "tiff",
                "gif",
              ],
            },
          ];

  const handleBrowse = async () => {
    if (isAndroid()) {
      const paths = await pickAndroidFiles({ multiple, accept }).catch(() => []);
      if (paths.length) onPaths(paths);
      return;
    }
    const picked = await open({ multiple, filters });
    if (!picked) return;
    onPaths(Array.isArray(picked) ? picked.map(String) : [String(picked)]);
  };

  return (
    <div
      className="dropzone flex flex-col items-center justify-center text-center gap-3"
      data-over={dragging}
      style={{ padding: compact ? "22px 18px" : "44px 24px" }}
      onClick={() => void handleBrowse()}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") void handleBrowse();
      }}
      role="button"
      tabIndex={0}
      aria-label={t("common.dropHere")}
    >
      <div
        className="rounded-2xl flex items-center justify-center"
        style={{
          width: compact ? 40 : 56,
          height: compact ? 40 : 56,
          background: "var(--accent-weak)",
          color: "var(--accent)",
        }}
      >
        <FolderOpen size={compact ? 20 : 26} />
      </div>
      <div>
        <p className="font-semibold text-[15px]">
          {title ?? (isAndroid() ? t("common.selectFiles") : t("common.dropHere"))}
        </p>
        <p className="text-sm muted mt-1">{hint ?? (isAndroid() ? t("common.tapHint") : t("common.dropHint"))}</p>
      </div>
      <Button
        variant="primary"
        size={compact ? "sm" : "md"}
        onClick={(event) => {
          event.stopPropagation();
          void handleBrowse();
        }}
      >
        {t("common.selectFiles")}
      </Button>
    </div>
  );
}

// ---------------------------------------------------------------------------
// File list (with reordering)
// ---------------------------------------------------------------------------

export function FileList({
  files,
  onRemove,
  onMove,
  onAdd,
  reorder = false,
  addLabel,
}: {
  files: SelectedFile[];
  onRemove?: (index: number) => void;
  onMove?: (from: number, to: number) => void;
  onAdd?: () => void;
  reorder?: boolean;
  addLabel?: string;
}) {
  const t = useT();
  const dragIndex = useRef<number | null>(null);
  // State (not a ref) so the drop-target highlight is honest about the drag.
  const [dragFromIndex, setDragFromIndex] = useState<number | null>(null);
  const [overIndex, setOverIndex] = useState<number | null>(null);

  const handlePointerDown = (index: number) => (event: React.PointerEvent) => {
    if (!reorder) return;
    if ((event.target as HTMLElement).closest("button")) return;
    dragIndex.current = index;
    setDragFromIndex(index);
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  };

  const handlePointerMove = (event: React.PointerEvent) => {
    if (dragIndex.current === null) return;
    const elements = document.elementsFromPoint(event.clientX, event.clientY);
    const target = elements.find((el) => el instanceof HTMLElement && el.dataset.fileIndex !== undefined) as
      HTMLElement | undefined;
    if (target) {
      const index = Number(target.dataset.fileIndex);
      setOverIndex(index);
    }
  };

  const handlePointerUp = () => {
    if (dragIndex.current !== null && overIndex !== null && dragIndex.current !== overIndex) {
      onMove?.(dragIndex.current, overIndex);
    }
    dragIndex.current = null;
    setDragFromIndex(null);
    setOverIndex(null);
  };

  return (
    <div className="flex flex-col gap-2">
      {files.map((file, index) => (
        <div
          key={`${file.path}-${index}`}
          data-file-index={index}
          draggable={false}
          onPointerDown={handlePointerDown(index)}
          onPointerMove={handlePointerMove}
          onPointerUp={handlePointerUp}
          className="card-soft flex items-center gap-3 px-3 py-2.5"
          style={{
            cursor: reorder ? "grab" : "default",
            outline:
              overIndex === index && dragFromIndex !== null && dragFromIndex !== index
                ? "2px solid var(--accent)"
                : "none",
          }}
        >
          {reorder ? (
            <div className="flex flex-col gap-0.5" aria-hidden>
              <IconButton
                label={t("common.moveLeft")}
                onClick={() => onMove?.(index, Math.max(0, index - 1))}
                disabled={index === 0}
              >
                <ArrowLeft size={13} />
              </IconButton>
              <IconButton
                label={t("common.moveRight")}
                onClick={() => onMove?.(index, Math.min(files.length - 1, index + 1))}
                disabled={index === files.length - 1}
              >
                <ArrowRight size={13} />
              </IconButton>
            </div>
          ) : (
            <div
              className="w-8 h-8 rounded-lg flex items-center justify-center shrink-0"
              style={{ background: "var(--accent-weak)", color: "var(--accent)" }}
            >
              {file.name.toLowerCase().endsWith(".pdf") ? <FileText size={16} /> : <FileImage size={16} />}
            </div>
          )}
          <div className="min-w-0 flex-1">
            <p className="truncate text-[13.5px] font-medium" title={file.path}>
              {file.name}
            </p>
            <p className="text-xs muted truncate">
              {formatBytes(file.sizeBytes)}
              {file.pages ? ` · ${file.pages} ${t("common.pages")}` : ""}
            </p>
          </div>
          {onRemove ? (
            <IconButton label={t("common.remove")} onClick={() => onRemove(index)}>
              <X size={15} />
            </IconButton>
          ) : null}
        </div>
      ))}
      {onAdd ? (
        <Button variant="ghost" size="sm" icon={<Plus size={14} />} onClick={onAdd} className="self-start">
          {addLabel ?? t("common.addFiles")}
        </Button>
      ) : null}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Document info strip
// ---------------------------------------------------------------------------

export function InfoStrip({ info, error }: { info: PdfInfo | null; error?: string | null }) {
  const t = useT();
  if (error) {
    return (
      <div className="flex items-center gap-2 text-sm" style={{ color: "var(--danger)" }}>
        <AlertTriangle size={15} />
        {error}
      </div>
    );
  }
  if (!info) return null;
  const sizes = new Map<string, number>();
  for (const geometry of info.pageGeometries) {
    const key = `${Math.round(geometry.display_width_pt)}x${Math.round(geometry.display_height_pt)}`;
    sizes.set(key, (sizes.get(key) ?? 0) + 1);
  }
  return (
    <div className="flex flex-wrap items-center gap-x-5 gap-y-2 text-[13px]">
      <span className="font-semibold truncate max-w-[280px]" title={info.path}>
        {info.fileName}
      </span>
      <span className="muted">
        {t("info.pageCount")}: <strong className="text-[var(--text)]">{info.pageCount}</strong>
      </span>
      <span className="muted">
        {t("info.fileSize")}: <strong className="text-[var(--text)]">{formatBytes(info.fileSizeBytes)}</strong>
      </span>
      <span className="muted">PDF {info.pdfVersion === "unknown" ? "—" : info.pdfVersion}</span>
      <Badge tone={info.encrypted ? "warn" : "ok"}>
        {info.encrypted ? <Lock size={11} /> : <CheckCircle2 size={11} />}
        {info.encrypted ? t("info.encrypted") : t("info.no")}
      </Badge>
      {!info.encrypted ? (
        <Badge tone={info.hasTextLayer ? "ok" : "warn"}>
          {info.hasTextLayer ? t("info.hasText") : t("info.noText")}
        </Badge>
      ) : null}
      {info.pageGeometries.length ? (
        <span className="muted">
          {[...sizes.entries()]
            .slice(0, 2)
            .map(([key, count]) => {
              const [w, h] = key.split("x").map(Number);
              return `${count}× ${formatPoints(w)}${count > 1 ? ` / ${formatPoints(h)}` : ""}`;
            })
            .join(" · ")}
        </span>
      ) : null}
      {info.imageCount > 0 ? (
        <span className="muted">
          {t("info.imageCount")}: {info.imageCount}
        </span>
      ) : null}
    </div>
  );
}

export function InfoCard({ info }: { info: PdfInfo }) {
  const t = useT();
  const rows: [string, ReactNode][] = [
    [t("info.fileSize"), formatBytes(info.fileSizeBytes)],
    [t("info.pageCount"), info.pageCount],
    [t("info.pdfVersion"), info.pdfVersion === "unknown" ? "—" : `PDF ${info.pdfVersion}`],
    [t("info.encrypted"), info.encrypted ? t("info.yes") : t("info.no")],
    [t("info.textLayer"), info.hasTextLayer ? t("info.hasText") : t("info.noText")],
    [t("info.imageCount"), info.imageCount],
    [t("metadata.fieldTitle"), info.title || "—"],
    [t("metadata.author"), info.author || "—"],
    [t("metadata.producer"), info.producer || "—"],
  ];
  return (
    <Card className="p-5">
      <div className="flex items-center gap-2 mb-4">
        <Info size={16} style={{ color: "var(--accent)" }} />
        <h3 className="font-semibold">{t("info.title")}</h3>
      </div>
      <dl className="grid grid-cols-[auto_1fr] gap-x-6 gap-y-2 text-[13px]">
        {rows.map(([label, value]) => (
          <div key={label} className="contents">
            <dt className="muted">{label}</dt>
            <dd className="font-medium truncate">{value}</dd>
          </div>
        ))}
      </dl>
    </Card>
  );
}

// ---------------------------------------------------------------------------
// Output bar + result card
// ---------------------------------------------------------------------------

export function OutputBar({
  session,
  runLabel,
  children,
  outputKind = "file",
  showOverwrite = false,
  disabled = false,
  onRun,
}: {
  session: ToolSession;
  runLabel: string;
  children?: ReactNode;
  outputKind?: "file" | "folder";
  showOverwrite?: boolean;
  disabled?: boolean;
  onRun: () => void;
}) {
  const t = useT();
  const percent = useProgressPercent(session.progress);

  const chooseFile = async () => {
    if (isAndroid()) {
      const target = await pickAndroidSaveTarget(fileBaseName(session.outputPath) || "document.pdf").catch(() => null);
      session.setAndroidTarget(target ? { file: target } : null);
      return;
    }
    const picked = await open({
      multiple: false,
      directory: false,
      title: t("common.output"),
      defaultPath: session.outputPath,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (picked) session.setOutputPath(String(picked));
  };

  const chooseFolder = async () => {
    if (isAndroid()) {
      const target = await pickAndroidFolder().catch(() => null);
      session.setAndroidTarget(target ? { dir: target } : null);
      return;
    }
    const picked = await open({ multiple: false, directory: true, title: t("common.chooseFolder") });
    if (picked) session.setOutputDir(String(picked));
  };

  const androidTargetName = session.androidTarget?.file?.name ?? session.androidTarget?.dir?.name ?? null;

  return (
    <Card className="p-4 flex flex-col gap-3">
      {outputKind === "file" ? (
        <div className="flex items-end gap-2">
          <div className="flex-1">
            <label className="label">{t("common.output")}</label>
            <TextInput
              value={session.outputPath}
              onChange={(event) => session.setOutputPath(event.target.value)}
              spellCheck={false}
            />
          </div>
          <Button variant="ghost" size="md" icon={<FolderOpen size={15} />} onClick={() => void chooseFile()}>
            {isAndroid() ? t("common.saveAs") : t("common.browse")}
          </Button>
        </div>
      ) : (
        <div className="flex items-end gap-2">
          <div className="flex-1">
            <label className="label">{t("common.outputFolder")}</label>
            <TextInput
              value={session.outputDir}
              onChange={(event) => session.setOutputDir(event.target.value)}
              spellCheck={false}
            />
          </div>
          <Button variant="ghost" size="md" icon={<FolderOpen size={15} />} onClick={() => void chooseFolder()}>
            {isAndroid() ? t("common.saveAs") : t("common.browse")}
          </Button>
        </div>
      )}

      {isAndroid() ? (
        <p className="text-xs muted">
          {androidTargetName
            ? t("common.androidChosenDestination", { name: androidTargetName })
            : t("common.androidDefaultDestination")}
        </p>
      ) : null}

      {showOverwrite ? (
        <div className="flex items-center gap-2 text-[13px]">
          <span className="muted">{t("common.overwrite")}:</span>
          <div className="seg">
            <button
              type="button"
              data-active={session.overwrite === "unique_name"}
              onClick={() => session.setOverwrite("unique_name")}
            >
              {t("overwrite.createNew")}
            </button>
            <button
              type="button"
              data-active={session.overwrite === "replace"}
              onClick={() => session.setOverwrite("replace")}
            >
              {t("overwrite.replace")}
            </button>
          </div>
        </div>
      ) : null}

      {children}

      {session.progress && session.running ? (
        <div className="flex flex-col gap-1.5">
          <div className="flex items-center justify-between text-xs muted">
            <span className="flex items-center gap-1.5">
              <Spinner size={13} />
              {t(`progress.${session.progress.stage.split(".")[0]}`) ===
              `progress.${session.progress.stage.split(".")[0]}`
                ? t("progress.processing")
                : t(`progress.${session.progress.stage.split(".")[0]}`)}
              {session.progress.message ? ` · ${session.progress.message}` : ""}
            </span>
            <span className="tabular-nums">
              {session.progress.total > 1 ? `${session.progress.current} / ${session.progress.total}` : ""}
              {percent !== null ? ` · ${percent}%` : ""}
            </span>
          </div>
          <div className="progress-track">
            <div className="progress-fill" style={{ width: `${percent ?? 8}%` }} />
          </div>
        </div>
      ) : null}

      <div className="flex items-center gap-2">
        <Button
          variant="primary"
          size="lg"
          onClick={onRun}
          disabled={disabled || session.running}
          icon={session.running ? <Loader2 size={16} className="spin" /> : undefined}
        >
          {session.running ? t("common.processing") : runLabel}
        </Button>
        {session.running ? (
          <Button variant="ghost" size="lg" onClick={session.cancel}>
            {t("progress.cancel")}
          </Button>
        ) : null}
      </div>
    </Card>
  );
}

export function ResultCard({ result, onReset }: { result: OpResult; onReset?: () => void }) {
  const t = useT();
  const reveal = useCallback(() => {
    void revealAnyFile(result.path).catch(() => undefined);
  }, [result.path]);
  const open = useCallback(() => {
    void openAnyFile(result.path).catch(() => undefined);
  }, [result.path]);

  return (
    <Card className="p-4 flex flex-wrap items-center gap-3 fade-in" soft>
      <CheckCircle2 size={18} style={{ color: "var(--ok)" }} />
      <div className="flex-1 min-w-0">
        <p className="font-semibold text-[13.5px] truncate" title={result.path}>
          {fileBaseName(result.path)}
        </p>
        <p className="text-xs muted">
          {result.outputBytes ? formatBytes(result.outputBytes) : ""}
          {result.reduction !== undefined && result.reduction !== null
            ? ` · ${t("common.reduction")}: ${Math.round(result.reduction * 100)}%`
            : ""}
          {result.pageCount !== undefined && result.pageCount !== null
            ? ` · ${result.pageCount} ${t("common.pages")}`
            : ""}
          {result.message ? ` · ${result.message}` : ""}
        </p>
      </div>
      <div className="flex items-center gap-2">
        <Button size="sm" icon={<FileText size={14} />} onClick={open}>
          {t("common.openFile")}
        </Button>
        <Button size="sm" variant="ghost" icon={<FolderOpen size={14} />} onClick={reveal}>
          {isAndroid() ? t("common.share") : t("common.openFolder")}
        </Button>
        {onReset ? (
          <IconButton label={t("common.close")} onClick={onReset}>
            <X size={15} />
          </IconButton>
        ) : null}
      </div>
    </Card>
  );
}

// ---------------------------------------------------------------------------
// Dialogs
// ---------------------------------------------------------------------------

export function OverwriteDialog() {
  const t = useT();
  const prompt = useOverwritePrompt((s) => s.prompt);
  const answer = useOverwritePrompt((s) => s.answer);
  if (!prompt) return null;
  return (
    <Modal
      title={t("overwrite.title")}
      onClose={() => answer(null)}
      footer={
        <>
          <Button variant="ghost" onClick={() => answer(null)}>
            {t("overwrite.cancel")}
          </Button>
          <Button onClick={() => answer("unique_name")}>{t("overwrite.createNew")}</Button>
          <Button variant="primary" onClick={() => answer("replace")}>
            {t("overwrite.replace")}
          </Button>
        </>
      }
    >
      <p className="text-[13.5px]">{t("overwrite.body", { name: prompt.fileName })}</p>
    </Modal>
  );
}

export function PasswordDialog() {
  const request = usePasswordPrompt((s) => s.request);
  if (!request) return null;
  // Mounting a fresh body per request resets the draft and the error flag
  // without a setState cascade.
  return <PasswordDialogBody key={request.path} />;
}

function PasswordDialogBody() {
  const t = useT();
  const request = usePasswordPrompt((s) => s.request)!;
  const answer = usePasswordPrompt((s) => s.answer);
  const [value, setValue] = useState("");
  const [checking, setChecking] = useState(false);
  const [incorrect, setIncorrect] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    setTimeout(() => inputRef.current?.focus(), 50);
  }, []);

  const submit = async () => {
    if (!value) return;
    setChecking(true);
    const ok = await checkPassword(request.path, value).catch(() => false);
    setChecking(false);
    if (ok) {
      answer(value);
    } else {
      setIncorrect(true);
    }
  };

  return (
    <Modal
      title={t("passwordModal.title")}
      onClose={() => answer(null)}
      footer={
        <>
          <Button variant="ghost" onClick={() => answer(null)}>
            {t("common.cancel")}
          </Button>
          <Button
            variant="primary"
            onClick={() => void submit()}
            disabled={!value || checking}
            icon={checking ? <Spinner size={14} /> : <Lock size={14} />}
          >
            {t("passwordModal.unlock")}
          </Button>
        </>
      }
    >
      <p className="text-[13.5px] mb-4">{t("passwordModal.body", { name: fileBaseName(request.path) })}</p>
      <input
        ref={inputRef}
        className="input"
        type="password"
        value={value}
        placeholder={t("passwordModal.placeholder")}
        onChange={(event) => {
          setValue(event.target.value);
          setIncorrect(false);
        }}
        onKeyDown={(event) => {
          if (event.key === "Enter") void submit();
        }}
      />
      {incorrect ? (
        <p className="text-xs mt-2" style={{ color: "var(--danger)" }}>
          {t("passwordModal.incorrect")}
        </p>
      ) : null}
      <p className="text-xs muted mt-3 flex items-center gap-1.5">
        <Lock size={12} /> Passwords are never stored or written to logs.
      </p>
    </Modal>
  );
}

// ---------------------------------------------------------------------------
// Toasts + global progress
// ---------------------------------------------------------------------------

export function Toasts() {
  const toasts = useToasts((s) => s.toasts);
  const dismiss = useToasts((s) => s.dismiss);
  return (
    <div className="fixed bottom-4 right-4 z-[70] flex flex-col gap-2">
      {toasts.map((toast) => (
        <div
          key={toast.id}
          className="toast fade-in"
          style={{
            borderLeftColor:
              toast.kind === "error" ? "var(--danger)" : toast.kind === "success" ? "var(--ok)" : "var(--accent)",
          }}
          role="status"
        >
          {toast.kind === "error" ? (
            <XCircle size={16} style={{ color: "var(--danger)", marginTop: 1 }} />
          ) : (
            <CheckCircle2 size={16} style={{ color: "var(--ok)", marginTop: 1 }} />
          )}
          <div className="flex-1 min-w-0">
            <p className="font-semibold text-[13px]">{toast.title}</p>
            {toast.detail ? <p className="text-xs muted break-words">{toast.detail}</p> : null}
          </div>
          <IconButton label="Dismiss" onClick={() => dismiss(toast.id)}>
            <X size={14} />
          </IconButton>
        </div>
      ))}
    </div>
  );
}
