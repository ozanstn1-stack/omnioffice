import { beforeEach, describe, expect, it, vi } from "vitest";
import { attachJobEvents, MAX_JOBS, useJobs } from "./jobs";

vi.mock("@tauri-apps/api/event", () => {
  throw new Error("tauri unavailable");
});

vi.mock("@tauri-apps/api/core", () => {
  throw new Error("tauri unavailable");
});

beforeEach(() => {
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

  it("updates progress for a known job only", () => {
    useJobs.getState().start({ id: "job-1", kind: "ai", title: "Summarize" });
    useJobs.getState().progress("job-1", { stage: "Reading", current: 2, total: 10, message: "page 2" });
    const job = useJobs.getState().jobs.find((entry) => entry.id === "job-1")!;
    expect(job.stage).toBe("Reading");
    expect(job.current).toBe(2);
    expect(job.total).toBe(10);
    expect(job.message).toBe("page 2");
    useJobs.getState().progress("missing", { stage: "Nope" });
    expect(useJobs.getState().jobs).toHaveLength(1);
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

  it("retries without a callback without touching the job", () => {
    useJobs.getState().start({ id: "job-1", kind: "pdf", title: "Merge" });
    useJobs.getState().finish("job-1", "failed", "boom");
    useJobs.getState().retry("job-1");
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

describe("attachJobEvents", () => {
  it("swallows import failures outside Tauri", async () => {
    await expect(attachJobEvents()).resolves.toBeUndefined();
    await expect(attachJobEvents()).resolves.toBeUndefined();
  });
});
