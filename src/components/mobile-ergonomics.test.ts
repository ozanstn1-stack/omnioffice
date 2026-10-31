import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

// jsdom does not apply the stylesheet, so the phone ergonomics rules (D8) are
// pinned here: a regression would only show on a device.
const root = path.resolve(__dirname, "..", "..");
const css = readFileSync(path.join(root, "src", "styles.css"), "utf8").replace(/\r\n/g, "\n");
const html = readFileSync(path.join(root, "index.html"), "utf8");
const d8 = css.slice(css.indexOf("mobile ergonomics (D8)"), css.indexOf("/* end D8 mobile ergonomics */"));

function rule(selector: string, block = d8): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return new RegExp(`\\n\\s*${escaped} \\{([^}]*)\\}`).exec(block)?.[1] ?? "";
}

describe("mobile ergonomics (D8)", () => {
  it("lets the layout reach under the notch and the system bars", () => {
    expect(html).toContain("viewport-fit=cover");
  });

  it("keeps the bottom action bar above the safe area and the keyboard", () => {
    const bar = rule(".screen-action-bar");
    expect(bar).toContain("env(safe-area-inset-bottom)");
    expect(bar).toContain("padding-left: calc(12px + env(safe-area-inset-left))");
    expect(bar).toContain("margin-bottom: var(--keyboard-inset, 0px)");
    // Buttons get a finger-sized target and share the row.
    const button = rule(".screen-action-bar .btn");
    expect(button).toContain("min-height: 44px");
    expect(button).toContain("flex: 1 1 auto");
  });

  it("scrolls a focused field clear of the keyboard", () => {
    const fields = rule('.input,\n.select,\n.textarea,\ntextarea,\ninput,\nselect,\n[contenteditable="true"]');
    expect(fields).toContain("scroll-margin-block: 128px");
  });

  it("keeps the chrome from eating a landscape phone screen", () => {
    expect(d8).toContain("@media (orientation: landscape) and (max-height: 500px)");
    // Inside the landscape block the chrome is compact: icon-only tabs, one
    // scrolling action row and dialogs capped to the viewport.
    const landscape = d8.slice(d8.indexOf("@media (orientation: landscape)"));
    expect(rule(".screen-action-bar", landscape)).toContain("flex-wrap: nowrap");
    expect(rule(".bottom-nav-label", landscape)).toContain("display: none");
    expect(rule(".modal-backdrop .modal", landscape)).toContain("max-height: calc(100vh - 16px)");
  });
});
