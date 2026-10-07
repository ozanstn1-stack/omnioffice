import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const settingsView = {
  configured: false,
  keyStorage: "none",
  maskedKey: "",
  baseUrl: "https://api.deepseek.com",
  model: "deepseek-flash",
  temperature: 0.2,
  maxTokens: 4096,
  thinking: true,
  reasoningEffort: "high",
  contextTokens: 200_000,
  maxOutputTokens: 384_000,
  provider: "deepseek",
};

const invoke = vi.fn(async (command: string, payload?: unknown) => {
  switch (command) {
    case "ai_get_settings":
      return settingsView;
    case "ai_models": {
      const provider = (payload as { provider?: string | null } | undefined)?.provider;
      return provider === "anthropic"
        ? [
            { id: "claude-opus-5-5", label: "Claude Opus 5.5", recommended: true },
            { id: "claude-sonnet-5-5", label: "Claude Sonnet 5.5", recommended: false },
            { id: "claude-haiku-4-5", label: "Claude Haiku 4.5", recommended: false },
          ]
        : [{ id: "deepseek-flash", label: "DeepSeek V4.1 Flash", recommended: true }];
    }
    case "ai_library_default_dir":
      return "C:/docs/AI";
    case "ai_save_settings":
      return { ...settingsView, ...(payload as { input: Record<string, unknown> }).input, configured: true };
    default:
      return null;
  }
});

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...(args as [string])) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));

import { AiSettings } from "./ai-settings";
import { useSettings } from "../lib/store";
import { DEFAULT_SETTINGS } from "../lib/types";

describe("AI provider settings", () => {
  beforeEach(() => {
    invoke.mockClear();
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS }, loaded: true });
  });

  it("lists Anthropic (Claude) and fills its base URL, default model and key note", async () => {
    const user = userEvent.setup();
    render(<AiSettings />);
    const select = (await screen.findByRole("combobox", { name: "Provider" })) as HTMLSelectElement;
    expect(Array.from(select.options).map((option) => option.textContent)).toContain("Anthropic (Claude)");

    await user.selectOptions(select, "anthropic");
    expect(screen.getByDisplayValue("https://api.anthropic.com")).toBeInTheDocument();
    expect(
      screen.getByText(/api\.anthropic\.com\) together with your API key from console\.anthropic\.com/),
    ).toBeInTheDocument();
    expect(screen.getByPlaceholderText("sk-ant-…")).toBeInTheDocument();

    // The static suggestions come from the backend for the selected provider.
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("ai_models", { provider: "anthropic" }));
    expect(await screen.findByRole("option", { name: /claude-opus-5-5/ })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: /claude-sonnet-5-5/ })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: /claude-haiku-4-5/ })).toBeInTheDocument();
    const modelSelect = screen.getAllByRole("combobox")[1] as HTMLSelectElement;
    expect(modelSelect.value).toBe("claude-opus-5-5");

    // Claude takes no sampling parameters: the temperature control is hidden.
    expect(screen.queryByText("Temperature")).toBeNull();
  });

  it("saves the Anthropic provider with its URL and model", async () => {
    const user = userEvent.setup();
    render(<AiSettings />);
    const select = await screen.findByRole("combobox", { name: "Provider" });
    await user.selectOptions(select, "anthropic");
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "ai_save_settings",
        expect.objectContaining({
          input: expect.objectContaining({
            provider: "anthropic",
            baseUrl: "https://api.anthropic.com",
            model: "claude-opus-5-5",
          }),
        }),
      ),
    );
  });
});
