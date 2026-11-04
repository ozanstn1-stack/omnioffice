import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));

import { PasswordField } from "./password-field";
import { DEFAULT_SETTINGS } from "../lib/types";
import { useSettings } from "../lib/store";

// One field, two screens: the Protect/Unlock panels and the converter had their
// own copies until this component replaced them. These tests pin the behaviour
// both copies shared, so a later edit cannot quietly drop it.
function Harness(props: { autoComplete?: "new-password" | "current-password" | "off" }) {
  const [value, setValue] = useState("");
  return <PasswordField value={value} onChange={setValue} placeholder="••••••••" autoComplete={props.autoComplete} />;
}

beforeEach(() => {
  useSettings.setState({ settings: { ...DEFAULT_SETTINGS, language: "en" }, loaded: true });
});

describe("PasswordField", () => {
  it("renders a masked input by default", () => {
    render(<Harness />);
    const input = screen.getByPlaceholderText("••••••••") as HTMLInputElement;
    expect(input.type).toBe("password");
  });

  it("toggles the input type between hidden and visible", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const input = screen.getByPlaceholderText("••••••••") as HTMLInputElement;
    await user.click(screen.getByRole("button", { name: "Show" }));
    expect(input.type).toBe("text");
    await user.click(screen.getByRole("button", { name: "Hide" }));
    expect(input.type).toBe("password");
  });

  it("labels the toggle from the dictionary, following the language", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    act(() => {
      useSettings.setState({ settings: { ...DEFAULT_SETTINGS, language: "tr" }, loaded: true });
    });
    const toggle = await screen.findByRole("button", { name: "Göster" });
    await user.click(toggle);
    expect(screen.getByRole("button", { name: "Gizle" })).toBeInTheDocument();
  });

  it("never submits the form around it", async () => {
    // A toggle inside a form is a button, not a submit control: a stray submit
    // here would start a PDF job with a half-typed password.
    const user = userEvent.setup();
    const onSubmit = vi.fn((event: React.FormEvent) => event.preventDefault());
    render(
      <form onSubmit={onSubmit}>
        <Harness />
      </form>,
    );
    const toggle = screen.getByRole("button", { name: "Show" });
    expect(toggle).toHaveAttribute("type", "button");
    await user.click(toggle);
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("keeps the value on the input and hands edits to onChange", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const input = screen.getByPlaceholderText("••••••••") as HTMLInputElement;
    await user.type(input, "secret123");
    expect(input.value).toBe("secret123");
  });

  it("passes autoComplete through, so the browser knows the field's purpose", () => {
    const { rerender } = render(<Harness autoComplete="new-password" />);
    const input = screen.getByPlaceholderText("••••••••");
    expect(input).toHaveAttribute("autocomplete", "new-password");
    rerender(<Harness />);
    // The converter checks an existing password instead of typing a new one.
    expect(input).toHaveAttribute("autocomplete", "off");
  });

  it("keeps the toggle out of the tab order", () => {
    // The field is the target; a Tab stop on the eye would sit between the
    // password and the Run button.
    render(<Harness />);
    expect(screen.getByRole("button", { name: "Show" })).toHaveAttribute("tabindex", "-1");
  });
});
