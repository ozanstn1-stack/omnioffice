import { describe, expect, it } from "vitest";

import inspectSource from "./Inspect.tsx?raw";
import typesSource from "../lib/types.ts?raw";

/**
 * The inspector reads `DocumentInspection`, which the backend serializes with
 * `#[serde(rename_all = "camelCase")]`. A property name that does not match
 * arrives as `undefined`: the overview read a field that does not exist, and
 * `report.pages.length` on a payload with no `pages` array blanked the entire
 * window. The type checker did not catch it, because a duplicate `PageGeometry`
 * interface had been merged into the real one.
 *
 * This test closes that gap from the other side. It reads payloads captured
 * from real documents (regenerate them with
 * `cargo test -p pdfcore --test inspection_contract -- --nocapture`) and checks
 * the inspector screen against them, so a field added on the Rust side cannot
 * then be quietly ignored, and a field the backend never sends cannot be read.
 */

const fixtureModules = import.meta.glob("../../crates/pdfcore/tests/fixtures/inspection-*.json", {
  eager: true,
  import: "default",
}) as Record<string, Record<string, unknown>>;

const fixtures = Object.values(fixtureModules);
const keys = [...new Set(fixtures.flatMap((fixture) => Object.keys(fixture)))].sort();

describe("the inspection payload and the inspector agree", () => {
  it("has captured at least one real document", () => {
    expect(fixtures.length).toBeGreaterThan(0);
  });

  it("declares every field the backend sends", () => {
    const declared = new Set<string>();
    const body = typesSource.slice(typesSource.indexOf("export interface DocumentInspection {"));
    for (const match of body.matchAll(/^ {2}([A-Za-z0-9]+)\??:/gm)) declared.add(match[1]);
    expect(
      keys.filter((key) => !declared.has(key)),
      "DocumentInspection is missing fields the backend sends",
    ).toEqual([]);
  });

  it("uses every field the backend sends", () => {
    // `path` and `viewerPreferences` are reported for completeness; neither
    // earns a row, and inventing one would be noise.
    const intentionallyUnused = new Set(["path", "viewerPreferences"]);
    expect(
      keys.filter((key) => !intentionallyUnused.has(key) && !inspectSource.includes(`.${key}`)),
      "the backend sends these but the inspector never shows them",
    ).toEqual([]);
  });

  it("does not read fields the backend never sends", () => {
    const read = [...new Set([...inspectSource.matchAll(/report\.([A-Za-z0-9]+)/g)].map((match) => match[1]))];
    expect(
      read.filter((key) => !keys.includes(key)),
      "the inspector reads fields that are not in the payload",
    ).toEqual([]);
  });

  it("captured documents that exercise fonts, images and findings", () => {
    const has = (key: string) => fixtures.some((fixture) => ((fixture[key] as unknown[]) ?? []).length > 0);
    expect(has("fonts"), "no sample has fonts, so the fonts tab is untested").toBe(true);
    expect(has("images"), "no sample has images, so the images tab is untested").toBe(true);
    expect(has("findings"), "no sample has findings").toBe(true);
    expect(fixtures.every((fixture) => (fixture.pageCount as number) > 0)).toBe(true);
  });
});
