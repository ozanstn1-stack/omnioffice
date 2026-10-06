import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

let storedSettings: Record<string, unknown> = {};
const invoke = vi.fn(async (command: string, _payload?: unknown) =>
  command === "load_settings" ? storedSettings : command === "ocr_languages" ? [] : null,
);

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...(args as [string])) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));

import { WelcomeCard } from "./welcome-card";
import { useSettings } from "../lib/store";
import { DEFAULT_SETTINGS } from "../lib/types";

describe("first-run welcome card", () => {
  beforeEach(() => {
    invoke.mockClear();
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS, onboardingDone: false }, loaded: true });
  });

  it("links to the office suite, the reader and the AI setup", async () => {
    const user = userEvent.setup();
    const onNavigate = vi.fn();
    render(<WelcomeCard onNavigate={onNavigate} />);
    expect(screen.getByRole("heading", { name: "Welcome to OmniOffice" })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Open the office suite" }));
    await user.click(screen.getByRole("button", { name: "Open a PDF" }));
    await user.click(screen.getByRole("button", { name: "Set up AI" }));
    expect(onNavigate.mock.calls.map(([target]) => target)).toEqual(["office", "reader", "ai"]);
  });

  it("stays closed once dismissed", async () => {
    const user = userEvent.setup();
    render(<WelcomeCard onNavigate={vi.fn()} />);
    await user.click(screen.getByRole("button", { name: "Got it" }));
    expect(screen.queryByRole("heading", { name: "Welcome to OmniOffice" })).toBeNull();
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "save_settings",
        expect.objectContaining({ settings: expect.objectContaining({ onboardingDone: true }) }),
      ),
    );
  });

  it("is shown on a first run but not after an upgrade", async () => {
    storedSettings = {};
    await useSettings.getState().init();
    expect(useSettings.getState().settings.onboardingDone).toBe(false);

    storedSettings = { theme: "light" };
    await useSettings.getState().init();
    expect(useSettings.getState().settings.onboardingDone).toBe(true);

    storedSettings = { theme: "light", onboardingDone: false };
    await useSettings.getState().init();
    expect(useSettings.getState().settings.onboardingDone).toBe(false);
  });
});
