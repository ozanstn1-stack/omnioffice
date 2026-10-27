import { renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();
// Recent-file writes go to their own mock so they do not consume the queue's
// scripted drain results.
const recentMock = vi.fn(async (_command: string, _args?: unknown) => [] as unknown);

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: unknown) =>
    command === "android_take_pending_open" ? invokeMock(command, args) : recentMock(command, args),
}));
vi.mock("./mobile", () => ({ isAndroid: () => true }));

import { useAndroidPendingOpen } from "./android-open-with";

/** A promise the test resolves by hand, like a drain that is still in flight. */
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

async function flush() {
  await Promise.resolve();
  await Promise.resolve();
}

describe("useAndroidPendingOpen", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    invokeMock.mockReset();
    invokeMock.mockResolvedValue([]);
    recentMock.mockClear();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("delivers a drain that resolves after the handler changed", async () => {
    // Cold start from "Open with": the first drain is in flight when the
    // settings load, `t` changes and App passes a new handler. The queue is
    // already truncated on the Rust side, so dropping this result loses the
    // document for good (the app stays on Home).
    const first = deferred<string[]>();
    invokeMock.mockReturnValueOnce(first.promise);
    const before = vi.fn();
    const after = vi.fn();
    const { rerender } = renderHook(
      ({ handler }: { handler: (paths: string[]) => void }) => useAndroidPendingOpen(handler),
      {
        initialProps: { handler: before },
      },
    );
    rerender({ handler: after });

    first.resolve(["/data/cache/incoming/a/report.docx"]);
    await flush();

    expect([...before.mock.calls, ...after.mock.calls]).toEqual([[["/data/cache/incoming/a/report.docx"]]]);
    expect(after).toHaveBeenCalledWith(["/data/cache/incoming/a/report.docx"]);
  });

  it("does not restart the poll when the handler changes", async () => {
    const { rerender } = renderHook(
      ({ handler }: { handler: (paths: string[]) => void }) => useAndroidPendingOpen(handler),
      {
        initialProps: { handler: vi.fn() },
      },
    );
    const callsAfterMount = invokeMock.mock.calls.length;
    rerender({ handler: vi.fn() });
    rerender({ handler: vi.fn() });
    await flush();
    expect(invokeMock.mock.calls.length).toBe(callsAfterMount);
  });

  it("delivers a path only once across drains", async () => {
    invokeMock.mockResolvedValueOnce(["/data/cache/incoming/b/one.pdf"]);
    invokeMock.mockResolvedValueOnce(["/data/cache/incoming/b/one.pdf"]);
    const handler = vi.fn();
    renderHook(() => useAndroidPendingOpen(handler));
    await flush();
    await vi.advanceTimersByTimeAsync(1000);
    expect(handler).toHaveBeenCalledTimes(1);
    expect(handler).toHaveBeenCalledWith(["/data/cache/incoming/b/one.pdf"]);
  });

  it("lists a shared PDF under Recent, but not shortcuts, refusals or office documents", async () => {
    invokeMock.mockResolvedValueOnce([
      "/data/cache/incoming/c/scan.pdf",
      "/data/cache/incoming/d/offer.docx",
      "__action:scan",
      "__error:TOO_LARGE:big.pdf",
    ]);
    renderHook(() => useAndroidPendingOpen(vi.fn()));
    await flush();
    const added = recentMock.mock.calls.filter(([command]) => command === "add_recent").map(([, args]) => args);
    expect(added).toEqual([
      {
        entry: expect.objectContaining({
          path: "/data/cache/incoming/c/scan.pdf",
          fileName: "scan.pdf",
          tool: "_openWith",
        }),
      },
    ]);
  });
});
