import { render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));

import { newParaBlock, type Block, type TableData } from "../../lib/office-types";
import { StaticBlocks } from "./views";
import { TableView } from "./table-view";

// The DOCX/ODT readers build cells from `TableCell::default()`, so a cell
// without gridSpan/vMerge arrives with colspan 0 and rowspan 0. In HTML
// `rowspan="0"` spans every remaining row: the first row's cells covered the
// whole table and the later rows were laid out to their right ("P e n" in a
// sliver column on a phone).
function table(): TableData {
  const cell = (text: string) => ({
    blocks: [newParaBlock("Normal", text)],
    colspan: 0,
    rowspan: 0,
    background: null,
    align: "",
    valign: "",
    widthPt: null,
  });
  return {
    rows: [
      { cells: [cell("Item"), cell("Quantity"), cell("Price")], heightPt: null, header: true },
      { cells: [cell("Pen"), cell("10"), cell("5.50")], heightPt: null, header: false },
    ] as TableData["rows"],
    columnWidthsPt: [153, 153, 153],
    borders: true,
    borderColor: "#000000",
    align: "left",
  };
}

function spans(container: HTMLElement) {
  return Array.from(container.querySelectorAll("td")).map((td) => [td.rowSpan, td.colSpan]);
}

describe("Writer table spans", () => {
  it("renders unmerged cells with a span of one on the page", () => {
    const block: Block = { type: "table", table: table() } as Block;
    const { container } = render(
      <StaticBlocks blocks={[block]} scope="body" page={0} pages={1} zoom={1} onOpen={() => undefined} />,
    );
    expect(container.querySelectorAll("tr")).toHaveLength(2);
    expect(spans(container)).toEqual(Array(6).fill([1, 1]));
  });

  it("renders unmerged cells with a span of one in the editable table", () => {
    const block = { type: "table", table: table() } as Extract<Block, { type: "table" }>;
    const { container } = render(
      <TableView block={block} index={0} zoom={1} onUpdate={() => undefined} onSyncCell={() => undefined} />,
    );
    expect(spans(container)).toEqual(Array(6).fill([1, 1]));
  });
});
