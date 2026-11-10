import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { BottomNav, keyboardLikelyOpen } from "./bottom-nav";

describe("phone bottom navigation", () => {
  it("offers Home, Office, Tools, Assistant and More within thumb reach", async () => {
    const user = userEvent.setup();
    const navigate = vi.fn();
    const more = vi.fn();
    render(<BottomNav screen="home" onNavigate={navigate} onMore={more} />);
    const nav = screen.getByRole("navigation", { name: /Main sections/ });
    const labels = Array.from(nav.querySelectorAll("button")).map((button) => button.textContent);
    expect(labels).toEqual(["Home", "Office", "Tools", "Assistant", "More"]);

    await user.click(screen.getByRole("button", { name: "Tools" }));
    expect(navigate).toHaveBeenCalledWith("tools");
    await user.click(screen.getByRole("button", { name: "Assistant" }));
    expect(navigate).toHaveBeenCalledWith("ai");
    await user.click(screen.getByRole("button", { name: "More" }));
    expect(more).toHaveBeenCalled();
  });

  it("marks the tab a screen belongs to as the current page", () => {
    render(<BottomNav screen="spreadsheets" onNavigate={vi.fn()} onMore={vi.fn()} />);
    expect(screen.getByRole("button", { name: "Office" })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("button", { name: "Home" })).not.toHaveAttribute("aria-current");
  });

  it("lights the tools tab for every tool screen, the scanner included", () => {
    const { unmount } = render(<BottomNav screen="scanner" onNavigate={vi.fn()} onMore={vi.fn()} />);
    expect(screen.getByRole("button", { name: "Tools" })).toHaveAttribute("aria-current", "page");
    unmount();
    render(<BottomNav screen="merge" onNavigate={vi.fn()} onMore={vi.fn()} />);
    expect(screen.getByRole("button", { name: "Tools" })).toHaveAttribute("aria-current", "page");
  });

  it("lights the assistant tab for the task packs and the library", () => {
    render(<BottomNav screen="aiPacks" onNavigate={vi.fn()} onMore={vi.fn()} />);
    expect(screen.getByRole("button", { name: "Assistant" })).toHaveAttribute("aria-current", "page");
  });

  it("steps aside while the on-screen keyboard is up", () => {
    expect(keyboardLikelyOpen(400, 800)).toBe(true);
    expect(keyboardLikelyOpen(780, 800)).toBe(false);
    expect(keyboardLikelyOpen(0, 0)).toBe(false);
  });
});
