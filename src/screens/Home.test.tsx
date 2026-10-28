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

describe("profession desks on Home", () => {
  beforeEach(() => {
    invoke.mockClear();
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS }, loaded: true });
  });

  it("asks what you do on first run and turns Home into that desk", async () => {
    const user = userEvent.setup();
    const navigate = vi.fn();
    render(<Home onNavigate={navigate} onDropFiles={vi.fn()} dragging={false} onFileList={vi.fn()} />);
    expect(await screen.findByRole("heading", { name: /What do you do\?/ })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /^Lawyer$/ }));
    expect(useSettings.getState().settings.profession).toBe("lawyer");
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "save_settings",
        expect.objectContaining({ settings: expect.objectContaining({ profession: "lawyer" }) }),
      ),
    );

    expect(screen.queryByRole("heading", { name: /What do you do\?/ })).not.toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /Lawyer's desk/ })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Write a petition/ }));
    expect(navigate).toHaveBeenCalledWith("petition");

    // The full catalogue moves behind "All tools" while a desk leads Home.
    expect(screen.queryByRole("button", { name: /^Split/ })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /All tools/ }));
    expect(screen.getByRole("button", { name: /^Split/ })).toBeInTheDocument();
  });

  it("still finds every tool through search while a desk is shown", async () => {
    const user = userEvent.setup();
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS, profession: "accountant" }, loaded: true });
    render(<Home onNavigate={vi.fn()} onDropFiles={vi.fn()} dragging={false} onFileList={vi.fn()} />);
    expect(screen.getByRole("heading", { name: /Accountant's desk/ })).toBeInTheDocument();
    await user.type(screen.getByRole("textbox", { name: /Search tools/ }), "split");
    expect(screen.getByRole("button", { name: /^Split/ })).toBeInTheDocument();
  });

  it("keeps the plain catalogue for the general choice", async () => {
    const user = userEvent.setup();
    render(<Home onNavigate={vi.fn()} onDropFiles={vi.fn()} dragging={false} onFileList={vi.fn()} />);
    await user.click(await screen.findByRole("button", { name: /Show all tools/ }));
    expect(useSettings.getState().settings.profession).toBe("general");
    expect(screen.queryByRole("heading", { name: /desk/ })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^Split/ })).toBeInTheDocument();
  });

  it("lets you change the desk from the desk itself", async () => {
    const user = userEvent.setup();
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS, profession: "student" }, loaded: true });
    render(<Home onNavigate={vi.fn()} onDropFiles={vi.fn()} dragging={false} onFileList={vi.fn()} />);
    await user.click(screen.getByRole("button", { name: /Change desk/ }));
    expect(screen.getByRole("heading", { name: /What do you do\?/ })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Public sector/ }));
    expect(screen.getByRole("heading", { name: /Public sector desk/ })).toBeInTheDocument();
  });
});
