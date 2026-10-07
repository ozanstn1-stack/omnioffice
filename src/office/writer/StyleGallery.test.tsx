import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { newTextDocument } from "../../lib/office-types";
import { GALLERY_STYLES, previewSizePx, StyleGallery } from "./StyleGallery";

/**
 * The quick style gallery previews each style in its own (scaled) look, marks
 * the style of the paragraph with the caret and applies a style without
 * taking focus away from that paragraph.
 */
describe("quick style gallery", () => {
  it("renders one preview button per everyday style with the style's look", () => {
    render(<StyleGallery document={newTextDocument("Doc")} active="Normal" onApply={() => undefined} />);
    const names = ["Normal", "Title", "Heading 1", "Heading 2", "Heading 3", "Quote"];
    expect(screen.getAllByRole("button").map((button) => button.textContent)).toEqual(names);

    const normal = screen.getByRole("button", { name: "Normal" });
    const title = screen.getByRole("button", { name: "Title" });
    const quote = screen.getByRole("button", { name: "Quote" });
    // Scaled down, but still ordered by the real point sizes (Title 28pt > Heading 1 20pt > Normal 11pt).
    expect(parseFloat(title.style.fontSize)).toBeGreaterThan(
      parseFloat(screen.getByRole("button", { name: "Heading 1" }).style.fontSize),
    );
    expect(parseFloat(title.style.fontSize)).toBeLessThanOrEqual(18);
    expect(title.style.fontWeight).toBe("700");
    expect(normal.style.fontWeight).toBe("400");
    expect(quote.style.fontStyle).toBe("italic");
    expect(normal.style.fontStyle).toBe("normal");
    expect(previewSizePx(11)).toBeLessThan(previewSizePx(13));
  });

  it("marks the active style and applies the clicked one", async () => {
    const user = userEvent.setup();
    const onApply = vi.fn();
    render(<StyleGallery document={newTextDocument("Doc")} active="Heading2" onApply={onApply} />);
    expect(screen.getByRole("button", { name: "Heading 2" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "Normal" })).toHaveAttribute("aria-pressed", "false");

    await user.click(screen.getByRole("button", { name: "Quote" }));
    expect(onApply).toHaveBeenCalledWith("Quote");
  });

  it("does not steal focus from the paragraph being styled", () => {
    render(<StyleGallery document={newTextDocument("Doc")} active="Normal" onApply={() => undefined} />);
    const button = screen.getByRole("button", { name: "Title" });
    // A cancelled mousedown is what keeps the caret in the paragraph.
    expect(fireEvent.mouseDown(button)).toBe(false);
  });

  it("only offers styles the document defines", () => {
    const document = newTextDocument("Doc");
    document.styles = document.styles.filter((style) => style.id !== "Quote" && style.id !== "Title");
    render(<StyleGallery document={document} active="Normal" onApply={() => undefined} />);
    expect(screen.getAllByRole("button")).toHaveLength(GALLERY_STYLES.length - 2);
    expect(screen.queryByRole("button", { name: "Quote" })).toBeNull();
  });
});
