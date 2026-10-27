import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

/**
 * Android "Share" on a tool result, a Recent entry or a document opened from
 * another app: the file goes from the app's storage straight to the share
 * sheet. It used to be published to `Downloads/OmniOffice` first, leaving one
 * more copy there for every tap on a Recent entry.
 */

const invokeMock = vi.fn();
const fs = vi.hoisted(() => ({
  getAndroidApiLevel: vi.fn(async () => 34),
  createNewPublicFile: vi.fn(async (_dir: unknown, path: string) => ({ uri: `content://downloads/${path}` })),
  copyFile: vi.fn(async () => undefined),
  setPublicFilePending: vi.fn(async () => undefined),
  scanPublicFile: vi.fn(async () => undefined),
  showShareFileAppChooser: vi.fn(async () => undefined),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }));
vi.mock("tauri-plugin-android-fs-api", () => ({ ...fs, PublicGeneralPurposeDir: { Download: "Download" } }));

let mobile: typeof import("./mobile");

beforeAll(async () => {
  Object.defineProperty(window.navigator, "userAgent", {
    value: "Mozilla/5.0 (Linux; Android 15)",
    configurable: true,
  });
  mobile = await import("./mobile");
});

beforeEach(() => {
  invokeMock.mockReset();
  for (const mock of Object.values(fs)) mock.mockClear();
});

describe("revealAnyFile on Android", () => {
  it("shares an app file directly, without a copy in Downloads", async () => {
    invokeMock.mockResolvedValue(undefined);
    await mobile.revealAnyFile("/data/user/0/app/files/output/merged.pdf");
    expect(invokeMock).toHaveBeenCalledWith("share_document_file", {
      path: "/data/user/0/app/files/output/merged.pdf",
      mimeType: "application/pdf",
      title: null,
    });
    expect(fs.createNewPublicFile).not.toHaveBeenCalled();
    expect(fs.copyFile).not.toHaveBeenCalled();
  });

  it("publishes and shares a copy when the file is outside the app's storage", async () => {
    invokeMock.mockRejectedValue("This document is not in the app's storage");
    await mobile.revealAnyFile("/storage/emulated/0/Other/scan.pdf");
    expect(fs.createNewPublicFile).toHaveBeenCalledTimes(1);
    expect(fs.showShareFileAppChooser).toHaveBeenCalledWith({ uri: "content://downloads/OmniOffice/scan.pdf" });
  });

  it("shares the published copy of a document saved this session", async () => {
    invokeMock.mockRejectedValue("not used");
    await mobile.publishOutputs(["/data/cache/out/report.docx"]);
    invokeMock.mockClear();
    await mobile.revealAnyFile("/data/cache/out/report.docx");
    expect(invokeMock).not.toHaveBeenCalled();
    expect(fs.showShareFileAppChooser).toHaveBeenCalledWith({ uri: "content://downloads/OmniOffice/report.docx" });
  });
});
