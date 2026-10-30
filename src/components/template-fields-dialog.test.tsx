import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

const invoke = vi.fn(async (command: string, _payload?: unknown) => {
  if (command === "person_card_list") return { cards: [], warnings: [], encrypted: true };
  return null;
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...(args as [string, unknown])) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null), save: vi.fn(async () => null) }));

import { TemplateFieldsDialog } from "./template-fields-dialog";

const fields = [
  { name: "tarih", count: 1 },
  { name: "ad_soyad", count: 1 },
  { name: "kurum", count: 1 },
];

describe("TemplateFieldsDialog", () => {
  it("shows a labelled input per field and proposes today's date", () => {
    render(
      <TemplateFieldsDialog
        templateName="Dilekçe (alanlı)"
        fields={fields}
        onSubmit={vi.fn()}
        onCreateEmpty={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    expect(screen.getByText(/Dilekçe \(alanlı\)/)).toBeInTheDocument();
    const date = screen.getByLabelText("Date") as HTMLInputElement;
    expect(date.value).not.toBe("");
    expect(screen.getByLabelText("Full name")).toBeInTheDocument();
    expect(screen.getByLabelText("Institution")).toBeInTheDocument();
  });

  it("submits the typed values", async () => {
    const onSubmit = vi.fn();
    const user = userEvent.setup();
    render(<TemplateFieldsDialog fields={fields} onSubmit={onSubmit} onCreateEmpty={vi.fn()} onClose={vi.fn()} />);

    await user.type(screen.getByLabelText("Full name"), "Ayşe Yılmaz");
    await user.type(screen.getByLabelText("Institution"), "İstanbul Valiliği");
    await user.click(screen.getByRole("button", { name: "Fill and create" }));

    expect(onSubmit).toHaveBeenCalledTimes(1);
    const values = onSubmit.mock.calls[0][0] as Record<string, string>;
    expect(values.ad_soyad).toBe("Ayşe Yılmaz");
    expect(values.kurum).toBe("İstanbul Valiliği");
    expect(values.tarih).not.toBe("");
  });

  it("offers creating the document without filling", async () => {
    const onCreateEmpty = vi.fn();
    const user = userEvent.setup();
    render(<TemplateFieldsDialog fields={fields} onSubmit={vi.fn()} onCreateEmpty={onCreateEmpty} onClose={vi.fn()} />);

    await user.click(screen.getByRole("button", { name: "Create without filling" }));
    expect(onCreateEmpty).toHaveBeenCalledTimes(1);
  });

  it("closes from the cancel button", async () => {
    const onClose = vi.fn();
    const user = userEvent.setup();
    render(<TemplateFieldsDialog fields={fields} onSubmit={vi.fn()} onCreateEmpty={vi.fn()} onClose={onClose} />);

    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
