import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));

import { invoke } from "@tauri-apps/api/core";
import { openOfficePath } from "./office-store";
import { useSettings, useToasts } from "./store";

const LIMIT = 'Import limit: sheet "Log" has cells beyond row 100000 or column 1000; they were not imported.';

function mockOpen(warnings: string[]) {
  vi.mocked(invoke).mockImplementation(async (command: string) => {
    if (command === "office_open_document") {
      return {
        kind: "calc",
        title: "Big",
        path: "/data/big.xlsx",
        model: { title: "Big", sheets: [] },
        warnings,
        legacy: false,
      };
    }
    if (command === "file_fingerprint") return { exists: true, sha256: "abc" };
    return null;
  });
}

describe("opening a document with import warnings", () => {
  beforeEach(() => {
    useToasts.setState({ toasts: [] });
    useSettings.setState((state) => ({
      settings: { ...state.settings, language: "en", showImportWarnings: true },
    }));
  });

  it("shows the size-limit notice translated and separately from the other notes", async () => {
    mockOpen(["Macros were not loaded.", LIMIT]);
    await openOfficePath("/data/big.xlsx");
    const toasts = useToasts.getState().toasts;
    expect(toasts).toHaveLength(2);
    expect(toasts[0].title).toBe("Part of the sheet was not imported");
    expect(toasts[0].detail).toContain('"Log"');
    expect(toasts[0].detail).toContain("100000");
    expect(toasts[1].title).toBe("Opened with notes");
    expect(toasts[1].detail).toBe("Macros were not loaded.");
  });

  it("uses the Turkish text when the app language is Turkish", async () => {
    useSettings.setState((state) => ({ settings: { ...state.settings, language: "tr" } }));
    mockOpen([LIMIT]);
    await openOfficePath("/data/big.xlsx");
    const [toast] = useToasts.getState().toasts;
    expect(toast.title).toBe("Sayfanın bir bölümü içe aktarılmadı");
    expect(toast.detail).toContain("100000");
  });

  it("still shows the size-limit notice when import notes are turned off", async () => {
    useSettings.setState((state) => ({ settings: { ...state.settings, showImportWarnings: false } }));
    mockOpen(["Macros were not loaded.", LIMIT]);
    await openOfficePath("/data/big.xlsx");
    const toasts = useToasts.getState().toasts;
    expect(toasts).toHaveLength(1);
    expect(toasts[0].title).toBe("Part of the sheet was not imported");
  });

  it("shows no toast for a clean import", async () => {
    mockOpen([]);
    await openOfficePath("/data/big.xlsx");
    expect(useToasts.getState().toasts).toHaveLength(0);
  });
});
