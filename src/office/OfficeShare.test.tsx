import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { invokeMock, shareMock } = vi.hoisted(() => ({ invokeMock: vi.fn(), shareMock: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null), save: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-fs", () => ({ readFile: vi.fn(async () => new Uint8Array()) }));
vi.mock("./WriterEditor", () => ({ WriterEditor: () => <div>writer editor</div> }));
vi.mock("./CalcEditor", () => ({ CalcEditor: () => <div>calc editor</div> }));
vi.mock("./ImpressEditor", () => ({ ImpressEditor: () => <div>impress editor</div> }));
vi.mock("../lib/mobile", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../lib/mobile")>()),
  isAndroid: () => true,
}));
vi.mock("../lib/share", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../lib/share")>()),
  shareOfficeDocument: shareMock,
}));

import { OfficeWorkspace } from "./OfficeWorkspace";
import { useOfficeTabs } from "../lib/office-store";
import { useSettings } from "../lib/store";
import { DEFAULT_SETTINGS } from "../lib/types";

describe("Office workspace share (Android)", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockImplementation(async (command: string) => (command === "recovery_list" ? [] : undefined));
    shareMock.mockReset();
    shareMock.mockResolvedValue("/data/cache/share/x/Rapor.xlsx");
    useOfficeTabs.setState({ tabs: [], activeId: null });
    useSettings.setState({ settings: DEFAULT_SETTINGS });
  });

  it("shares the active tab as its own format or as PDF", async () => {
    const user = userEvent.setup();
    const id = useOfficeTabs.getState().create("calc", "Rapor");
    render(<OfficeWorkspace />);

    await user.click(screen.getByRole("button", { name: "Share" }));
    await user.click(await screen.findByRole("button", { name: "Share as XLSX" }));
    expect(shareMock).toHaveBeenCalledTimes(1);
    expect(shareMock.mock.calls[0][0]).toMatchObject({ id, kind: "calc", title: "Rapor" });
    expect(shareMock.mock.calls[0][1]).toBe("document");
    // A successful share closes the dialog.
    expect(screen.queryByRole("button", { name: "Share as XLSX" })).toBeNull();

    await user.click(screen.getByRole("button", { name: "Share" }));
    await user.click(await screen.findByRole("button", { name: "Share as PDF" }));
    expect(shareMock.mock.calls[1][1]).toBe("pdf");
  });
});
