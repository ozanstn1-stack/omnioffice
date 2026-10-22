import { describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import { pickDocumentToken, pickDocumentTokens, pickSaveToken, releaseFileToken } from "./file-tokens";

describe("file token wrappers", () => {
  it("asks the Rust dialog for read tokens and passes the filters", async () => {
    invoke.mockResolvedValueOnce([
      { token: "abc", name: "report.pdf" },
      { token: "def", name: "second.pdf" },
    ]);
    await expect(pickDocumentTokens(["pdf", "docx"], true)).resolves.toEqual([
      { token: "abc", name: "report.pdf" },
      { token: "def", name: "second.pdf" },
    ]);
    expect(invoke).toHaveBeenCalledWith("file_token_pick", { extensions: ["pdf", "docx"], multiple: true });
  });

  it("unwraps the single-file and save helpers", async () => {
    invoke.mockResolvedValueOnce([]);
    await expect(pickDocumentToken(["pdf"])).resolves.toBeNull();
    expect(invoke).toHaveBeenCalledWith("file_token_pick", { extensions: ["pdf"], multiple: false });

    invoke.mockResolvedValueOnce([{ token: "out", name: "out.pdf" }]);
    await expect(pickSaveToken("out.pdf", ["pdf"])).resolves.toEqual({ token: "out", name: "out.pdf" });
    expect(invoke).toHaveBeenCalledWith("file_token_save", { name: "out.pdf", extensions: ["pdf"] });
  });

  it("releases tokens", async () => {
    invoke.mockResolvedValueOnce(true);
    await expect(releaseFileToken("abc")).resolves.toBe(true);
    expect(invoke).toHaveBeenCalledWith("file_token_release", { token: "abc" });
  });
});
