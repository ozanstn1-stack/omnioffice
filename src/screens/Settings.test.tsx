import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn(async (command: string, _payload?: unknown) => {
  switch (command) {
    case "app_info":
      return { appVersion: "3.9.0", coreVersion: "3.9.0", platform: "test" };
    case "load_settings":
      return {};
    default:
      return null;
  }
});

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...(args as [string])) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("../components/ai-settings", () => ({ AiSettings: () => null }));

import { Settings } from "./Settings";
import { useSettings } from "../lib/store";
import { DEFAULT_SETTINGS } from "../lib/types";

describe("online revocation setting", () => {
  beforeEach(() => {
    invoke.mockClear();
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS }, loaded: true });
  });

  it("is off by default and says it contacts the CA's servers", async () => {
    expect(DEFAULT_SETTINGS.onlineRevocationCheck).toBe(false);
    render(<Settings />);
    await screen.findByText(/v3\.9\.0/);
    const box = screen.getByRole("checkbox", { name: /Check certificate revocation online/ });
    expect(box).not.toBeChecked();
    expect(screen.getByText(/Contacts the certificate authority's OCSP and CRL servers/)).toBeInTheDocument();
  });

  it("saves the choice when it is switched on", async () => {
    const user = userEvent.setup();
    render(<Settings />);
    await screen.findByText(/v3\.9\.0/);
    await user.click(screen.getByRole("checkbox", { name: /Check certificate revocation online/ }));
    expect(useSettings.getState().settings.onlineRevocationCheck).toBe(true);
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "save_settings",
        expect.objectContaining({ settings: expect.objectContaining({ onlineRevocationCheck: true }) }),
      ),
    );
  });
});
