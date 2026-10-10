import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const settingsView = {
  configured: true,
  keyStorage: "dpapi",
  maskedKey: "sk-d••••1234",
  providerKeys: {
    deepseek: { configured: true, maskedKey: "sk-d••••1234", keyStorage: "dpapi" },
    anthropic: { configured: false, maskedKey: "", keyStorage: "none" },
    ollama: { configured: true, maskedKey: "", keyStorage: "none" },
  } as Record<string, { configured: boolean; maskedKey: string; keyStorage: string }>,
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
    case "ai_discover_models":
      if (discoverFailure) throw discoverFailure;
      return discoverResult;
    case "ai_library_default_dir":
      return "C:/docs/AI";
    case "ai_save_settings":
      return { ...settingsView, ...(payload as { input: Record<string, unknown> }).input, configured: true };
    case "ai_clear_key":
      return settingsView;
    default:
      return null;
  }
});

let discoverResult: unknown = {
  models: [
    { id: "deepseek-v4-pro", label: "DeepSeek V4.1 Pro", recommended: false },
    { id: "deepseek-v4-flash", label: "DeepSeek V4.1 Flash", recommended: false },
  ],
  discovered: true,
  message: "",
};
let discoverFailure: unknown = null;

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...(args as [string])) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));

import { AiSettings } from "./ai-settings";
import { useSettings } from "../lib/store";
import { DEFAULT_SETTINGS } from "../lib/types";

describe("AI provider settings", () => {
  beforeEach(() => {
    invoke.mockClear();
    discoverFailure = null;
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
  it("shows each provider's own key state and never carries a typed key to another provider", async () => {
    const user = userEvent.setup();
    render(<AiSettings />);
    const select = (await screen.findByRole("combobox", { name: "Provider" })) as HTMLSelectElement;
    // The saved DeepSeek key is shown for DeepSeek.
    expect(await screen.findByPlaceholderText("sk-d••••1234")).toBeInTheDocument();
    await user.type(screen.getByPlaceholderText("sk-d••••1234"), "typed-deepseek-key");

    // Switching to Anthropic: no key there, Test is disabled, nothing typed carries over.
    await user.selectOptions(select, "anthropic");
    const field = screen.getByPlaceholderText("sk-ant-…") as HTMLInputElement;
    expect(field.value).toBe("");
    expect(screen.getByRole("button", { name: /Test connection/ })).toBeDisabled();
    expect(screen.queryByRole("button", { name: "Remove key" })).toBeNull();

    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("ai_save_settings", expect.anything()));
    const saved = invoke.mock.calls.find(([command]) => command === "ai_save_settings")![1] as {
      input: { apiKey?: string; provider: string };
    };
    expect(saved.input.provider).toBe("anthropic");
    expect(saved.input.apiKey).toBeUndefined();

    // Back to DeepSeek: its stored key is shown again, and removing a key
    // names the provider it belongs to.
    await user.selectOptions(select, "deepseek");
    expect(screen.getByPlaceholderText("sk-d••••1234")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Remove key" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("ai_clear_key", { provider: "deepseek" }));
  });

  it("discovers models from the provider and fills the dropdown", async () => {
    const user = userEvent.setup();
    render(<AiSettings />);
    const modelSelect = (await screen.findAllByRole("combobox"))[1] as HTMLSelectElement;
    // The static fallback list is there before discovery.
    await waitFor(() => expect(modelSelect.value).toBe("deepseek-flash"));

    await user.click(screen.getByRole("button", { name: "Discover models" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("ai_discover_models"));
    // The live list replaces the static options and the first entry is selected
    // because the previously saved model is not in the discovered list.
    expect(await screen.findByRole("option", { name: /deepseek-v4-pro/ })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: /deepseek-v4-flash — DeepSeek V4\.1 Flash/ })).toBeInTheDocument();
    await waitFor(() => expect(modelSelect.value).toBe("deepseek-v4-pro"));
  });

  it("shows the discovery failure message", async () => {
    discoverFailure = { code: "internal", message: "connection refused" };
    const user = userEvent.setup();
    render(<AiSettings />);
    await user.click(await screen.findByRole("button", { name: "Discover models" }));
    expect(await screen.findByText("The model list could not be read: connection refused")).toBeInTheDocument();
  });

  it("reports a provider that lists no models", async () => {
    discoverResult = {
      models: [],
      discovered: false,
      message: "The provider did not list any models; using the built-in suggestions.",
    };
    const user = userEvent.setup();
    render(<AiSettings />);
    await user.click(await screen.findByRole("button", { name: "Discover models" }));
    expect(
      await screen.findByText(
        "The model list could not be read: The provider did not list any models; using the built-in suggestions.",
      ),
    ).toBeInTheDocument();
  });
});
