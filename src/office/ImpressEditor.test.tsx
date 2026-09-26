import { describe, expect, it, vi } from "vitest";

// The editor module pulls in the session hook, which touches Tauri at import
// time; stubs keep the pure helpers testable in jsdom.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null), save: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-fs", () => ({ readFile: vi.fn(async () => new Uint8Array()) }));

import {
  animationObjectStyle,
  animationTimeline,
  formatClock,
  groupSelection,
  inheritedObjects,
  refreshGroupBounds,
  scaleObject,
  translateObject,
  ungroupSelection,
} from "./ImpressEditor";
import { newAnimation, newDeck, newSlide, newSlideMaster, newSlideObject, type SlideObject } from "../lib/office-types";

function rect(id: string, x: number, y: number, w: number, h: number, z: number): SlideObject {
  return { ...newSlideObject("rect", x, y, w, h), id, z };
}

describe("group and ungroup transforms", () => {
  it("groups a selection into absolute children under a bounding box", () => {
    const a = rect("a", 100, 100, 100, 50, 1);
    const b = rect("b", 220, 140, 80, 80, 2);
    const grouped = groupSelection([a, b], ["a", "b"], "g1");

    expect(grouped).toHaveLength(1);
    const group = grouped[0];
    expect(group.kind).toBe("group");
    expect({ x: group.x, y: group.y, w: group.w, h: group.h }).toEqual({ x: 100, y: 100, w: 200, h: 120 });
    expect(group.children?.map((child) => child.id)).toEqual(["a", "b"]);
    // Child coordinates stay absolute, not relative to the group box.
    expect(group.children?.map((child) => ({ x: child.x, y: child.y }))).toEqual([
      { x: 100, y: 100 },
      { x: 220, y: 140 },
    ]);
  });

  it("returns group children to the slide on ungroup", () => {
    const grouped = groupSelection([rect("a", 100, 100, 100, 50, 1), rect("b", 220, 140, 80, 80, 2)], ["a", "b"], "g1");
    const result = ungroupSelection(grouped, ["g1"]);

    expect(result.map((object) => object.id)).toEqual(["a", "b"]);
    expect(result.map((object) => object.z)).toEqual([1, 2]);
    expect(result.every((object) => object.kind !== "group")).toBe(true);
  });

  it("translates every nested child by the same delta", () => {
    const group = groupSelection([rect("a", 100, 100, 100, 50, 1), rect("b", 220, 140, 80, 80, 2)], ["a", "b"], "g1")[0];
    const moved = translateObject(group, 10, -5);

    expect({ x: moved.x, y: moved.y }).toEqual({ x: 110, y: 95 });
    expect(moved.children?.map((child) => ({ x: child.x, y: child.y }))).toEqual([
      { x: 110, y: 95 },
      { x: 230, y: 135 },
    ]);
  });

  it("scales children about the group origin when the group resizes", () => {
    const group = groupSelection([rect("a", 100, 100, 100, 50, 1), rect("b", 220, 140, 80, 80, 2)], ["a", "b"], "g1")[0];
    const scaled = scaleObject(group, 2, 2, group.x, group.y);

    expect({ w: scaled.w, h: scaled.h }).toEqual({ w: 400, h: 240 });
    expect(scaled.children?.[1]).toMatchObject({ x: 340, y: 180, w: 160, h: 160 });
  });

  it("recomputes the group box after a child moved", () => {
    const group = groupSelection([rect("a", 100, 100, 100, 50, 1), rect("b", 220, 140, 80, 80, 2)], ["a", "b"], "g1")[0];
    const nudged = refreshGroupBounds([{ ...group, children: [{ ...group.children![0], x: 0, y: 0 }, group.children![1]] }]);

    expect({ x: nudged[0].x, y: nudged[0].y, w: nudged[0].w, h: nudged[0].h }).toEqual({ x: 0, y: 0, w: 300, h: 220 });
  });

  it("nests groups recursively and ungroups one level at a time", () => {
    const inner = groupSelection([rect("a", 0, 0, 50, 50, 1), rect("b", 60, 0, 50, 50, 2)], ["a", "b"], "g1");
    const outer = groupSelection([inner[0], rect("c", 0, 80, 40, 40, 2)], ["g1", "c"], "g2");

    expect(outer).toHaveLength(1);
    expect(outer[0].children?.map((child) => child.id)).toEqual(["g1", "c"]);
    expect(outer[0].children?.[0].kind).toBe("group");

    const oneLevel = ungroupSelection(outer, ["g2"]);
    expect(oneLevel.map((object) => object.id)).toEqual(["g1", "c"]);
    expect(oneLevel[0].kind).toBe("group");

    const twoLevels = ungroupSelection(oneLevel, ["g1"]);
    expect(twoLevels.map((object) => object.id)).toEqual(["a", "b", "c"]);
  });
});

describe("animation ordering", () => {
  it("merges withPrevious into the running step and anchors afterPrevious to its end", () => {
    const onClick = { ...newAnimation("o1", "entrance", "fade", "onClick", 1), id: "a1", durationMs: 400, delayMs: 0 };
    const withPrevious = { ...newAnimation("o2", "entrance", "zoom", "withPrevious", 2), id: "a2", durationMs: 900, delayMs: 100 };
    const afterPrevious = { ...newAnimation("o3", "exit", "fadeOut", "afterPrevious", 3), id: "a3", durationMs: 300, delayMs: 50 };

    // Deliberately unsorted input: the timeline orders by `order`.
    const steps = animationTimeline([afterPrevious, withPrevious, onClick]);

    expect(steps).toHaveLength(2);
    expect(steps[0].animations.map((animation) => animation.id)).toEqual(["a1", "a2"]);
    expect(steps[0].waitForClick).toBe(true);
    expect(steps[0].relativeTo).toBe("start");
    expect(steps[0].durationMs).toBe(1000);
    expect(steps[1].animations.map((animation) => animation.id)).toEqual(["a3"]);
    expect(steps[1].waitForClick).toBe(false);
    expect(steps[1].relativeTo).toBe("previousEnd");
    expect(steps[1].durationMs).toBe(350);
  });

  it("keeps auto steps free of click gating", () => {
    const auto = { ...newAnimation("o1", "entrance", "fade", "afterPrevious", 1), id: "b1" };
    const clicked = { ...newAnimation("o2", "entrance", "fade", "onClick", 2), id: "b2" };
    const steps = animationTimeline([auto, clicked]);

    expect(steps.map((step) => step.waitForClick)).toEqual([false, true]);
  });

  it("paints pending entrances hidden and finished exits hidden", () => {
    const entrance = { ...newAnimation("o1", "entrance", "flyIn", "onClick", 1), id: "e1" };
    const exit = { ...newAnimation("o1", "exit", "fadeOut", "onClick", 1), id: "x1" };

    expect(animationObjectStyle("o1", [entrance], {}, {})).toMatchObject({ opacity: 0, transform: "translateY(40px)" });
    expect(animationObjectStyle("o1", [entrance], {}, { e1: true })).toEqual({});
    expect(animationObjectStyle("o1", [exit], {}, { x1: true })).toEqual({ opacity: 0 });

    const running = { e1: { effect: entrance, phase: "to" as const } };
    const style = animationObjectStyle("o1", [entrance], running, {});
    expect(style.opacity).toBeUndefined();
    expect(String(style.transition)).toContain("500ms");
  });
});

describe("master and layout inheritance", () => {
  it("returns master objects then layout objects with prefixed ids", () => {
    const deck = newDeck();
    const master = newSlideMaster("Master");
    master.objects = [{ ...rect("m1", 0, 0, 100, 50, 1), placeholder: "title" }];
    const layout = master.layouts[1];
    layout.objects = [{ ...rect("l1", 10, 10, 100, 50, 1), placeholder: "content" }];
    deck.masters = [master];
    const slide = { ...newSlide("titleContent"), masterId: master.id, layoutId: layout.id, objects: [] };

    expect(inheritedObjects(deck, slide).map((object) => object.id)).toEqual(["master:m1", `layout:l1`]);
  });

  it("suppresses a placeholder role the slide already fills", () => {
    const deck = newDeck();
    const master = newSlideMaster("Master");
    master.objects = [{ ...rect("m1", 0, 0, 100, 50, 1), placeholder: "title" }];
    const layout = master.layouts[1];
    layout.objects = [{ ...rect("l1", 10, 10, 100, 50, 1), placeholder: "content" }];
    deck.masters = [master];
    const slide = { ...newSlide("titleContent"), masterId: master.id, layoutId: layout.id, objects: [{ ...rect("s1", 5, 5, 10, 10, 1), placeholder: "content" }] };

    expect(inheritedObjects(deck, slide).map((object) => object.id)).toEqual(["master:m1"]);
  });
});

describe("presenter clock", () => {
  it("formats elapsed time as mm:ss", () => {
    expect(formatClock(0)).toBe("00:00");
    expect(formatClock(9_000)).toBe("00:09");
    expect(formatClock(65_000)).toBe("01:05");
    expect(formatClock(3_600_000)).toBe("60:00");
  });
});
