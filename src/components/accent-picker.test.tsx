import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

import { AccentPicker } from "./accent-picker";
import { paintAccent } from "../lib/accent";
import { useSettings } from "../lib/store";
import { DEFAULT_SETTINGS } from "../lib/types";

afterEach(() => {
  delete document.documentElement.dataset.accent;
  useSettings.setState({ settings: { ...DEFAULT_SETTINGS } });
});

describe("accent colour (7.33.1)", () => {
  it("marks the root with a non-default accent, except in the themes with their own palette", () => {
    const root = document.documentElement;
    paintAccent(root, "teal", "light");
    expect(root.dataset.accent).toBe("teal");
    paintAccent(root, "teal", "paper");
    expect(root.dataset.accent).toBeUndefined();
    paintAccent(root, "pink", "dark");
    expect(root.dataset.accent).toBe("pink");
    paintAccent(root, "indigo", "dark");
    expect(root.dataset.accent).toBeUndefined();
    paintAccent(root, undefined, "light");
    expect(root.dataset.accent).toBeUndefined();
  });

  it("offers four swatches as a radio group and applies the chosen one", async () => {
    const user = userEvent.setup();
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS, theme: "light" } });
    render(<AccentPicker />);
    const group = screen.getByRole("radiogroup", { name: "Accent colour" });
    expect(group.querySelectorAll('[role="radio"]')).toHaveLength(4);
    expect(screen.getByRole("radio", { name: "Indigo" })).toHaveAttribute("aria-checked", "true");

    await user.click(screen.getByRole("radio", { name: "Blue" }));
    expect(useSettings.getState().settings.accent).toBe("blue");
    expect(screen.getByRole("radio", { name: "Blue" })).toHaveAttribute("aria-checked", "true");
    expect(document.documentElement.dataset.accent).toBe("blue");
  });

  it("is disabled, with a note, in the midnight and paper themes", () => {
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS, theme: "midnight" } });
    render(<AccentPicker />);
    expect(screen.getByRole("radio", { name: "Teal" })).toBeDisabled();
    expect(screen.getByText("The midnight and paper themes use their own colours.")).toBeInTheDocument();
  });
});
