import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { newSheet, newWorkbook, type ChartData } from "../../../lib/office-types";
import { ChartLayer } from "./ChartLayer";
import { ChartProperties, withKind } from "./ChartDialogs";

const CHART: ChartData = {
  kind: "doughnut",
  title: "Chart",
  categories: "A2:A4",
  series: [{ name: "Sales", range: "B2:B4", color: null }],
  legend: true,
  xTitle: "",
  yTitle: "",
  stacked: false,
  showLabels: false,
  holeSize: 40,
};

describe("Calc chart properties (4.1a)", () => {
  it("saves the edited title, kind, legend and series", () => {
    const onSave = vi.fn();
    render(<ChartProperties chart={CHART} onSave={onSave} onClose={vi.fn()} />);
    fireEvent.change(screen.getByLabelText("Title"), { target: { value: "Revenue" } });
    fireEvent.click(screen.getByRole("radio", { name: "Column" }));
    fireEvent.click(screen.getByLabelText("Show legend"));
    fireEvent.change(screen.getByLabelText("Series 1 name"), { target: { value: "Net" } });
    fireEvent.change(screen.getByLabelText("Series 1 range"), { target: { value: " C2:C4 " } });
    fireEvent.click(screen.getByRole("button", { name: "Save chart" }));

    const saved: ChartData = onSave.mock.calls[0][0];
    expect(saved).toMatchObject({ kind: "column", title: "Revenue", legend: false, categories: "A2:A4" });
    expect(saved.series).toEqual([{ name: "Net", range: "C2:C4", color: null }]);
    // The hole only belongs to a doughnut.
    expect(saved.holeSize).toBeUndefined();
  });

  it("does not save while a range is not a cell reference", () => {
    const onSave = vi.fn();
    render(<ChartProperties chart={CHART} onSave={onSave} onClose={vi.fn()} />);
    const range = screen.getByLabelText("Series 1 range");
    fireEvent.change(range, { target: { value: "sales" } });
    expect(range.getAttribute("aria-invalid")).toBe("true");
    const save = screen.getByRole("button", { name: "Save chart" });
    expect((save as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(save);
    expect(onSave).not.toHaveBeenCalled();
  });

  it("keeps the kind-only options with their kind", () => {
    const column = withKind(CHART, "column");
    expect(column.holeSize).toBeUndefined();
    expect(withKind(column, "doughnut").holeSize).toBeGreaterThan(0);
    expect(withKind({ ...CHART, kind: "scatter", scatterStyle: "line" }, "line").scatterStyle).toBeUndefined();
  });
});

describe("Calc chart layer (4.1a)", () => {
  function setup() {
    const sheet = {
      ...newSheet("Sheet1"),
      charts: [{ id: "c1", chart: CHART, anchor: "D2", widthPx: 420, heightPx: 260 }],
    };
    const workbook = { ...newWorkbook(), sheets: [sheet] };
    const onChange = vi.fn();
    render(
      <ChartLayer
        sheet={sheet}
        workbook={workbook}
        place={(row, col) => ({ x: col * 10, y: row * 10 })}
        onChange={onChange}
      />,
    );
    return onChange;
  }

  it("edits a chart through its properties dialog", () => {
    const onChange = setup();
    fireEvent.click(screen.getByTitle("Edit chart"));
    fireEvent.change(screen.getByLabelText("Title"), { target: { value: "Q1" } });
    fireEvent.click(screen.getByRole("button", { name: "Save chart" }));
    expect(onChange).toHaveBeenCalledWith("c1", expect.objectContaining({ title: "Q1" }));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("deletes a chart", () => {
    const onChange = setup();
    fireEvent.click(screen.getByTitle("Delete chart"));
    expect(onChange).toHaveBeenCalledWith("c1", null);
  });
});
