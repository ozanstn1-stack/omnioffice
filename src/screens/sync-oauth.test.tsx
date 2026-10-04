import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

/**
 * OAuth panel regression: the cloud options were hardcoded disabled and the
 * backend refused them. The screen must load the OAuth status, let the user
 * save a client id and connect, and show the connected account plus where the
 * tokens are stored.
 */
const invoke = vi.fn(async (command: string, _payload?: unknown) => {
  switch (command) {
    case "sync_get_config":
      return {
        enabled: true,
        provider: "webdav",
        url: "",
        username: "",
        hasPassword: false,
        passwordStorage: "none",
        allowInsecureHttp: false,
        remoteDir: "OmniOffice",
      };
    case "sync_capabilities":
      return { maxTransferBytes: 1024, backgroundSync: false, autoMerge: false, providers: [] };
    case "oauth_status":
      return [
        {
          provider: "onedrive",
          configured: true,
          connected: false,
          account: "",
          clientId: "client-1",
          tenant: "common",
          store: "",
        },
        {
          provider: "google-drive",
          configured: false,
          connected: false,
          account: "",
          clientId: "",
          tenant: "",
          store: "",
        },
      ];
    case "oauth_connect":
      return [
        {
          provider: "onedrive",
          configured: true,
          connected: true,
          account: "Ada",
          clientId: "client-1",
          tenant: "common",
          store: "keychain",
        },
        {
          provider: "google-drive",
          configured: false,
          connected: false,
          account: "",
          clientId: "",
          tenant: "",
          store: "",
        },
      ];
    case "oauth_save_client":
      return [
        {
          provider: "onedrive",
          configured: true,
          connected: false,
          account: "",
          clientId: "client-1",
          tenant: "common",
          store: "",
        },
        {
          provider: "google-drive",
          configured: false,
          connected: false,
          account: "",
          clientId: "",
          tenant: "",
          store: "",
        },
      ];
    case "load_recent":
      return [];
    case "save_settings":
    case "log_operation":
    case "log_frontend":
      return null;
    default:
      return null;
  }
});

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...(args as [string])) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/api/path", () => ({
  appCacheDir: vi.fn(async () => "C:/cache"),
  join: vi.fn(async (...parts: string[]) => parts.join("/")),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(async () => null),
  save: vi.fn(async () => null),
}));

import { Sync } from "./Sync";

describe("cloud sync OAuth providers", () => {
  beforeEach(() => {
    invoke.mockClear();
  });

  it("enables the OAuth option, saves a client and connects", async () => {
    const user = userEvent.setup();
    render(<Sync />);
    await waitFor(() => expect(invoke.mock.calls.some(([command]) => command === "oauth_status")).toBe(true));

    await user.selectOptions(screen.getByRole("combobox"), "onedrive");
    expect(await screen.findByDisplayValue("client-1")).toBeInTheDocument();
    expect(screen.getByText(/not connected/i)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /^connect$/i }));
    await waitFor(() => expect(invoke.mock.calls.some(([command]) => command === "oauth_connect")).toBe(true));
    const call = invoke.mock.calls.find(([command]) => command === "oauth_connect") as unknown as [
      string,
      { provider: string },
    ];
    expect(call[1].provider).toBe("onedrive");
    expect(await screen.findByText(/Connected as Ada/)).toBeInTheDocument();
    await waitFor(() => expect(document.body.textContent ?? "").toContain("OS credential vault"));
  });
});
