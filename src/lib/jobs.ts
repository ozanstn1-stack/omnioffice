/**
 * Background job center store.
 *
 * Two layers live here:
 *  - the in-memory view (`useJobs`) consumed by the Jobs screen and the sidebar
 *    badge, updated by the `job:progress` / `ai:progress` / `ai:chunk` events,
 *  - a best-effort sync bridge to the Rust `JobStore` (src-tauri/src/jobs.rs),
 *    which persists the same records to `<app config dir>/jobs.json` so history
 *    survives Android activity recreation and full app restarts.
 *
 * Wire contracts (keep both sides in sync, see src-tauri/src/jobs.rs):
 *  - events: `job:progress` { jobId, stage, current, total, message? }
 *            `ai:progress`  { jobId, stage, current, total }
 *            `ai:chunk`     { jobId, delta, kind: "content" | "reasoning" }
 *  - commands: jobs_list, jobs_clear_finished, jobs_register, jobs_progress,
 *    jobs_finish, jobs_retry.
 *
 * Android honesty: a Rust worker keeps running while the process lives. This
 * persistence makes the *state* survive process death, it does NOT keep the
 * process alive; without a foreground service (out of scope for V3.1) Android
 * can kill background work. A job that died that way shows up as `interrupted`
 * on the next start with a Retry action, and no UI text here claims liveness.
 */
import { create } from "zustand";
import type { ProgressPayload } from "./types";

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

export type JobInput = Pick<BackgroundJob, "id" | "kind" | "title"> &
  Partial<Omit<BackgroundJob, "id" | "kind" | "title">>;

/** Result of a Retry press; `unavailable` means no screen registered a handler. */
export type RetryOutcome = "started" | "missing" | "unavailable" | "failed";

/** A screen that knows how to re-run its own job kind registers one of these. */
export type JobRetryHandler = (record: JobRecord) => void | Promise<void>;

export interface JobsState {
  jobs: BackgroundJob[];
  start: (job: JobInput) => string;
  progress: (id: string, update: JobProgressUpdate) => void;
  finish: (id: string, status: Exclude<JobStatus, "running" | "queued">, error?: string) => void;
  cancel: (id: string) => void;
  retry: (id: string) => Promise<RetryOutcome>;
  /** Merges persisted records (newest survives live entries with the same id). */
  hydrate: (records: JobRecord[]) => void;
  clearFinished: () => void;
  running: () => number;
}

export const MAX_JOBS = 50;

const MAX_STREAM_CHARS = 4000;

/** Mirror at most one progress update per job per this interval (Rust also
 * throttles its disk writes at 500 ms; this keeps the IPC traffic small). */
const PROGRESS_SYNC_MS = 500;

interface AiProgressPayload {
  jobId: string;
  stage: string;
  current: number;
  total: number;
}

interface AiChunkPayload {
  jobId: string;
  delta: string;
  kind: "content" | "reasoning";
}

const retryHandlers = new Map<string, JobRetryHandler>();

/**
 * Registers how a job kind is re-run (e.g. "pdf" -> reopen the tool with the
 * persisted payload). Returns an unregister function for effect cleanup.
 * Kinds without a handler still show a Retry button, but pressing it reports
 * `unavailable` instead of silently doing nothing.
 */
export function registerJobRetryHandler(kind: string, handler: JobRetryHandler): () => void {
  retryHandlers.set(kind, handler);
  return () => {
    if (retryHandlers.get(kind) === handler) retryHandlers.delete(kind);
  };
}

export function hasJobRetryHandler(kind: string): boolean {
  return retryHandlers.has(kind);
}

/** Per-job timestamps of the last progress mirror; exported for tests. */
const progressSyncedAt = new Map<string, number>();

/** Clears the progress mirror throttle (used by tests). */
export function resetJobSyncThrottle(): void {
  progressSyncedAt.clear();
}

function prune(jobs: BackgroundJob[]): BackgroundJob[] {
  if (jobs.length <= MAX_JOBS) return jobs;
  const result = [...jobs];
  for (let index = result.length - 1; index >= 0 && result.length > MAX_JOBS; index -= 1) {
    if (result[index].status !== "running") result.splice(index, 1);
  }
  for (let index = result.length - 1; index >= 0 && result.length > MAX_JOBS; index -= 1) {
    result.splice(index, 1);
  }
  return result;
}

function appendStreamDelta(jobId: string, delta: string): void {
  if (!delta) return;
  useJobs.setState((state) => ({
    jobs: state.jobs.map((job) => {
      if (job.id !== jobId || job.status !== "running") return job;
      const combined = `${job.message ?? ""}${delta}`;
      return {
        ...job,
        message: combined.length > MAX_STREAM_CHARS ? combined.slice(-MAX_STREAM_CHARS) : combined,
      };
    }),
  }));
}

/** Persisted record -> UI row. `done` becomes `succeeded`, the fraction becomes
 * a 0..100 pair so the existing progress bar needs no special case. */
function fromRecord(record: JobRecord): BackgroundJob {
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

// ---------------------------------------------------------------------------
// Rust store bridge (fire-and-forget; the app must work without Tauri in tests
// and in a plain web preview, where every call is swallowed)
// ---------------------------------------------------------------------------

async function persistRegister(job: BackgroundJob): Promise<void> {
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("jobs_register", {
      id: job.id,
      kind: job.kind,
      title: job.title,
      payload: job.payload ?? null,
    });
  } catch {
    // No Tauri (tests/web preview): keep the job in memory only.
  }
}

async function persistProgress(id: string, update: JobProgressUpdate): Promise<void> {
  const now = Date.now();
  const last = progressSyncedAt.get(id) ?? 0;
  if (now - last < PROGRESS_SYNC_MS) return;
  progressSyncedAt.set(id, now);
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("jobs_progress", {
      id,
      stage: update.stage ?? "",
      current: update.current ?? 0,
      total: update.total ?? 0,
      message: update.message ?? null,
    });
  } catch {
    // Best effort: the backend is absent in browser tests, and a job that
    // cannot be persisted must still run.
  }
}

async function persistFinish(
  id: string,
  status: "succeeded" | "failed" | "cancelled" | "interrupted",
  error?: string,
): Promise<void> {
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("jobs_finish", { id, status, error: error ?? null });
  } catch {
    // Best effort: the backend is absent in browser tests, and a job that
    // cannot be persisted must still run.
  }
}

async function persistCancel(id: string): Promise<void> {
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("cancel_job", { jobId: id });
    await invoke("jobs_finish", { id, status: "cancelled", error: null });
  } catch {
    // Best effort: the backend is absent in browser tests, and a job that
    // cannot be persisted must still run.
  }
}

async function persistClearFinished(): Promise<void> {
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("jobs_clear_finished");
  } catch {
    // Best effort: the backend is absent in browser tests, and a job that
    // cannot be persisted must still run.
  }
}

export const useJobs = create<JobsState>((set, get) => ({
  jobs: [],
  start: (input) => {
    const job: BackgroundJob = {
      ...input,
      status: "running",
      stage: input.stage ?? "",
      current: input.current ?? 0,
      total: input.total ?? 0,
      startedAt: input.startedAt ?? Date.now(),
      finishedAt: undefined,
      error: undefined,
    };
    set((state) => ({
      jobs: prune([job, ...state.jobs.filter((entry) => entry.id !== job.id)]),
    }));
    void persistRegister(job);
    return job.id;
  },
  progress: (id, update) => {
    set((state) => {
      const existing = state.jobs.find((job) => job.id === id);
      if (!existing) {
        // The Rust side tracks jobs a screen never registered through `start`
        // (every PDF tool does; before this, live rows were invisible until an
        // app restart because `progress` only updated existing entries).
        const created: BackgroundJob = {
          id,
          kind: "pdf",
          title: id,
          status: "running",
          stage: update.stage ?? "",
          current: update.current ?? 0,
          total: update.total ?? 0,
          message: update.message,
          startedAt: Date.now(),
        };
        return { jobs: prune([created, ...state.jobs]) };
      }
      const jobs = state.jobs.map((job) => {
        if (job.id !== id) return job;
        const next = { ...job };
        if (update.stage !== undefined) next.stage = update.stage;
        if (update.current !== undefined) next.current = update.current;
        if (update.total !== undefined) next.total = update.total;
        if (update.message !== undefined) next.message = update.message;
        return next;
      });
      return { jobs };
    });
    // Mirror AI progress (Rust emits `ai:progress` itself, not job:progress).
    const job = get().jobs.find((entry) => entry.id === id);
    if (job?.status === "running") void persistProgress(id, update);
  },
  finish: (id, status, error) => {
    set((state) => ({
      jobs: state.jobs.map((job) =>
        job.id === id
          ? { ...job, status, finishedAt: Date.now(), error: status === "failed" ? error ?? job.error : undefined }
          : job,
      ),
    }));
    void persistFinish(id, status, error);
  },
  cancel: (id) => {
    const job = get().jobs.find((entry) => entry.id === id);
    if (!job || job.status !== "running") return;
    try {
      job.cancel?.();
    } catch {
      // A throwing cancel callback must not stop the fallback below.
    }
    // Restored rows have no live callback; talk to the Rust registry directly
    // so an actually-running worker still stops.
    if (!job.cancel) void persistCancel(id);
    set((state) => ({
      jobs: state.jobs.map((entry) =>
        entry.id === id && entry.status === "running"
          ? { ...entry, status: "cancelled", finishedAt: Date.now() }
          : entry,
      ),
    }));
  },
  retry: async (id) => {
    const job = get().jobs.find((entry) => entry.id === id);
    if (!job) return "missing";
    // Live jobs keep their original callback: the screen re-runs itself.
    if (job.retry) {
      try {
        job.retry();
      } catch {
        return "failed";
      }
      set((state) => ({
        jobs: state.jobs.map((entry) =>
          entry.id === id && entry.status !== "running"
            ? { ...entry, status: "running", current: 0, message: undefined, finishedAt: undefined, error: undefined }
            : entry,
        ),
      }));
      return "started";
    }
    // Restored jobs carry a payload; route it to the handler registered for
    // their kind (the originating screen), if any.
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const record = await invoke<JobRecord>("jobs_retry", { id });
      const handler = retryHandlers.get(record.kind);
      if (!handler) return "unavailable";
      await handler(record);
      return "started";
    } catch {
      return "failed";
    }
  },
  hydrate: (records) => {
    set((state) => {
      const live = new Map(state.jobs.map((job) => [job.id, job]));
      const restored = records.filter((record) => !live.has(record.id)).map(fromRecord);
      const merged = [...live.values(), ...restored].sort((a, b) => b.startedAt - a.startedAt);
      return { jobs: prune(merged) };
    });
  },
  clearFinished: () => {
    set((state) => ({
      jobs: state.jobs.filter((job) => job.status === "running" || job.status === "queued"),
    }));
    void persistClearFinished();
  },
  running: () => get().jobs.filter((job) => job.status === "running").length,
}));

// ---------------------------------------------------------------------------
// Event wiring / bootstrap
// ---------------------------------------------------------------------------

let attached = false;
let attaching: Promise<void> | null = null;

export function attachJobEvents(): Promise<void> {
  if (attached) return Promise.resolve();
  if (attaching) return attaching;
  attaching = (async () => {
    try {
      const eventApi = await import("@tauri-apps/api/event");
      await import("@tauri-apps/api/core");
      await Promise.all([
        eventApi.listen<ProgressPayload>("job:progress", (event) => {
          useJobs.getState().progress(event.payload.jobId, event.payload);
        }),
        eventApi.listen<AiProgressPayload>("ai:progress", (event) => {
          useJobs.getState().progress(event.payload.jobId, event.payload);
        }),
        eventApi.listen<AiChunkPayload>("ai:chunk", (event) => {
          appendStreamDelta(event.payload.jobId, event.payload.delta);
        }),
      ]);
      attached = true;
    } catch {
      // Without the event bridge the job still runs; only live progress is lost.
    } finally {
      attaching = null;
    }
  })();
  return attaching;
}

/**
 * App bootstrap hook: subscribes to the job events (once) and loads the
 * persisted history from the Rust store, so interrupted jobs show up after an
 * activity recreation or a restart. Safe to call more than once.
 *
 * Integration (App.tsx is owned elsewhere): call `void resumeJobs();` once in
 * the existing bootstrap effect, right next to `attachJobs()`.
 */
export async function resumeJobs(): Promise<void> {
  await attachJobEvents();
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const records = await invoke<JobRecord[]>("jobs_list");
    if (Array.isArray(records)) useJobs.getState().hydrate(records);
  } catch {
    // Outside Tauri there is nothing to restore.
  }
}
