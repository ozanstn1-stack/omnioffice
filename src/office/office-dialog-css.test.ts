import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

// jsdom does not apply the stylesheet, so a dialog whose classes have no rules
// passes every component test while rendering inside the page flow on a real
// screen (the office Dialog did, until v6.2.11). Pin that its classes exist.
const css = readFileSync(path.resolve(__dirname, "..", "styles.css"), "utf8").replace(/\r\n/g, "\n");
const source = readFileSync(path.resolve(__dirname, "office-ui.tsx"), "utf8");

function rule(selector: string): string | null {
  const match = new RegExp(`(^|\\n)${selector.replace(".", "\\.")} \\{([^}]*)\\}`).exec(css);
  return match ? match[2] : null;
}

describe("office Dialog styles", () => {
  it("styles every class the Dialog renders", () => {
    for (const name of ["modal-backdrop", "modal-head", "modal-body", "modal-wide"]) {
      expect(source).toContain(name);
      expect(rule(`.${name}`), `.${name} has no rule in styles.css`).not.toBeNull();
    }
  });

  it("lays the backdrop over the whole window", () => {
    const backdrop = rule(".modal-backdrop") ?? "";
    expect(backdrop).toContain("position: fixed");
    expect(backdrop).toContain("inset: 0");
  });
});
