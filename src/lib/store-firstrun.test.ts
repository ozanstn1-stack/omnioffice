import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// The settings store talks to Tauri through ./api; the loaders are mocked so
// `init` can be driven as a fresh install or an upgrade.
const loadSettings = vi.fn(async () => ({}) as Record<string, unknown>);
const engineStatus = vi.fn(async () => null);
const ocrLanguages = vi.fn(async () => []);
const saveSettings = vi.fn(async (_settings: unknown) => undefined);
const logFrontend = vi.fn(async (_level: string, _message: string) => undefined);

vi.mock("./api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./api")>()),
  loadSettings: () => loadSettings(),
  engineStatus: () => engineStatus(),
  ocrLanguages: () => ocrLanguages(),
  saveSettings: (settings: unknown) => saveSettings(settings),
  logFrontend: (level: string, message: string) => logFrontend(level, message),
}));

import { useSettings } from "./store";

function setOsLanguage(language: string): void {
  vi.spyOn(navigator, "language", "get").mockReturnValue(language);
}

beforeEach(() => {
  loadSettings.mockReset().mockResolvedValue({});
  engineStatus.mockReset().mockResolvedValue(null);
  ocrLanguages.mockReset().mockResolvedValue([]);
  saveSettings.mockClear();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("first run defaults", () => {
  it("follows a Turkish operating-system language", async () => {
    setOsLanguage("tr-TR");
    await useSettings.getState().init();
    const settings = useSettings.getState().settings;
    expect(settings.language).toBe("tr");
    expect(settings.ocrLanguages).toEqual(["tur", "eng"]);
  });

  it("keeps English for other languages", async () => {
    setOsLanguage("en-US");
    await useSettings.getState().init();
    expect(useSettings.getState().settings.language).toBe("en");
    expect(useSettings.getState().settings.ocrLanguages).toEqual(["eng"]);
  });

  it("never overrides a stored settings file on upgrade", async () => {
    setOsLanguage("tr-TR");
    loadSettings.mockResolvedValue({ language: "en", ocrLanguages: ["deu"], onboardingDone: true });
    await useSettings.getState().init();
    const settings = useSettings.getState().settings;
    expect(settings.language).toBe("en");
    expect(settings.ocrLanguages).toEqual(["deu"]);
    expect(settings.onboardingDone).toBe(true);
  });
});
