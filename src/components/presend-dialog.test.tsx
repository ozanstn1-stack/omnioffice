import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const inspection = {
  pageCount: 1,
  annotations: [] as { page: number; subtype: string; contents: string; hidden: boolean }[],
  embeddedFiles: [] as string[],
  attachmentCount: 0,
  hasJavascript: false,
  javascriptEntries: [],
  hasOpenAction: false,
  formFields: [] as { name: string; kind: string; readOnly: boolean }[],
  authorOverride: "",
  subjectOverride: "",
  creator: "",
  producer: "",
};
let current = { ...inspection };
let text = "";

const invoke = vi.fn(async (command: string, _payload?: unknown) => {
  switch (command) {
    case "inspect_document":
      return current;
    case "pdf_verify_signatures":
      return { signatures: [], warnings: [] };
    case "pdfa_validate":
      return { valid: true };
    case "page_text":
      return text;
    default:
      return null;
  }
});
const share = vi.fn(async (_path: string, _title?: string) => undefined);

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invoke(...(args as [string, unknown])),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/api/path", () => ({
  appCacheDir: async () => "/data/cache",
  join: async (...parts: string[]) => parts.join("/"),
}));
vi.mock("../lib/share", () => ({ shareDocumentFile: (path: string, title?: string) => share(path, title) }));

import { PresendDialog } from "./presend-dialog";
import { shareWithCheck, usePresend } from "../lib/presend-gate";
import { useSettings } from "../lib/store";
import { DEFAULT_SETTINGS } from "../lib/types";

describe("pre-send check before sharing", () => {
  beforeEach(() => {
    invoke.mockClear();
    share.mockClear();
    current = { ...inspection };
    text = "";
    usePresend.setState({ request: null });
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS }, loaded: true });
  });

  it("is on by default and shares anything but a PDF straight away", async () => {
    expect(DEFAULT_SETTINGS.presendCheck).toBe(true);
    await shareWithCheck("/docs/letter.docx", "Share");
    expect(share).toHaveBeenCalledWith("/docs/letter.docx", "Share");
    expect(usePresend.getState().request).toBeNull();
  });

  it("shares a PDF without asking when the setting is off", async () => {
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS, presendCheck: false }, loaded: true });
    await shareWithCheck("/docs/a.pdf");
    expect(share).toHaveBeenCalledWith("/docs/a.pdf", undefined);
  });

  it("says a clean PDF is fine and sends the file itself", async () => {
    const user = userEvent.setup();
    render(<PresendDialog onNavigate={vi.fn()} />);
    await act(() => shareWithCheck("/docs/clean.pdf", "Share"));
    expect(await screen.findByText(/Nothing that should not leave/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /^Send$/ }));
    await waitFor(() => expect(share).toHaveBeenCalledWith("/docs/clean.pdf", "Share"));
    expect(usePresend.getState().request).toBeNull();
  });

  it("lists metadata and attachments and sends a cleaned copy", async () => {
    const user = userEvent.setup();
    current = { ...inspection, authorOverride: "Av. Ayşe", embeddedFiles: ["draft.docx"], attachmentCount: 1 };
    render(<PresendDialog onNavigate={vi.fn()} />);
    await act(() => shareWithCheck("/docs/brief.pdf"));
    expect(await screen.findByText(/Attached files/)).toBeInTheDocument();
    expect(screen.getByText(/draft\.docx/)).toBeInTheDocument();
    expect(screen.getByText(/Author/)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /Clean and send/ }));
    await waitFor(() => expect(share).toHaveBeenCalled());
    const sanitize = invoke.mock.calls.find(([command]) => command === "sanitize_pdf");
    const request = (sanitize?.[1] as { request: { input: string; output: { path: string } } }).request;
    expect(request.input).toBe("/docs/brief.pdf");
    expect(request.output.path).toMatch(/^\/data\/cache\/share\/.+\/brief\.pdf$/);
    expect(share.mock.calls[0][0]).toBe(request.output.path);
  });

  it("stops on a TCKN and offers the redaction tool for that file", async () => {
    const user = userEvent.setup();
    const navigate = vi.fn();
    text = "Davacı TCKN 10000000146";
    render(<PresendDialog onNavigate={navigate} />);
    await act(() => shareWithCheck("/docs/dilekce.pdf"));
    expect(await screen.findByText(/Personal data/)).toBeInTheDocument();
    expect(screen.queryByText(/10000000146/)).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Clean and send/ })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Redact/ }));
    expect(navigate).toHaveBeenCalledWith("redact", { files: ["/docs/dilekce.pdf"] });
    expect(share).not.toHaveBeenCalled();
  });

  it("lets you send anyway when the file cannot be checked", async () => {
    const user = userEvent.setup();
    invoke.mockImplementationOnce(async () => {
      throw new Error("encrypted");
    });
    render(<PresendDialog onNavigate={vi.fn()} />);
    await act(() => shareWithCheck("/docs/locked.pdf"));
    expect(await screen.findByText(/could not be checked/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Send anyway/ }));
    await waitFor(() => expect(share).toHaveBeenCalledWith("/docs/locked.pdf", undefined));
  });
});
