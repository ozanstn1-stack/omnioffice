import { describe, expect, it } from "vitest";

import type { TextRunInfo } from "./api";
import { clampSpeechRate, pageTextFromRuns, speechSupported, textLayerItems } from "./reader-text";

function run(patch: Partial<TextRunInfo>): TextRunInfo {
  return {
    page: 1,
    index: 0,
    text: "",
    font: null,
    fontSizePt: 12,
    x: 0,
    y: 0,
    widthPt: 10,
    renderMode: 0,
    editable: true,
    note: null,
    ...patch,
  };
}

describe("text layer geometry", () => {
  it("scales run positions to the displayed page width", () => {
    // A 600×800 pt page shown at 300 CSS px → 0.5 scale.
    const items = textLayerItems(
      [run({ text: "Merhaba", x: 100, y: 700, widthPt: 60, fontSizePt: 12 })],
      { display_width_pt: 600, display_height_pt: 800 },
      300,
    );
    expect(items).toHaveLength(1);
    expect(items[0].left).toBeCloseTo(50, 5);
    // top = (800 - 700 - 12) * 0.5 = 44
    expect(items[0].top).toBeCloseTo(44, 5);
    expect(items[0].width).toBeCloseTo(30, 5);
    expect(items[0].fontSize).toBeCloseTo(6, 5);
  });

  it("skips empty runs and guards a zero-width page", () => {
    const items = textLayerItems(
      [run({ text: "" }), run({ text: "x", x: 10, y: 10, widthPt: 5 })],
      { display_width_pt: 0, display_height_pt: 0 },
      200,
    );
    expect(items.map((item) => item.text)).toEqual(["x"]);
    // A degenerate geometry must not produce NaN or Infinity.
    expect(items.every((item) => Number.isFinite(item.left) && Number.isFinite(item.top))).toBe(true);
  });
});

describe("page text from runs", () => {
  it("keeps a word split across runs glued together", () => {
    expect(pageTextFromRuns([run({ text: "Mer", x: 0, widthPt: 20 }), run({ text: "haba", x: 20, widthPt: 40 })])).toBe(
      "Merhaba",
    );
  });

  it("inserts a space at a positional gap and keeps punctuation attached", () => {
    const runs = [
      run({ text: "Merhaba", x: 0, widthPt: 30 }),
      run({ text: "dünya", x: 40, widthPt: 30 }),
      run({ text: "!", x: 70, widthPt: 4 }),
    ];
    expect(pageTextFromRuns(runs)).toBe("Merhaba dünya!");
  });

  it("returns an empty string for a page with no text layer", () => {
    expect(pageTextFromRuns([])).toBe("");
    expect(pageTextFromRuns([run({ text: "   " })])).toBe("");
  });
});

describe("speech capability detection", () => {
  it("is true when speech synthesis and the utterance constructor exist", () => {
    expect(
      speechSupported({
        speechSynthesis: {
          speak: () => undefined,
          cancel: () => undefined,
          pause: () => undefined,
          resume: () => undefined,
        },
        SpeechSynthesisUtterance: function () {},
      }),
    ).toBe(true);
  });

  it("is false when the API is missing (the button must be disabled)", () => {
    expect(speechSupported(undefined)).toBe(false);
    expect(speechSupported({})).toBe(false);
    expect(speechSupported({ speechSynthesis: null, SpeechSynthesisUtterance: function () {} })).toBe(false);
    // A synthesis object without `speak` is not usable.
    expect(speechSupported({ speechSynthesis: { cancel: () => undefined } })).toBe(false);
  });

  it("clamps the speech rate to the control's range", () => {
    expect(clampSpeechRate(0.1)).toBe(0.5);
    expect(clampSpeechRate(5)).toBe(2);
    expect(clampSpeechRate(1.25)).toBe(1.3);
    expect(clampSpeechRate(Number.NaN)).toBe(1);
  });
});
