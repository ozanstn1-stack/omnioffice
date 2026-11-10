import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ToolsScreen } from "./Tools";
import { CARDS } from "../lib/tool-cards";
import { enabledRoutes, routeEnabled } from "../lib/routes";

describe("Tools hub (7.32)", () => {
  it("shows every enabled tool once, grouped, and opens the one tapped", async () => {
    const user = userEvent.setup();
    const navigate = vi.fn();
    render(<ToolsScreen onNavigate={navigate} />);

    const enabled = CARDS.filter((card) => routeEnabled(card.id));
    const tiles = document.querySelectorAll(".tool-tile");
    expect(tiles).toHaveLength(enabled.length);
    expect(new Set(enabled.map((card) => card.id)).size).toBe(enabled.length);

    // The scanner left the phone tab bar in 7.32, so it leads the PDF group.
    const pdf = screen.getByRole("region", { name: "PDF & documents" });
    expect(within(pdf).getAllByRole("button")[0]).toHaveAccessibleName("Scan");
    await user.click(within(pdf).getByRole("button", { name: "Scan" }));
    expect(navigate).toHaveBeenCalledWith("scanner");
  });

  it("has a tile for every screen it promises", () => {
    // Home and the hub itself are places, Settings has its own gear.
    const places = new Set(["home", "tools", "settings"]);
    const carded = new Set(CARDS.map((card) => card.id));
    const missing = enabledRoutes()
      .map((route) => route.id)
      .filter((id) => !places.has(id) && !carded.has(id));
    expect(missing).toEqual([]);
  });

  it("narrows by group chip and by search, and says when nothing matches", async () => {
    const user = userEvent.setup();
    render(<ToolsScreen onNavigate={vi.fn()} />);

    await user.click(screen.getByRole("button", { name: "Productivity tools" }));
    expect(screen.getByRole("button", { name: "Productivity tools" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.queryByRole("region", { name: "PDF & documents" })).toBeNull();
    expect(screen.getByRole("button", { name: "Notes" })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "All" }));
    await user.type(screen.getByRole("searchbox", { name: "Search tools" }), "watermark");
    expect(document.querySelectorAll(".tool-tile")).toHaveLength(1);
    expect(screen.getByRole("button", { name: "Watermark" })).toBeInTheDocument();

    await user.clear(screen.getByRole("searchbox", { name: "Search tools" }));
    await user.type(screen.getByRole("searchbox", { name: "Search tools" }), "zzqx");
    expect(screen.getByText("No tool matches your search")).toBeInTheDocument();
  });
});
