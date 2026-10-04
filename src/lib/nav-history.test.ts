import { describe, expect, it, vi } from "vitest";
import {
  isOverlayHistoryState,
  navigationActionFromState,
  overlayHistoryState,
  parseScreenHistoryState,
  recordScreenVisit,
  screenHistoryState,
} from "./nav-history";

describe("nav history state", () => {
  it("round-trips a screen entry", () => {
    const state = screenHistoryState("reader", ["C:\\docs\\a.pdf"]);
    expect(parseScreenHistoryState(state)).toEqual({ screen: "reader", files: ["C:\\docs\\a.pdf"] });
  });

  it("copies the file list so later mutations do not leak into history", () => {
    const files = ["/a.pdf"];
    const state = screenHistoryState("reader", files);
    files.push("/b.pdf");
    expect(state.files).toEqual(["/a.pdf"]);
  });

  it("rejects foreign and malformed states", () => {
    expect(parseScreenHistoryState(null)).toBeNull();
    expect(parseScreenHistoryState(undefined)).toBeNull();
    expect(parseScreenHistoryState("reader")).toBeNull();
    expect(parseScreenHistoryState({ marker: "other", screen: "reader" })).toBeNull();
    expect(parseScreenHistoryState({ marker: "pdfsak-screen" })).toBeNull();
  });

  it("drops non-string file entries", () => {
    const state = { marker: "pdfsak-screen", screen: "office", files: ["/a.docx", 7, null] };
    expect(parseScreenHistoryState(state)).toEqual({ screen: "office", files: ["/a.docx"] });
  });

  it("treats a missing files array as empty", () => {
    const state = { marker: "pdfsak-screen", screen: "home" };
    expect(parseScreenHistoryState(state)).toEqual({ screen: "home", files: [] });
  });

  it("identifies overlay guards", () => {
    expect(isOverlayHistoryState(overlayHistoryState())).toBe(true);
    expect(isOverlayHistoryState(screenHistoryState("home"))).toBe(false);
    expect(isOverlayHistoryState(null)).toBe(false);
  });
});

describe("recordScreenVisit", () => {
  function fakeHistory() {
    return { pushState: vi.fn(), replaceState: vi.fn() };
  }

  it("pushes the first visit and replaces same-screen refreshes", () => {
    const history = fakeHistory();
    expect(recordScreenVisit(history, null, { screen: "reader", files: [] })).toBe("push");
    expect(recordScreenVisit(history, { screen: "reader", files: [] }, { screen: "reader", files: ["/a.pdf"] })).toBe(
      "replace",
    );
    expect(history.pushState).toHaveBeenCalledTimes(1);
    expect(history.replaceState).toHaveBeenCalledTimes(1);
    expect(history.replaceState.mock.calls[0][0]).toMatchObject({ screen: "reader", files: ["/a.pdf"] });
  });

  it("pushes when the screen changes", () => {
    const history = fakeHistory();
    expect(recordScreenVisit(history, { screen: "home", files: [] }, { screen: "office", files: [] })).toBe("push");
    expect(history.pushState.mock.calls[0][0]).toMatchObject({ marker: "pdfsak-screen", screen: "office" });
  });
});

describe("navigationActionFromState", () => {
  it("closes overlays for guard entries", () => {
    expect(navigationActionFromState(overlayHistoryState())).toEqual({ kind: "close-overlays" });
  });

  it("navigates for screen entries", () => {
    expect(navigationActionFromState(screenHistoryState("split", ["/a.pdf"]))).toEqual({
      kind: "navigate",
      screen: "split",
      files: ["/a.pdf"],
    });
  });

  it("falls back to home for the entry before the app was loaded", () => {
    expect(navigationActionFromState(null)).toEqual({ kind: "home" });
    expect(navigationActionFromState({ some: "foreign state" })).toEqual({ kind: "home" });
  });
});

describe("popstate wiring", () => {
  it("emits app states through the real history API", async () => {
    const seen: unknown[] = [];
    const listener = (event: PopStateEvent) => seen.push(event.state);
    window.addEventListener("popstate", listener);
    try {
      window.history.replaceState(screenHistoryState("home"), "");
      recordScreenVisit(window.history, { screen: "home", files: [] }, { screen: "reader", files: [] });
      window.history.back();
      // jsdom traverses the session history through two nested queued tasks.
      await new Promise((resolve) => setTimeout(resolve, 25));
      expect(seen.map((state) => navigationActionFromState(state))).toEqual([
        { kind: "navigate", screen: "home", files: [] },
      ]);
    } finally {
      window.removeEventListener("popstate", listener);
      window.history.replaceState(null, "");
    }
  });
});
