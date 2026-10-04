import { useEffect } from "react";
import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

/**
 * Data Loss Protection.
 *
 * The gate runs the real `office_compatibility` command before any write into
 * a format that is not the lossless `.oswk` master. These tests mock the
 * backend with the exact wire payload the Rust structs serialize
 * (`CompatibilityReport { target, items: [{ feature, status, message }] }`,
 * serde camelCase on crates/officecore/src/compat.rs) and cover every branch:
 * lossless, lossy + Continue / Cancel / Save as .oswk, fail-open, and the PDF
 * render-target carve-out.
 */

interface Call {
  command: string;
  args: Record<string, unknown>;
}

const calls: Call[] = [];
let compatibilityByTarget: Record<string, unknown> = {};
let compatibilityFailure: Error | null = null;

async function handleInvoke(command: string, args?: Record<string, unknown>): Promise<unknown> {
  const payload = (args ?? {}) as Record<string, unknown>;
  calls.push({ command, args: payload });
  if (command === "office_compatibility") {
    if (compatibilityFailure) throw compatibilityFailure;
    const target = String(payload.target ?? "");
    return compatibilityByTarget[target] ?? { target, items: [] };
  }
  if (command === "office_save_document" || command === "office_save_unit" || command === "office_export_pdf") {
    return { path: String(payload.path ?? ""), warnings: [] };
  }
  return null;
}

const invoke = vi.fn(handleInvoke);
const saveDialogMock = vi.fn(async (_options?: unknown) => null as string | null);
const openDialogMock = vi.fn(async (_options?: unknown) => null);

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...(args as [string])) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/api/path", () => ({
  appCacheDir: vi.fn(async () => "C:/cache"),
  join: vi.fn(async (...parts: string[]) => parts.join("/")),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  // Deferred calls: the factory runs before the consts above are initialised.
  open: (...args: unknown[]) => openDialogMock(...(args as [never])),
  save: (...args: unknown[]) => saveDialogMock(...(args as [never])),
}));
vi.mock("@tauri-apps/plugin-fs", () => ({ readFile: vi.fn(async () => new Uint8Array()) }));

import { useOfficeSession } from "./useOfficeSession";
import { useOfficeTabs, type OfficeTab } from "../lib/office-store";
import { useToasts } from "../lib/store";
import { DataLossDialogHost, useDataLossPrompt } from "../components/data-loss-dialog";
import { lossRowFlags, lossyItems, type CompatibilityReport } from "../components/compatibility";
import type { OfficeKind } from "../lib/office-types";

type Session = ReturnType<typeof useOfficeSession>;

const lastCall = (command: string) => [...calls].reverse().find((call) => call.command === command);

let session: Session | null = null;

/** Renders the hook together with the dialog host the workspace mounts. */
function Harness({ tab }: { tab: OfficeTab }) {
  const current = useOfficeSession(tab);
  useEffect(() => {
    session = current;
  }, [current]);
  return <DataLossDialogHost />;
}

function renderSession(kind: OfficeKind = "writer"): Session {
  useOfficeTabs.setState({ tabs: [], activeId: null });
  const id = useOfficeTabs.getState().create(kind, "Report");
  // Mark the tab dirty the way an editor does, so a cancelled save can be
  // checked against the document state.
  useOfficeTabs.getState().edit(id, (model) => model);
  const tab = useOfficeTabs.getState().tabs.find((candidate) => candidate.id === id)!;
  session = null;
  render(<Harness tab={tab} />);
  return session!;
}

/**
 * Starts a save; the returned promise stays pending because the loss dialog
 * waits for the user. Callers must await the dialog before interacting.
 */
function startSave(session: Session, path: string): Promise<string | null> {
  let pending: Promise<string | null> | undefined;
  act(() => {
    pending = session.save(path);
  });
  return pending!;
}

function startExportPdf(session: Session): Promise<string | null> {
  let pending: Promise<string | null> | undefined;
  act(() => {
    pending = session.exportPdf();
  });
  return pending!;
}

beforeEach(() => {
  calls.length = 0;
  compatibilityByTarget = {};
  compatibilityFailure = null;
  useToasts.setState({ toasts: [] });
  useDataLossPrompt.setState({ request: null });
  invoke.mockReset();
  invoke.mockImplementation(handleInvoke);
  saveDialogMock.mockReset();
  saveDialogMock.mockImplementation(async () => null);
  openDialogMock.mockReset();
  openDialogMock.mockImplementation(async () => null);
});

afterEach(() => {
  useDataLossPrompt.setState({ request: null });
});

describe("Data Loss Protection before lossy office saves", () => {
  it("saves a lossless target without showing the warning", async () => {
    compatibilityByTarget.docx = {
      target: "docx",
      items: [
        { feature: "sections", status: "unchanged", message: "Section breaks are written as real sectPr parts." },
      ],
    } satisfies CompatibilityReport;
    const session = renderSession();
    let saved: string | null = null;
    await act(async () => {
      saved = await session.save("C:/docs/report.docx");
    });
    expect(saved).toBe("C:/docs/report.docx");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(lastCall("office_save_document")?.args.path).toBe("C:/docs/report.docx");
  });

  it("shows the real report rows before writing a lossy target, then continues on request", async () => {
    const user = userEvent.setup();
    compatibilityByTarget.rtf = {
      target: "rtf",
      items: [
        { feature: "sections", status: "transformed", message: "Sections become page breaks." },
        { feature: "comments", status: "lost", message: "Comments are not written to RTF; keep the .oswk copy." },
      ],
    } satisfies CompatibilityReport;
    const session = renderSession();
    const pending = startSave(session, "C:/docs/report.rtf");
    expect(await screen.findByText("Data loss warning")).toBeInTheDocument();

    // The exact rows from the report, plus the required matrix columns.
    expect(screen.getByText("comments")).toBeInTheDocument();
    expect(screen.getByText(/Comments are not written to RTF/)).toBeInTheDocument();
    for (const header of ["Feature", "Supported?", "Imported?", "Exported?", "Transformed?", "Lost?"]) {
      expect(screen.getByText(header)).toBeInTheDocument();
    }
    expect(screen.getByText(/will be lost and 1 converted/)).toBeInTheDocument();
    // The request uses the real wire arguments: { kind, model, target }.
    expect(lastCall("office_compatibility")?.args).toMatchObject({ kind: "writer", target: "rtf" });
    expect(lastCall("office_compatibility")?.args.model).toBe(useOfficeTabs.getState().tabs[0].model);
    // Nothing has been written while the question is open.
    expect(lastCall("office_save_document")).toBeUndefined();

    await user.click(screen.getByRole("button", { name: /^continue$/i }));
    await act(async () => {
      await pending;
    });
    expect(lastCall("office_save_document")?.args.path).toBe("C:/docs/report.rtf");
  });

  it("keeps the document dirty and writes nothing when the warning is cancelled", async () => {
    const user = userEvent.setup();
    compatibilityByTarget.rtf = {
      target: "rtf",
      items: [
        { feature: "comments", status: "lost", message: "Comments are not written to RTF; keep the .oswk copy." },
      ],
    } satisfies CompatibilityReport;
    const session = renderSession();
    const pending = startSave(session, "C:/docs/report.rtf");
    expect(await screen.findByRole("dialog")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /^cancel$/i }));
    let saved: string | null = "not settled";
    await act(async () => {
      saved = await pending;
    });
    expect(saved).toBeNull();
    expect(lastCall("office_save_document")).toBeUndefined();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    const tab = useOfficeTabs.getState().tabs[0];
    expect(tab.dirty).toBe(true);
    expect(tab.path).toBeNull();
  });

  it("writes the lossless .oswk master instead when the user chooses it", async () => {
    const user = userEvent.setup();
    compatibilityByTarget.rtf = {
      target: "rtf",
      items: [
        { feature: "comments", status: "lost", message: "Comments are not written to RTF; keep the .oswk copy." },
      ],
    } satisfies CompatibilityReport;
    // A real destination choice: the .oswk flow opens its own save dialog.
    saveDialogMock.mockResolvedValueOnce("C:/docs/report.oswk");
    const session = renderSession();
    const pending = startSave(session, "C:/docs/report.rtf");
    expect(await screen.findByRole("dialog")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /save as \.oswk/i }));
    let saved: string | null = null;
    await act(async () => {
      saved = await pending;
    });
    expect(saved).toBe("C:/docs/report.oswk");
    expect(lastCall("office_save_unit")?.args.path).toBe("C:/docs/report.oswk");
    expect(lastCall("office_save_document")).toBeUndefined();
    expect(saveDialogMock).toHaveBeenCalledWith(expect.objectContaining({ defaultPath: "C:/docs/report.oswk" }));
  });

  it("never runs the compatibility report for a .oswk save", async () => {
    const session = renderSession();
    await act(async () => {
      await session.save("C:/docs/report.oswk");
    });
    expect(calls.some((call) => call.command === "office_compatibility")).toBe(false);
    expect(lastCall("office_save_unit")?.args.path).toBe("C:/docs/report.oswk");
  });

  it("fails open with a note when the compatibility check cannot be loaded", async () => {
    compatibilityFailure = new Error("compat backend offline");
    const session = renderSession();
    await act(async () => {
      await session.save("C:/docs/report.rtf");
    });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(lastCall("office_save_document")?.args.path).toBe("C:/docs/report.rtf");
    expect(useToasts.getState().toasts.some((toast) => toast.title === "Compatibility check unavailable")).toBe(true);
  });
});

describe("the PDF render-target carve-out", () => {
  it("does not gate a pure PDF export on a container-loss row", async () => {
    saveDialogMock.mockResolvedValue("C:/docs/Report.pdf");
    compatibilityByTarget.pdf = {
      target: "pdf",
      items: [{ feature: "format", status: "lost", message: "This format is not a Writer target." }],
    } satisfies CompatibilityReport;
    const session = renderSession();
    await act(async () => {
      await session.exportPdf();
    });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(lastCall("office_export_pdf")?.args.path).toBe("C:/docs/Report.pdf");
  });

  it("still warns about a real rendering loss before a PDF export", async () => {
    const user = userEvent.setup();
    saveDialogMock.mockResolvedValue("C:/docs/Deck.pdf");
    compatibilityByTarget.pdf = {
      target: "pdf",
      items: [{ feature: "animations", status: "lost", message: "Animations do not apply to PDF output." }],
    } satisfies CompatibilityReport;
    const session = renderSession("impress");
    const pending = startExportPdf(session);
    expect(await screen.findByText("animations")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /^cancel$/i }));
    let saved: string | null = "not settled";
    await act(async () => {
      saved = await pending;
    });
    expect(saved).toBeNull();
    expect(lastCall("office_export_pdf")).toBeUndefined();
  });
});

describe("the derived loss matrix uses the real report rows", () => {
  it("counts a partial row as transformed, matching the backend's lossy()", () => {
    const report: CompatibilityReport = {
      target: "pptx",
      items: [{ feature: "animations", status: "partial", message: "Effects are simplified." }],
    };
    expect(lossyItems(report)).toHaveLength(1);
    expect(lossRowFlags(report.items[0])).toMatchObject({
      supported: true,
      imported: true,
      exported: false,
      transformed: true,
      lost: false,
    });
  });

  it("maps a lost row to dropped and unsupported", () => {
    const report: CompatibilityReport = {
      target: "odt",
      items: [
        { feature: "comments", status: "lost", message: "Comments are not written to ODT; keep the .oswk copy." },
      ],
    };
    expect(lossRowFlags(report.items[0])).toMatchObject({
      supported: false,
      imported: true,
      exported: false,
      transformed: false,
      lost: true,
    });
  });
});
