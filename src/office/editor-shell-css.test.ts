import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

// jsdom does not apply the stylesheet, so the 7.33 editor shell is pinned here.
const css = readFileSync(path.resolve(__dirname, "..", "styles.css"), "utf8").replace(/\r\n/g, "\n");

/** The body of the first top-level rule for exactly `selector`. */
function rule(selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return new RegExp(`(^|\\n)${escaped} \\{([^}]*)\\}`).exec(css)?.[2] ?? "";
}

/** The value a theme block gives a custom property. */
function token(block: string, name: string): string | undefined {
  return new RegExp(`${name}:\\s*([^;]+);`).exec(rule(block))?.[1]?.trim();
}

describe("editor shell styles (7.33)", () => {
  it("marks the active ribbon tab with the accent underline, not a raised tab", () => {
    const active = rule(".ribbon-tab.is-active");
    expect(active).toContain("box-shadow: inset 0 -2px 0 var(--accent)");
    expect(active).not.toContain("border:");
    expect(rule(".ribbon-body")).toContain("background: var(--surface)");
  });

  it("paints a pressed tool in the accent's ink, never white on the light tint", () => {
    const active = rule(".tool-btn.is-active");
    expect(active).toContain("background: var(--accent-weak)");
    expect(active).toContain("color: var(--accent-ink)");
    expect(active).not.toContain("--accent-text");
  });

  it("never paints the white --accent-text on a light tint or the white page", () => {
    // --accent-text is the text ON the accent (white in the light theme). Ink on
    // a tint uses --accent-ink; footnote markers sit on the always-white page.
    for (const selector of [
      ".calc-row-header.is-active",
      ".sheet-tab.is-active",
      ".chip.is-active",
      ".calc-header-menu-item:focus-visible",
      ".recovery-banner",
      ".template-icon",
    ]) {
      expect(rule(selector), selector).toContain("color: var(--accent-ink)");
    }
    expect(rule(".writer-note-ref")).toContain("color: var(--accent)");
    expect(rule(".writer-note sup")).toContain("color: var(--accent)");
  });

  it("gives every theme an accent ink", () => {
    for (const block of [":root", ".dark", ':root[data-theme="midnight"]', ':root[data-theme="paper"]']) {
      expect(token(block, "--accent-ink"), `${block} has no --accent-ink`).toBeTruthy();
    }
  });

  it("keeps tool buttons and outline rows at a comfortable size", () => {
    expect(rule(".tool-btn")).toContain("min-height: 32px");
    expect(rule(".tool-btn.is-icon-only")).toContain("min-width: 32px");
    expect(rule(".writer-nav-item")).toContain("min-height: 30px");
    expect(rule(".editor-status")).toContain("font-variant-numeric: tabular-nums");
  });
});
