import { describe, expect, it } from "vitest";
import { pointToDisplay, strokesToPaths } from "./geometry";

describe("annotation display-space helpers", () => {
  const bounds = { left: 10, top: 20, width: 500, height: 700 };

  it("maps client coordinates to display points", () => {
    const point = pointToDisplay({ clientX: 110, clientY: 370 }, bounds, 595.28, 841.89);
    expect(point?.x).toBeCloseTo(119.056, 3);
    expect(point?.y).toBeCloseTo(420.945, 3);
  });

  it("clamps points to the page and rejects degenerate boxes", () => {
    const point = pointToDisplay({ clientX: 10000, clientY: -500 }, bounds, 595.28, 841.89);
    expect(point).toEqual({ x: 595.28, y: 0 });
    expect(pointToDisplay({ clientX: 0, clientY: 0 }, { ...bounds, width: 0 }, 595.28, 841.89)).toBeNull();
  });

  it("turns freehand strokes into SVG paths, dropping single points", () => {
    expect(
      strokesToPaths([
        [
          [1, 2],
          [3, 4],
          [5, 6],
        ],
      ]),
    ).toEqual(["M 1 2 L 3 4 L 5 6"]);
    expect(strokesToPaths([[[1, 2]]])).toEqual([]);
    expect(strokesToPaths(undefined)).toEqual([]);
  });
});
