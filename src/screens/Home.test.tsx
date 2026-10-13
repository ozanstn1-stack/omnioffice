import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn(async (command: string) => {
  switch (command) {
    case "load_recent":
      return [];
    case "load_settings":
      return {};
    default:
      return null;
  }
});

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...(args as [string])) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));

import { Home } from "./Home";
import { useSettings } from "../lib/store";
import { DEFAULT_SETTINGS } from "../lib/types";

describe("home recommendations and pinning", () => {
  beforeEach(() => {
    invoke.mockClear();
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS }, loaded: true });
  });

  it("pins a tool and persists the choice", async () => {
    const user = userEvent.setup();
    render(<Home onNavigate={vi.fn()} onDropFiles={vi.fn()} dragging={false} onFileList={vi.fn()} />);
    const pins = await screen.findAllByRole("button", { name: /Pin to the top/i });
    await user.click(pins[0]);
    expect(useSettings.getState().settings.pinnedTools?.length).toBe(1);
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "save_settings",
        expect.objectContaining({
          settings: expect.objectContaining({ pinnedTools: expect.any(Array) }),
        }),
      ),
    );
    // The pinned tool now leads the list under its own heading.
    expect(await screen.findByText(/^Pinned$/i)).toBeInTheDocument();
  });
});
