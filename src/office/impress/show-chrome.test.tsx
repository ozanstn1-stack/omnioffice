import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { newDeck } from "../../lib/office-types";
import type { Theme } from "./shared";
import { PresenterPanel, SlideshowNav } from "./SlideshowChrome";
import { ViewTab } from "./ImpressRibbonTabs";

const theme: Theme = {
  id: "minimal",
  name: "Minimal",
  background: "#FFFFFF",
  title: "Segoe UI",
  body: "Segoe UI",
  accent: "#2563EB",
  titleColor: "#111827",
  bodyColor: "#334155",
};

describe("Impress show chrome (3.3c)", () => {
  it("counts slides, picks a tool and closes from the show bar", async () => {
    const user = userEvent.setup();
    const onTool = vi.fn();
    const onClose = vi.fn();
    const onClearInk = vi.fn();
    render(
      <SlideshowNav
        index={2}
        count={5}
        presenter={false}
        tool="pointer"
        onTool={onTool}
        onPrevious={vi.fn()}
        onNext={vi.fn()}
        onTogglePresenter={vi.fn()}
        onClearInk={onClearInk}
        onClose={onClose}
      />,
    );
    expect(screen.getByText("3 / 5")).toBeInTheDocument();
    await user.click(screen.getByTitle("Laser pointer"));
    expect(onTool).toHaveBeenCalledWith("laser");
    await user.click(screen.getByTitle("Clear ink"));
    expect(onClearInk).toHaveBeenCalledTimes(1);
    await user.click(screen.getByRole("button", { name: "Close" }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("shows the next slide in presenter view only when there is one", () => {
    const deck = newDeck("Show");
    const props = {
      deck,
      slide: deck.slides[0],
      theme,
      objectStyle: () => undefined,
      elapsedMs: 65_000,
      onPrevious: vi.fn(),
      onNext: vi.fn(),
    };
    const { rerender } = render(<PresenterPanel {...props} nextSlide={deck.slides[0]} />);
    expect(screen.getByText("Next slide")).toBeInTheDocument();
    rerender(<PresenterPanel {...props} nextSlide={undefined} />);
    expect(screen.queryByText("Next slide")).toBeNull();
  });

  it("starts the show with or without presenter view", async () => {
    const user = userEvent.setup();
    const onStart = vi.fn();
    render(<ViewTab view="normal" onView={vi.fn()} onStart={onStart} />);
    await user.click(screen.getByRole("button", { name: "Start show" }));
    await user.click(screen.getByRole("button", { name: "From beginning" }));
    expect(onStart.mock.calls).toEqual([[false], [true]]);
  });
});
