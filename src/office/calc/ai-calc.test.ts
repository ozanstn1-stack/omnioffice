import { describe, expect, it } from "vitest";
import { headerContext, MAX_HEADER_CELLS, MAX_HEADER_CHARS } from "./ai-calc";
import { columnLabel, formatAddress, type Scalar } from "./formula";

describe("headerContext", () => {
  it("lists the first-row headers with their column letters", () => {
    const computed = new Map<string, Scalar>([
      [formatAddress(0, 0), "Name"],
      [formatAddress(0, 1), "Amount"],
    ]);
    expect(headerContext(computed, 2)).toBe("A: Name; B: Amount");
  });

  it("sends at most 30 headers of at most 40 characters", () => {
    const computed = new Map<string, Scalar>();
    for (let col = 0; col < 80; col += 1) computed.set(formatAddress(0, col), `H${col}-${"x".repeat(100)}`);
    const parts = headerContext(computed, 80).split("; ");
    expect(parts).toHaveLength(MAX_HEADER_CELLS);
    expect(MAX_HEADER_CELLS).toBe(30);
    expect(MAX_HEADER_CHARS).toBe(40);
    for (const [index, part] of parts.entries()) {
      expect(part.startsWith(`${columnLabel(index)}: `)).toBe(true);
      expect(part.slice(part.indexOf(": ") + 2).length).toBeLessThanOrEqual(MAX_HEADER_CHARS);
    }
  });

  it("skips blank header cells without counting them", () => {
    const computed = new Map<string, Scalar>();
    for (let col = 0; col < 70; col += 2) computed.set(formatAddress(0, col), `h${col}`);
    expect(headerContext(computed, 70).split("; ")).toHaveLength(30);
  });
});
