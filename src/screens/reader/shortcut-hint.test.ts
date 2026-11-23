import { afterEach, describe, expect, it, vi } from "vitest";

function mockPointer(coarse: boolean) {
  vi.stubGlobal(
    "matchMedia",
    vi.fn((query: string) => ({ matches: coarse && query.includes("coarse"), media: query })),
  );
}

describe("reader shortcut hint", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.resetModules();
  });

  it("is shown on a desktop with a mouse", async () => {
    mockPointer(false);
    const { showShortcutHint } = await import("./immersive");
    expect(showShortcutHint()).toBe(true);
  });

  it("is hidden on a touch screen, so the page gets the space", async () => {
    mockPointer(true);
    const { showShortcutHint } = await import("./immersive");
    expect(showShortcutHint()).toBe(false);
  });

  it("is hidden in the Android app", async () => {
    mockPointer(false);
    vi.spyOn(navigator, "userAgent", "get").mockReturnValue("Mozilla/5.0 (Linux; Android 15; wv) AppleWebKit/537.36");
    const { showShortcutHint } = await import("./immersive");
    expect(showShortcutHint()).toBe(false);
  });
});
