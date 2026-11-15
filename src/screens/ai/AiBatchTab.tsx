import type { Dispatch, SetStateAction } from "react";
import { Plus, X } from "lucide-react";
import { fileBaseName } from "../../lib/format";
import type { AiBatchResult } from "../../lib/types";
import { Badge, Button, Field, Segmented, TextInput } from "../../components/ui";

import { AiMatrixPanel } from "./AiMatrixPanel";

export type AiBatchAction = "summarize" | "translate" | "classify" | "extract" | "matrix";

export interface AiBatchTabProps {
  label: (key: string, fallback: string) => string;
  batchFiles: string[];
  setBatchFiles: Dispatch<SetStateAction<string[]>>;
  primaryPath: string | undefined;
  handlePickBatchFiles: () => void;
  batchAction: AiBatchAction;
  setBatchAction: (action: AiBatchAction) => void;
  batchLang: string;
  setBatchLang: (lang: string) => void;
  matrixColumns: string;
  setMatrixColumns: (cols: string) => void;
  batchPasswords: Record<string, string>;
  setBatchPasswords: Dispatch<SetStateAction<Record<string, string>>>;
  batchProgress: { index: number; total: number; path: string } | null;
  batchResult: AiBatchResult | null;
}

export function AiBatchTab(props: AiBatchTabProps) {
  const {
    label,
    batchFiles,
    setBatchFiles,
    primaryPath,
    handlePickBatchFiles,
    batchAction,
    setBatchAction,
    batchLang,
    setBatchLang,
    matrixColumns,
    setMatrixColumns,
    batchPasswords,
    setBatchPasswords,
    batchProgress,
    batchResult,
  } = props;

  return (
    <div className="flex flex-col gap-3">
      <p className="text-[13px] muted">
        {label(
          "ai.batchIntro",
          "Queue multiple documents for batch summarization, translation, classification, or extraction.",
        )}
      </p>

      <div className="flex items-center gap-2 flex-wrap">
        <Button size="sm" icon={<Plus size={14} />} onClick={handlePickBatchFiles}>
          {label("ai.addBatchFiles", "Add Files")}
        </Button>
        {primaryPath && !batchFiles.includes(primaryPath) ? (
          <Button size="sm" variant="ghost" onClick={() => setBatchFiles((prev) => [...prev, primaryPath])}>
            + {label("ai.addCurrentFile", "Add Active File")}
          </Button>
        ) : null}
        {batchFiles.length > 0 ? (
          <Button size="sm" variant="ghost" onClick={() => setBatchFiles([])}>
            {label("ai.clearBatch", "Clear All")}
          </Button>
        ) : null}
      </div>

      <div className="grid gap-3" style={{ gridTemplateColumns: "repeat(auto-fit, minmax(200px, 1fr))" }}>
        <Field label={label("ai.batchAction", "Batch Action")}>
          <Segmented<AiBatchAction>
            value={batchAction}
            onChange={setBatchAction}
            options={[
              { value: "summarize", label: label("ai.actionSummarize", "Summarize") },
              { value: "translate", label: label("ai.actionTranslate", "Translate") },
              { value: "classify", label: label("ai.actionClassify", "Classify") },
              { value: "extract", label: label("ai.actionExtract", "Extract") },
              { value: "matrix", label: label("ai.actionMatrix", "Matrix") },
            ]}
          />
        </Field>

        {batchAction === "translate" ? (
          <Field label={label("ai.batchTargetLang", "Target Language")}>
            <TextInput
              value={batchLang}
              onChange={(e) => setBatchLang(e.target.value)}
              placeholder={label("ai.batchTargetLangPlaceholder", "tr, en, de")}
            />
          </Field>
        ) : null}
      </div>

      {batchAction === "matrix" ? (
        <AiMatrixPanel
          label={label}
          columnsText={matrixColumns}
          setColumnsText={setMatrixColumns}
          batchResult={batchResult}
        />
      ) : null}

      <div className="card-soft p-3 flex flex-col gap-1.5 max-h-[180px] overflow-y-auto">
        <span className="text-xs font-semibold">
          {label("ai.queuedFiles", "Queued Files")} ({batchFiles.length}):
        </span>
        {batchFiles.length === 0 ? (
          <p className="text-xs muted">{label("ai.noFilesQueued", "No files in the batch queue.")}</p>
        ) : (
          batchFiles.map((file, idx) => (
            <div
              key={idx}
              className="flex items-center justify-between gap-2 text-xs p-1 rounded bg-[var(--surface-2)]"
            >
              <span className="truncate flex-1" title={file}>
                {fileBaseName(file)}
              </span>
              <input
                type="password"
                className="input input-sm"
                style={{ width: 110 }}
                placeholder={label("ai.itemPassword", "password")}
                value={batchPasswords[file] ?? ""}
                onChange={(event) => setBatchPasswords((prev) => ({ ...prev, [file]: event.target.value }))}
              />
              <Button
                size="sm"
                variant="ghost"
                onClick={() => setBatchFiles((prev) => prev.filter((_, i) => i !== idx))}
              >
                <X size={12} />
              </Button>
            </div>
          ))
        )}
      </div>

      {batchProgress ? (
        <div className="card-soft p-2 flex flex-col gap-1 text-xs">
          <div className="flex justify-between">
            <span>
              {label("ai.batchProgress", "Processing file")} {batchProgress.index} / {batchProgress.total}
            </span>
            <span className="font-medium">{fileBaseName(batchProgress.path)}</span>
          </div>
          <div className="progress-track">
            <div
              className="progress-fill"
              style={{
                width: `${Math.round((batchProgress.index / batchProgress.total) * 100)}%`,
              }}
            />
          </div>
        </div>
      ) : null}

      {batchResult ? (
        <div className="card-soft p-3 flex flex-col gap-2 max-h-[280px] overflow-y-auto">
          <span className="text-xs font-semibold">
            {label("ai.batchResults", "Queue Results")} ({batchResult.results.length}):
          </span>
          {batchResult.results.map((res, i) => (
            <div
              key={i}
              className="flex flex-col gap-1 p-2 rounded text-xs bg-[var(--surface-2)] border border-[var(--border)]"
            >
              <div className="flex items-center justify-between">
                <span className="font-semibold">{fileBaseName(res.path)}</span>
                <Badge tone={res.success ? "ok" : "danger"}>
                  {res.success ? label("ai.statusSuccess", "Success") : label("ai.statusFailed", "Failed")}
                </Badge>
              </div>
              {res.output ? (
                <p className="text-[12px] muted whitespace-pre-wrap max-h-[80px] overflow-y-auto">{res.output}</p>
              ) : null}
              {res.error ? (
                <p className="text-[12px]" style={{ color: "var(--danger)" }}>
                  {res.error}
                </p>
              ) : null}
              <span className="text-[10px] muted">
                {(res.elapsedMs / 1000).toFixed(2)}s · {res.action}
                {res.estimatedTokens ? ` · ~${res.estimatedTokens} ${label("ai.tokensShort", "tok")}` : ""}
                {res.estimatedCostUsd ? ` · ~$${res.estimatedCostUsd.toFixed(4)}` : ""}
              </span>
            </div>
          ))}
        </div>
      ) : null}
    </div>
  );
}
