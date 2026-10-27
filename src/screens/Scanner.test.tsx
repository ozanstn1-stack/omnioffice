import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn(async (command: string) => {
  if (command === "file_token_pick")
    return ["C:/pages/a.png", "C:/pages/b.png"].map((path) => ({ token: path, name: path.split("/").pop(), path }));
  if (command === "file_token_save") return [{ token: "C:/out.pdf", name: "out.pdf", path: "C:/out.pdf" }];
  if (command === "office_scan_pages") return { output: "C:/out.pdf", pages: 2, corrected: 0, warnings: [] };
  if (command === "office_scan_preview") return { dataUrl: "data:image/jpeg;base64,AAAA", width: 1000, height: 2000 };
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

  it("offers the camera through the system camera app, with no Google scanner", async () => {
    render(<Scanner />);
    expect(await screen.findByRole("button", { name: /Take a page photo/i })).toBeInTheDocument();
    expect(screen.queryByText(/ML Kit/i)).not.toBeInTheDocument();
    const input = document.querySelector<HTMLInputElement>('input[type="file"]');
    expect(input?.getAttribute("capture")).toBe("environment");
  });

  it("allows reordering and clearing pages", async () => {
    const user = userEvent.setup();
    render(<Scanner />);
    await user.click(screen.getByRole("button", { name: /Add page photos/i }));
    expect(await screen.findByText("a.png")).toBeInTheDocument();
    expect(screen.getByText("b.png")).toBeInTheDocument();

    const clearBtn = screen.getByRole("button", { name: /Clear all/i });
    expect(clearBtn).toBeInTheDocument();
    await user.click(clearBtn);
    expect(screen.queryByText("a.png")).not.toBeInTheDocument();
  });

  it("crops a page with the corner picker and sends the corners in photo pixels", async () => {
    const user = userEvent.setup();
    render(<Scanner />);
    await user.click(screen.getByRole("button", { name: /Add page photos/i }));
    await user.click((await screen.findAllByRole("button", { name: /Adjust corners/i }))[0]);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("office_scan_preview", { path: "C:/pages/a.png" }));
    // The picker opens on the upright preview with the corners a little inside.
    expect(await screen.findByRole("img", { name: /a\.png/ })).toBeInTheDocument();

    // Arrow keys move the focused corner by 1 % of the photo.
    const topLeft = screen.getByRole("button", { name: /Top-left corner/i });
    topLeft.focus();
    await user.keyboard("{ArrowLeft}{ArrowUp}");
    await user.click(screen.getByRole("button", { name: /^Save$/ }));
    expect(screen.getByText(/cropped/i)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /Build PDF/i }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "office_scan_pages",
        expect.objectContaining({
          request: expect.objectContaining({
            pages: [
              {
                path: "C:/pages/a.png",
                corners: [
                  [50, 100],
                  [940, 120],
                  [940, 1880],
                  [60, 1880],
                ],
              },
              { path: "C:/pages/b.png" },
            ],
          }),
        }),
      ),
    );
  });

  it("drops the crop when the whole photo is chosen", async () => {
    const user = userEvent.setup();
    render(<Scanner />);
    await user.click(screen.getByRole("button", { name: /Add page photos/i }));
    await user.click((await screen.findAllByRole("button", { name: /Adjust corners/i }))[0]);
    await user.click(await screen.findByRole("button", { name: /Whole photo/i }));
    await user.click(screen.getByRole("button", { name: /^Save$/ }));
    expect(screen.queryByText(/cropped/i)).not.toBeInTheDocument();
  });
});
