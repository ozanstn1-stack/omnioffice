/**
 * Persistence helpers bridging the in-memory job store to Rust's `JobStore`
 * (`src-tauri/src/jobs.rs`). All IPC calls are fire-and-forget so tests and
 * plain web previews continue operating without a Tauri backend.
 */
import { asJson, call, callPlain } from "./typed-call";

/** UI lifecycle. `succeeded` is the persisted `done` mapped at the boundary. */
export type JobStatus = "queued" | "running" | "succeeded" | "failed" | "cancelled" | "interrupted";

/** Lifecycle as stored by the Rust JobStore (`done`, not `succeeded`). */
export type PersistedJobStatus = "queued" | "running" | "done" | "failed" | "cancelled" | "interrupted";

/** Persisted record returned by `jobs_list` / `jobs_retry` (camelCase serde). */
export interface JobRecord {
  id: string;
  kind: string;
  title: string;
  status: PersistedJobStatus;
  /** Normalized 0..1. */
  progress: number;
  detail: string | null;
  /** Serialized operation inputs captured at start; handed to Retry handlers. */
  payload: unknown;
  error: string | null;
  createdAt: number;
  updatedAt: number;
}

export interface BackgroundJob {
  id: string;
  kind: string;
  title: string;
  status: JobStatus;
  stage: string;
  current: number;
  total: number;
  message?: string;
  startedAt: number;
  finishedAt?: number;
  error?: string;
  /** Serialized inputs forwarded to `jobs_register` so a restart can retry. */
  payload?: unknown;
  /** True when the row was restored from Rust and has no live callbacks. */
  persisted?: boolean;
  cancel?: () => void;
  retry?: () => void;
}

export interface JobProgressUpdate {
  stage?: string;
  current?: number;
  total?: number;
  message?: string;
}

/**
 * Mirror at most one progress update per job per this interval (Rust also
 * throttles its disk writes at 500 ms; this keeps the IPC traffic small).
 */
const PROGRESS_SYNC_MS = 500;

/** Per-job timestamps of the last progress mirror. */
const progressSyncedAt = new Map<string, number>();

/** Clears the progress mirror throttle (used by tests). */
export function resetJobSyncThrottle(): void {
  progressSyncedAt.clear();
}

/**
 * Persisted record -> UI row. `done` becomes `succeeded`, the fraction becomes
 * a 0..100 pair so the existing progress bar needs no special case.
 */
export function fromRecord(record: JobRecord): BackgroundJob {
  const status: JobStatus = record.status === "done" ? "succeeded" : record.status;
  const fraction = Number.isFinite(record.progress) ? Math.min(1, Math.max(0, record.progress)) : 0;
  const active = status === "running" || status === "queued";
  return {
    id: record.id,
    kind: record.kind,
    title: record.title,
    status,
    stage: record.detail ?? "",
    current: Math.round(fraction * 100),
    total: 100,
    message: record.detail ?? undefined,
    startedAt: record.createdAt,
    finishedAt: active ? undefined : record.updatedAt,
    error: record.error ?? undefined,
    payload: record.payload ?? undefined,
    persisted: true,
  };
}

async function quiet(work: () => Promise<unknown>): Promise<void> {
  try {
    await work();
  } catch {
    // Best effort: the backend is absent in browser tests, and a job that
    // cannot be persisted must still run.
  }
}

export function persistRegister(job: BackgroundJob): Promise<void> {
  return quiet(() => call((c) => c.jobsRegister(job.id, job.kind, job.title, asJson(job.payload ?? null))));
}

export async function persistProgress(id: string, update: JobProgressUpdate): Promise<void> {
  const now = Date.now();
  const last = progressSyncedAt.get(id) ?? 0;
  if (now - last < PROGRESS_SYNC_MS) return;
  progressSyncedAt.set(id, now);
  await quiet(() =>
    callPlain((c) =>
      c.jobsProgress(id, update.stage ?? "", update.current ?? 0, update.total ?? 0, update.message ?? null),
    ),
  );
}

export function persistFinish(
  id: string,
  status: "succeeded" | "failed" | "cancelled" | "interrupted",
  error?: string,
): Promise<void> {
  progressSyncedAt.delete(id);
  return quiet(() => call((c) => c.jobsFinish(id, status, error ?? null)));
}

export async function persistCancel(id: string): Promise<void> {
  progressSyncedAt.delete(id);
  await quiet(async () => {
    await callPlain((c) => c.cancelJob(id));
    await call((c) => c.jobsFinish(id, "cancelled", null));
  });
}

export function persistClearFinished(): Promise<void> {
  return quiet(() => callPlain((c) => c.jobsClearFinished()));
}
