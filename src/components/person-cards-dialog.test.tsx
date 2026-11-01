import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const ayse = {
  id: "card-ayse",
  name: "Ayşe Yılmaz",
  taxId: "12345678901",
  address: "Atatürk Cad. 1",
  phone: "+90 555 000 00 00",
  email: "ayse@example.org",
  note: "",
  updatedAt: "2026-10-30T10:00:00Z",
};

let cards = [ayse];
const invoke = vi.fn(async (command: string, payload?: unknown) => {
  switch (command) {
    case "person_card_list":
      return { cards, warnings: [], encrypted: true };
    case "person_card_save": {
      const card = (payload as { card: Record<string, unknown> }).card;
      const saved = { ...card, id: `card-${String(card.name)}`, updatedAt: "2026-10-30T10:00:00Z" };
      return saved;
    }
    case "person_card_delete":
      return true;
    default:
      return null;
  }
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...(args as [string, unknown])) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null), save: vi.fn(async () => null) }));

import { useToasts } from "../lib/store";
import { PersonCardsDialog } from "./person-cards-dialog";
import en from "../lib/i18n/en.json";

function installClipboard() {
  const calls: string[] = [];
  Object.defineProperty(window.navigator, "clipboard", {
    value: {
      writeText: async (value: string) => {
        calls.push(value);
      },
    },
    configurable: true,
  });
  return calls;
}

describe("PersonCardsDialog (C6/2)", () => {
  beforeEach(() => {
    invoke.mockClear();
    cards = [ayse];
    useToasts.setState({ toasts: [] });
  });

  it("lists the saved cards with their details", async () => {
    render(<PersonCardsDialog onClose={vi.fn()} />);
    expect(await screen.findByText("Ayşe Yılmaz")).toBeInTheDocument();
    expect(screen.getByText(/ayse@example\.org/)).toBeInTheDocument();
  });

  it("saves a new card through the command", async () => {
    const user = userEvent.setup();
    render(<PersonCardsDialog onClose={vi.fn()} />);
    await screen.findByText("Ayşe Yılmaz");

    await user.click(screen.getByRole("button", { name: "Add card" }));
    await user.type(screen.getByLabelText("Full name"), "Mehmet Kaya");
    await user.type(screen.getByLabelText("E-mail"), "mehmet@example.org");
    await user.click(screen.getByRole("button", { name: "Save card" }));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("person_card_save", expect.anything()));
    const request = invoke.mock.calls.find(([command]) => command === "person_card_save")?.[1] as {
      card: Record<string, unknown>;
    };
    expect(request.card).toMatchObject({ name: "Mehmet Kaya", email: "mehmet@example.org" });
    await waitFor(() => expect(useToasts.getState().toasts.map((toast) => toast.title)).toContain("Person card saved"));
  });

  it("deletes a card", async () => {
    const user = userEvent.setup();
    render(<PersonCardsDialog onClose={vi.fn()} />);
    await screen.findByText("Ayşe Yılmaz");

    await user.click(screen.getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("person_card_delete", { id: "card-ayse" }));
  });

  it("imports pasted CSV rows", async () => {
    const user = userEvent.setup();
    render(<PersonCardsDialog onClose={vi.fn()} />);
    await screen.findByText("Ayşe Yılmaz");

    await user.type(
      screen.getByLabelText("Import cards (CSV or vCard)"),
      "name,email\nZeynep Kaya,zeynep@example.org\nAli Veli,ali@example.org",
    );
    await user.click(screen.getByRole("button", { name: "Import" }));

    await waitFor(() =>
      expect(invoke.mock.calls.filter(([command]) => command === "person_card_save")).toHaveLength(2),
    );
    await waitFor(() =>
      expect(useToasts.getState().toasts.map((toast) => toast.title)).toContain(
        en["personCards.imported.other"].replaceAll("{count}", "2"),
      ),
    );
  });

  it("copies the cards as CSV only when asked", async () => {
    const user = userEvent.setup();
    const clipboard = installClipboard();
    render(<PersonCardsDialog onClose={vi.fn()} />);
    await screen.findByText("Ayşe Yılmaz");

    expect(clipboard).toHaveLength(0);
    await user.click(screen.getByRole("button", { name: "Copy as CSV" }));
    await waitFor(() => expect(clipboard).toHaveLength(1));
    expect(clipboard[0]).toContain("Ayşe Yılmaz");
  });
});
