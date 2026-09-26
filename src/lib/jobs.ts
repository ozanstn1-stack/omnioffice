import { create } from "zustand";
import type { ProgressPayload } from "./types";

export type JobStatus = "running" | "succeeded" | "failed" | "cancelled";

export interface BackgroundJob {
  id: string;
  kind: "pdf" | "ai" | "vault" | "office";
  title: string;
  status: JobStatus;
  stage: string;
  current: number;
  total: number;
  message?: string;
  startedAt: number;
  finishedAt?: number;
  error?: string;
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

export interface JobsState {
  jobs: BackgroundJob[];
  start: (job: JobInput) => string;
  progress: (id: string, update: JobProgressUpdate) => void;
  finish: (id: string, status: Exclude<JobStatus, "running">, error?: string) => void;
  cancel: (id: string) => void;
  retry: (id: string) => void;
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
    return job.id;
  },
  progress: (id, update) => {
    set((state) => {
      let found = false;
      const jobs = state.jobs.map((job) => {
        if (job.id !== id) return job;
        found = true;
        const next = { ...job };
        if (update.stage !== undefined) next.stage = update.stage;
        if (update.current !== undefined) next.current = update.current;
        if (update.total !== undefined) next.total = update.total;
        if (update.message !== undefined) next.message = update.message;
        return next;
      });
      return found ? { jobs } : state;
    });
  },
  finish: (id, status, error) => {
    set((state) => ({
      jobs: state.jobs.map((job) =>
        job.id === id
          ? { ...job, status, finishedAt: Date.now(), error: status === "failed" ? error ?? job.error : undefined }
          : job,
      ),
    }));
  },
  cancel: (id) => {
    const job = get().jobs.find((entry) => entry.id === id);
    if (!job || job.status !== "running") return;
    try {
      job.cancel?.();
    } catch {
    }
    set((state) => ({
      jobs: state.jobs.map((entry) =>
        entry.id === id && entry.status === "running"
          ? { ...entry, status: "cancelled", finishedAt: Date.now() }
          : entry,
      ),
    }));
  },
  retry: (id) => {
    const job = get().jobs.find((entry) => entry.id === id);
    if (!job?.retry) return;
    const callback = job.retry;
    try {
      callback();
    } catch {
    }
    set((state) => ({
      jobs: state.jobs.map((entry) =>
        entry.id === id && entry.status !== "running"
          ? { ...entry, status: "running", current: 0, message: undefined, finishedAt: undefined, error: undefined }
          : entry,
      ),
    }));
  },
  clearFinished: () => {
    set((state) => ({ jobs: state.jobs.filter((job) => job.status === "running") }));
  },
  running: () => get().jobs.filter((job) => job.status === "running").length,
}));

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
    } finally {
      attaching = null;
    }
  })();
  return attaching;
}
