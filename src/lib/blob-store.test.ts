import { describe, expect, it } from "vitest";

import { BlobSendCache, contentHash, hydrateImageBlobs, stripImageBlobs } from "./blob-store";

function modelWithImages(): Record<string, unknown> {
  // Realistic base64 image data: long enough that replacing it with a 32-char
  // digest actually shrinks the serialized model.
  const imageA = "A".repeat(400);
  const imageB = "B".repeat(400);
  return {
    id: "doc",
    title: "Belge",
    blocks: [
      { type: "paragraph", runs: [{ text: "hi" }] },
      {
        type: "image",
        image: { name: "a.png", mime: "image/png", dataBase64: imageA, alt: "kedi" },
      },
      {
        type: "image",
        image: { name: "b.png", mime: "image/png", dataBase64: imageB, alt: "" },
      },
      // The same image twice: only one blob must be stored.
      {
        type: "image",
        image: { name: "a-again.png", mime: "image/png", dataBase64: imageA, alt: "" },
      },
    ],
  };
}

describe("image blob store", () => {
  it("lifts images out of the model and deduplicates by content", () => {
    const { model, blobs, stripped } = stripImageBlobs(modelWithImages());
    expect(stripped).toBe(3);
    expect(blobs.size).toBe(2);
    expect(blobs.get(contentHash("A".repeat(400)))).toBe("A".repeat(400));

    const json = JSON.stringify(model);
    expect(json).not.toContain("dataBase64");
    expect(json).toContain("blobRef");
    // The stripped model is smaller than the original (the point of the store).
    expect(json.length).toBeLessThan(JSON.stringify(modelWithImages()).length);
  });

  it("round-trips losslessly", () => {
    const original = modelWithImages();
    const { model, blobs } = stripImageBlobs(original);
    const restored = hydrateImageBlobs(model, blobs);
    expect(restored).toEqual(original);
  });

  it("throws when a referenced blob is missing (integrity, not silent loss)", () => {
    const { model } = stripImageBlobs(modelWithImages());
    expect(() => hydrateImageBlobs(model, new Map())).toThrow(/Missing image blob/);
  });

  it("leaves a model with no images unchanged", () => {
    const plain = { id: "doc", blocks: [{ type: "paragraph", runs: [{ text: "hi" }] }] };
    const { model, blobs, stripped } = stripImageBlobs(plain);
    expect(stripped).toBe(0);
    expect(blobs.size).toBe(0);
    expect(model).toEqual(plain);
  });

  it("hashes are stable and distinct", () => {
    expect(contentHash("AAAA")).toBe(contentHash("AAAA"));
    expect(contentHash("AAAA")).not.toBe(contentHash("BBBB"));
    expect(contentHash("AAAA")).toMatch(/^[0-9a-f]{32}$/);
  });

  it("sends each blob only once per session, after the backend confirms it", () => {
    const cache = new BlobSendCache();
    const { blobs } = stripImageBlobs(modelWithImages());
    const first = cache.pending(blobs);
    expect(first.size).toBe(2);
    // Nothing is marked until the backend confirms the write.
    expect(cache.pending(blobs).size).toBe(2);
    cache.commit(first.keys());
    expect(cache.pending(blobs).size).toBe(0);
    // A new image is still sent.
    const extra = new Map([...blobs, [contentHash("CCCC"), "CCCC"]]);
    expect(cache.pending(extra).size).toBe(1);
    cache.reset();
    expect(cache.pending(blobs).size).toBe(2);
  });

  it("resends the blobs after a failed save", () => {
    const cache = new BlobSendCache();
    const { blobs } = stripImageBlobs(modelWithImages());
    // The save read `pending` and then failed, so nothing was committed.
    expect(cache.pending(blobs).size).toBe(2);
    // The retry must still carry every blob, or recovery would lose them.
    expect(cache.pending(blobs).size).toBe(2);
  });
});
