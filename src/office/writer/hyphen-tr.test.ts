import { describe, expect, it } from "vitest";
import golden from "../../../crates/officecore/tests/fixtures/hyphen-tr-golden.txt?raw";
import { hyphenBreakOffsets, syllablesTr } from "./hyphen-tr";

describe("Turkish hyphenation (TS twin)", () => {
  it("matches the Rust golden list word for word", () => {
    let checked = 0;
    for (const line of golden.split(/\r?\n/)) {
      const trimmed = line.trim();
      if (!trimmed || trimmed.startsWith("#")) continue;
      const [word, expected] = trimmed.split("\t");
      expect(syllablesTr(word).join("-"), word).toBe(expected);
      checked += 1;
    }
    expect(checked).toBe(500);
  });

  it("keeps the hand-checked cases and the line-break view", () => {
    expect(syllablesTr("İstanbul").join("-")).toBe("İs-tan-bul");
    expect(syllablesTr("güneş").join("-")).toBe("gü-neş");
    expect(syllablesTr("araba").join("-")).toBe("a-ra-ba");
    expect(hyphenBreakOffsets("araba")).toEqual([3]);
    expect(syllablesTr("tren")).toEqual(["tren"]);
    expect(hyphenBreakOffsets("12.34")).toEqual([]);
  });
});
