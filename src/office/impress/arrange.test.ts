import { describe, expect, it } from "vitest";
import { alignBox, distributePositions, type ArrangeBox } from "./arrange";

function box(id: string, x: number, y: number, w = 50, h = 30): ArrangeBox {
  return { id, x, y, w, h };
}

describe("alignBox", () => {
  const bounds = { x: 100, y: 200, w: 400, h: 100 };

  it("aligns edges exactly", () => {
    expect(alignBox(box("a", 10, 10), bounds, "left")).toEqual({ x: 100, y: 10 });
    expect(alignBox(box("a", 10, 10), bounds, "right")).toEqual({ x: 450, y: 10 });
    expect(alignBox(box("a", 10, 10), bounds, "top")).toEqual({ x: 10, y: 200 });
    expect(alignBox(box("a", 10, 10), bounds, "bottom")).toEqual({ x: 10, y: 270 });
  });

  it("centres on both axes", () => {
    expect(alignBox(box("a", 10, 10, 100, 40), bounds, "center")).toEqual({ x: 250, y: 10 });
    expect(alignBox(box("a", 10, 10, 100, 40), bounds, "middle")).toEqual({ x: 10, y: 230 });
  });

  it("keeps the other axis untouched", () => {
    expect(alignBox(box("a", 10, 10), bounds, "center").y).toBe(10);
    expect(alignBox(box("a", 10, 10), bounds, "middle").x).toBe(10);
  });
});

describe("distributePositions", () => {
  it("equalises the gaps between the left and right edges", () => {
    const boxes = [box("a", 0, 0, 40, 20), box("b", 60, 0, 100, 20), box("c", 300, 0, 40, 20)];
    const positions = distributePositions(boxes, "x");
    // Span 0..340 holds widths 40+100+40=180, so each of the two gaps is 80.
    expect(positions.get("a")).toBe(0);
    expect(positions.get("b")).toBe(120);
    expect(positions.get("c")).toBe(300);
  });

  it("distributes vertically by top/bottom edges", () => {
    const boxes = [box("a", 0, 0, 40, 20), box("b", 0, 50, 40, 20), box("c", 0, 200, 40, 20)];
    const positions = distributePositions(boxes, "y");
    // Span 0..220 holds heights 20+20+20=60, so each gap is (220-60)/2 = 80.
    expect(positions.get("a")).toBe(0);
    expect(positions.get("b")).toBe(100);
    expect(positions.get("c")).toBe(200);
  });

  it("leaves the outer objects in place even when they are not sorted in the list", () => {
    const boxes = [box("c", 300, 0), box("a", 0, 0), box("b", 150, 0)];
    const positions = distributePositions(boxes, "x");
    expect(positions.get("a")).toBe(0);
    expect(positions.get("c")).toBe(300);
    // Every gap is equal.
    const gapAB = (positions.get("b") ?? 0) - ((positions.get("a") ?? 0) + 50);
    const gapBC = 300 - ((positions.get("b") ?? 0) + 50);
    expect(gapAB).toBe(gapBC);
  });

  it("needs at least three boxes and otherwise reports the current positions", () => {
    const two = [box("a", 5, 7), box("b", 90, 7)];
    const positions = distributePositions(two, "x");
    expect(positions.get("a")).toBe(5);
    expect(positions.get("b")).toBe(90);
    expect(positions.size).toBe(2);
  });

  it("handles overlapping boxes without negative gaps breaking the order", () => {
    const boxes = [box("a", 0, 0, 100, 10), box("b", 10, 0, 100, 10), box("c", 20, 0, 100, 10)];
    const positions = distributePositions(boxes, "x");
    const values = ["a", "b", "c"].map((id) => positions.get(id) ?? Number.NaN);
    expect(values[0]).toBeLessThanOrEqual(values[1]);
    expect(values[1]).toBeLessThanOrEqual(values[2]);
  });
});
