import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn(async (command: string) => {
  if (command === "file_token_pick")
    return ["C:/pages/a.png", "C:/pages/b.png"].map((path) => ({ token: path, name: path.split("/").pop(), path }));
  if (command === "file_token_save") return [{ token: "C:/out.pdf", name: "out.pdf", path: "C:/out.pdf" }];
  if (command === "office_scan_pages") return { output: "C:/out.pdf", pages: 2, corrected: 0, warnings: [] };
  return null;
});

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...(args as [string])) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(async () => ["C:/pages/a.png", "C:/pages/b.png"]),
  save: vi.fn(async () => "C:/out.pdf"),
}));

import { Scanner } from "./Scanner";

describe("document scanner scaffold", () => {
  beforeEach(() => {
    invoke.mockClear();
  });

  it("assembles a PDF from the picked page photos through the Rust scanner", async () => {
    const user = userEvent.setup();
    render(<Scanner />);
    await user.click(screen.getByRole("button", { name: /Add page photos/i }));
    expect(await screen.findByText("a.png")).toBeInTheDocument();
    expect(screen.getByText("b.png")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /Build PDF/i }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "office_scan_pages",
        expect.objectContaining({
          request: expect.objectContaining({
            output: "C:/out.pdf",
            pages: [{ path: "C:/pages/a.png" }, { path: "C:/pages/b.png" }],
          }),
        }),
      ),
    );
  });

  it("renders the camera capture action and ML Kit note", async () => {
    render(<Scanner />);
    expect(await screen.findByRole("button", { name: /Kameradan Fotoğraf Çek/i })).toBeInTheDocument();
    expect(screen.getByText(/ML Kit/i)).toBeInTheDocument();
  });

  it("allows reordering and clearing pages", async () => {
    const user = userEvent.setup();
    render(<Scanner />);
    await user.click(screen.getByRole("button", { name: /Add page photos/i }));
    expect(await screen.findByText("a.png")).toBeInTheDocument();
    expect(screen.getByText("b.png")).toBeInTheDocument();

    const clearBtn = screen.getByRole("button", { name: /Tümünü Temizle/i });
    expect(clearBtn).toBeInTheDocument();
    await user.click(clearBtn);
    expect(screen.queryByText("a.png")).not.toBeInTheDocument();
  });
});
