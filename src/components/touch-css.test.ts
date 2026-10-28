import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

// jsdom does not apply the stylesheet, so the touch sizes are pinned here:
// a regression would only show on a phone.
const css = readFileSync(path.resolve(__dirname, "..", "styles.css"), "utf8").replace(/\r\n/g, "\n");

/** The body of the `@media (pointer: coarse)` block that starts with the C11 marker. */
function touchBlock(): string {
  const start = css.indexOf("/* C11 touch targets */");
  expect(start, "the C11 touch block is missing").toBeGreaterThan(-1);
  return css.slice(start, css.indexOf("/* end C11 touch targets */", start));
}

function rule(block: string, selector: string): string {
  const match = new RegExp(`${selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\s*\\{([^}]*)\\}`).exec(block);
  return match ? match[1] : "";
}

describe("touch ergonomics (C11)", () => {
  it("gives buttons and icon buttons a 44 px touch target on a touch screen", () => {
    const block = touchBlock();
    expect(block).toContain("@media (pointer: coarse)");
    expect(rule(block, ".btn,\n  .icon-btn")).toContain("min-height: 44px");
    // The icon button on its own (not the combined `.btn, .icon-btn` rule) is square.
    expect(block).toMatch(/\n {2}\.icon-btn \{\s*min-width: 44px;/);
    expect(rule(block, ".btn-sm")).toContain("min-height: 40px");
  });

  it("keeps form fields at 16 px so focusing one does not zoom the page", () => {
    const fields = rule(touchBlock(), ".input,\n  .select");
    expect(fields).toContain("font-size: 16px");
    expect(fields).toContain("min-height: 44px");
  });

  it("lays the bottom bar above the system gesture area", () => {
    const bar = /\n\.bottom-nav \{([^}]*)\}/.exec(css)?.[1] ?? "";
    expect(bar).toContain("env(safe-area-inset-bottom)");
    const item = /\n\.bottom-nav-item \{([^}]*)\}/.exec(css)?.[1] ?? "";
    expect(item).toContain("min-height: 56px");
  });
});
