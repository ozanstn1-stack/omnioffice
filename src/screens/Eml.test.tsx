import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const message = {
  subject: "Tebligat örneği",
  from: "KEP Gönderici <kep@example.org>",
  to: "avukat@example.org",
  cc: "",
  date: "Mon, 12 Oct 2026 09:30:00 +0300",
  messageId: "<abc123@kep.example.org>",
  headers: [
    { name: "Subject", value: "Tebligat örneği" },
    { name: "From", value: "kep@example.org" },
  ],
  bodyText: "Sayın Avukat, tebligat ekte gönderilmiştir.",
  bodyHtml: "",
  attachments: [
    { name: "tebligat.pdf", contentType: "application/pdf", size: 2048, path: "C:/cache/tebligat.pdf" },
    { name: "fatura.exe", contentType: "application/octet-stream", size: 4096, path: "C:/cache/fatura.exe" },
  ],
  warnings: [],
};

const invoke = vi.fn(async (command: string) => {
  if (command === "office_open_eml") return message;
  return null;
});

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...(args as [string])) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null), save: vi.fn(async () => null) }));

import { Eml } from "./Eml";

describe("KEP .eml viewer", () => {
  beforeEach(() => {
    invoke.mockClear();
  });

  it("shows the headers, body and attachments, and opens a PDF attachment in the Reader", async () => {
    const onNavigate = vi.fn();
    const user = userEvent.setup();
    render(<Eml initialFiles={["C:/mail/kep.eml"]} onNavigate={onNavigate} />);

    expect(await screen.findByText("Tebligat örneği")).toBeInTheDocument();
    expect(screen.getByText("Sayın Avukat, tebligat ekte gönderilmiştir.")).toBeInTheDocument();
    expect(screen.getByText("tebligat.pdf")).toBeInTheDocument();
    expect(screen.getByText("application/pdf")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /^open$/i }));
    expect(onNavigate).toHaveBeenCalledWith("reader", { files: ["C:/cache/tebligat.pdf"] });
  });

  it("never offers to launch an executable attachment", async () => {
    render(<Eml initialFiles={["C:/mail/kep.eml"]} onNavigate={vi.fn()} />);
    await screen.findByText("Tebligat örneği");
    expect(screen.getByText("fatura.exe")).toBeInTheDocument();
    // The risky attachment is flagged and the single "Open" button belongs to the PDF.
    expect(screen.getByText(/not launched/i)).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /^open$/i })).toHaveLength(1);
  });
});
