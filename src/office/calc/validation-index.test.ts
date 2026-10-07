/**
 * Data-validation lookup.
 *
 * The grid used to ask `addressesInRange(rule.range, 100).includes(address)`
 * for every visible cell: that expanded each rule into strings per cell, and
 * the 100-address cap meant a cell past the 100th of a rule's range was never
 * validated. The lookup now parses every rule once into numeric bounds.
 */
import { describe, expect, it } from "vitest";
import { boundsContain, parseAreas, validationLookup } from "./validation-index";

const rule = (id: string, range: string) => ({ id, range });

describe("parseAreas", () => {
  it("parses a range into numeric bounds", () => {
    expect(parseAreas("B2:D5")).toEqual([{ top: 1, left: 1, bottom: 4, right: 3 }]);
    expect(parseAreas("$B$2:$D$5")).toEqual([{ top: 1, left: 1, bottom: 4, right: 3 }]);
  });

  it("normalises a reversed range and a single cell", () => {
    expect(parseAreas("D5:B2")).toEqual([{ top: 1, left: 1, bottom: 4, right: 3 }]);
    expect(parseAreas("C3")).toEqual([{ top: 2, left: 2, bottom: 2, right: 2 }]);
  });

  it("reads the several areas XLSX separates with spaces", () => {
    expect(parseAreas("A1:A3  C5:D6")).toEqual([
      { top: 0, left: 0, bottom: 2, right: 0 },
      { top: 4, left: 2, bottom: 5, right: 3 },
    ]);
  });

  it("skips what is not an address", () => {
    expect(parseAreas("")).toEqual([]);
    expect(parseAreas("nonsense A1")).toEqual([{ top: 0, left: 0, bottom: 0, right: 0 }]);
    expect(boundsContain({ top: 0, left: 0, bottom: 1, right: 1 }, 1, 1)).toBe(true);
    expect(boundsContain({ top: 0, left: 0, bottom: 1, right: 1 }, 2, 1)).toBe(false);
  });
});

describe("validationLookup", () => {
  it("finds the rule covering a cell and nothing for others", () => {
    const lookup = validationLookup([rule("a", "B2:C3")]);
    expect(lookup.find(1, 1)?.id).toBe("a");
    expect(lookup.find(2, 2)?.id).toBe("a");
    expect(lookup.find(0, 0)).toBeUndefined();
    expect(lookup.find(3, 1)).toBeUndefined();
    expect(lookup.find(1, 3)).toBeUndefined();
  });

  it("validates a cell beyond the 100th of a rule's range", () => {
    // Regression: only the first 100 addresses of each rule were checked, so
    // A101 and everything below it escaped validation.
    const lookup = validationLookup([rule("tall", "A1:A500")]);
    expect(lookup.find(99, 0)?.id).toBe("tall");
    expect(lookup.find(100, 0)?.id).toBe("tall");
    expect(lookup.find(499, 0)?.id).toBe("tall");
    expect(lookup.find(500, 0)).toBeUndefined();
    // A wide block: Z10 is address number 260 in row-major order.
    const block = validationLookup([rule("block", "A1:Z10")]);
    expect(block.find(9, 25)?.id).toBe("block");
    expect(block.find(4, 25)?.id).toBe("block");
  });

  it("handles a whole-sheet range without expanding it", () => {
    const lookup = validationLookup([rule("all", "A1:XFD1048576")]);
    expect(lookup.find(1_048_575, 16_383)?.id).toBe("all");
  });

  it("prefers the first rule in the order they are listed", () => {
    const lookup = validationLookup([rule("first", "A1:A10"), rule("second", "A5:A20")]);
    expect(lookup.find(6, 0)?.id).toBe("first");
    expect(lookup.find(15, 0)?.id).toBe("second");
  });

  it("matches every area of a multi-area rule", () => {
    const lookup = validationLookup([rule("multi", "A1:A3 C5:D6")]);
    expect(lookup.find(1, 0)?.id).toBe("multi");
    expect(lookup.find(5, 3)?.id).toBe("multi");
    expect(lookup.find(4, 0)).toBeUndefined();
  });

  it("ignores a rule whose range cannot be read", () => {
    const lookup = validationLookup([rule("bad", "??"), rule("ok", "A1")]);
    expect(lookup.find(0, 0)?.id).toBe("ok");
  });

  it("parses the rules once per list, not once per lookup", () => {
    const rules = [rule("a", "A1:A5")];
    const first = validationLookup(rules);
    expect(validationLookup(rules)).toBe(first);
    expect(validationLookup([...rules])).not.toBe(first);
  });

  it("stays fast across many rules and many lookups", () => {
    const rules = Array.from({ length: 200 }, (_, index) =>
      rule(`r${index}`, `A${index * 50 + 1}:C${index * 50 + 50}`),
    );
    const lookup = validationLookup(rules);
    const started = performance.now();
    let hits = 0;
    for (let row = 0; row < 10_000; row += 1) {
      for (let col = 0; col < 3; col += 1) if (lookup.find(row, col)) hits += 1;
    }
    expect(hits).toBe(30_000);
    expect(performance.now() - started).toBeLessThan(2000);
  });
});
