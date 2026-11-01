import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import en from "../lib/i18n/en.json";
import { ScannerPageList, type ScannerPage } from "./scanner-page-list";

// The queue is where a scan is reordered or dropped, and a book spread yields
// two rows from one photo: these tests pin the row identity (the page id, not
// the path), the badges that say how each page was cropped, and the callbacks.
const pages = (overrides: ScannerPage[] = []): ScannerPage[] => [
  // Detected automatically, no manual crop.
  { id: "p1", path: "C:/scan/photo-1.jpg", auto: true },
  // The two halves of one spread, both hand-cropped.
  { id: "p2", path: "C:/scan/photo-1.jpg", half: 0, corners: [[0, 0]] },
  { id: "p3", path: "C:/scan/photo-1.jpg", half: 1, corners: [[0, 0]] },
  ...overrides,
];

function renderList(overrides: Partial<Parameters<typeof ScannerPageList>[0]> = {}) {
  const onFilter = vi.fn();
  const onMove = vi.fn();
  const onRemove = vi.fn();
  const onAdjust = vi.fn();
  const onClear = vi.fn();
  render(
    <ScannerPageList
      pages={pages()}
      busy={false}
      detecting={false}
      filter="colour"
      onFilter={onFilter}
      onMove={onMove}
      onRemove={onRemove}
      onAdjust={onAdjust}
      onClear={onClear}
      {...overrides}
    />,
  );
  return { onFilter, onMove, onRemove, onAdjust, onClear };
}

describe("ScannerPageList", () => {
  it("renders one row per page, with both halves of a spread", () => {
    renderList();
    // Two rows share the photo name; the page id keeps them distinct.
    expect(screen.getAllByText("photo-1.jpg")).toHaveLength(3);
    expect(screen.getAllByText(en["scanner.spreadBadge"])).toHaveLength(2);
    expect(screen.getByText(`· ${en["scanner.spreadLeft"]}`)).toBeInTheDocument();
    expect(screen.getByText(`· ${en["scanner.spreadRight"]}`)).toBeInTheDocument();
  });

  it("marks an auto-detected page and a hand-cropped one differently", () => {
    renderList();
    expect(screen.getByText(en["scanner.autoBadge"])).toBeInTheDocument();
    expect(screen.getAllByText(en["scanner.cropped"])).toHaveLength(2);
  });

  it("reports the filter the user picked", () => {
    const { onFilter } = renderList();
    fireEvent.click(screen.getByRole("button", { name: en["scanner.filterGrey"] }));
    expect(onFilter).toHaveBeenCalledWith("greyscale");
  });

  it("moves, adjusts and removes a page by its id", () => {
    const { onMove, onAdjust, onRemove } = renderList();
    fireEvent.click(screen.getAllByRole("button", { name: en["common.moveDown"] })[0]);
    expect(onMove).toHaveBeenCalledWith(0, 1);
    fireEvent.click(screen.getAllByRole("button", { name: en["scanner.adjust"] })[1]);
    expect(onAdjust).toHaveBeenCalledWith("p2");
    fireEvent.click(screen.getAllByRole("button", { name: en["common.remove"] })[2]);
    expect(onRemove).toHaveBeenCalledWith("p3");
  });

  it("cannot move the first row up or the last row down", () => {
    renderList();
    expect(screen.getAllByRole("button", { name: en["common.moveUp"] })[0]).toBeDisabled();
    const down = screen.getAllByRole("button", { name: en["common.moveDown"] });
    expect(down[down.length - 1]).toBeDisabled();
  });

  it("clears the queue", () => {
    const { onClear } = renderList();
    fireEvent.click(screen.getByRole("button", { name: en["scanner.clearAll"] }));
    expect(onClear).toHaveBeenCalled();
  });

  it("says so when nothing is scanned yet", () => {
    renderList({ pages: [] });
    expect(screen.getByText(en["scanner.empty"])).toBeInTheDocument();
  });

  it("locks every action while a scan or the detector is running", () => {
    renderList({ detecting: true });
    expect(screen.getByRole("button", { name: en["scanner.filterColour"] })).toBeDisabled();
    expect(screen.getAllByRole("button", { name: en["common.remove"] })[0]).toBeDisabled();
  });
});
