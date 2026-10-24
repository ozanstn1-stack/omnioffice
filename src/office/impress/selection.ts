/**
 * Pure Impress helpers: selection paths, groups, inheritance, animation order.
 */
import type { CSSProperties } from "react";
import { uid, type Animation, type Deck, type Slide, type SlideObject } from "../../lib/office-types";

export type SelectionPath = string[];

/** Animations that start together, with how their start time is anchored. */
export interface AnimationStep {
  animations: Animation[];
  waitForClick: boolean;
  relativeTo: "start" | "previousStart" | "previousEnd";
  durationMs: number;
}

export function pathKey(path: SelectionPath): string {
  return path.join("/");
}

export function cloneObjects(objects: SlideObject[]): { objects: SlideObject[]; idMap: Map<string, string> } {
  const idMap = new Map<string, string>();
  const clone = (list: SlideObject[]): SlideObject[] =>
    list.map((object) => {
      const id = uid();
      idMap.set(object.id, id);
      return {
        ...(JSON.parse(JSON.stringify(object)) as SlideObject),
        id,
        children: object.children ? clone(object.children) : undefined,
      };
    });
  return { objects: clone(objects), idMap };
}

export function isInheritedId(id: string): boolean {
  return id.startsWith("master:") || id.startsWith("layout:");
}

/** Deep clone that gives every object (and group child) a fresh id. */
export function cloneObject(object: SlideObject): SlideObject {
  return {
    ...(JSON.parse(JSON.stringify(object)) as SlideObject),
    id: uid(),
    children: object.children?.map(cloneObject),
  };
}

/** Clone a list of objects and report the old to new id mapping. */

/** Bounding box of a set of objects (rotation is ignored, as in PowerPoint). */
export function objectBounds(objects: SlideObject[]): { x: number; y: number; w: number; h: number } {
  if (objects.length === 0) return { x: 0, y: 0, w: 0, h: 0 };
  const left = Math.min(...objects.map((object) => object.x));
  const top = Math.min(...objects.map((object) => object.y));
  const right = Math.max(...objects.map((object) => object.x + object.w));
  const bottom = Math.max(...objects.map((object) => object.y + object.h));
  return { x: left, y: top, w: Math.max(1, right - left), h: Math.max(1, bottom - top) };
}

export function objectAtPath(objects: SlideObject[], path: SelectionPath): SlideObject | undefined {
  let list = objects;
  let found: SlideObject | undefined;
  for (const id of path) {
    found = list.find((object) => object.id === id);
    if (!found) return undefined;
    list = found.children ?? [];
  }
  return found;
}

export function replaceObjectAtPath(
  objects: SlideObject[],
  path: SelectionPath,
  patch: Partial<SlideObject>,
): SlideObject[] {
  const [head, ...rest] = path;
  return objects.map((object) => {
    if (object.id !== head) return object;
    if (rest.length === 0) return { ...object, ...patch };
    return {
      ...object,
      children: object.children ? replaceObjectAtPath(object.children, rest, patch) : object.children,
    };
  });
}

function removeAtPath(objects: SlideObject[], path: SelectionPath): SlideObject[] {
  const [head, ...rest] = path;
  if (rest.length === 0) return objects.filter((object) => object.id !== head);
  return objects.map((object) =>
    object.id === head && object.children ? { ...object, children: removeAtPath(object.children, rest) } : object,
  );
}

/** Translate an object and every group child by the same delta. */
export function translateObject(object: SlideObject, dx: number, dy: number): SlideObject {
  return {
    ...object,
    x: object.x + dx,
    y: object.y + dy,
    children: object.children?.map((child) => translateObject(child, dx, dy)),
  };
}

/** Scale an object and every group child about an origin (absolute child coords). */
export function scaleObject(
  object: SlideObject,
  sx: number,
  sy: number,
  originX: number,
  originY: number,
): SlideObject {
  return {
    ...object,
    x: originX + (object.x - originX) * sx,
    y: originY + (object.y - originY) * sy,
    w: Math.max(4, object.w * sx),
    h: Math.max(4, object.h * sy),
    children: object.children?.map((child) => scaleObject(child, sx, sy, originX, originY)),
  };
}

/** Recompute every group's bounding box from its children (deepest first). */
export function refreshGroupBounds(objects: SlideObject[]): SlideObject[] {
  return objects.map((object) => {
    if (object.kind !== "group" || !object.children || object.children.length === 0) return object;
    const children = refreshGroupBounds(object.children);
    return { ...object, ...objectBounds(children), children };
  });
}

/**
 * Group top-level objects into a real `{ kind: "group", children }` hierarchy.
 * Child coordinates stay absolute; the group's box is their bounding box.
 */
export function groupSelection(objects: SlideObject[], ids: string[], groupId = uid()): SlideObject[] {
  const selected = new Set(ids);
  const members = objects.filter((object) => selected.has(object.id)).sort((a, b) => a.z - b.z);
  if (members.length < 2) return objects;
  const bounds = objectBounds(members);
  const before = objects.findIndex((object) => selected.has(object.id));
  const insertAt = objects
    .slice(0, before < 0 ? objects.length : before)
    .filter((object) => !selected.has(object.id)).length;
  const group: SlideObject = {
    id: groupId,
    kind: "group",
    x: bounds.x,
    y: bounds.y,
    w: bounds.w,
    h: bounds.h,
    rotation: 0,
    z: 0,
    text: null,
    image: null,
    style: null,
    line: null,
    table: null,
    chart: null,
    groupId: null,
    children: members.map((member, index) => ({
      ...(JSON.parse(JSON.stringify(member)) as SlideObject),
      z: index + 1,
    })),
    placeholder: null,
    name: "Group",
  };
  const rest = objects.filter((object) => !selected.has(object.id));
  rest.splice(insertAt, 0, group);
  return rest.map((object, index) => ({ ...object, z: index + 1 }));
}

/** Dissolve one level of grouping: selected groups return their children. */
export function ungroupSelection(objects: SlideObject[], ids: string[]): SlideObject[] {
  const selected = new Set(ids);
  const result: SlideObject[] = [];
  for (const object of [...objects].sort((a, b) => a.z - b.z)) {
    if (selected.has(object.id) && object.kind === "group" && object.children && object.children.length > 0) {
      result.push(...[...object.children].sort((a, b) => a.z - b.z).map((child) => ({ ...child, groupId: null })));
    } else {
      result.push(object);
    }
  }
  return result.map((object, index) => ({ ...object, z: index + 1 }));
}

/** Remove selected paths; a selected parent wins over its selected children. */
export function removeSelectionPaths(objects: SlideObject[], paths: SelectionPath[]): SlideObject[] {
  const topLevel = new Set(paths.filter((path) => path.length === 1).map((path) => path[0]));
  let next = objects.filter((object) => !topLevel.has(object.id));
  for (const path of paths.filter((item) => item.length > 1 && !topLevel.has(item[0]))) {
    next = removeAtPath(next, path);
  }
  return refreshGroupBounds(next);
}

/**
 * Master objects followed by layout objects, ids prefixed with `master:` /
 * `layout:` (which marks them non-selectable). A placeholder role that the
 * slide already fills is suppressed, exactly like PowerPoint.
 */
export function inheritedObjects(deck: Deck, slide: Slide): SlideObject[] {
  const masters = deck.masters ?? [];
  if (masters.length === 0) return [];
  const master = masters.find((candidate) => candidate.id === slide.masterId) ?? masters[0];
  const layout =
    master.layouts.find((candidate) => candidate.id === slide.layoutId) ??
    master.layouts.find((candidate) => candidate.kind === slide.layout) ??
    null;
  const filled = new Set(
    slide.objects.map((object) => object.placeholder).filter((role): role is string => Boolean(role)),
  );
  const inherit = (objects: SlideObject[], prefix: string): SlideObject[] =>
    objects
      .filter((object) => !object.placeholder || !filled.has(object.placeholder))
      .sort((a, b) => a.z - b.z)
      .map((object) => ({ ...cloneObject(object), id: `${prefix}:${object.id}` }));
  return [...inherit(master.objects, "master"), ...(layout ? inherit(layout.objects, "layout") : [])];
}

/**
 * Order animations into steps that start together.
 *
 * - `withPrevious` merges into the current step.
 * - `afterPrevious` starts a step when the previous step ends.
 * - `onClick` waits for the presenter to click or press space.
 */
export function animationTimeline(animations: Animation[]): AnimationStep[] {
  const sorted = [...animations].sort((a, b) => a.order - b.order);
  const steps: AnimationStep[] = [];
  for (const animation of sorted) {
    const own = Math.max(0, animation.delayMs) + Math.max(0, animation.durationMs);
    const current = steps[steps.length - 1];
    if (animation.trigger === "withPrevious" && current) {
      current.animations.push(animation);
      current.durationMs = Math.max(current.durationMs, own);
      continue;
    }
    steps.push({
      animations: [animation],
      waitForClick: animation.trigger === "onClick",
      relativeTo:
        animation.trigger === "afterPrevious"
          ? "previousEnd"
          : animation.trigger === "withPrevious"
            ? "previousStart"
            : "start",
      durationMs: own,
    });
  }
  return steps;
}

/** The style one animation phase paints on its object (transitions are added by the caller). */
export function animationEffectStyle(kind: string, effect: string, phase: "from" | "to" | "back"): CSSProperties {
  if (phase === "back") return {};
  if (kind === "entrance") {
    if (phase === "from") {
      if (effect === "flyIn") return { opacity: 0, transform: "translateY(40px)" };
      if (effect === "zoom") return { opacity: 0, transform: "scale(0.5)" };
      return { opacity: 0 };
    }
    return {};
  }
  if (kind === "emphasis") {
    if (phase === "to") {
      if (effect === "pulse") return { transform: "scale(1.15)" };
      if (effect === "grow") return { transform: "scale(1.3)" };
      if (effect === "shrink") return { transform: "scale(0.75)" };
      if (effect === "spin") return { transform: "rotate(360deg)" };
    }
    return {};
  }
  if (phase === "to") {
    if (effect === "flyOut") return { opacity: 0, transform: "translateY(-40px)" };
    return { opacity: 0 };
  }
  return {};
}

/** Style for a slideshow object from its animation state (pending/running/finished). */
export function animationObjectStyle(
  objectId: string,
  animations: Animation[],
  running: AnimationRunState,
  done: AnimationDoneState,
): CSSProperties {
  const mine = animations.filter((animation) => animation.objectId === objectId);
  const active = mine.find((animation) => Boolean(running[animation.id]));
  if (active) {
    const state = running[active.id];
    if (state) {
      const ms = active.kind === "emphasis" ? Math.max(0, active.durationMs / 2) : Math.max(0, active.durationMs);
      return {
        ...animationEffectStyle(active.kind, active.effect, state.phase),
        transition: `opacity ${ms}ms ease, transform ${ms}ms ease`,
      };
    }
  }
  const entrance = mine.find((animation) => animation.kind === "entrance");
  if (entrance && !done[entrance.id]) return animationEffectStyle("entrance", entrance.effect, "from");
  const exit = mine.find((animation) => animation.kind === "exit");
  if (exit && done[exit.id]) return animationEffectStyle("exit", exit.effect, "to");
  return {};
}

/** mm:ss clock for the presenter view. */
export function formatClock(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const minutes = Math.floor(total / 60);
  const seconds = total % 60;
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

/** Top-level objects whose boxes intersect a marquee rectangle (slide units). */
export function objectsInRect(
  objects: SlideObject[],
  rect: { x: number; y: number; w: number; h: number },
): SlideObject[] {
  return objects.filter(
    (object) =>
      object.x < rect.x + rect.w &&
      object.x + object.w > rect.x &&
      object.y < rect.y + rect.h &&
      object.y + object.h > rect.y,
  );
}

type AnimationRunState = Record<string, { effect: Animation; phase: "to" | "back" }>;
type AnimationDoneState = Record<string, boolean>;
