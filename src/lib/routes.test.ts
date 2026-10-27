import { afterEach, describe, expect, it } from "vitest";

import { NAV_SECTIONS, ROUTES, SCREEN_IDS, enabledRoutes, navSections, routeEnabled, routeFor } from "./routes";

afterEach(() => {
  localStorage.clear();
});

describe("route manifest", () => {
  it("covers every screen exactly once", () => {
    const ids = ROUTES.map((entry) => entry.id);
    expect(new Set(ids).size).toBe(ids.length);
    expect(ids.slice().sort()).toEqual([...SCREEN_IDS].sort());
  });

  it("keeps every entry addressable and labelled", () => {
    for (const id of SCREEN_IDS) {
      const entry = routeFor(id);
      expect(entry.id).toBe(id);
      expect(entry.labelKey).toBe(`nav.${id}`);
    }
  });

  it("only references sections that exist", () => {
    const sections = new Set(NAV_SECTIONS.map((section) => section.id));
    for (const entry of ROUTES) {
      if (entry.section) expect(sections.has(entry.section)).toBe(true);
    }
  });

  it("gates flagged routes out of the nav and the palette", () => {
    for (const id of ["kvkk", "documentHealth"] as const) {
      expect(routeEnabled(id)).toBe(false);
      expect(enabledRoutes().map((entry) => entry.id)).not.toContain(id);
      expect(navSections().flatMap((section) => section.routes.map((entry) => entry.id))).not.toContain(id);
    }
    localStorage.setItem("flag:experimentalKvkkWizard", "1");
    expect(routeEnabled("kvkk")).toBe(true);
    expect(enabledRoutes().map((entry) => entry.id)).toContain("kvkk");
    expect(navSections().flatMap((section) => section.routes.map((entry) => entry.id))).toContain("kvkk");
  });

  it("lists the document scanner, which the Android Scan shortcut opens", () => {
    expect(routeEnabled("scanner")).toBe(true);
    expect(navSections().flatMap((section) => section.routes.map((entry) => entry.id))).toContain("scanner");
  });

  it("keeps the settings route in the palette with its keybinding", () => {
    const settings = routeFor("settings");
    expect(settings.category).toBe("settings");
    expect(settings.keybinding).toBe("Ctrl+,");
  });
});
