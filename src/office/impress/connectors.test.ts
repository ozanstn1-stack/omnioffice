import { describe, expect, it } from "vitest";
import type { LineSpec, SlideObject } from "../../lib/office-types";
import { newSlideObject } from "../../lib/office-types";
import {
  connectorPatch,
  distanceToSegment,
  nearestSite,
  recomputeConnectors,
  remapConnectorTargets,
  routeConnector,
  siteAnchor,
} from "./connectors";

function rect(id: string, x: number, y: number, w: number, h: number): SlideObject {
  return { ...newSlideObject("rect", x, y, w, h), id };
}

function connector(id: string, line: Partial<LineSpec>): SlideObject {
  const x2 = line.x2 ?? 10;
  const y2 = line.y2 ?? 10;
  return {
    ...newSlideObject("line", 0, 0, 10, 10),
    id,
    line: { x2, y2, beginArrow: false, endArrow: true, dash: "solid", ...line },
  };
}

describe("nearestSite", () => {
  it("picks the edge closest to the point", () => {
    const shape = rect("a", 100, 100, 200, 100);
    expect(nearestSite(shape, { x: 200, y: 90 })).toBe(0);
    expect(nearestSite(shape, { x: 90, y: 150 })).toBe(1);
    expect(nearestSite(shape, { x: 200, y: 210 })).toBe(2);
    expect(nearestSite(shape, { x: 310, y: 150 })).toBe(3);
  });

  it("measures the distance to the segment, not to a corner", () => {
    expect(distanceToSegment({ x: 5, y: 5 }, { x: 0, y: 0 }, { x: 10, y: 0 })).toBe(5);
    expect(distanceToSegment({ x: -4, y: 3 }, { x: 0, y: 0 }, { x: 10, y: 0 })).toBe(5);
  });
});

describe("siteAnchor", () => {
  it("anchors each site at the midpoint of its edge", () => {
    const shape = rect("a", 100, 100, 200, 100);
    expect(siteAnchor(shape, 0)).toEqual({ x: 200, y: 100 });
    expect(siteAnchor(shape, 1)).toEqual({ x: 100, y: 150 });
    expect(siteAnchor(shape, 2)).toEqual({ x: 200, y: 200 });
    expect(siteAnchor(shape, 3)).toEqual({ x: 300, y: 150 });
  });

  it("spreads multiple connectors along the edge and clamps the offset", () => {
    const shape = rect("a", 0, 0, 100, 50);
    expect(siteAnchor(shape, 0, 1)).toEqual({ x: 66, y: 0 });
    expect(siteAnchor(shape, 0, 10)).toEqual({ x: 92, y: 0 });
    expect(siteAnchor(shape, 1, -10)).toEqual({ x: 0, y: 8 });
  });
});

describe("routeConnector", () => {
  it("falls back to the line's own position and local offset when unglued", () => {
    const line = connector("l1", { x2: 30, y2: 40 });
    line.x = 5;
    line.y = 7;
    expect(routeConnector(line, [line])).toEqual({ x1: 5, y1: 7, x2: 35, y2: 47 });
  });

  it("resolves glued ends to the target site anchors", () => {
    const a = rect("a", 0, 0, 100, 100);
    const b = rect("b", 300, 0, 100, 100);
    const line = connector("l1", { beginObject: "a", endObject: "b", beginSite: 3, endSite: 1 });
    expect(routeConnector(line, [a, b, line])).toEqual({ x1: 100, y1: 50, x2: 300, y2: 50 });
  });

  it("falls back per end when only one target is present", () => {
    const a = rect("a", 0, 0, 100, 100);
    const line = connector("l1", { beginObject: "a", beginSite: 2, endObject: "missing", endSite: 0 });
    expect(routeConnector(line, [a, line])).toEqual({ x1: 50, y1: 100, x2: 10, y2: 10 });
  });

  it("finds targets nested inside groups", () => {
    const child = rect("child", 10, 10, 20, 20);
    const group: SlideObject = { ...rect("g1", 0, 0, 100, 100), kind: "group", children: [child] };
    const line = connector("l1", { beginObject: "child", beginSite: 0 });
    expect(routeConnector(line, [group, line]).x1).toBe(20);
    expect(routeConnector(line, [group, line]).y1).toBe(10);
  });
});

describe("connectorPatch and recomputeConnectors", () => {
  it("normalises the box around the routed endpoints", () => {
    const a = rect("a", 0, 0, 100, 100);
    const b = rect("b", 300, 0, 100, 100);
    const line = connector("l1", { beginObject: "a", endObject: "b", beginSite: 3, endSite: 1 });
    const patched = connectorPatch(line, [a, b, line]);
    expect({ x: patched.x, y: patched.y, w: patched.w, h: patched.h }).toEqual({ x: 100, y: 50, w: 200, h: 1 });
    expect(patched.line).toMatchObject({ x2: 200, y2: 0 });
  });

  it("follows a shape that moved", () => {
    const a = rect("a", 0, 0, 100, 100);
    const b = rect("b", 300, 0, 100, 100);
    const line = connector("l1", { beginObject: "a", endObject: "b", beginSite: 3, endSite: 1 });
    const moved = { ...a, y: 200 };
    const [patched] = recomputeConnectors([moved, b, line]).filter((object) => object.id === "l1");
    expect(patched.x).toBe(100);
    expect(patched.y).toBe(50);
    expect(patched.h).toBe(200);
    expect(routeConnector(patched, [moved, b, patched])).toEqual({ x1: 100, y1: 250, x2: 300, y2: 50 });
  });

  it("returns the same array when nothing is glued", () => {
    const objects = [rect("a", 0, 0, 10, 10), connector("l1", {})];
    expect(recomputeConnectors(objects)).toBe(objects);
  });

  it("leaves unglued lines alone even when other connectors exist", () => {
    const a = rect("a", 0, 0, 100, 100);
    const glued = connector("l1", { beginObject: "a", beginSite: 0 });
    const free = connector("l2", { x2: 20, y2: 30 });
    const next = recomputeConnectors([a, glued, free]);
    expect(next.find((object) => object.id === "l2")).toBe(free);
  });
});

describe("remapConnectorTargets", () => {
  it("redirects glue ids through the clone map and drops missing targets", () => {
    const line = connector("l1", { beginObject: "a", endObject: "b" });
    const remapped = remapConnectorTargets([line], new Map([["a", "a2"]]));
    expect(remapped[0].line).toMatchObject({ beginObject: "a2", endObject: null });
  });

  it("walks group children", () => {
    const child = connector("l1", { beginObject: "a" });
    const group: SlideObject = { ...rect("g1", 0, 0, 10, 10), kind: "group", children: [child] };
    const [remapped] = remapConnectorTargets([group], new Map([["a", "a2"]]));
    expect(remapped.children?.[0].line?.beginObject).toBe("a2");
  });
});
