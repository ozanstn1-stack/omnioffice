import { createEvent, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { newSlideObject } from "../../lib/office-types";
import { ObjectGeometry, TextFormatTools } from "./ObjectProperties";
import type { TextSelectionInfo } from "./TextFrameEditor";

describe("Impress object properties (3.3e)", () => {
  it("routes position, size and the other fields to their handlers", () => {
    const onPosition = vi.fn();
    const onSize = vi.fn();
    const onPatch = vi.fn();
    render(
      <ObjectGeometry
        object={newSlideObject("rect", 10, 20, 100, 50)}
        onPosition={onPosition}
        onSize={onSize}
        onPatch={onPatch}
      />,
    );
    fireEvent.change(screen.getByTitle("X"), { target: { value: "40" } });
    expect(onPosition).toHaveBeenCalledWith("x", 40);
    fireEvent.change(screen.getByTitle("H"), { target: { value: "75" } });
    expect(onSize).toHaveBeenCalledWith("h", 75);
    fireEvent.change(screen.getByTitle("Rotation"), { target: { value: "30" } });
    expect(onPatch).toHaveBeenCalledWith({ rotation: 30 });
  });

  it("formats through the editing handle and keeps the focus in the frame", async () => {
    const user = userEvent.setup();
    const handle = { applyFormat: vi.fn(), toggleBullet: vi.fn(), changeLevel: vi.fn() };
    const selection: TextSelectionInfo = {
      paragraph: 0,
      from: 0,
      to: 3,
      bold: true,
      italic: false,
      underline: false,
      color: null,
      sizePt: null,
      bullet: false,
      level: 0,
    };
    render(<TextFormatTools selection={selection} fallbackSizePt={24} handle={() => handle} colors={["#DC2626"]} />);

    const bold = screen.getByTitle("Bold");
    expect(bold).toHaveAttribute("aria-pressed", "true");
    await user.click(bold);
    expect(handle.applyFormat).toHaveBeenCalledWith({ bold: false });
    await user.click(screen.getByTitle("Increase level"));
    expect(handle.changeLevel).toHaveBeenCalledWith(1);
    expect(screen.getByTitle("Font size")).toHaveValue(24);

    const swatch = screen.getByRole("button", { name: "#DC2626" });
    const mouseDown = createEvent.mouseDown(swatch);
    fireEvent(swatch, mouseDown);
    expect(mouseDown.defaultPrevented).toBe(true);
    await user.click(swatch);
    expect(handle.applyFormat).toHaveBeenLastCalledWith({ color: "#DC2626" });
  });
});
