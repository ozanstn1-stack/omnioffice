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
 * process alive. While a job runs, Android starts a foreground service (see
 * src-tauri/src/android_background.rs) so the system does not freeze the app
 * in the background, but swiping it away or a battery manager can still end
 * the process. A job that died that way shows up as `interrupted` on the next
 * start with a Retry action, and no UI text here claims liveness.
 */
import { call, callPlain } from "./typed-call";
import { create } from "zustand";
import type { ProgressPayload } from "./types";
import {
  fromRecord,
  persistCancel,
  persistClearFinished,
  persistFinish,
  persistProgress,
  persistRegister,
  resetJobSyncThrottle,
  type BackgroundJob,
  type JobProgressUpdate,
  type JobRecord,
  type JobStatus,
  type PersistedJobStatus,
} from "./jobs-persist";

export type { BackgroundJob, JobProgressUpdate, JobRecord, JobStatus, PersistedJobStatus };
export { resetJobSyncThrottle };

export type JobInput = Pick<BackgroundJob, "id" | "kind" | "title"> &
  Partial<Omit<BackgroundJob, "id" | "kind" | "title">>;

/** Result of a Retry press; `unavailable` means no screen registered a handler. */
export type RetryOutcome = "started" | "missing" | "unavailable" | "failed";

/** A screen that knows how to re-run its own job kind registers one of these. */
export type JobRetryHandler = (record: JobRecord) => void | Promise<void>;

export interface JobsState {
  jobs: BackgroundJob[];
  /** Live raw progress payloads keyed by `jobId`; bounded and evicted on finish/cancel/clear/prune. */
  progressById: Record<string, ProgressPayload>;
  attach: () => Promise<() => void>;
  start: (job: JobInput) => string;
  progress: (id: string, update: JobProgressUpdate) => void;
  finish: (id: string, status: Exclude<JobStatus, "running" | "queued">, error?: string) => void;
  /** Finishes a job or drops transient progress when a tool session completes without registering a row. */
  settle: (id: string, status: Exclude<JobStatus, "running" | "queued">, error?: string) => void;
  cancel: (id: string) => void;
  retry: (id: string) => Promise<RetryOutcome>;
  /** Merges persisted records (newest survives live entries with the same id). */
  hydrate: (records: JobRecord[]) => void;
  clearFinished: () => void;
  running: () => number;
}

export const MAX_JOBS = 50;

const MAX_STREAM_CHARS = 4000;

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

function omitProgressKey(
  progressById: Record<string, ProgressPayload> | undefined,
  id: string,
): Record<string, ProgressPayload> {
  const map = progressById ?? {};
  if (!(id in map)) return map;
  const next = { ...map };
  delete next[id];
  return next;
}

function pruneProgressMap(
  jobs: BackgroundJob[],
  progressById: Record<string, ProgressPayload> | undefined,
): Record<string, ProgressPayload> {
  const map = progressById ?? {};
  const activeIds = new Set(
    jobs.filter((job) => job.status === "running" || job.status === "queued").map((job) => job.id),
  );
  const next: Record<string, ProgressPayload> = {};
  for (const [id, payload] of Object.entries(map)) {
    if (activeIds.has(id)) next[id] = payload;
  }
  return next;
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

export const useJobs = create<JobsState>((set, get) => ({
  jobs: [],
  progressById: {},
  attach: async () => {
    await attachJobEvents();
    return () => undefined;
  },
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
    set((state) => {
      const jobs = prune([job, ...state.jobs.filter((entry) => entry.id !== job.id)]);
      return {
        jobs,
        progressById: pruneProgressMap(jobs, omitProgressKey(state.progressById, job.id)),
      };
    });
    void persistRegister(job);
    return job.id;
  },
  progress: (id, update) => {
    set((state) => {
      const existing = state.jobs.find((job) => job.id === id);
      const active = !existing || existing.status === "running" || existing.status === "queued";
      const prevPayload = state.progressById?.[id];
      const nextPayload: ProgressPayload = {
        jobId: id,
        stage: update.stage ?? prevPayload?.stage ?? existing?.stage ?? "",
        current: update.current ?? prevPayload?.current ?? existing?.current ?? 0,
        total: update.total ?? prevPayload?.total ?? existing?.total ?? 0,
        message: update.message ?? prevPayload?.message ?? existing?.message,
      };
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
        const jobs = prune([created, ...state.jobs]);
        const progressById = pruneProgressMap(jobs, { ...(state.progressById ?? {}), [id]: nextPayload });
        return { jobs, progressById };
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
      const progressById = active
        ? pruneProgressMap(jobs, { ...(state.progressById ?? {}), [id]: nextPayload })
        : omitProgressKey(state.progressById, id);
      return { jobs, progressById };
    });
    // Mirror AI progress (Rust emits `ai:progress` itself, not job:progress).
    const job = get().jobs.find((entry) => entry.id === id);
    if (job?.status === "running") void persistProgress(id, update);
  },
  finish: (id, status, error) => {
    set((state) => ({
      jobs: state.jobs.map((job) =>
        job.id === id
          ? { ...job, status, finishedAt: Date.now(), error: status === "failed" ? (error ?? job.error) : undefined }
          : job,
      ),
      progressById: omitProgressKey(state.progressById, id),
    }));
    void persistFinish(id, status, error);
  },
  settle: (id, status, error) => {
    const state = get();
    const tracked = state.jobs.some((entry) => entry.id === id);
    if (tracked) {
      state.finish(id, status, error);
      return;
    }
    if (state.progressById?.[id]) {
      set((current) => ({ progressById: omitProgressKey(current.progressById, id) }));
    }
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
      progressById: omitProgressKey(state.progressById, id),
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
        progressById: omitProgressKey(state.progressById, id),
      }));
      return "started";
    }
    // Restored jobs carry a payload; route it to the handler registered for
    // their kind (the originating screen), if any.
    try {
      const record = await call((c) => c.jobsRetry(id));
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
      const merged = prune([...live.values(), ...restored].sort((a, b) => b.startedAt - a.startedAt));
      return {
        jobs: merged,
        progressById: pruneProgressMap(merged, state.progressById),
      };
    });
  },
  clearFinished: () => {
    set((state) => {
      const jobs = state.jobs.filter((job) => job.status === "running" || job.status === "queued");
      return {
        jobs,
        progressById: pruneProgressMap(jobs, state.progressById),
      };
    });
    void persistClearFinished();
  },
  running: () => get().jobs.filter((job) => job.status === "running").length,
}));

/** Progress of a single running/queued job (evicted once finished or pruned). */
export function useJobProgress(jobId: string): ProgressPayload | null {
  return useJobs((state) => (jobId ? ((state.progressById ?? {})[jobId] ?? null) : null));
}

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
 * Integration: call `void resumeJobs();` once in the existing bootstrap effect.
 */
export async function resumeJobs(): Promise<void> {
  await attachJobEvents();
  try {
    const records = await callPlain((c) => c.jobsList());
    if (Array.isArray(records)) useJobs.getState().hydrate(records);
  } catch {
    // Outside Tauri there is nothing to restore.
  }
}
