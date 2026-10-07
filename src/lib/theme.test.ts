import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

import { applyTheme, useSettings } from "./store";

/** A controllable `prefers-color-scheme: dark` media query list. */
function installMatchMedia(initialDark: boolean) {
  let dark = initialDark;
  const listeners = new Set<(event: MediaQueryListEvent) => void>();
  const list = {
    get matches() {
      return dark;
    },
    media: "(prefers-color-scheme: dark)",
    onchange: null,
    addEventListener: vi.fn((type: string, listener: (event: MediaQueryListEvent) => void) => {
      if (type === "change") listeners.add(listener);
    }),
    removeEventListener: vi.fn((type: string, listener: (event: MediaQueryListEvent) => void) => {
      if (type === "change") listeners.delete(listener);
    }),
    addListener: vi.fn(),
    removeListener: vi.fn(),
    dispatchEvent: () => false,
  };
  window.matchMedia = vi.fn(() => list) as unknown as typeof window.matchMedia;
  return {
    list,
    listenerCount: () => listeners.size,
    /** The OS flips between light and dark. */
    setDark(next: boolean) {
      dark = next;
      for (const listener of [...listeners]) listener({ matches: next } as MediaQueryListEvent);
    },
  };
}

describe("system theme", () => {
  const originalMatchMedia = window.matchMedia;

  beforeEach(() => {
    // restoreMocks resets the implementation between tests: saving settings
    // only needs the IPC call to resolve.
    invokeMock.mockResolvedValue(undefined);
    document.documentElement.className = "";
    delete document.documentElement.dataset.theme;
  });

  afterEach(() => {
    // Drop any watcher a test left behind, then restore the jsdom stub.
    applyTheme("dark");
    window.matchMedia = originalMatchMedia;
  });

  it("applies the OS preference once and follows later changes while the theme is system", () => {
    const media = installMatchMedia(false);
    applyTheme("system");
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(document.documentElement.classList.contains("dark")).toBe(false);

    media.setDark(true);
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(document.documentElement.classList.contains("dark")).toBe(true);

    media.setDark(false);
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(document.documentElement.classList.contains("dark")).toBe(false);
  });

  it("stops following the OS once an explicit theme is chosen", () => {
    const media = installMatchMedia(false);
    applyTheme("system");
    expect(media.listenerCount()).toBe(1);

    applyTheme("midnight");
    expect(media.listenerCount()).toBe(0);
    expect(document.documentElement.dataset.theme).toBe("midnight");

    media.setDark(false);
    media.setDark(true);
    expect(document.documentElement.dataset.theme).toBe("midnight");
  });

  it("never stacks listeners when the system theme is applied repeatedly", () => {
    const media = installMatchMedia(true);
    applyTheme("system");
    applyTheme("system");
    applyTheme("system");
    expect(media.listenerCount()).toBe(1);
    expect(media.list.removeEventListener).toHaveBeenCalledTimes(2);
  });

  it("does not subscribe for fixed themes", () => {
    const media = installMatchMedia(true);
    applyTheme("light");
    applyTheme("dark");
    expect(media.list.addEventListener).not.toHaveBeenCalled();
    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  it("goes live when the setting is changed to system through the store", async () => {
    const media = installMatchMedia(true);
    await useSettings.getState().update({ theme: "system" });
    expect(document.documentElement.dataset.theme).toBe("dark");
    media.setDark(false);
    expect(document.documentElement.dataset.theme).toBe("light");

    await useSettings.getState().update({ theme: "light" });
    expect(media.listenerCount()).toBe(0);
  });

  it("falls back to the legacy listener API on old webviews", () => {
    const media = installMatchMedia(false);
    // Safari < 14 / old Android WebView only have addListener/removeListener.
    (media.list as { addEventListener?: unknown }).addEventListener = undefined;
    (media.list as { removeEventListener?: unknown }).removeEventListener = undefined;
    applyTheme("system");
    expect(media.list.addListener).toHaveBeenCalledTimes(1);
    applyTheme("dark");
    expect(media.list.removeListener).toHaveBeenCalledTimes(1);
  });
});
