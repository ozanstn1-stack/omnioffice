import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { GlobalOverlays } from "./global-overlays";

function overlays(paletteOpen: boolean, searchOpen: boolean) {
  return (
    <GlobalOverlays
      screen="home"
      backgroundJobs={0}
      paletteOpen={paletteOpen}
      onClosePalette={vi.fn()}
      searchOpen={searchOpen}
      onCloseSearch={vi.fn()}
      onNavigate={vi.fn()}
      onOpenPath={vi.fn()}
      onOpenRecent={vi.fn()}
    />
  );
}

describe("global overlays (3.5)", () => {
  it("loads the command palette and global search only when opened", async () => {
    const { rerender } = render(overlays(false, false));
    expect(screen.queryByRole("dialog")).toBeNull();

    rerender(overlays(true, false));
    expect(await screen.findByRole("dialog")).toBeTruthy();

    rerender(overlays(false, false));
    expect(screen.queryByRole("dialog")).toBeNull();

    rerender(overlays(false, true));
    expect(await screen.findByRole("dialog")).toBeTruthy();
  });

  it("renders the presend slot when provided", () => {
    render(
      <GlobalOverlays
        screen="home"
        backgroundJobs={0}
        paletteOpen={false}
        onClosePalette={vi.fn()}
        searchOpen={false}
        onCloseSearch={vi.fn()}
        onNavigate={vi.fn()}
        onOpenPath={vi.fn()}
        onOpenRecent={vi.fn()}
        presend={<div role="dialog">Presend check</div>}
      />,
    );
    expect(screen.getByRole("dialog")).toHaveTextContent("Presend check");
  });
});
