import { useState } from "react";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));

import { NARROW_RIBBON_QUERY, Ribbon } from "./office-ui";

/**
 * Narrow ribbons: jsdom has no layout, so each group reports the row it would
 * wrap onto through a data attribute (offsetTop) and a fixed height.
 */
const originalMatchMedia = window.matchMedia;
const offsetTop = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "offsetTop");
const offsetHeight = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "offsetHeight");

function setNarrow(narrow: boolean) {
  window.matchMedia = ((query: string) => ({
    matches: narrow && query === NARROW_RIBBON_QUERY,
    media: query,
    onchange: null,
    addListener: () => undefined,
    removeListener: () => undefined,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
    dispatchEvent: () => false,
  })) as typeof window.matchMedia;
}

function Harness({ rows }: { rows: number[] }) {
  const [active, setActive] = useState("home");
  return (
    <Ribbon
      tabs={[
        { id: "home", label: "Home" },
        { id: "insert", label: "Insert" },
      ]}
      active={active}
      onSelect={setActive}
    >
      {rows.map((row, index) => (
        <div key={index} data-top={row * 60}>
          group {index}
        </div>
      ))}
    </Ribbon>
  );
}

describe("ribbon on narrow screens", () => {
  beforeEach(() => {
    Object.defineProperty(HTMLElement.prototype, "offsetTop", {
      configurable: true,
      get(this: HTMLElement) {
        return Number(this.dataset.top ?? 0);
      },
    });
    Object.defineProperty(HTMLElement.prototype, "offsetHeight", {
      configurable: true,
      get: () => 56,
    });
  });

  afterEach(() => {
    window.matchMedia = originalMatchMedia;
    if (offsetTop) Object.defineProperty(HTMLElement.prototype, "offsetTop", offsetTop);
    if (offsetHeight) Object.defineProperty(HTMLElement.prototype, "offsetHeight", offsetHeight);
  });

  it("shows the first row and folds the rest behind More", async () => {
    setNarrow(true);
    const user = userEvent.setup();
    const { container } = render(<Harness rows={[0, 0, 1, 2]} />);
    const body = container.querySelector<HTMLElement>(".ribbon-body")!;
    const more = screen.getByRole("button", { name: "More" });
    expect(more).toHaveAttribute("aria-expanded", "false");
    expect(body.style.maxHeight).toBe("56px");

    await user.click(more);
    expect(screen.getByRole("button", { name: "Less" })).toHaveAttribute("aria-expanded", "true");
    expect(body.style.maxHeight).toBe("");

    // Another tab starts folded again.
    await user.click(screen.getByRole("button", { name: "Insert" }));
    expect(screen.getByRole("button", { name: "More" })).toHaveAttribute("aria-expanded", "false");
    expect(body.style.maxHeight).toBe("56px");
  });

  it("adds nothing when the groups fit on one row or the window is wide", () => {
    setNarrow(true);
    const { unmount } = render(<Harness rows={[0, 0, 0]} />);
    expect(screen.queryByRole("button", { name: "More" })).toBeNull();
    unmount();

    setNarrow(false);
    const { container } = render(<Harness rows={[0, 1, 2]} />);
    expect(screen.queryByRole("button", { name: "More" })).toBeNull();
    expect(container.querySelector(".ribbon")).not.toHaveClass("is-narrow");
  });
});
