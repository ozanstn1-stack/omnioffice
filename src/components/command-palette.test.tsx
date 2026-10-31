import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CommandPalette } from "./command-palette";
import { registerCommand, unregisterCommand } from "../lib/commands";

const tracked: string[] = [];
afterEach(() => {
  for (const id of tracked.splice(0)) unregisterCommand(id);
});

function define(id: string, titleKey: string, extra: Record<string, unknown> = {}) {
  registerCommand({
    id,
    titleKey,
    category: "task",
    keybinding: "Ctrl+Alt+R",
    enabled: () => true,
    execute: () => undefined,
    ...extra,
  } as never);
  tracked.push(id);
}

describe("command palette desk tasks (D13)", () => {
  it("shows a disabled task with the reason and does not run it", async () => {
    const execute = vi.fn();
    define("task.redact", "desk.redact", {
      enabled: () => false,
      disabledReasonKey: "palette.taskNeedsFile",
      execute,
    });
    const user = userEvent.setup();
    render(<CommandPalette open onClose={vi.fn()} />);
    await user.type(screen.getByPlaceholderText(/command/i), "redact");

    const row = await screen.findByRole("button", { name: /redact the selected pdf/i });
    expect(row).toHaveAttribute("aria-disabled", "true");
    expect(row).toHaveTextContent("Select a document first");
    await user.click(row);
    expect(execute).not.toHaveBeenCalled();
  });

  it("runs an enabled task from the palette", async () => {
    const execute = vi.fn();
    define("task.petition", "desk.petition", { execute });
    const onClose = vi.fn();
    const user = userEvent.setup();
    render(<CommandPalette open onClose={onClose} />);
    await user.type(screen.getByPlaceholderText(/command/i), "petition");
    await user.click(await screen.findByRole("button", { name: /review a petition/i }));
    expect(execute).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(onClose).toHaveBeenCalled());
  });
});
