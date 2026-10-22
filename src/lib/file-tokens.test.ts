import { describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import { pickDocumentToken, pickSaveToken, releaseFileToken } from "./file-tokens";

describe("file token wrappers", () => {
  it("asks the Rust dialog for a read token and passes the extensions", async () => {
    invoke.mockResolvedValueOnce({ token: "abc", name: "report.pdf" });
    await expect(pickDocumentToken(["pdf", "docx"])).resolves.toEqual({ token: "abc", name: "report.pdf" });
    expect(invoke).toHaveBeenCalledWith("file_token_pick", { extensions: ["pdf", "docx"] });
  });

  it("asks for a write token with the suggested name", async () => {
    invoke.mockResolvedValueOnce({ token: "def", name: "out.pdf" });
    await expect(pickSaveToken("out.pdf", ["pdf"])).resolves.toEqual({ token: "def", name: "out.pdf" });
    expect(invoke).toHaveBeenCalledWith("file_token_save", { name: "out.pdf", extensions: ["pdf"] });
  });

  it("keeps null for a cancelled dialog and releases tokens", async () => {
    invoke.mockResolvedValueOnce(null);
    await expect(pickDocumentToken()).resolves.toBeNull();
    expect(invoke).toHaveBeenCalledWith("file_token_pick", { extensions: undefined });
    invoke.mockResolvedValueOnce(true);
    await expect(releaseFileToken("abc")).resolves.toBe(true);
    expect(invoke).toHaveBeenCalledWith("file_token_release", { token: "abc" });
  });
});
