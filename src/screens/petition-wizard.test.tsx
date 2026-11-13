import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn(async (command: string) => {
  switch (command) {
    case "file_token_save":
      return [{ token: "tok-out", name: "out", path: "C:/docs/out" }];
    case "office_save_document":
      return { path: "C:/docs/out", bytes: 1024 };
    case "office_export_pdf":
      return "C:/docs/out.pdf";
    default:
      return null;
  }
});

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...(args as [string])) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/api/path", () => ({
  documentDir: vi.fn(async () => "C:/docs"),
  appDataDir: vi.fn(async () => "C:/appdata"),
  appCacheDir: vi.fn(async () => "C:/cache"),
  join: vi.fn(async (...parts: string[]) => parts.join("/")),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(async () => null),
  save: vi.fn(async () => "C:/docs/out"),
}));

import { PetitionWizard } from "./PetitionWizard";
import { useOfficeTabs } from "../lib/office-store";
import { useSettings, useToasts } from "../lib/store";
import { DEFAULT_SETTINGS } from "../lib/types";

function inputByLabel(label: string): HTMLInputElement {
  const input = screen.getByText(label).closest("div")?.querySelector("input");
  if (!input) throw new Error(`Missing input for label: ${label}`);
  return input;
}

async function fillRequiredFields(user: ReturnType<typeof userEvent.setup>) {
  await user.type(
    screen.getByPlaceholderText("e.g. TO THE MAYOR OF ANKARA METROPOLITAN MUNICIPALITY"),
    "TO ÇANKAYA MUNICIPALITY",
  );
  await user.type(screen.getByPlaceholderText("e.g. Request to correct the residence address"), "Address Update");
  await user.type(
    screen.getByPlaceholderText("Write the main text of your petition here…"),
    "Please update my registered residence address.",
  );
  await user.type(inputByLabel("Full name"), "Ayşe Yılmaz");
}

describe("PetitionWizard", () => {
  beforeEach(() => {
    invoke.mockClear();
    localStorage.clear();
    useToasts.setState({ toasts: [] });
    useOfficeTabs.setState({ tabs: [], activeId: null });
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS, language: "en" } });
  });

  it("blocks submission and shows an info toast when required fields are empty", async () => {
    const user = userEvent.setup();
    const onNavigate = vi.fn();
    render(<PetitionWizard onNavigate={onNavigate} />);

    await user.click(screen.getByRole("button", { name: "Open in Writer" }));

    expect(useToasts.getState().toasts.map((toast) => toast.title)).toContain(
      "Please fill in the authority, subject, full name and description fields.",
    );
    expect(useOfficeTabs.getState().tabs).toHaveLength(0);
    expect(onNavigate).not.toHaveBeenCalled();
  });

  it("rejects an invalid TCKN using mod-11 validation", async () => {
    const user = userEvent.setup();
    render(<PetitionWizard />);

    await fillRequiredFields(user);
    await user.type(screen.getByPlaceholderText("11-digit identity number"), "12345678901");
    await user.click(screen.getByRole("button", { name: "Save as DOCX" }));

    expect(useToasts.getState().toasts.map((toast) => toast.title)).toContain("Invalid T.C. ID number.");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("creates a writer tab and navigates to office when opening in Writer", async () => {
    const user = userEvent.setup();
    const onNavigate = vi.fn();
    render(<PetitionWizard onNavigate={onNavigate} />);

    await fillRequiredFields(user);
    await user.type(screen.getByPlaceholderText("11-digit identity number"), "10000000146");
    await user.click(screen.getByRole("button", { name: "Open in Writer" }));

    const { tabs, activeId } = useOfficeTabs.getState();
    expect(tabs).toHaveLength(1);
    expect(tabs[0].kind).toBe("writer");
    expect(tabs[0].title).toBe("Address Update");
    expect(activeId).toBe(tabs[0].id);
    expect(onNavigate).toHaveBeenCalledWith("office");
  });

  it("exports DOCX and PDF petitions via save tokens", async () => {
    const user = userEvent.setup();
    render(<PetitionWizard />);

    await fillRequiredFields(user);
    await user.click(screen.getByRole("button", { name: "Save as DOCX" }));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("office_save_document", expect.anything()));
    expect(invoke).toHaveBeenCalledWith("file_token_save", { name: "Address Update.docx", extensions: ["docx"] });
    expect(useToasts.getState().toasts.map((toast) => toast.title)).toContain("DOCX document saved");

    invoke.mockClear();
    useToasts.setState({ toasts: [] });

    await user.click(screen.getByRole("button", { name: "Save as PDF" }));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("office_export_pdf", expect.anything()));
    expect(invoke).toHaveBeenCalledWith("file_token_save", { name: "Address Update.pdf", extensions: ["pdf"] });
    expect(useToasts.getState().toasts.map((toast) => toast.title)).toContain("PDF document saved");
  });

  it("reveals Save as UYAP UDF only when the experimentalUdf flag is enabled", async () => {
    const user = userEvent.setup();
    const first = render(<PetitionWizard />);
    expect(screen.queryByRole("button", { name: "Save as UYAP UDF" })).toBeNull();
    expect(
      screen.getByText("Create standards-compliant DOCX and PDF petitions for submission to official authorities."),
    ).toBeInTheDocument();
    first.unmount();

    localStorage.setItem("flag:experimentalUdf", "1");
    render(<PetitionWizard />);

    expect(
      screen.getByText(
        "Create standards-compliant UDF, DOCX and PDF petitions for submission to official authorities.",
      ),
    ).toBeInTheDocument();
    await fillRequiredFields(user);
    await user.click(screen.getByRole("button", { name: "Save as UYAP UDF" }));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("office_save_document", expect.anything()));
    expect(invoke).toHaveBeenCalledWith("file_token_save", { name: "Address Update.udf", extensions: ["udf"] });
    expect(useToasts.getState().toasts.map((toast) => toast.title)).toContain("UDF document saved");
  });
});
