import { fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null), save: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-fs", () => ({ readFile: vi.fn(async () => new Uint8Array()) }));

import { useOfficeTabs, type OfficeTab } from "../../lib/office-store";
import type { Block, ChartData, TextDocument } from "../../lib/office-types";
import { newTextDocument } from "../../lib/office-types";
import { ChartDialog } from "../charts/ChartDialog";
import { defaultWriterChart } from "../charts/chart-helpers";
import { WriterEditor } from "../WriterEditor";
import { WriterChartBlock, type ChartBlock } from "./WriterChartBlock";

function Harness({ id }: { id: string }) {
  const tab = useOfficeTabs((state) => state.tabs.find((candidate) => candidate.id === id));
  if (!tab) return null;
  return <WriterEditor tab={tab as OfficeTab & { model: TextDocument }} />;
}

function sampleChartBlock(overrides: Partial<ChartBlock> = {}): ChartBlock {
  return {
    type: "chart",
    chart: defaultWriterChart("Revenue", "Q", "Sales"),
    widthPt: 420,
    heightPt: 260,
    align: "center",
    caption: "Figure 1",
    ...overrides,
  };
}

describe("Writer chart UI and shared ChartDialog", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
  });

  it("renders WriterChartBlock toolbar on selection and updates alignment, caption, edit, and delete", () => {
    const onSelect = vi.fn();
    const onUpdate = vi.fn<(block: Block) => void>();
    const onEditData = vi.fn();
    const onDelete = vi.fn();
    const block = sampleChartBlock();

    const { rerender } = render(
      <WriterChartBlock
        block={block}
        index={2}
        zoom={1}
        selected={false}
        onSelect={onSelect}
        onUpdate={onUpdate}
        onEditData={onEditData}
        onDelete={onDelete}
      />,
    );

    fireEvent.click(screen.getByRole("button"));
    expect(onSelect).toHaveBeenCalledWith(2);

    rerender(
      <WriterChartBlock
        block={block}
        index={2}
        zoom={1}
        selected={true}
        onSelect={onSelect}
        onUpdate={onUpdate}
        onEditData={onEditData}
        onDelete={onDelete}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Right" }));
    expect(onUpdate).toHaveBeenLastCalledWith({ ...block, align: "right" });

    fireEvent.change(screen.getByLabelText("Caption"), { target: { value: "Updated cap" } });
    expect(onUpdate).toHaveBeenLastCalledWith({ ...block, caption: "Updated cap" });

    fireEvent.click(screen.getByRole("button", { name: "Edit data" }));
    expect(onEditData).toHaveBeenCalledWith(2);

    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    expect(onDelete).toHaveBeenCalledWith(2);
  });

  it("supports all 7 chart kinds, doughnut holeSize, scatter style, and clipboard paste in ChartDialog", () => {
    const onSave = vi.fn<(chart: ChartData) => void>();
    const chart = defaultWriterChart("Sales", "Cat ", "Rev");
    render(<ChartDialog chart={chart} title="Chart" onClose={vi.fn()} onSave={onSave} />);

    const dialog = screen.getByRole("dialog");
    const firstCatCell = dialog.querySelector<HTMLInputElement>('[data-cell="0:0"]')!;
    fireEvent.focus(firstCatCell);
    const grid = firstCatCell.closest("div")!;
    fireEvent.paste(grid, {
      clipboardData: {
        getData: () => "Jan\t10\nFeb\t25",
      },
    });
    expect(dialog.querySelector<HTMLInputElement>('[data-cell="0:0"]')!.value).toBe("Jan");

    fireEvent.click(within(dialog).getByRole("button", { name: "Chart" }));
    const selects = within(dialog).getAllByRole("combobox") as HTMLSelectElement[];
    const kindSelect = selects[0];
    expect(Array.from(kindSelect.options).map((opt) => opt.value)).toEqual([
      "column",
      "bar",
      "line",
      "area",
      "pie",
      "doughnut",
      "scatter",
    ]);

    fireEvent.change(kindSelect, { target: { value: "doughnut" } });
    const holeInput = within(dialog).getByLabelText(/Doughnut hole/i) as HTMLInputElement;
    fireEvent.change(holeInput, { target: { value: "65" } });

    fireEvent.change(kindSelect, { target: { value: "scatter" } });
    const scatterSelect = within(dialog).getAllByRole("combobox")[1] as HTMLSelectElement;
    fireEvent.change(scatterSelect, { target: { value: "smoothMarker" } });

    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    expect(onSave).toHaveBeenCalledTimes(1);
    const saved = onSave.mock.calls[0][0];
    expect(saved.kind).toBe("scatter");
    expect(saved.holeSize).toBe(65);
    expect(saved.scatterStyle).toBe("smoothMarker");
    expect(saved.categoriesCache?.slice(0, 2)).toEqual(["Jan", "Feb"]);
  });

  it("inserts a chart block from the Writer Insert ribbon and saves modal data", () => {
    const doc = newTextDocument("Chart Doc");
    const id = useOfficeTabs.getState().create("writer", "Chart Doc", doc);
    render(<Harness id={id} />);

    fireEvent.click(screen.getByRole("button", { name: "Insert" }));
    fireEvent.click(screen.getByRole("button", { name: "Chart" }));

    const dialog = screen.getByRole("dialog");
    const titleInput = within(dialog).queryByDisplayValue("Chart Title");
    if (titleInput) fireEvent.change(titleInput, { target: { value: "Quarterly KPI" } });

    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    const model = useOfficeTabs.getState().tabs[0].model as TextDocument;
    const inserted = model.blocks.find((block): block is ChartBlock => block.type === "chart");
    expect(inserted).toBeDefined();
    expect(inserted?.chart.kind).toBe("column");
  });
});
