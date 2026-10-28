import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { BottomNav, keyboardLikelyOpen } from "./bottom-nav";

describe("phone bottom navigation", () => {
  it("offers Home, Office, Read, Scan and More within thumb reach", async () => {
    const user = userEvent.setup();
    const navigate = vi.fn();
    const more = vi.fn();
    render(<BottomNav screen="home" onNavigate={navigate} onMore={more} />);
    const nav = screen.getByRole("navigation", { name: /Main sections/ });
    const labels = Array.from(nav.querySelectorAll("button")).map((button) => button.textContent);
    expect(labels).toEqual(["Home", "Office", "Read", "Scan", "More"]);

    await user.click(screen.getByRole("button", { name: "Scan" }));
    expect(navigate).toHaveBeenCalledWith("scanner");
    await user.click(screen.getByRole("button", { name: "More" }));
    expect(more).toHaveBeenCalled();
  });

  it("marks the tab a screen belongs to as the current page", () => {
    render(<BottomNav screen="spreadsheets" onNavigate={vi.fn()} onMore={vi.fn()} />);
    expect(screen.getByRole("button", { name: "Office" })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("button", { name: "Home" })).not.toHaveAttribute("aria-current");
  });

  it("steps aside while the on-screen keyboard is up", () => {
    expect(keyboardLikelyOpen(400, 800)).toBe(true);
    expect(keyboardLikelyOpen(780, 800)).toBe(false);
    expect(keyboardLikelyOpen(0, 0)).toBe(false);
  });
});
