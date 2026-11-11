/**
 * The specta bindings (`npm run bindings:gen`) are the naming source of truth
 * for the IPC DTOs; the app still narrows some of them by hand (string literal
 * unions instead of `string`, optional members for `#[serde(default)]`
 * fields). A rename on the Rust side regenerates the binding and this test
 * fails on the hand-written copy, instead of the value silently arriving as
 * `undefined` (the v5.0.0 audit's `headerFooter` / `taxIdValid` failures).
 */
import { describe, expect, it } from "vitest";

import apiSource from "./api.ts?raw";
import typesSource from "./types.ts?raw";
import spectaSource from "./generated/specta.ts?raw";

/**
 * Field names of a generated `export type X = { a: T, b?: U, };`. Types that
 * read and write differently come as `X_Deserialize` (what the app sends);
 * doc comments are dropped so their words cannot read as fields.
 */
function generatedFields(typeName: string): string[] {
  const split = spectaSource.indexOf(`export type ${typeName}_Deserialize = {`);
  const start = split >= 0 ? split : spectaSource.indexOf(`export type ${typeName} = {`);
  if (start < 0) throw new Error(`generated type ${typeName} not found`);
  const body = spectaSource.slice(start, spectaSource.indexOf("};", start)).replace(/\/\*[\s\S]*?\*\//g, "");
  return [...body.matchAll(/(?:^|[,{\s])(\w+)(?:\?)?:\s/g)].map((match) => match[1]).sort();
}

/** Field names of a hand-written `export interface X { ... }`. */
function declaredFields(source: string, name: string): string[] {
  const start = source.indexOf(`export interface ${name} {`);
  if (start < 0) throw new Error(`declared interface ${name} not found`);
  const body = source.slice(start, source.indexOf("\n}", start));
  return [...body.matchAll(/^\s{2}(\w+)(?:\?)?:/gm)].map((match) => match[1]).sort();
}

/** `{ kind, [fields] }` per variant of a tagged union. */
function unionVariants(source: string, typeName: string): Map<string, string[]> {
  const start = source.indexOf(`export type ${typeName} =`);
  if (start < 0) throw new Error(`union ${typeName} not found`);
  // The hand-written union spans several lines (one variant per line, separated
  // by `;`), the generated one is a single line; a blank line ends both.
  const rest = source.slice(start);
  const end = rest.indexOf("\n\n");
  const body = end > 0 ? rest.slice(0, end) : rest;
  const out = new Map<string, string[]>();
  for (const match of body.matchAll(/\{([^{}]*)\}/g)) {
    const kind = /"?kind"?\s*:\s*"([a-zA-Z-]+)"/.exec(match[1])?.[1];
    if (!kind) continue;
    const fields = [...match[1].matchAll(/(\w+)\s*\??:/g)]
      .map((entry) => entry[1])
      .filter((name) => name !== "kind")
      .sort();
    out.set(kind, fields);
  }
  return out;
}

describe("generated bindings match the hand-written types", () => {
  const pairs = [
    "CompressOptions",
    "OcrPreprocess",
    "OcrOptions",
    "HeaderFooterOptions",
    "BatesOptions",
    "RedactionArea",
    "RedactionOptions",
  ];

  for (const name of pairs) {
    it(`${name} uses the same field names as the Rust struct`, () => {
      expect(declaredFields(typesSource, name)).toEqual(generatedFields(name));
    });
  }

  // The app's RecipeStep is the tauri-specta binding (#316); what Rust sends
  // back must carry the same variants and fields as what the app sends.
  it("RecipeStep carries the same variants and fields as the Rust enum", () => {
    expect(apiSource).toContain("export type RecipeStep = specta.RecipeStep_Deserialize;");
    expect(unionVariants(spectaSource, "RecipeStep_Deserialize")).toEqual(
      unionVariants(spectaSource, "RecipeStep_Serialize"),
    );
  });

  it("documents the snake_case wire names of the compress/OCR options", () => {
    // These two structs (unlike the rest) have no `rename_all`, so their
    // multi-word fields stay snake_case on the wire; the test pins that so a
    // future rename is a deliberate wire change.
    expect(generatedFields("CompressOptions")).toContain("jpeg_quality");
    expect(generatedFields("CompressOptions")).toContain("remove_metadata");
    expect(generatedFields("OcrOptions")).toContain("output_mode");
    expect(generatedFields("OcrOptions")).toContain("skip_text_pages");
  });
});
