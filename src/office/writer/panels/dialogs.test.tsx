import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { defaultPageSetup, defaultSectionProps } from "../../../lib/office-types";
import { CrossReferenceDialog } from "./CrossReferenceDialog";
import { SectionsDialog } from "./SectionsDialog";

describe("Writer dialogs (3.3b)", () => {
  it("inserts a cross-reference only once a target is chosen, then closes", async () => {
    const user = userEvent.setup();
    const onInsert = vi.fn();
    const onClose = vi.fn();
    const { rerender } = render(
      <CrossReferenceDialog
        kind="refPage"
        onKind={vi.fn()}
        target=""
        onTarget={vi.fn()}
        bookmarks={[{ id: "b1", name: "intro", block: 0, offset: 0 }]}
        bookmarkName=""
        onBookmarkName={vi.fn()}
        onInsert={onInsert}
        onAddBookmark={vi.fn()}
        onClose={onClose}
      />,
    );
    await user.click(screen.getByRole("button", { name: "Insert" }));
    expect(onInsert).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();

    rerender(
      <CrossReferenceDialog
        kind="refPage"
        onKind={vi.fn()}
        target="intro"
        onTarget={vi.fn()}
        bookmarks={[{ id: "b1", name: "intro", block: 0, offset: 0 }]}
        bookmarkName=""
        onBookmarkName={vi.fn()}
        onInsert={onInsert}
        onAddBookmark={vi.fn()}
        onClose={onClose}
      />,
    );
    await user.click(screen.getByRole("button", { name: "Insert" }));
    expect(onInsert).toHaveBeenCalledWith("refPage", "intro");
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("swaps the page for landscape and leaves the last section's start alone", () => {
    const onUpdate = vi.fn();
    const portrait = defaultPageSetup("a4", "portrait");
    const sections = [defaultSectionProps(portrait), defaultSectionProps(portrait)];
    render(<SectionsDialog sections={sections} onUpdate={onUpdate} onRemoveBreak={vi.fn()} onClose={vi.fn()} />);

    const [firstOrientation] = screen.getAllByDisplayValue("Portrait");
    fireEvent.change(firstOrientation, { target: { value: "landscape" } });
    const page = onUpdate.mock.calls[0][1].page;
    expect([page.orientation, page.widthPt, page.heightPt]).toEqual(["landscape", portrait.heightPt, portrait.widthPt]);

    const starts = screen.getAllByDisplayValue("New page");
    fireEvent.change(starts[1], { target: { value: "continuous" } });
    expect(onUpdate).toHaveBeenCalledTimes(1);
    fireEvent.change(starts[0], { target: { value: "continuous" } });
    expect(onUpdate).toHaveBeenLastCalledWith(0, { start: "continuous" });
  });
});
