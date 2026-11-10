import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

// WCAG 2.x contrast for the colour tokens every screen paints text with, in
// all four themes (7.33.1). jsdom applies no stylesheet, so the tokens are
// read from styles.css and the ratio is computed here.
const css = readFileSync(path.resolve(__dirname, "..", "styles.css"), "utf8").replace(/\r\n/g, "\n");

function block(selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return new RegExp(`(^|\\n)${escaped} \\{([^}]*)\\}`).exec(css)?.[2] ?? "";
}

type Tokens = Record<string, string>;

function tokens(selector: string): Tokens {
  return Object.fromEntries([...block(selector).matchAll(/(--[\w-]+):\s*([^;]+);/g)].map((m) => [m[1], m[2].trim()]));
}

const light = tokens(":root");
const dark = { ...light, ...tokens(".dark") };
const THEMES: Record<string, Tokens> = {
  light,
  dark,
  // Both variants also carry the light or .dark class (lib/store.ts paintTheme).
  midnight: { ...dark, ...tokens(':root[data-theme="midnight"]') },
  paper: { ...light, ...tokens(':root[data-theme="paper"]') },
};
// Each accent (lib/accent.ts) in the light and the dark theme.
for (const accent of ["blue", "teal", "pink"]) {
  const lightAccent = tokens(`:root[data-accent="${accent}"]`);
  THEMES[`light/${accent}`] = { ...light, ...lightAccent };
  THEMES[`dark/${accent}`] = { ...dark, ...lightAccent, ...tokens(`:root.dark[data-accent="${accent}"]`) };
}

type Rgb = [number, number, number];

function resolve(value: string, theme: Tokens): string {
  let out = value;
  for (let guard = 0; /var\(/.test(out) && guard < 8; guard += 1) {
    out = out.replace(/var\((--[\w-]+)\)/, (_, name: string) => theme[name] ?? "");
  }
  return out;
}

/** A hex or rgba() token, composited over `under` when it is translucent. */
function colour(value: string, under: Rgb = [255, 255, 255]): Rgb {
  const rgba = /rgba?\(([^)]+)\)/.exec(value);
  if (rgba) {
    const [r, g, b, a = 1] = rgba[1].split(",").map(Number);
    return [r, g, b].map((channel, i) => Math.round(channel * a + under[i] * (1 - a))) as Rgb;
  }
  const hex = value.replace("#", "");
  const full = hex.length === 3 ? [...hex].map((c) => c + c).join("") : hex;
  return [0, 2, 4].map((i) => parseInt(full.slice(i, i + 2), 16)) as Rgb;
}

function luminance([r, g, b]: Rgb): number {
  const [lr, lg, lb] = [r, g, b].map((channel) => {
    const c = channel / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * lr + 0.7152 * lg + 0.0722 * lb;
}

function contrast(a: Rgb, b: Rgb): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

describe("theme contrast (WCAG AA, 7.33.1)", () => {
  it("reads body, secondary, accent and document-kind text at 4.5:1 in every theme and accent", () => {
    const failing: string[] = [];
    for (const [name, theme] of Object.entries(THEMES)) {
      const get = (token: string, under?: Rgb) => colour(resolve(theme[token] ?? "", theme), under);
      const surface = get("--surface");
      const ground = get("--bg");
      const pairs: [string, number][] = [
        ["--text on --surface", contrast(get("--text"), surface)],
        ["--muted on --surface", contrast(get("--muted"), surface)],
        ["--muted on --bg", contrast(get("--muted"), ground)],
        ["--accent-ink on --accent-weak", contrast(get("--accent-ink"), get("--accent-weak", surface))],
        ["--accent-text on --accent", contrast(get("--accent-text"), get("--accent"))],
      ];
      for (const token of ["--doc-writer", "--doc-calc", "--doc-impress", "--doc-pdf"]) {
        pairs.push([`${token} on --surface`, contrast(get(token), surface)]);
      }
      for (const [label, ratio] of pairs) if (ratio < 4.5) failing.push(`${name}: ${label} ${ratio.toFixed(2)}`);
    }
    expect(Object.keys(THEMES)).toHaveLength(10);
    expect(failing).toEqual([]);
  });

  it("defines all three non-default accents for both themes", () => {
    for (const accent of ["blue", "teal", "pink"]) {
      expect(tokens(`:root[data-accent="${accent}"]`)["--accent-ink"]).toBeTruthy();
      expect(tokens(`:root.dark[data-accent="${accent}"]`)["--accent-ink"]).toBeTruthy();
    }
  });

  it("computes the WCAG reference ratios", () => {
    expect(contrast([0, 0, 0], [255, 255, 255])).toBeCloseTo(21, 5);
    expect(contrast([118, 118, 118], [255, 255, 255])).toBeCloseTo(4.54, 2);
  });
});
