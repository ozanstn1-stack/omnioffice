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

describe("Calc chart properties (4.1a, 4.1c)", () => {
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

  it("saves stacking, value labels and axis titles", () => {
    const onSave = vi.fn();
    render(<ChartProperties chart={{ ...CHART, kind: "column" }} onSave={onSave} onClose={vi.fn()} />);
    fireEvent.click(screen.getByLabelText("Stack the series"));
    fireEvent.click(screen.getByLabelText("Show values"));
    fireEvent.change(screen.getByLabelText("Category axis title"), { target: { value: "Month" } });
    fireEvent.change(screen.getByLabelText("Value axis title"), { target: { value: "Sales" } });
    fireEvent.click(screen.getByRole("button", { name: "Save chart" }));
    expect(onSave.mock.calls[0][0]).toMatchObject({
      stacked: true,
      showLabels: true,
      xTitle: "Month",
      yTitle: "Sales",
    });
  });

  it("offers no stacking or axis titles on a doughnut", () => {
    render(<ChartProperties chart={CHART} onSave={vi.fn()} onClose={vi.fn()} />);
    expect(screen.queryByLabelText("Stack the series")).toBeNull();
    expect(screen.queryByLabelText("Category axis title")).toBeNull();
    expect(screen.getByLabelText("Show values")).toBeTruthy();
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

describe("Calc chart layer (4.1a, 4.1b)", () => {
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
        rowLayout={{ offsetOf: (row) => row * 24, rowAtY: (y) => Math.floor(y / 24) }}
        zoom={1}
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
    expect(onChange).toHaveBeenCalledWith("c1", { chart: expect.objectContaining({ title: "Q1" }) });
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("moves a chart by its title bar to the cell under the drop", () => {
    const onChange = setup();
    // Two columns right (96 px each) and three rows down (24 px each), at zoom 1.
    fireEvent.pointerDown(screen.getByText("Chart"), { button: 0, clientX: 100, clientY: 100 });
    fireEvent.pointerMove(window, { clientX: 100 + 2 * 96, clientY: 100 + 3 * 24 });
    fireEvent.pointerUp(window);
    expect(onChange).toHaveBeenCalledWith("c1", { anchor: "F5" });
  });

  it("resizes a chart by its corner and ignores a click", () => {
    const onChange = setup();
    const handle = document.querySelector(".chart-resize")!;
    fireEvent.pointerDown(handle, { button: 0, clientX: 0, clientY: 0 });
    fireEvent.pointerUp(window);
    expect(onChange).not.toHaveBeenCalled();
    fireEvent.pointerDown(handle, { button: 0, clientX: 0, clientY: 0 });
    fireEvent.pointerMove(window, { clientX: 80, clientY: 40 });
    fireEvent.pointerUp(window);
    expect(onChange).toHaveBeenCalledWith("c1", { widthPx: 500, heightPx: 300 });
  });

  it("deletes a chart", () => {
    const onChange = setup();
    fireEvent.click(screen.getByTitle("Delete chart"));
    expect(onChange).toHaveBeenCalledWith("c1", null);
  });
});
