import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

// The security posture lives in configuration and crate roots, not in runtime
// code: jsdom cannot run a Tauri webview, so these invariants are pinned here
// (E1).
const root = path.resolve(__dirname, "..", "..");
const config = JSON.parse(readFileSync(path.join(root, "src-tauri", "tauri.conf.json"), "utf8")) as {
  app: { security: { csp: string; freezePrototype?: boolean } };
};

describe("webview security configuration (E1)", () => {
  it("blocks objects, base changes, form posts and framing", () => {
    const csp = config.app.security.csp;
    expect(csp).toContain("object-src 'none'");
    expect(csp).toContain("base-uri 'none'");
    expect(csp).toContain("form-action 'none'");
    expect(csp).toContain("frame-ancestors 'none'");
    // The existing allow-lists stay: local assets, data/blob images, IPC.
    expect(csp).toContain("default-src 'self'");
    expect(csp).toContain("connect-src 'self' ipc: http://ipc.localhost");
  });

  it("freezes the prototypes the webview exposes", () => {
    expect(config.app.security.freezePrototype).toBe(true);
  });
});

describe("library crates forbid unsafe code (E1)", () => {
  it("declares #![forbid(unsafe_code)] in every library crate", () => {
    for (const crate of ["aicore", "compat", "officecore", "pdfcore", "synccore", "turkish"]) {
      const source = readFileSync(path.join(root, "crates", crate, "src", "lib.rs"), "utf8");
      expect(source, crate).toContain("#![forbid(unsafe_code)]");
    }
  });
});
