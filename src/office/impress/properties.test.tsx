import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { newAnimation, newDeck, newSlideMaster, newSlideObject } from "../../lib/office-types";
import { SlideNotesAndAnimations, SlideSettings } from "./SlideProperties";

describe("Impress slide properties (3.3d)", () => {
  it("lists the given master's layouts and toggles the hidden flag", () => {
    const onHidden = vi.fn();
    const master = newSlideMaster("Corporate");
    const slide = newDeck("Deck").slides[0];
    render(
      <SlideSettings
        slide={slide}
        masters={[master]}
        layouts={[{ id: "l1", name: "Title only", kind: "title", objects: [] }]}
        onMaster={vi.fn()}
        onLayout={vi.fn()}
        onHidden={onHidden}
      />,
    );
    expect(screen.getByRole("option", { name: "Title only" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "Corporate" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("checkbox"));
    expect(onHidden).toHaveBeenCalledWith(true);
  });

  it("orders animations, guards the ends, deletes by id and needs an object to add", async () => {
    const user = userEvent.setup();
    const onDelete = vi.fn();
    const base = newDeck("Deck").slides[0];
    const object = { ...newSlideObject("text", 0, 0, 10, 10), name: "Box" };
    const second = { ...newAnimation(object.id), id: "a2", order: 2, effect: "zoom" };
    const first = { ...newAnimation(object.id), id: "a1", order: 1, effect: "fade" };
    const props = { onNotes: vi.fn(), onReorder: vi.fn(), onEdit: vi.fn(), onDelete, onAdd: vi.fn() };
    const { rerender } = render(
      <SlideNotesAndAnimations slide={{ ...base, objects: [object], animations: [second, first] }} {...props} />,
    );
    const rows = screen.getAllByText(/^Box · /);
    expect(rows.map((row) => row.textContent)).toEqual([
      expect.stringContaining("fade"),
      expect.stringContaining("zoom"),
    ]);
    const ups = screen.getAllByTitle("Move up");
    expect(ups[0]).toBeDisabled();
    expect(ups[1]).toBeEnabled();
    await user.click(screen.getAllByTitle("Delete")[1]);
    expect(onDelete).toHaveBeenCalledWith("a2");
    expect(screen.getByRole("button", { name: "Add animation" })).toBeEnabled();

    rerender(<SlideNotesAndAnimations slide={{ ...base, objects: [], animations: [] }} {...props} />);
    expect(screen.getByRole("button", { name: "Add animation" })).toBeDisabled();
  });
});
