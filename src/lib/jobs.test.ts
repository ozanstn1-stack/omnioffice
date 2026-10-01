import { createElement } from "react";
import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

// Both Tauri APIs are mocked with configurable spies. The default behaviour is
// "Tauri unavailable" (rejects) so the store must keep working in-memory; the
// persistence tests override the implementation per test.
const { invokeMock, listenMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  listenMock: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: listenMock }));

import {
  attachJobEvents,
  MAX_JOBS,
  registerJobRetryHandler,
  resetJobSyncThrottle,
  resumeJobs,
  useJobs,
  type JobRecord,
} from "./jobs";
import { JobsScreen } from "../screens/Jobs";

function makeRecord(patch: Partial<JobRecord> & Pick<JobRecord, "id">): JobRecord {
  return {
    kind: "pdf",
    title: patch.id,
    status: "interrupted",
    progress: 0,
    detail: null,
    payload: null,
    error: null,
    createdAt: 1,
    updatedAt: 2,
    ...patch,
  };
}

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockRejectedValue(new Error("tauri unavailable"));
  listenMock.mockReset();
  listenMock.mockRejectedValue(new Error("tauri unavailable"));
  resetJobSyncThrottle();
  useJobs.setState({ jobs: [] });
});

describe("job store", () => {
  it("starts a running job", () => {
    const id = useJobs.getState().start({ id: "job-1", kind: "pdf", title: "Merge PDFs" });
    expect(id).toBe("job-1");
    const [job] = useJobs.getState().jobs;
    expect(job).toMatchObject({
      id: "job-1",
      kind: "pdf",
      title: "Merge PDFs",
      status: "running",
      stage: "",
      current: 0,
      total: 0,
    });
    expect(typeof job.startedAt).toBe("number");
    expect(job.finishedAt).toBeUndefined();
  });

  it("updates progress for a known job", () => {
    useJobs.getState().start({ id: "job-1", kind: "ai", title: "Summarize" });
    useJobs.getState().progress("job-1", { stage: "Reading", current: 2, total: 10, message: "page 2" });
    const job = useJobs.getState().jobs.find((entry) => entry.id === "job-1")!;
    expect(job.stage).toBe("Reading");
    expect(job.current).toBe(2);
    expect(job.total).toBe(10);
    expect(job.message).toBe("page 2");
  });

  it("surfaces a Rust-tracked job that never called start", () => {
    // Every PDF tool reports progress through this store without registering
    // first; before this, live rows were invisible until an app restart.
    useJobs.getState().progress("rust-job", { stage: "merge", current: 1, total: 4 });
    const job = useJobs.getState().jobs.find((entry) => entry.id === "rust-job");
    expect(job?.status).toBe("running");
    expect(job?.current).toBe(1);
    expect(job?.total).toBe(4);
  });

  it("finishes jobs as succeeded or failed", () => {
    useJobs.getState().start({ id: "ok", kind: "pdf", title: "Ok" });
    useJobs.getState().finish("ok", "succeeded");
    const ok = useJobs.getState().jobs.find((entry) => entry.id === "ok")!;
    expect(ok.status).toBe("succeeded");
    expect(typeof ok.finishedAt).toBe("number");
    expect(ok.error).toBeUndefined();

    useJobs.getState().start({ id: "bad", kind: "vault", title: "Bad" });
    useJobs.getState().finish("bad", "failed", "disk full");
    const bad = useJobs.getState().jobs.find((entry) => entry.id === "bad")!;
    expect(bad.status).toBe("failed");
    expect(bad.error).toBe("disk full");
  });

  it("cancels a running job through its callback", () => {
    const cancel = vi.fn();
    useJobs.getState().start({ id: "job-1", kind: "pdf", title: "Merge", cancel });
    useJobs.getState().cancel("job-1");
    expect(cancel).toHaveBeenCalledTimes(1);
    const job = useJobs.getState().jobs.find((entry) => entry.id === "job-1")!;
    expect(job.status).toBe("cancelled");
    expect(typeof job.finishedAt).toBe("number");
    useJobs.getState().cancel("job-1");
    expect(cancel).toHaveBeenCalledTimes(1);
  });

  it("cancels a restored running job through the Rust registry", async () => {
    useJobs.setState({
      jobs: [
        {
          id: "job-restored",
          kind: "vault",
          title: "Vault scan",
          status: "running",
          stage: "",
          current: 0,
          total: 0,
          startedAt: 1,
          persisted: true,
        },
      ],
    });
    useJobs.getState().cancel("job-restored");
    expect(useJobs.getState().jobs[0].status).toBe("cancelled");
    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("cancel_job", { jobId: "job-restored" }),
    );
  });

  it("retries a finished job through its callback and restarts it", () => {
    const retry = vi.fn();
    useJobs.getState().start({ id: "job-1", kind: "ai", title: "Ask", retry });
    useJobs.getState().finish("job-1", "failed", "network");
    useJobs.getState().retry("job-1");
    expect(retry).toHaveBeenCalledTimes(1);
    const job = useJobs.getState().jobs.find((entry) => entry.id === "job-1")!;
    expect(job.status).toBe("running");
    expect(job.error).toBeUndefined();
    expect(job.finishedAt).toBeUndefined();
    useJobs.getState().retry("missing");
    expect(retry).toHaveBeenCalledTimes(1);
  });

  it("retries without a callback without touching the job", async () => {
    useJobs.getState().start({ id: "job-1", kind: "pdf", title: "Merge" });
    useJobs.getState().finish("job-1", "failed", "boom");
    await useJobs.getState().retry("job-1");
    expect(useJobs.getState().jobs[0].status).toBe("failed");
  });

  it("clears finished jobs and keeps running ones", () => {
    useJobs.getState().start({ id: "a", kind: "pdf", title: "A" });
    useJobs.getState().start({ id: "b", kind: "pdf", title: "B" });
    useJobs.getState().finish("a", "succeeded");
    expect(useJobs.getState().running()).toBe(1);
    useJobs.getState().clearFinished();
    expect(useJobs.getState().jobs.map((entry) => entry.id)).toEqual(["b"]);
    expect(useJobs.getState().running()).toBe(1);
  });

  it("keeps at most 50 jobs, dropping the oldest finished ones", () => {
    for (let index = 0; index < 55; index += 1) {
      useJobs.getState().start({ id: `job-${index}`, kind: "pdf", title: `Job ${index}`, startedAt: index });
      useJobs.getState().finish(`job-${index}`, "succeeded");
    }
    const jobs = useJobs.getState().jobs;
    expect(jobs).toHaveLength(MAX_JOBS);
    expect(jobs.some((job) => job.id === "job-0")).toBe(false);
    expect(jobs.some((job) => job.id === "job-54")).toBe(true);
  });

  it("enforces the cap even when every job is running", () => {
    for (let index = 0; index < 55; index += 1) {
      useJobs.getState().start({ id: `job-${index}`, kind: "office", title: `Job ${index}` });
    }
    expect(useJobs.getState().jobs).toHaveLength(MAX_JOBS);
    expect(useJobs.getState().running()).toBe(MAX_JOBS);
  });
});

describe("persisted history", () => {
  it("hydrates records, maps done to succeeded and sorts newest first", () => {
    useJobs.getState().hydrate([
      makeRecord({ id: "done-job", status: "done", progress: 1, createdAt: 10, updatedAt: 20 }),
      makeRecord({
        id: "stopped",
        kind: "vault",
        title: "Vault scan",
        status: "interrupted",
        progress: 0.5,
        detail: "scan",
        payload: { folders: ["C:/vault"] },
        createdAt: 30,
        updatedAt: 40,
      }),
    ]);
    const jobs = useJobs.getState().jobs;
    expect(jobs.map((job) => job.id)).toEqual(["stopped", "done-job"]);
    expect(jobs[0]).toMatchObject({
      status: "interrupted",
      persisted: true,
      current: 50,
      total: 100,
      startedAt: 30,
      finishedAt: 40,
    });
    expect(jobs[1].status).toBe("succeeded");
  });

  it("does not clobber a live job with the same id", () => {
    useJobs.getState().start({ id: "job-1", kind: "pdf", title: "Live", payload: { fresh: true } });
    useJobs.getState().hydrate([makeRecord({ id: "job-1", title: "Stale", createdAt: 5000 })]);
    const jobs = useJobs.getState().jobs;
    expect(jobs).toHaveLength(1);
    expect(jobs[0].title).toBe("Live");
    expect(jobs[0].persisted).toBeUndefined();
  });

  it("resumes persisted history on bootstrap", async () => {
    const record = makeRecord({ id: "stopped", status: "interrupted", payload: { path: "a.pdf" } });
    invokeMock.mockImplementation(async (command: string) => (command === "jobs_list" ? [record] : null));
    await resumeJobs();
    const jobs = useJobs.getState().jobs;
    expect(jobs).toHaveLength(1);
    expect(jobs[0]).toMatchObject({ id: "stopped", status: "interrupted", persisted: true });
  });

  it("routes a restored job back through jobs_retry and the kind handler", async () => {
    const record = makeRecord({
      id: "stopped",
      kind: "ocr",
      title: "OCR scan",
      progress: 0.42,
      payload: { input: "C:/in.pdf", output: "C:/out.pdf" },
    });
    invokeMock.mockImplementation(async (command: string) => (command === "jobs_retry" ? record : null));
    const handler = vi.fn();
    const unregister = registerJobRetryHandler("ocr", handler);
    useJobs.getState().hydrate([record]);

    await expect(useJobs.getState().retry("stopped")).resolves.toBe("started");
    expect(invokeMock).toHaveBeenCalledWith("jobs_retry", { id: "stopped" });
    expect(handler).toHaveBeenCalledWith(record);
    unregister();
  });

  it("reports unavailable when no retry handler covers the kind", async () => {
    const record = makeRecord({ id: "stopped", kind: "mystery-kind" });
    invokeMock.mockImplementation(async (command: string) => (command === "jobs_retry" ? record : null));
    useJobs.getState().hydrate([record]);
    await expect(useJobs.getState().retry("stopped")).resolves.toBe("unavailable");
  });

  it("clears interrupted rows but keeps queued and running work, mirroring it to Rust", async () => {
    useJobs.getState().hydrate([
      makeRecord({ id: "stopped", status: "interrupted", createdAt: 1 }),
      makeRecord({ id: "queued", status: "queued", createdAt: 2 }),
      makeRecord({ id: "running", status: "running", createdAt: 3 }),
    ]);
    useJobs.getState().clearFinished();
    expect(useJobs.getState().jobs.map((job) => job.id)).toEqual(["running", "queued"]);
    await vi.waitFor(() => expect(invokeMock).toHaveBeenCalledWith("jobs_clear_finished"));
  });

  it("mirrors start/progress/finish to the Rust store commands", async () => {
    useJobs.getState().start({ id: "job-1", kind: "pdf", title: "Merge", payload: { inputs: ["a.pdf"] } });
    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("jobs_register", {
        id: "job-1",
        kind: "pdf",
        title: "Merge",
        payload: { inputs: ["a.pdf"] },
      }),
    );
    useJobs.getState().progress("job-1", { stage: "merge", current: 1, total: 4 });
    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("jobs_progress", {
        id: "job-1",
        stage: "merge",
        current: 1,
        total: 4,
        message: null,
      }),
    );
    useJobs.getState().finish("job-1", "failed", "boom");
    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("jobs_finish", {
        id: "job-1",
        status: "failed",
        error: "boom",
      }),
    );
  });
});

describe("JobsScreen", () => {
  it("renders an interrupted job with an interrupted badge and a retry action", () => {
    useJobs.setState({
      jobs: [
        {
          id: "stopped",
          kind: "vault",
          title: "Vault scan",
          status: "interrupted",
          stage: "",
          current: 0,
          total: 0,
          startedAt: 1,
          finishedAt: 2,
          persisted: true,
          payload: { folders: [] },
        },
      ],
    });
    render(createElement(JobsScreen));
    expect(screen.getByText("Interrupted")).toBeInTheDocument();
    expect(screen.getByText("Vault scan")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Retry/ })).toBeInTheDocument();
    // The honest hint: the job is not running anymore.
    expect(screen.getByText(/not working anymore/)).toBeInTheDocument();
  });

  it("renders a running job without a retry action", () => {
    useJobs.setState({
      jobs: [
        {
          id: "live",
          kind: "pdf",
          title: "Merge",
          status: "running",
          stage: "merge",
          current: 1,
          total: 2,
          startedAt: 1,
        },
      ],
    });
    render(createElement(JobsScreen));
    expect(screen.getByText("Running")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Retry/ })).toBeNull();
  });
});

describe("attachJobEvents", () => {
  it("swallows listener failures outside Tauri", async () => {
    await expect(attachJobEvents()).resolves.toBeUndefined();
    await expect(attachJobEvents()).resolves.toBeUndefined();
  });
});
