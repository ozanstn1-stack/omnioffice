import { readFileSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Modal } from "./ui";

// The 6.4.9 device check found screens with no padding, no gaps and no
// scrolling on a phone: their classes had no CSS. jsdom does not apply the
// stylesheet, so the rules are pinned here.
const src = path.resolve(__dirname, "..");
const css = readFileSync(path.join(src, "styles.css"), "utf8").replace(/\r\n/g, "\n");

/** The body of the top-level rule for exactly `selector`. */
function rule(selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return new RegExp(`\\n${escaped} \\{([^}]*)\\}`).exec(css)?.[1] ?? "";
}

function sourceFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((entry) => {
    const full = path.join(dir, entry);
    if (statSync(full).isDirectory()) return entry === "generated" ? [] : sourceFiles(full);
    return /\.tsx$/.test(entry) && !/\.test\.tsx$/.test(entry) ? [full] : [];
  });
}

describe("layout classes the screens rely on", () => {
  it("leaves no screen in the unstyled screen / screen-head wrapper", () => {
    const offenders = sourceFiles(src).filter((file) => /className="screen(-head)?"/.test(readFileSync(file, "utf8")));
    expect(offenders.map((file) => path.relative(src, file))).toEqual([]);
  });

  it("styles the shared row, stack, field and empty-state classes", () => {
    expect(rule(".row")).toContain("display: flex");
    expect(rule(".row")).toContain("flex-wrap: wrap");
    expect(rule(".row")).toContain("gap:");
    expect(rule(".stack")).toContain("flex-direction: column");
    expect(rule(".field")).toContain("flex-direction: column");
    expect(rule(".empty-state")).toContain("padding:");
    expect(rule(".empty-state")).toContain("align-items: center");
  });

  it("keeps the drawer clear of the system navigation bar", () => {
    expect(rule(".drawer")).toContain("padding-bottom: env(safe-area-inset-bottom)");
  });

  it("ends a long select value with an ellipsis before the arrow", () => {
    const select = rule(".select");
    expect(select).toContain("text-overflow: ellipsis");
    expect(select).toContain("padding-right:");
  });

  it("leaves room for a field's icon outside the utility layer", () => {
    // Tailwind's pl-* utilities live in a cascade layer, so the unlayered
    // .input-sm padding wins over them; the icon classes are unlayered too.
    expect(rule(".input-lead-icon")).toContain("padding-left: 32px");
    expect(rule(".input-trail-icon")).toContain("padding-right: 32px");
    const offenders = sourceFiles(src).filter((file) =>
      /className="[^"]*input-sm[^"]*\bp[lr]-\d/.test(readFileSync(file, "utf8")),
    );
    expect(offenders.map((file) => path.relative(src, file))).toEqual([]);
  });

  it("wraps a dialog's buttons instead of pushing them off a narrow screen", () => {
    render(
      <Modal title="Gönderim" onClose={() => undefined} footer={<button type="button">Gönder</button>}>
        <p>İçerik</p>
      </Modal>,
    );
    expect(screen.getByRole("button", { name: "Gönder" }).parentElement).toHaveClass("flex-wrap");
  });
});
