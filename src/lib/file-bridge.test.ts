import { beforeEach, describe, expect, it, vi } from "vitest";

/**
 * pickFileBytes/saveFileBytes are the one entry point the editors and the
 * Data/Draw tools use to read and write a user-chosen file. On Android they
 * must go through the SAF bridge: the desktop dialog plugin cannot open the
 * Android picker, and plugin-fs has no scope for content:// documents.
 */
const dialogOpen = vi.fn(async (_options?: unknown): Promise<string | null> => "C:/pics/photo.webp");
const dialogSave = vi.fn(async (_options?: unknown): Promise<string | null> => "C:/out/data.csv");
const fsRead = vi.fn(async (_path: string) => new Uint8Array([1, 2, 3]));
const fsWrite = vi.fn(async (_path: string, _bytes: Uint8Array) => undefined);
const safOpen = vi.fn(async (_options?: unknown) => [{ uri: "content://picked/1" }]);
const safSave = vi.fn(async (_name: string, _mime: string | null) => ({ uri: "content://saved/1" }));
const safRead = vi.fn(async (_uri: unknown) => new Uint8Array([9, 8]));
const safWrite = vi.fn(async (_uri: unknown, _bytes: Uint8Array) => undefined);

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (options: unknown) => dialogOpen(options),
  save: (options: unknown) => dialogSave(options),
}));
vi.mock("@tauri-apps/plugin-fs", () => ({
  readFile: (path: string) => fsRead(path),
  writeFile: (path: string, bytes: Uint8Array) => fsWrite(path, bytes),
}));
vi.mock("tauri-plugin-android-fs-api", () => ({
  showOpenFilePicker: (options: unknown) => safOpen(options),
  showSaveFilePicker: (name: string, mime: string | null) => safSave(name, mime),
  getName: async (uri: { uri: string }) => (uri.uri.includes("picked") ? "scan.png" : "data.csv"),
  readFile: (uri: unknown) => safRead(uri),
  writeFile: (uri: unknown, bytes: Uint8Array) => safWrite(uri, bytes),
}));

async function loadBridge(android: boolean) {
  vi.resetModules();
  Object.defineProperty(window.navigator, "userAgent", {
    configurable: true,
    value: android ? "Mozilla/5.0 (Linux; Android 14)" : "Mozilla/5.0 (Windows NT 10.0)",
  });
  return import("./mobile");
}

describe("file bridge", () => {
  beforeEach(() => {
    for (const mock of [dialogOpen, dialogSave, fsRead, fsWrite, safOpen, safSave, safRead, safWrite]) {
      mock.mockClear();
    }
  });

  it("reads and writes through the dialog and fs plugins on the desktop", async () => {
    const { pickFileBytes, saveFileBytes } = await loadBridge(false);
    const picked = await pickFileBytes({ name: "Images", extensions: ["webp"], mimeTypes: ["image/*"] });
    expect(picked).toEqual({ name: "photo.webp", bytes: new Uint8Array([1, 2, 3]) });
    expect(fsRead).toHaveBeenCalledWith("C:/pics/photo.webp");

    const saved = await saveFileBytes(new Uint8Array([4]), "data.csv", { name: "CSV", extensions: ["csv"] });
    expect(saved).toBe("C:/out/data.csv");
    expect(fsWrite).toHaveBeenCalledWith("C:/out/data.csv", new Uint8Array([4]));
    expect(safOpen).not.toHaveBeenCalled();
    expect(safSave).not.toHaveBeenCalled();
  });

  it("uses the SAF picker and content URIs on Android", async () => {
    const { pickFileBytes, saveFileBytes } = await loadBridge(true);
    const picked = await pickFileBytes({ name: "Images", extensions: ["png"], mimeTypes: ["image/*"] });
    expect(picked).toEqual({ name: "scan.png", bytes: new Uint8Array([9, 8]) });
    expect(safOpen).toHaveBeenCalledWith(expect.objectContaining({ mimeTypes: ["image/*"], multiple: false }));

    const saved = await saveFileBytes(new Uint8Array([7]), "data.csv", { name: "CSV", extensions: ["csv"] });
    expect(saved).toBe("data.csv");
    expect(safSave).toHaveBeenCalledWith("data.csv", "text/csv");
    expect(safWrite).toHaveBeenCalledWith({ uri: "content://saved/1" }, new Uint8Array([7]));
    expect(dialogOpen).not.toHaveBeenCalled();
    expect(dialogSave).not.toHaveBeenCalled();
    expect(fsRead).not.toHaveBeenCalled();
    expect(fsWrite).not.toHaveBeenCalled();
  });

  it("returns null when the user cancels", async () => {
    const { pickFileBytes, saveFileBytes } = await loadBridge(true);
    safOpen.mockResolvedValueOnce([]);
    safSave.mockResolvedValueOnce(null as never);
    expect(await pickFileBytes({ name: "Data", extensions: ["csv"], mimeTypes: ["*/*"] })).toBeNull();
    expect(await saveFileBytes(new Uint8Array(), "x.csv", { name: "CSV", extensions: ["csv"] })).toBeNull();
    expect(safWrite).not.toHaveBeenCalled();
  });
});
