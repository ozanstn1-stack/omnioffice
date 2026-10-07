import { afterEach, describe, expect, it, vi } from "vitest";
import { canProbeRegex, probeRegex, PROBE_TIMEOUT_MS } from "./regex-probe";

/** A worker stand-in that answers (or hangs, like a catastrophic pattern). */
class FakeWorker {
  static answer = true;
  static instances: FakeWorker[] = [];
  onmessage: ((event: MessageEvent) => void) | null = null;
  onerror: (() => void) | null = null;
  terminated = false;
  received: unknown = null;
  constructor(_url: string) {
    FakeWorker.instances.push(this);
  }
  postMessage(data: unknown) {
    this.received = data;
    if (FakeWorker.answer) queueMicrotask(() => this.onmessage?.({ data: "ok" } as MessageEvent));
  }
  terminate() {
    this.terminated = true;
  }
}

describe("regex probe", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.useRealTimers();
    FakeWorker.instances = [];
  });

  it("matches directly where workers do not exist", async () => {
    vi.stubGlobal("Worker", undefined);
    expect(canProbeRegex()).toBe(false);
    await expect(probeRegex(/a/gu, ["a"]).promise).resolves.toBe("ok");
  });

  it("reports ok once the worker has scanned every paragraph", async () => {
    vi.stubGlobal("Worker", FakeWorker);
    FakeWorker.answer = true;
    await expect(probeRegex(/b+/giu, ["abc", "bb"]).promise).resolves.toBe("ok");
    const [worker] = FakeWorker.instances;
    expect(worker.received).toEqual({ source: "b+", flags: "giu", texts: ["abc", "bb"] });
    expect(worker.terminated).toBe(true);
  });

  it("stops a pattern that does not finish in time and reports it as slow", async () => {
    vi.useFakeTimers();
    vi.stubGlobal("Worker", FakeWorker);
    FakeWorker.answer = false;
    const probe = probeRegex(/(\p{L}+\s?)+;/gu, ["a long paragraph without a semicolon"]);
    vi.advanceTimersByTime(PROBE_TIMEOUT_MS);
    await expect(probe.promise).resolves.toBe("slow");
    expect(FakeWorker.instances[0].terminated).toBe(true);
  });
});
