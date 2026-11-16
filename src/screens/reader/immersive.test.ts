import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { SINGLE_TAP_DELAY_MS, useImmersiveReader, useReaderImmersive } from "./immersive";

const coarse = (matches: boolean) =>
  vi.spyOn(window, "matchMedia").mockImplementation(
    (query: string) =>
      ({
        matches: matches && query.includes("coarse"),
        media: query,
        addEventListener() {},
        removeEventListener() {},
      }) as unknown as MediaQueryList,
  );

describe("immersive reading", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    useReaderImmersive.setState({ immersive: false });
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it("toggles on a single tap once the double-tap window has passed", () => {
    const { result } = renderHook(() => useImmersiveReader());
    act(() => result.current.onSingleTap());
    expect(useReaderImmersive.getState().immersive).toBe(false);
    act(() => vi.advanceTimersByTime(SINGLE_TAP_DELAY_MS));
    expect(useReaderImmersive.getState().immersive).toBe(true);
    act(() => result.current.onSingleTap());
    act(() => vi.advanceTimersByTime(SINGLE_TAP_DELAY_MS));
    expect(useReaderImmersive.getState().immersive).toBe(false);
  });

  it("does not toggle when the tap turns into a double-tap zoom", () => {
    const { result } = renderHook(() => useImmersiveReader());
    act(() => result.current.onSingleTap());
    act(() => result.current.onDoubleTap());
    act(() => vi.advanceTimersByTime(SINGLE_TAP_DELAY_MS * 2));
    expect(useReaderImmersive.getState().immersive).toBe(false);
  });

  it("hides the chrome when a touch screen scrolls down, not on a mouse screen", () => {
    coarse(false);
    const { result } = renderHook(() => useImmersiveReader());
    act(() => result.current.onScroll(400));
    expect(useReaderImmersive.getState().immersive).toBe(false);
    coarse(true);
    act(() => result.current.onScroll(900));
    expect(useReaderImmersive.getState().immersive).toBe(true);
  });

  it("restores the shell when the reader goes away", () => {
    const { unmount } = renderHook(() => useImmersiveReader());
    act(() => useReaderImmersive.setState({ immersive: true }));
    unmount();
    expect(useReaderImmersive.getState().immersive).toBe(false);
  });
});
