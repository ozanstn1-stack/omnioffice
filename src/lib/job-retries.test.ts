import { beforeEach, describe, expect, it, vi } from "vitest";

/** Backend mock: `jobs_retry` returns the persisted record, everything else null. */
const invokeMock = vi.fn(async (_command: string, _args?: unknown) => null as unknown);
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invokeMock(...(args as [string])) }));

import { installJobRetryHandlers, sanitizeRetryPayload, trackJobInvocation } from "./job-retries";
import { useJobs, type JobRecord } from "./jobs";

const persistedRecord: JobRecord = {
  id: "job-9",
  kind: "compress",
  title: "a.pdf · compress",
  status: "failed",
  progress: 1,
  detail: null,
  payload: { request: { input: "C:/docs/a.pdf", jobId: "job-9" } },
  error: "boom",
  createdAt: 1,
  updatedAt: 2,
};

describe("tracked job invocations", () => {
  beforeEach(() => {
    useJobs.setState({ jobs: [] });
    invokeMock.mockClear();
    invokeMock.mockImplementation(async (command: string) => (command === "jobs_retry" ? persistedRecord : null));
  });

  it("persists kind, title and a sanitized payload before the work runs", async () => {
    trackJobInvocation("compress_pdf", {
      request: { input: "C:/docs/a.pdf", password: "top-secret", jobId: "job-1" },
    });
    const job = useJobs.getState().jobs.find((entry) => entry.id === "job-1");
    expect(job?.kind).toBe("compress");
    expect(job?.title).toBe("a.pdf · compress");
    const payload = job?.payload as { request: { password: string | null; jobId: string } };
    expect(payload.request.password).toBeNull();
    expect(payload.request.jobId).toBe("job-1");
    // Persisting is fire-and-forget through a dynamic import; wait for it.
    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "jobs_register",
        expect.objectContaining({ id: "job-1", kind: "compress" }),
      ),
    );
  });

  it("ignores untracked commands and requests without a job id", () => {
    trackJobInvocation("app_info", {});
    trackJobInvocation("compress_pdf", { request: { input: "C:/docs/a.pdf" } });
    expect(useJobs.getState().jobs).toHaveLength(0);
  });

  it("blanks credential fields at every nesting depth", () => {
    const sanitized = sanitizeRetryPayload({
      request: {
        password: "a",
        userPassword: "b",
        options: { ownerPassword: "c", pfxPassword: "d", quality: 80 },
        values: [{ name: "x", apiKey: "e" }],
      },
    }) as {
      request: {
        password: null;
        userPassword: null;
        options: { ownerPassword: null; pfxPassword: null; quality: number };
      };
    };
    expect(sanitized.request.password).toBeNull();
    expect(sanitized.request.userPassword).toBeNull();
    expect(sanitized.request.options.ownerPassword).toBeNull();
    expect(sanitized.request.options.pfxPassword).toBeNull();
    expect(sanitized.request.options.quality).toBe(80);
  });

  it("re-runs a failed job from its persisted payload and marks it running", async () => {
    const uninstall = installJobRetryHandlers();
    try {
      useJobs.getState().hydrate([persistedRecord]);
      invokeMock.mockClear();
      const outcome = await useJobs.getState().retry("job-9");
      expect(outcome).toBe("started");
      expect(invokeMock).toHaveBeenCalledWith("compress_pdf", persistedRecord.payload);
      const job = useJobs.getState().jobs.find((entry) => entry.id === "job-9");
      expect(job?.status).toBe("running");
    } finally {
      uninstall();
    }
  });

  it("reports unavailable once the handlers are uninstalled", async () => {
    const uninstall = installJobRetryHandlers();
    uninstall();
    useJobs.getState().hydrate([persistedRecord]);
    invokeMock.mockClear();
    expect(await useJobs.getState().retry("job-9")).toBe("unavailable");
    expect(invokeMock.mock.calls.some(([command]) => command === "compress_pdf")).toBe(false);
  });
});
