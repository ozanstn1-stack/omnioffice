import { afterEach, describe, expect, it, vi } from "vitest";
import { renderHook } from "@testing-library/react";
import { isTextField, keyboardInset, keyboardLikelyOpen, scrollFieldIntoView, useKeyboardInset } from "./viewport";

// jsdom has no visual viewport; the helpers are pinned with a stand-in.
afterEach(() => {
  Object.defineProperty(window, "innerHeight", { configurable: true, value: 768 });
});

describe("phone viewport helpers (D8)", () => {
  it("treats a shrunken visual viewport as an open keyboard", () => {
    expect(keyboardLikelyOpen(400, 800)).toBe(true);
    expect(keyboardLikelyOpen(780, 800)).toBe(false);
    expect(keyboardLikelyOpen(0, 0)).toBe(false);
  });

  it("measures the pixels the keyboard covers", () => {
    Object.defineProperty(window, "innerHeight", { configurable: true, value: 800 });
    Object.defineProperty(window, "visualViewport", {
      configurable: true,
      value: { height: 500, offsetTop: 0 },
    } as unknown as VisualViewport);
    expect(keyboardInset()).toBe(300);
    Object.defineProperty(window, "visualViewport", {
      configurable: true,
      value: { height: 800, offsetTop: 0 },
    } as unknown as VisualViewport);
    expect(keyboardInset()).toBe(0);
    Object.defineProperty(window, "visualViewport", { configurable: true, value: undefined });
    expect(keyboardInset()).toBe(0);
  });

  it("marks the root while the keyboard is up, for the floating phone ribbon (7.33)", () => {
    const listeners: Record<string, () => void> = {};
    const viewport = {
      height: 800,
      offsetTop: 0,
      addEventListener: (name: string, listener: () => void) => (listeners[name] = listener),
      removeEventListener: vi.fn(),
    };
    Object.defineProperty(window, "innerHeight", { configurable: true, value: 800 });
    Object.defineProperty(window, "visualViewport", { configurable: true, value: viewport });
    const root = document.documentElement;
    const { unmount } = renderHook(() => useKeyboardInset());
    expect(root.dataset.keyboard).toBe("closed");

    viewport.height = 450;
    listeners.resize();
    expect(root.dataset.keyboard).toBe("open");
    expect(root.style.getPropertyValue("--keyboard-inset")).toBe("350px");

    unmount();
    expect(root.dataset.keyboard).toBeUndefined();
    Object.defineProperty(window, "visualViewport", { configurable: true, value: undefined });
  });

  it("scrolls a focused text field into view and leaves other elements alone", () => {
    const scroll = vi.fn();
    const input = document.createElement("input");
    expect(isTextField(input)).toBe(true);
    expect(scrollFieldIntoView(input, scroll)).toBe(true);
    expect(scroll).toHaveBeenCalledWith(input);

    const textarea = document.createElement("textarea");
    expect(scrollFieldIntoView(textarea, scroll)).toBe(true);

    const block = document.createElement("div");
    expect(isTextField(block)).toBe(false);
    expect(scrollFieldIntoView(block, scroll)).toBe(false);
    expect(scroll).toHaveBeenCalledTimes(2);
  });
});
