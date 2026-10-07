import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { invokeMock, crash } = vi.hoisted(() => ({ invokeMock: vi.fn(), crash: { writer: true } }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null), save: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-fs", () => ({ readFile: vi.fn(async () => new Uint8Array()) }));

// The real editors are exercised by their own suites; here a Writer that can be
// told to crash while the Calc stub always works is all the workspace needs.
vi.mock("./WriterEditor", () => ({
  WriterEditor: ({ tab }: { tab: { title: string } }) => {
    if (crash.writer) throw new Error("writer render exploded");
    return <div>writer editor for {tab.title}</div>;
  },
}));
vi.mock("./CalcEditor", () => ({
  CalcEditor: ({ tab }: { tab: { title: string } }) => <div>calc editor for {tab.title}</div>,
}));
vi.mock("./ImpressEditor", () => ({ ImpressEditor: () => <div>impress editor</div> }));

import { OfficeWorkspace } from "./OfficeWorkspace";
import { useOfficeTabs } from "../lib/office-store";
import { useSettings } from "../lib/store";
import { DEFAULT_SETTINGS } from "../lib/types";

function calls(command: string) {
  return invokeMock.mock.calls.filter(([name]) => name === command).map(([, args]) => args as Record<string, unknown>);
}

describe("Office workspace crash isolation", () => {
  beforeEach(() => {
    crash.writer = true;
    invokeMock.mockReset();
    invokeMock.mockImplementation(async (command: string) => (command === "recovery_list" ? [] : undefined));
    useOfficeTabs.setState({ tabs: [], activeId: null });
    useSettings.setState({ settings: DEFAULT_SETTINGS });
    vi.spyOn(console, "error").mockImplementation(() => undefined);
  });

  it("keeps the tab bar and the other tabs alive when one editor crashes", async () => {
    const user = userEvent.setup();
    const calc = useOfficeTabs.getState().create("calc", "Budget");
    useOfficeTabs.getState().create("writer", "Draft");
    render(<OfficeWorkspace />);

    const host = document.querySelector(".office-editor-host") as HTMLElement;
    expect(within(host).getByRole("alert")).toHaveTextContent("Something went wrong");
    expect(within(host).getByRole("alert")).toHaveTextContent("writer render exploded");
    // Both tabs are still in the bar and the working one opens normally.
    expect(screen.getAllByRole("tab")).toHaveLength(2);
    await user.click(screen.getByRole("tab", { name: /Budget/ }));
    expect(useOfficeTabs.getState().activeId).toBe(calc);
    expect(within(host).getByText("calc editor for Budget")).toBeInTheDocument();
    expect(within(host).queryByRole("alert")).toBeNull();
  });

  it("logs the crash and writes a recovery copy of a dirty document", async () => {
    const id = useOfficeTabs.getState().create("writer", "Draft");
    useOfficeTabs.getState().edit(id, (model) => model);
    render(<OfficeWorkspace />);

    await waitFor(() => expect(calls("recovery_save")).toHaveLength(1));
    const [saved] = calls("recovery_save");
    expect(saved).toMatchObject({ documentId: id, kind: "writer", title: "Draft" });
    // The tab keeps its data: the crash does not touch the store.
    expect(useOfficeTabs.getState().tabs[0].dirty).toBe(true);
    const logged = calls("log_frontend").map((args) => String(args.message));
    expect(logged.some((message) => message.includes("scope: office:writer"))).toBe(true);
    // The fallback explains what happened to unsaved work.
    expect(screen.getByRole("alert")).toHaveTextContent("recovery copy");
  });

  it("does not snapshot a document that has no unsaved changes", async () => {
    useOfficeTabs.getState().create("writer", "Clean");
    render(<OfficeWorkspace />);
    expect(screen.getByRole("alert")).toBeInTheDocument();
    await waitFor(() => expect(calls("log_frontend")).toHaveLength(1));
    expect(calls("recovery_save")).toHaveLength(0);
  });

  it("reopens the editor with Try again once the problem is gone", async () => {
    const user = userEvent.setup();
    useOfficeTabs.getState().create("writer", "Draft");
    render(<OfficeWorkspace />);
    expect(screen.getByRole("alert")).toBeInTheDocument();

    crash.writer = false;
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByText("writer editor for Draft")).toBeInTheDocument();
  });
});
