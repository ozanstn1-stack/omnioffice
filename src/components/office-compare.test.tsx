import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { defaultParaProps, defaultRun, newTextDocument } from "../lib/office-types";

function writerModel(texts: string[]) {
  const document = newTextDocument("test");
  document.blocks = texts.map((text) => ({
    type: "paragraph" as const,
    props: defaultParaProps(),
    runs: [defaultRun(text)],
  }));
  return document;
}

const picks: Array<Array<{ token: string; name: string }>> = [];

const invoke = vi.fn(async (command: string, payload?: unknown) => {
  if (command === "file_token_pick") {
    return picks.shift() ?? [];
  }
  if (command === "office_open_document") {
    const path = (payload as { path?: string })?.path ?? "";
    const texts = path === "tok-a" ? ["Birinci", "İkinci"] : ["Birinci", "İkinci güncel"];
    return { kind: "writer", model: writerModel(texts), legacy: false, token: path };
  }
  return null;
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...(args as [string, unknown])) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null), save: vi.fn(async () => null) }));

import { OfficeCompare } from "./office-compare";

describe("OfficeCompare (C13)", () => {
  beforeEach(() => {
    invoke.mockClear();
    picks.length = 0;
    picks.push([{ token: "tok-a", name: "once.docx" }], [{ token: "tok-b", name: "sonra.docx" }]);
  });

  it("opens two office files and shows the model diff", async () => {
    const user = userEvent.setup();
    render(<OfficeCompare />);

    const pickButtons = screen.getAllByRole("button", { name: "Choose" });
    await user.click(pickButtons[0]);
    await user.click(screen.getAllByRole("button", { name: "Choose" })[1]);
    expect(await screen.findByText("once.docx")).toBeInTheDocument();
    expect(screen.getByText("sonra.docx")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Compare" }));
    await waitFor(() => expect(screen.getByText("Document comparison")).toBeInTheDocument());
    expect(screen.getByText(/Paragraph 2/)).toBeInTheDocument();
    expect(screen.getByText(/güncel/)).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("office_open_document", { path: "tok-a" });
    expect(invoke).toHaveBeenCalledWith("office_open_document", { path: "tok-b" });
  });

  it("cannot run before both files are picked", () => {
    render(<OfficeCompare />);
    expect(screen.getByRole("button", { name: "Compare" })).toBeDisabled();
  });
});
