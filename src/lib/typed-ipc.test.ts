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
