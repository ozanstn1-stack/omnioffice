import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn(async (command: string) => {
  if (command === "office_mail_merge") {
    return {
      produced: ["C:/out/Ayşe.pdf", "C:/out/Mehmet.pdf"],
      errors: [{ row: 3, message: "missing field(s): soyad" }],
    };
  }
  return null;
});

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...(args as [string])) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null) }));

import { MailMerge } from "./MailMerge";
import en from "../lib/i18n/en.json";

describe("mail merge screen", () => {
  beforeEach(() => {
    invoke.mockClear();
  });

  it("runs the merge and shows produced documents and row errors", async () => {
    const user = userEvent.setup();
    render(<MailMerge />);
    const inputs = screen.getAllByRole("textbox");
    await user.type(inputs[0], "C:/t/mektup.docx");
    await user.type(inputs[1], "C:/t/veri.csv");
    await user.type(inputs[2], "C:/out");
    await user.type(inputs[3], "ad");
    await user.click(screen.getByRole("button", { name: /Merge/i }));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "office_mail_merge",
        expect.objectContaining({
          request: expect.objectContaining({
            template: "C:/t/mektup.docx",
            data: "C:/t/veri.csv",
            outputDir: "C:/out",
            format: "pdf",
            nameField: "ad",
          }),
        }),
      ),
    );
    expect(
      await screen.findByText(new RegExp(en["mailMerge.produced.other"].replaceAll("{count}", "2"), "i")),
    ).toBeInTheDocument();
    expect(screen.getByText(/missing field\(s\): soyad/i)).toBeInTheDocument();
  });
});
