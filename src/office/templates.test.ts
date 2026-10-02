import { describe, expect, it } from "vitest";
import { TEMPLATES, templatesFor } from "./templates";

/**
 * The built-in templates seed real documents, so their contents are part of
 * the product contract: ids stay unique (React keys, tab identity), names are
 * present, and each build() returns the model kind its card promises.
 */
describe("built-in office templates", () => {
  it("has unique ids and complete metadata", () => {
    const ids = TEMPLATES.map((template) => template.id);
    expect(new Set(ids).size).toBe(ids.length);
    for (const template of TEMPLATES) {
      expect(template.name.trim().length).toBeGreaterThan(0);
      expect(template.description.trim().length).toBeGreaterThan(0);
      expect(["writer", "calc", "impress"]).toContain(template.kind);
    }
  });

  it("builds a model that matches the template kind", () => {
    for (const template of TEMPLATES) {
      const model = template.build();
      if (template.kind === "writer") {
        expect("blocks" in model).toBe(true);
      } else if (template.kind === "calc") {
        expect("sheets" in model).toBe(true);
      } else {
        expect("slides" in model).toBe(true);
      }
    }
  });

  it("filters by kind without dropping templates", () => {
    const writer = templatesFor("writer");
    const calc = templatesFor("calc");
    const impress = templatesFor("impress");
    expect(writer.length + calc.length + impress.length).toBe(TEMPLATES.length);
    expect(writer.every((template) => template.kind === "writer")).toBe(true);
    expect(calc.every((template) => template.kind === "calc")).toBe(true);
    expect(impress.every((template) => template.kind === "impress")).toBe(true);
  });
});
