/**
 * Connector geometry for Impress shapes.
 *
 * A connector is a plain `line`/`arrow` object whose `LineSpec` names the
 * objects its ends are glued to (`beginObject`/`endObject`) plus the connection
 * site on each shape (0 top, 1 left, 2 bottom, 3 right). The line's own box is
 * kept in sync with the routed endpoints, so the existing line renderer can
 * draw it without knowing anything about gluing.
 */
import type { LineSpec, SlideObject } from "../../lib/office-types";

export interface Point {
  x: number;
  y: number;
}

export interface ConnectorEndpoints {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
}

/** One edge of an object box, in the OOXML site order. */
function edgeSegment(object: SlideObject, site: number): [Point, Point] {
  switch (site) {
    case 0:
      return [
        { x: object.x, y: object.y },
        { x: object.x + object.w, y: object.y },
      ];
    case 1:
      return [
        { x: object.x, y: object.y },
        { x: object.x, y: object.y + object.h },
      ];
    case 2:
      return [
        { x: object.x, y: object.y + object.h },
        { x: object.x + object.w, y: object.y + object.h },
      ];
    default:
      return [
        { x: object.x + object.w, y: object.y },
        { x: object.x + object.w, y: object.y + object.h },
      ];
  }
}

/** Distance from a point to a segment, used to pick the nearest site. */
export function distanceToSegment(point: Point, start: Point, end: Point): number {
  const dx = end.x - start.x;
  const dy = end.y - start.y;
  const lengthSquared = dx * dx + dy * dy;
  if (lengthSquared === 0) return Math.hypot(point.x - start.x, point.y - start.y);
  const t = Math.max(0, Math.min(1, ((point.x - start.x) * dx + (point.y - start.y) * dy) / lengthSquared));
  const x = start.x + t * dx;
  const y = start.y + t * dy;
  return Math.hypot(point.x - x, point.y - y);
}

/** The connection site (0 top, 1 left, 2 bottom, 3 right) closest to a point. */
export function nearestSite(object: SlideObject, point: Point): number {
  let best = 0;
  let bestDistance = Number.POSITIVE_INFINITY;
  for (let site = 0; site < 4; site += 1) {
    const [start, end] = edgeSegment(object, site);
    const distance = distanceToSegment(point, start, end);
    if (distance < bestDistance) {
      bestDistance = distance;
      best = site;
    }
  }
  return best;
}

/**
 * The anchor point of a connection site. `index` spreads several connectors
 * attached to the same site along the edge so they do not overlap; 0 is the
 * midpoint.
 */
export function siteAnchor(object: SlideObject, site: number, index = 0): Point {
  const offset = index * 16;
  switch (site) {
    case 0: {
      const limit = Math.max(0, object.w / 2 - 8);
      return { x: object.x + object.w / 2 + Math.max(-limit, Math.min(limit, offset)), y: object.y };
    }
    case 1: {
      const limit = Math.max(0, object.h / 2 - 8);
      return { x: object.x, y: object.y + object.h / 2 + Math.max(-limit, Math.min(limit, offset)) };
    }
    case 2: {
      const limit = Math.max(0, object.w / 2 - 8);
      return { x: object.x + object.w / 2 + Math.max(-limit, Math.min(limit, offset)), y: object.y + object.h };
    }
    default: {
      const limit = Math.max(0, object.h / 2 - 8);
      return { x: object.x + object.w, y: object.y + object.h / 2 + Math.max(-limit, Math.min(limit, offset)) };
    }
  }
}

/** Depth-first lookup that also reaches group children. */
function findObject(objects: SlideObject[], id: string): SlideObject | undefined {
  for (const object of objects) {
    if (object.id === id) return object;
    const child = object.children ? findObject(object.children, id) : undefined;
    if (child) return child;
  }
  return undefined;
}

/**
 * Endpoints of a connector in slide coordinates. A glued end resolves to the
 * site anchor of its target; an unglued end (or a missing target) falls back to
 * the line object's own position plus its local `x2`/`y2` offset.
 */
export function routeConnector(object: SlideObject, objects: SlideObject[]): ConnectorEndpoints {
  const line = object.line;
  if (!line) {
    return { x1: object.x, y1: object.y, x2: object.x + object.w, y2: object.y + object.h };
  }
  let x1 = object.x;
  let y1 = object.y;
  let x2 = object.x + line.x2;
  let y2 = object.y + line.y2;
  if (line.beginObject) {
    const target = findObject(objects, line.beginObject);
    if (target) {
      const anchor = siteAnchor(target, line.beginSite ?? 0);
      x1 = anchor.x;
      y1 = anchor.y;
    }
  }
  if (line.endObject) {
    const target = findObject(objects, line.endObject);
    if (target) {
      const anchor = siteAnchor(target, line.endSite ?? 0);
      x2 = anchor.x;
      y2 = anchor.y;
    }
  }
  return { x1, y1, x2, y2 };
}

function patchFromEndpoints(object: SlideObject, endpoints: ConnectorEndpoints): SlideObject {
  const line = object.line as LineSpec;
  const x = Math.round(Math.min(endpoints.x1, endpoints.x2));
  const y = Math.round(Math.min(endpoints.y1, endpoints.y2));
  return {
    ...object,
    x,
    y,
    w: Math.max(1, Math.round(Math.abs(endpoints.x2 - endpoints.x1))),
    h: Math.max(1, Math.round(Math.abs(endpoints.y2 - endpoints.y1))),
    line: { ...line, x2: Math.round(endpoints.x2) - x, y2: Math.round(endpoints.y2) - y },
  };
}

/** A connector object with its box recomputed from its routed endpoints. */
export function connectorPatch(object: SlideObject, objects: SlideObject[]): SlideObject {
  if (!object.line || (!object.line.beginObject && !object.line.endObject)) return object;
  return patchFromEndpoints(object, routeConnector(object, objects));
}

/** Every connector in a slide with its box recomputed (touches nothing else). */
export function recomputeConnectors(objects: SlideObject[]): SlideObject[] {
  const glues = (list: SlideObject[]): boolean =>
    list.some(
      (object) =>
        Boolean(object.line && (object.line.beginObject || object.line.endObject)) ||
        (object.children ? glues(object.children) : false),
    );
  if (!glues(objects)) return objects;
  const patch = (list: SlideObject[]): SlideObject[] =>
    list.map((object) => {
      const next =
        object.line && (object.line.beginObject || object.line.endObject)
          ? patchFromEndpoints(object, routeConnector(object, objects))
          : object;
      return next.children && next.children.length > 0 ? { ...next, children: patch(next.children) } : next;
    });
  return patch(objects);
}

/**
 * Rewrites the glue ids of copied objects through the clone id map, so a pasted
 * connector follows the pasted shapes and drops a glue whose target was not
 * copied along.
 */
export function remapConnectorTargets(objects: SlideObject[], idMap: Map<string, string>): SlideObject[] {
  return objects.map((object) => {
    const next = object.line
      ? {
          ...object,
          line: {
            ...object.line,
            beginObject: object.line.beginObject
              ? (idMap.get(object.line.beginObject) ?? null)
              : object.line.beginObject,
            endObject: object.line.endObject ? (idMap.get(object.line.endObject) ?? null) : object.line.endObject,
          },
        }
      : object;
    return next.children ? { ...next, children: remapConnectorTargets(next.children, idMap) } : next;
  });
}
