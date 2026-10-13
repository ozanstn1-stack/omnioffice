/**
 * Image blob store.
 *
 * Office models carry images as base64 inside the document JSON, so every
 * autosave shipped every image over IPC even when nothing changed. This module
 * lifts the image payload out of the model: `dataBase64` becomes a
 * content-addressed `blobRef` digest, and only blobs the session has not seen
 * are sent to the backend. The model still round-trips losslessly (see
 * `hydrateImageBlobs`), and a missing blob is an error, never a silent drop.
 */

/** A 128-bit content digest (four 32-bit lanes), hex, for addressing a blob. */
export function contentHash(value: string): string {
  let h1 = 0x811c9dc5;
  let h2 = 0x01000193;
  let h3 = 0x9e3779b9;
  let h4 = 0x85ebca6b;
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index);
    h1 = Math.imul(h1 ^ code, 0x01000193) >>> 0;
    h2 = Math.imul(h2 + code, 0x85ebca6b) >>> 0;
    h3 = (h3 ^ Math.imul(code + index, 0xc2b2ae35)) >>> 0;
    h4 = Math.imul(h4 ^ (code + (index & 0xffff)), 0x27d4eb2f) >>> 0;
  }
  return [h1, h2, h3, h4].map((lane) => lane.toString(16).padStart(8, "0")).join("");
}

export interface StripResult {
  /** The model with each `dataBase64` replaced by a `blobRef` digest. */
  model: unknown;
  /** Unique blobs keyed by digest (only the ones found in this model). */
  blobs: Map<string, string>;
  /** How many images were lifted out. */
  stripped: number;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/**
 * Replaces every image's `dataBase64` with a `blobRef` digest and collects the
 * unique blobs. A model with no images is returned unchanged.
 */
export function stripImageBlobs(model: unknown): StripResult {
  const blobs = new Map<string, string>();
  let stripped = 0;

  const visit = (value: unknown): unknown => {
    if (Array.isArray(value)) return value.map(visit);
    if (!isRecord(value)) return value;
    const next: Record<string, unknown> = {};
    for (const [key, child] of Object.entries(value)) next[key] = visit(child);
    if (typeof value.dataBase64 === "string" && value.dataBase64.length > 0) {
      const digest = contentHash(value.dataBase64);
      blobs.set(digest, value.dataBase64);
      stripped += 1;
      delete next.dataBase64;
      next.blobRef = digest;
    }
    return next;
  };

  return { model: visit(model), blobs, stripped };
}

/**
 * Restores `dataBase64` from `blobRef` using `store`. Throws when a referenced
 * blob is missing so a corrupt store surfaces instead of losing an image.
 */
export function hydrateImageBlobs(model: unknown, store: ReadonlyMap<string, string>): unknown {
  const visit = (value: unknown): unknown => {
    if (Array.isArray(value)) return value.map(visit);
    if (!isRecord(value)) return value;
    const next: Record<string, unknown> = {};
    for (const [key, child] of Object.entries(value)) next[key] = visit(child);
    if (typeof value.blobRef === "string") {
      const blob = store.get(value.blobRef);
      if (blob === undefined) throw new Error(`Missing image blob: ${value.blobRef}`);
      delete next.blobRef;
      next.dataBase64 = blob;
    }
    return next;
  };
  return visit(model);
}

/**
 * Tracks which blobs a session has already sent, so an autosave only ships the
 * images it has never sent before. Purely in-memory: the backend owns the
 * durable, content-addressed store.
 */
export class BlobSendCache {
  private readonly sent = new Set<string>();

  /** The subset of `blobs` not sent yet, marking them sent. */
  pending(blobs: ReadonlyMap<string, string>): Map<string, string> {
    const fresh = new Map<string, string>();
    for (const [digest, data] of blobs) {
      if (this.sent.has(digest)) continue;
      this.sent.add(digest);
      fresh.set(digest, data);
    }
    return fresh;
  }

  /** Forgets the sent set (a new document/session). */
  reset(): void {
    this.sent.clear();
  }

  get size(): number {
    return this.sent.size;
  }
}
