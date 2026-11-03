import { describe, expect, it } from "vitest";

import { devVaultFolders } from "./vault-dev";

describe("vault development folders", () => {
  it("takes the folder of a POSIX path", () => {
    expect(devVaultFolders(["/w/vault-e2e/field-notes.md"])).toEqual(["/w/vault-e2e"]);
  });

  it("keeps the Windows separator", () => {
    expect(devVaultFolders(["C:\\w\\vault-e2e\\field-notes.md"])).toEqual(["C:\\w\\vault-e2e"]);
  });

  it("deduplicates one folder, case-insensitively", () => {
    expect(devVaultFolders(["/w/Notes/a.md", "/w/notes/b.txt"])).toEqual(["/w/Notes"]);
  });

  it("skips a bare file name and an empty input", () => {
    expect(devVaultFolders(["a.md", ""])).toEqual([]);
  });
});
