import { fireEvent, render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ColumnHeaders, RowHeaders } from "./GridHeaders";

describe("Calc grid headers (3.3g)", () => {
  it("marks the active column, a hidden neighbour and pinned columns, and opens the menu", () => {
    const onMenu = vi.fn();
    const { container } = render(
      <ColumnHeaders
        columns={[
          { col: 0, x: 0 },
          { col: 2, x: 64 },
        ]}
        activeCol={2}
        colWidths={{ "1": 0 }}
        frozenCols={1}
        scrollX={0}
        offsetTop={0}
        onMenu={onMenu}
      />,
    );
    const a = container.querySelector<HTMLElement>('[data-col-header="0"]')!;
    const c = container.querySelector<HTMLElement>('[data-col-header="2"]')!;
    expect(a.textContent).toBe("A");
    expect(a.style.zIndex).toBe("1");
    expect(c.className).toContain("is-active");
    expect(c.className).toContain("has-hidden-before");
    expect(c.style.zIndex).toBe("");
    expect(container.querySelector('[data-col-resize="2"]')).not.toBeNull();
    fireEvent.contextMenu(c);
    expect(onMenu).toHaveBeenCalledWith(2, expect.anything());
  });

  it("numbers rows from one at their layout offsets and opens the menu", () => {
    const onMenu = vi.fn();
    const layout = {
      offsetOf: (row: number) => row * 20,
      heightOf: () => 20,
      isHidden: (row: number) => row === 3,
    };
    const { container } = render(
      <RowHeaders
        rows={[0, 4]}
        activeRow={0}
        layout={layout}
        frozenRows={0}
        scrollY={0}
        offsetLeft={0}
        onMenu={onMenu}
      />,
    );
    const first = container.querySelector<HTMLElement>('[data-row-header="0"]')!;
    const fifth = container.querySelector<HTMLElement>('[data-row-header="4"]')!;
    expect(first.textContent).toBe("1");
    expect(first.className).toContain("is-active");
    expect(fifth.textContent).toBe("5");
    expect(fifth.style.top).toBe("80px");
    expect(fifth.className).toContain("has-hidden-before");
    fireEvent.contextMenu(fifth);
    expect(onMenu).toHaveBeenCalledWith(4, expect.anything());
  });
});
