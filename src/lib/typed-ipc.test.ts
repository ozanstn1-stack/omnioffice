import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { unwrap } from "./typed-ipc";

const root = path.resolve(__dirname, "..", "..");
const read = (relative: string) => readFileSync(path.join(root, relative), "utf8").replace(/\r\n/g, "\n");

describe("typed command results", () => {
  it("returns the data of an ok result", async () => {
    await expect(unwrap(Promise.resolve({ status: "ok" as const, data: 7 }))).resolves.toBe(7);
  });

  it("throws the command's error object, as a plain invoke rejects with it", async () => {
    const error = { code: "not_found", message: "missing" };
    await expect(unwrap(Promise.resolve({ status: "error" as const, error }))).rejects.toBe(error);
  });
});

// The migration moves one command group at a time (#316). A migrated group is
// complete: every command of the domain is in the generated bindings, and its
// screens call the typed wrappers instead of a hand-written invoke.
describe("typed bindings: the vault group", () => {
  const ipc = read("src-tauri/src/ipc.rs");
  const vaultBlock = /vault_commands \{\n([\s\S]*?)\n\s*\}\n\}/.exec(ipc)?.[1] ?? "";
  const vaultCommands = [...vaultBlock.matchAll(/vault::([a-z_]+),/g)].map((match) => match[1]);
  const generated = read("src/lib/generated/specta.ts");

  it("generates a typed wrapper for every vault command", () => {
    expect(vaultCommands.length).toBe(9);
    for (const name of vaultCommands) {
      expect(generated, name).toContain(`__TAURI_INVOKE("${name}"`);
    }
  });

  it("calls the typed wrappers from the vault screen", () => {
    expect(read("src/screens/Vault.tsx")).not.toMatch(/invoke(?:<[^>]*>)?\(\s*"vault_/);
  });

  // The palette sits in the entry chunk: it keeps `invoke` but takes the
  // request and response types from the generated file.
  it("types the palette's vault search with the generated types", () => {
    const palette = read("src/components/command-palette.tsx");
    expect(palette).toMatch(/invoke<VaultSearchResponse_Serialize>\("vault_search"/);
    expect(palette).toContain("const request: VaultSearchRequest_Deserialize");
  });
});

describe("typed bindings: the sync group", () => {
  const ipc = read("src-tauri/src/ipc.rs");
  const syncBlock = /sync_commands \{\n([\s\S]*?)\n\s*\}\n\}/.exec(ipc)?.[1] ?? "";
  const syncCommands = [...syncBlock.matchAll(/(?:sync|oauth)::([a-z_]+),/g)].map((match) => match[1]);
  const generated = read("src/lib/generated/specta.ts");

  // A command without a Result is generated as `__TAURI_INVOKE<T>("name")`.
  it("generates a typed wrapper for every sync and OAuth command", () => {
    expect(syncCommands.length).toBe(15);
    for (const name of syncCommands) {
      expect(generated, name).toMatch(new RegExp(`__TAURI_INVOKE(?:<[^>]*>)?\\("${name}"`));
    }
  });

  it("leaves no hand-written sync or OAuth invoke in the app", () => {
    for (const file of ["src/lib/api.ts", "src/screens/Sync.tsx"]) {
      expect(read(file), file).not.toMatch(/invoke(?:<[^>]*>)?\(\s*"(?:sync|oauth)_/);
    }
  });
});

describe("typed bindings: the plugin group", () => {
  const ipc = read("src-tauri/src/ipc.rs");
  const pluginBlock = /plugin_commands \{\n([\s\S]*?)\n\s*\}\n\}/.exec(ipc)?.[1] ?? "";
  const pluginCommands = [...pluginBlock.matchAll(/plugin::([a-z_]+),/g)].map((match) => match[1]);
  const generated = read("src/lib/generated/specta.ts");

  it("generates a typed wrapper for every plugin command", () => {
    expect(pluginCommands.length).toBe(10);
    for (const name of pluginCommands) {
      expect(generated, name).toMatch(new RegExp(`__TAURI_INVOKE(?:<[^>]*>)?\\("${name}"`));
    }
  });

  it("calls the typed wrappers from the plugin transport", () => {
    expect(read("src/lib/plugins.ts")).not.toMatch(/invoke(?:<[^>]*>)?\(\s*"plugin_/);
  });
});

// The AI commands are called through api.ts, which loads the generated
// wrappers on first use; the long jobs stay on `invokeTracked` so the Jobs
// screen can re-run them.
describe("typed bindings: the ai group", () => {
  const ipc = read("src-tauri/src/ipc.rs");
  const aiBlock = /ai_commands \{\n([\s\S]*?)\n\s*\}\n\}/.exec(ipc)?.[1] ?? "";
  const aiCommands = [...aiBlock.matchAll(/[a-z_]+::([a-z_]+),/g)].map((match) => match[1]);
  const generated = read("src/lib/generated/specta.ts");

  it("generates a typed wrapper for every AI command", () => {
    expect(aiCommands.length).toBe(31);
    for (const name of aiCommands) {
      expect(generated, name).toMatch(new RegExp(`__TAURI_INVOKE(?:<[^>]*>)?\\("${name}"`));
    }
  });

  it("leaves no hand-written AI invoke outside the job tracker", () => {
    expect(read("src/lib/api.ts")).not.toMatch(/invoke(?:<[^>]*>)?\(\s*"ai_/);
  });
});
