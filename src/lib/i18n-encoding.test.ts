import { describe, expect, it } from "vitest";

import en from "./i18n/en.json";
import tr from "./i18n/tr.json";

const tables = { en, tr } as const;

/**
 * A bulk insert once ran the Turkish strings through
 * `value.encode("utf-8").decode("unicode_escape")`, which reads UTF-8 bytes as
 * Latin-1. Every non-ASCII character came out doubled - "ç" became "Ã§" - and
 * the application shipped with mojibake in the entire Turkish interface while
 * both the type checker and every functional test stayed green.
 *
 * The tell is a Latin-1 letter in the range U+00C0-U+00FF immediately followed
 * by a character in U+0080-U+00BF, which is what UTF-8 bytes look like when
 * someone reads them as Latin-1. Real Turkish text has no such sequence.
 */

const MOJIBAKE = /[Â-ÿ][-¿]/;

function table(name: "en" | "tr"): string {
  // The dictionaries are JSON assets; the checks below only need key/value
  // pairs, so serialising the imported object keeps the regex-driven helpers
  // working regardless of the file format behind it.
  return JSON.stringify(tables[name]);
}

function entries(body: string): { key: string; value: string }[] {
  const out: { key: string; value: string }[] = [];
  for (const [key, value] of Object.entries(JSON.parse(body) as Record<string, string>)) {
    out.push({ key, value });
  }
  return out;
}

describe("translation tables are not mojibake", () => {
  for (const name of ["en", "tr"] as const) {
    it(`${name} has no double-encoded characters`, () => {
      const offenders = entries(table(name))
        .filter((entry) => MOJIBAKE.test(entry.value))
        .map((entry) => `${entry.key} = ${entry.value}`);
      expect(offenders).toEqual([]);
    });

    it(`${name} has no replacement characters`, () => {
      const offenders = entries(table(name))
        .filter((entry) => entry.value.includes("\ufffd"))
        .map((entry) => entry.key);
      expect(offenders).toEqual([]);
    });
  }

  it("Turkish really contains Turkish characters, not a fallback", () => {
    const tr = table("tr");
    // A table that is entirely ASCII is not translated, whatever it is called.
    const accented = entries(tr).filter((entry) => /[çğıöşü]/i.test(entry.value));
    expect(accented.length).toBeGreaterThan(100);
    // The screens added for redaction, comparison and inspection must be there.
    for (const key of ["nav.redact", "nav.compare", "nav.inspect"]) {
      expect(tr).toContain(`"${key}"`);
    }
  });
});
