import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const petitionAnswer = JSON.stringify({
  summary: "Dilekçe genel olarak uygun.",
  issues: [{ severity: "low", title: "Noktalama", suggestion: "Cümle sonlarına nokta koyun." }],
  corrected: "T.C. ... Sayın İlgili Makam ...",
});

let settingsProvider = "ollama";
let answer = petitionAnswer;
let settingsReads = 0;

const invoke = vi.fn(async (command: string, _payload?: unknown) => {
  if (command === "ai_get_settings") {
    settingsReads += 1;
    const provider = settingsReads === 1 ? settingsProvider : "ollama";
    return {
      configured: true,
      provider,
      providerLabel: provider === "ollama" ? "Ollama (local)" : "DeepSeek",
      baseUrl: provider === "ollama" ? "http://localhost:11434" : "https://api.deepseek.com",
      model: provider === "ollama" ? "llama3.2" : "deepseek-flash",
      maskedKey: "",
      keyStorage: "dpapi",
      temperature: 0.2,
      maxTokens: 2048,
      thinking: false,
      reasoningEffort: "auto",
      contextTokens: 16000,
      maxOutputTokens: 8192,
      capabilities: { chat: true, embeddings: false, vision: false, structured_output: true, streaming: true },
      embeddingModel: null,
      providerKeys: {},
    };
  }
  if (command === "ai_save_settings") return null;
  if (command === "file_token_pick") return [{ token: "tok-doc", name: "dilekce.pdf" }];
  if (command === "ai_chat") {
    return { answer, citations: [], retrievalMode: "text", model: "llama3.2", elapsedMs: 12 };
  }
  return null;
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...(args as [string, unknown])) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null), save: vi.fn(async () => null) }));

import { AiPacks } from "./AiPacks";

describe("AiPacks (C7)", () => {
  const fetchSpy = vi.fn();

  beforeEach(() => {
    invoke.mockClear();
    fetchSpy.mockClear();
    vi.stubGlobal("fetch", fetchSpy);
    settingsProvider = "ollama";
    answer = petitionAnswer;
    settingsReads = 0;
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("shows the local badge for the Ollama provider", async () => {
    render(<AiPacks />);
    expect(await screen.findByText("Local")).toBeInTheDocument();
    expect(screen.getByText("Ollama (local)")).toBeInTheDocument();
  });

  it("runs a pack through ai_chat and verifies the structured answer", async () => {
    const user = userEvent.setup();
    render(<AiPacks />);
    await screen.findByText("Local");

    await user.click(screen.getByRole("button", { name: "Choose" }));
    expect(await screen.findByText("dilekce.pdf")).toBeInTheDocument();
    await user.click(screen.getByRole("checkbox"));
    await user.click(screen.getByRole("button", { name: "Run task" }));

    expect(await screen.findByText("Answer verified")).toBeInTheDocument();
    expect(screen.getByText("All checks passed")).toBeInTheDocument();
    const call = invoke.mock.calls.find(([command]) => command === "ai_chat");
    expect(call).toBeTruthy();
    const request = (call?.[1] as { request: Record<string, unknown> }).request;
    expect(request.question).toContain("Yanıtı SADECE");
    expect(request.question).toContain('"issues"');
    expect(request.consent).toBe(true);
    // The screen itself never talks to the network; the provider call is the command.
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it("reports a non-JSON answer instead of trusting it", async () => {
    answer = "Dilekçe güzel görünüyor.";
    const user = userEvent.setup();
    render(<AiPacks />);
    await screen.findByText("Local");
    await user.click(screen.getByRole("button", { name: "Choose" }));
    await screen.findByText("dilekce.pdf");
    await user.click(screen.getByRole("checkbox"));
    await user.click(screen.getByRole("button", { name: "Run task" }));

    expect(await screen.findByText("Answer needs attention")).toBeInTheDocument();
    expect(screen.getByText(/The answer is not a JSON object/)).toBeInTheDocument();
  });

  it("offers switching a cloud provider to the local Ollama preset", async () => {
    settingsProvider = "deepseek";
    settingsReads = 0;
    const user = userEvent.setup();
    render(<AiPacks />);

    expect(await screen.findByText("Cloud provider")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Switch to Ollama (local)" }));

    await waitFor(() => expect(screen.getByText("Local")).toBeInTheDocument());
    const saved = invoke.mock.calls.find(([command]) => command === "ai_save_settings")?.[1] as {
      input?: Record<string, unknown>;
    };
    expect(saved?.input).toMatchObject({ provider: "ollama", baseUrl: "http://localhost:11434" });
  });
});
