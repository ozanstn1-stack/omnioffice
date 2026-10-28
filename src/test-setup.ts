import "@testing-library/jest-dom/vitest";
import { afterEach, beforeAll, vi } from "vitest";
import { cleanup } from "@testing-library/react";
import en from "./lib/i18n/en.json";
import tr from "./lib/i18n/tr.json";
import { primeDictionary } from "./lib/i18n-runtime";
import { preloadTypedCommands } from "./lib/typed-call";

// The dictionaries are JSON assets fetched at runtime in production; tests
// import the same files and prime the cache before the test modules load, so
// module-scope `makeTranslate(…)` calls resolve real strings without a fetch.
// Imported from `i18n-runtime` (not `i18n`) so the settings store and the
// Tauri API are not pulled in ahead of each file's `vi.mock` calls.
primeDictionary("en", en as Record<string, string>);
primeDictionary("tr", tr as Record<string, string>);

// The typed command wrappers (#316) load as main.tsx loads them at launch,
// before the first screen. This runs once the test file is collected, so the
// generated wrappers bind that file's `vi.mock` of `@tauri-apps/api/core`.
beforeAll(async () => {
  await preloadTypedCommands();
});

// jsdom does not implement these, but the editors rely on them.
if (!window.matchMedia) {
  window.matchMedia = ((query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: () => undefined,
    removeListener: () => undefined,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
    dispatchEvent: () => false,
  })) as typeof window.matchMedia;
}

if (!globalThis.ResizeObserver) {
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
}

if (!globalThis.IntersectionObserver) {
  globalThis.IntersectionObserver = class {
    root = null;
    rootMargin = "";
    thresholds: number[] = [];
    observe() {}
    unobserve() {}
    disconnect() {}
    takeRecords() {
      return [];
    }
  } as unknown as typeof IntersectionObserver;
}

// jsdom has no layout engine: every element reports a zero box. Editors that
// measure text need deterministic numbers, so give them a stable stub.
if (!globalThis.requestAnimationFrame) {
  globalThis.requestAnimationFrame = ((callback: FrameRequestCallback) =>
    setTimeout(() => callback(Date.now()), 0) as unknown as number) as typeof requestAnimationFrame;
  globalThis.cancelAnimationFrame = ((handle: number) => clearTimeout(handle)) as typeof cancelAnimationFrame;
}

if (!Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = function scrollIntoView() {
    // no-op
  };
}

if (!URL.createObjectURL) {
  URL.createObjectURL = vi.fn(() => "blob:mock");
  URL.revokeObjectURL = vi.fn();
}

afterEach(() => {
  cleanup();
});
