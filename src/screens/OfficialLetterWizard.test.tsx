import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

let pdfaValid = true;
const invoke = vi.fn(async (command: string, _payload?: unknown) => {
  switch (command) {
    case "file_token_save":
      return [{ token: "tok-letter", name: "seminer.pdf", path: "C:/docs/seminer.pdf" }];
    case "office_export_pdfa":
      return {
        valid: pdfaValid,
        level: "2b",
        checks: [],
        failures: pdfaValid ? 0 : 2,
        warnings: 0,
        converted: true,
        fontEmbedding: [],
      };
    case "office_save_document":
      return { path: "C:/docs/seminer.docx", warnings: [] };
    default:
      return null;
  }
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...(args as [string])) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));

import { OfficialLetterWizard } from "./OfficialLetterWizard";
import { deskTasks } from "../lib/desks";
import { useOfficeTabs } from "../lib/office-store";
import { useSettings, useToasts } from "../lib/store";
import { DEFAULT_SETTINGS } from "../lib/types";
import type { TextDocument } from "../lib/office-types";

const payloadOf = <T,>(command: string) => invoke.mock.calls.find(([name]) => name === command)?.[1] as T;

async function fillLetter(user: ReturnType<typeof userEvent.setup>) {
  fireEvent.change(screen.getByLabelText(/Institution lines/), {
    target: { value: "İstanbul Valiliği\nİl Millî Eğitim Müdürlüğü" },
  });
  await user.type(screen.getByLabelText("DETSİS number"), "12345678");
  await user.type(screen.getByLabelText("File plan code"), "010.01");
  await user.type(screen.getByLabelText("Subject"), "Eğitim semineri");
  await user.type(screen.getByLabelText("Addressee"), "Kadıköy Kaymakamlığına");
  fireEvent.change(screen.getByLabelText(/^Text/), {
    target: { value: "Seminer 5 Kasım 2026 tarihinde yapılacaktır." },
  });
  await user.type(screen.getByLabelText("Signer's name and surname"), "ayşe yılmaz");
  await user.type(screen.getByLabelText("Title"), "Müdür");
}

describe("official letter wizard (#341 C5)", () => {
  beforeEach(() => {
    cleanup();
    localStorage.clear();
    invoke.mockClear();
    pdfaValid = true;
    useOfficeTabs.setState({ tabs: [], activeId: null } as never);
    useToasts.setState({ toasts: [] } as never);
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS, language: "en" }, loaded: true });
  });

  it("numbers a compliant letter and opens it in Writer", async () => {
    const user = userEvent.setup();
    const onNavigate = vi.fn();
    render(<OfficialLetterWizard onNavigate={onNavigate} />);
    await fillLetter(user);

    expect(screen.getByText("Number: E-12345678-010.01-1")).toBeInTheDocument();
    expect(screen.getByText("The regulation check finds nothing to fix.")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Open in Writer" }));
    const tab = useOfficeTabs.getState().tabs.at(-1)!;
    expect(tab.kind).toBe("writer");
    const doc = tab.model as TextDocument;
    const lines = doc.blocks.map((block) => (block.type === "paragraph" ? block.runs.map((r) => r.text).join("") : ""));
    expect(lines).toContain("İSTANBUL VALİLİĞİ");
    expect(lines.some((line) => line.startsWith("Sayı\t: E-12345678-010.01-1\t"))).toBe(true);
    expect(lines).toContain("Ayşe YILMAZ");
    expect(onNavigate).toHaveBeenCalledWith("office");

    // The number is used up: the next letter gets 2.
    expect(screen.getByText("Number: E-12345678-010.01-2")).toBeInTheDocument();
  });

  it("saves the letter as PDF/A and reports the validation", async () => {
    const user = userEvent.setup();
    render(<OfficialLetterWizard />);
    await fillLetter(user);

    await user.click(screen.getByRole("button", { name: "Save as PDF/A" }));
    await waitFor(() => expect(payloadOf("office_export_pdfa")).toBeTruthy());
    const payload = payloadOf<{ kind: string; path: string; level: string; model: TextDocument }>("office_export_pdfa");
    expect(payload).toMatchObject({ kind: "writer", path: "tok-letter", level: "2b" });
    expect(payload.model.page.marginLeftPt).toBeCloseTo(70.87);
    await waitFor(() =>
      expect(useToasts.getState().toasts.some((toast) => toast.title === "PDF/A-2b saved and validated.")).toBe(true),
    );

    pdfaValid = false;
    await user.click(screen.getByRole("button", { name: "Save as PDF/A" }));
    await waitFor(() =>
      expect(
        useToasts.getState().toasts.some((toast) => toast.title === "Saved, but PDF/A validation reports 2 failures."),
      ).toBe(true),
    );
  });

  it("refuses to build before the required fields are filled", async () => {
    const user = userEvent.setup();
    render(<OfficialLetterWizard />);
    await user.click(screen.getByRole("button", { name: "Open in Writer" }));
    expect(useOfficeTabs.getState().tabs).toHaveLength(0);
    expect(
      useToasts
        .getState()
        .toasts.some((toast) => toast.title === "Fill in the subject, addressee, text and signer first."),
    ).toBe(true);
  });

  it("remembers the institution, signer and contact details for the next letter", async () => {
    const user = userEvent.setup();
    const { unmount } = render(<OfficialLetterWizard />);
    await fillLetter(user);
    await user.type(screen.getByLabelText("KEP address"), "istanbulmem@hs01.kep.tr");
    unmount();

    render(<OfficialLetterWizard />);
    expect(screen.getByLabelText("DETSİS number")).toHaveValue("12345678");
    expect(screen.getByLabelText("Title")).toHaveValue("Müdür");
    expect(screen.getByLabelText("KEP address")).toHaveValue("istanbulmem@hs01.kep.tr");
    expect(screen.getByLabelText("Subject")).toHaveValue("");
  });

  it("is what the public servant desk's official letter task opens", () => {
    expect(deskTasks("public").find((task) => task.key === "officialLetter")?.screen).toBe("officialLetter");
  });
});
