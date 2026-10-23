import { describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import {
  pickDirectoryToken,
  pickDocumentToken,
  pickDocumentTokens,
  pickSaveToken,
  releaseFileToken,
  suggestedOutputPath,
} from "./file-tokens";

describe("file token wrappers", () => {
  it("asks the Rust dialog for a read token and passes the extensions", async () => {
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

  it("asks for a folder token for multi-output tools", async () => {
    invoke.mockResolvedValueOnce({ token: "dir-1", name: "Outputs", path: "C:/Outputs" });
    await expect(pickDirectoryToken()).resolves.toEqual({ token: "dir-1", name: "Outputs", path: "C:/Outputs" });
    expect(invoke).toHaveBeenCalledWith("file_token_pick_directory");
  });

  it("releases tokens", async () => {
    invoke.mockResolvedValueOnce(true);
    await expect(releaseFileToken("abc")).resolves.toBe(true);
    expect(invoke).toHaveBeenCalledWith("file_token_release", { token: "abc" });
  });

  it("asks for a suggested save target and falls back to a display path", async () => {
    invoke.mockResolvedValueOnce({ token: "write-1", path: "C:/out.pdf" });
    await expect(suggestedOutputPath("C:/in.pdf", "_out", "C:/fallback.pdf")).resolves.toBe("C:/out.pdf");
    expect(invoke).toHaveBeenCalledWith("suggest_output", {
      input: "C:/in.pdf",
      suffix: "_out",
      directory: undefined,
    });
    invoke.mockRejectedValueOnce(new Error("engine"));
    await expect(suggestedOutputPath("C:/in.pdf", "_out", "C:/fallback.pdf")).resolves.toBe("C:/fallback.pdf");
  });
});
