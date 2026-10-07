import { beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  startupFiles: vi.fn(),
  takeLaunchFiles: vi.fn(),
  onLaunchFilesQueued: vi.fn(),
}));
vi.mock("./api", () => api);

import { watchLaunchFiles } from "./launch-files";

/** Lets the promise chains the watcher builds run to completion. */
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("watchLaunchFiles", () => {
  let fire: () => void;
  let unlisten: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    unlisten = vi.fn();
    fire = () => undefined;
    api.startupFiles.mockResolvedValue([]);
    api.takeLaunchFiles.mockResolvedValue([]);
    api.onLaunchFilesQueued.mockImplementation(async (handler: () => void) => {
      fire = handler;
      return unlisten;
    });
  });

  it("delivers the files of this launch", async () => {
    api.startupFiles.mockResolvedValue(["C:\\docs\\a.docx"]);
    const deliver = vi.fn();
    watchLaunchFiles(deliver);
    await settle();
    expect(deliver).toHaveBeenCalledWith(["C:\\docs\\a.docx"]);
  });

  it("picks up files that were forwarded before the listener existed", async () => {
    api.takeLaunchFiles.mockResolvedValueOnce(["C:\\docs\\early.docx", "C:\\docs\\early.xlsx"]);
    const deliver = vi.fn();
    watchLaunchFiles(deliver);
    await settle();
    expect(deliver).toHaveBeenCalledWith(["C:\\docs\\early.docx", "C:\\docs\\early.xlsx"]);
  });

  it("drains the queue again whenever a later launch announces files", async () => {
    const deliver = vi.fn();
    watchLaunchFiles(deliver);
    await settle();
    expect(deliver).not.toHaveBeenCalled();

    api.takeLaunchFiles.mockResolvedValueOnce(["C:\\docs\\later.pptx"]);
    fire();
    await settle();
    expect(deliver).toHaveBeenCalledTimes(1);
    expect(deliver).toHaveBeenCalledWith(["C:\\docs\\later.pptx"]);
  });

  it("never delivers an empty batch", async () => {
    const deliver = vi.fn();
    watchLaunchFiles(deliver);
    await settle();
    fire();
    await settle();
    expect(deliver).not.toHaveBeenCalled();
  });

  it("stops listening and delivering after cleanup", async () => {
    const deliver = vi.fn();
    const stop = watchLaunchFiles(deliver);
    await settle();
    stop();
    expect(unlisten).toHaveBeenCalledTimes(1);

    api.takeLaunchFiles.mockResolvedValueOnce(["C:\\docs\\late.docx"]);
    fire();
    await settle();
    expect(deliver).not.toHaveBeenCalled();
  });

  it("unsubscribes when cleanup runs before the listener is registered", async () => {
    let register: (stop: () => void) => void = () => undefined;
    api.onLaunchFilesQueued.mockImplementation(() => new Promise<() => void>((resolve) => (register = resolve)));
    const stop = watchLaunchFiles(vi.fn());
    stop();
    register(unlisten);
    await settle();
    expect(unlisten).toHaveBeenCalledTimes(1);
    expect(api.takeLaunchFiles).not.toHaveBeenCalled();
  });

  it("survives a backend that is unavailable", async () => {
    api.startupFiles.mockRejectedValue(new Error("no tauri"));
    api.takeLaunchFiles.mockRejectedValue(new Error("no tauri"));
    api.onLaunchFilesQueued.mockRejectedValue(new Error("no tauri"));
    const deliver = vi.fn();
    expect(() => watchLaunchFiles(deliver)).not.toThrow();
    await settle();
    expect(deliver).not.toHaveBeenCalled();
  });
});
