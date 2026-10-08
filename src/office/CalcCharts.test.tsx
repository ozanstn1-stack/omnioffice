import { act, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null), save: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-fs", () => ({ readFile: vi.fn(async () => new Uint8Array()) }));

import { useOfficeTabs } from "../lib/office-store";
import { useToasts } from "../lib/store";
import type { ChartData, Workbook } from "../lib/office-types";
import { Harness, seedWorkbook, selectRange, setModel, workbookOf } from "./calc/ui/testing";

const DATA = {
  A1: "x",
  B1: "y",
  C1: "z",
  A2: "1",
  B2: "4",
  C2: "2",
  A3: "2",
  B3: "9",
  C3: "3",
  A4: "3",
  B4: "16",
  C4: "5",
};

async function insertChart(user: ReturnType<typeof userEvent.setup>, kind: string, range = "A1:B4") {
  selectRange(range);
  await user.click(screen.getByRole("button", { name: "Insert" }));
  await user.click(screen.getByRole("button", { name: "Chart" }));
  const dialog = screen.getByRole("dialog", { name: "Chart" });
  await user.click(within(dialog).getByRole("radio", { name: kind }));
  return dialog;
}

const chartSvg = () => document.querySelector<SVGSVGElement>("svg[data-chart-kind]")!;
const firstChart = (): ChartData => workbookOf().sheets[0].charts[0].chart;

describe("Calc chart dialog and renderer", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
    useToasts.setState({ toasts: [] });
  });

  it("offers every chart kind, scatter and doughnut included", async () => {
    const user = userEvent.setup();
    render(<Harness id={seedWorkbook(DATA)} />);
    const dialog = await insertChart(user, "Column");
    const names = within(dialog)
      .getAllByRole("radio")
      .map((radio) => radio.textContent);
    expect(names).toEqual(["Column", "Bar", "Line", "Pie", "Doughnut", "Area", "Scatter (XY)"]);
    expect(within(dialog).getByRole("radio", { name: "Column" })).toBeChecked();
  });

  it("inserts a scatter chart that reads X from the first column, with the chosen style", async () => {
    const user = userEvent.setup();
    render(<Harness id={seedWorkbook(DATA)} />);
    const dialog = await insertChart(user, "Scatter (XY)", "A1:C4");
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Points and lines" }), "lineMarker");
    await user.click(within(dialog).getByRole("button", { name: "Insert chart" }));

    expect(firstChart()).toMatchObject({
      kind: "scatter",
      categories: "A2:A4",
      scatterStyle: "lineMarker",
    });
    expect(firstChart().series.map((series) => series.range)).toEqual(["B2:B4", "C2:C4"]);

    const svg = chartSvg();
    expect(svg).toHaveAttribute("data-chart-kind", "scatter");
    expect(svg.querySelectorAll("circle")).toHaveLength(6);
    expect(svg.querySelectorAll("polyline")).toHaveLength(2);
    // The axes carry tick labels.
    expect(svg.textContent).toContain("20");
  });

  it("draws markers only by default and smooth curves for a smooth style", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook(DATA);
    render(<Harness id={id} />);
    const dialog = await insertChart(user, "Scatter (XY)");
    await user.click(within(dialog).getByRole("button", { name: "Insert chart" }));
    expect(firstChart().scatterStyle).toBeNull();
    expect(chartSvg().querySelectorAll("circle")).toHaveLength(3);
    expect(chartSvg().querySelectorAll("polyline, path")).toHaveLength(0);

    const model = workbookOf();
    act(() =>
      setModel(id, {
        ...model,
        sheets: model.sheets.map((sheet) => ({
          ...sheet,
          charts: sheet.charts.map((placed) => ({ ...placed, chart: { ...placed.chart, scatterStyle: "smooth" } })),
        })),
      } as Workbook),
    );
    expect(chartSvg().querySelectorAll("circle")).toHaveLength(0);
    expect(chartSvg().querySelectorAll("path")).toHaveLength(1);
  });

  it("puts text X values at 1, 2, 3 in a scatter chart", async () => {
    const user = userEvent.setup();
    render(<Harness id={seedWorkbook({ A1: "k", B1: "v", A2: "a", B2: "5", A3: "b", B3: "6" })} />);
    const dialog = await insertChart(user, "Scatter (XY)", "A1:B3");
    await user.click(within(dialog).getByRole("button", { name: "Insert chart" }));
    const circles = Array.from(chartSvg().querySelectorAll("circle"));
    expect(circles).toHaveLength(2);
    expect(Number(circles[0].getAttribute("cx"))).toBeLessThan(Number(circles[1].getAttribute("cx")));
  });

  it("inserts a doughnut with a 50 percent hole and renders one ring slice per value", async () => {
    const user = userEvent.setup();
    render(<Harness id={seedWorkbook(DATA)} />);
    const dialog = await insertChart(user, "Doughnut");
    expect(within(dialog).getByRole("slider")).toHaveValue("50");
    await user.click(within(dialog).getByRole("button", { name: "Insert chart" }));
    expect(firstChart()).toMatchObject({ kind: "doughnut", holeSize: 50 });
    const slices = chartSvg().querySelectorAll("path");
    expect(slices).toHaveLength(3);
    // Each slice goes back along an inner arc: that is the hole.
    expect(slices[0].getAttribute("d")).toMatch(/A \d+(\.\d+)? \d+(\.\d+)? 0 0 0/);
    // The legend names the categories, not the series.
    const legend = document.querySelector(".chart-legend")!;
    expect(legend.textContent).toContain("1");
    expect(legend.textContent).not.toContain("y");
  });

  it("stores the chosen hole size and draws a bigger hole for a bigger percentage", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook(DATA);
    render(<Harness id={id} />);
    const dialog = await insertChart(user, "Doughnut");
    fireEvent.change(within(dialog).getByRole("slider"), { target: { value: "75" } });
    await user.click(within(dialog).getByRole("button", { name: "Insert chart" }));
    expect(firstChart().holeSize).toBe(75);
    const innerRadius = (path: string) => Number(/A (\d+(?:\.\d+)?) \d+(?:\.\d+)? 0 \d 0/.exec(path)![1]);
    const wide = innerRadius(chartSvg().querySelector("path")!.getAttribute("d")!);

    const model = workbookOf();
    act(() =>
      setModel(id, {
        ...model,
        sheets: model.sheets.map((sheet) => ({
          ...sheet,
          charts: sheet.charts.map((placed) => ({ ...placed, chart: { ...placed.chart, holeSize: 25 } })),
        })),
      } as Workbook),
    );
    expect(innerRadius(chartSvg().querySelector("path")!.getAttribute("d")!)).toBeLessThan(wide);
  });

  it("nests the rings of a doughnut with several series", async () => {
    const user = userEvent.setup();
    render(<Harness id={seedWorkbook(DATA)} />);
    const dialog = await insertChart(user, "Doughnut", "A1:C4");
    await user.click(within(dialog).getByRole("button", { name: "Insert chart" }));
    expect(chartSvg().querySelectorAll("path")).toHaveLength(6);
  });

  it("still draws a pie as whole wedges and the other kinds as before", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook(DATA);
    render(<Harness id={id} />);
    const dialog = await insertChart(user, "Pie");
    await user.click(within(dialog).getByRole("button", { name: "Insert chart" }));
    expect(firstChart().holeSize).toBeUndefined();
    const paths = chartSvg().querySelectorAll("path");
    expect(paths).toHaveLength(3);
    expect(paths[0].getAttribute("d")).toMatch(/^M [\d.]+ [\d.]+ L /);

    for (const [kind, selector, count] of [
      ["column", "rect", 3],
      ["bar", "rect", 3],
      ["line", "polyline", 1],
      ["area", "polygon", 1],
    ] as const) {
      const model = workbookOf();
      act(() =>
        setModel(id, {
          ...model,
          sheets: model.sheets.map((sheet) => ({
            ...sheet,
            charts: sheet.charts.map((placed) => ({ ...placed, chart: { ...placed.chart, kind } })),
          })),
        } as Workbook),
      );
      expect(chartSvg().querySelectorAll(selector), kind).toHaveLength(count);
    }
  });

  it("labels the slices with their percentage when labels are on", async () => {
    const user = userEvent.setup();
    const id = seedWorkbook({ A1: "k", B1: "v", A2: "a", B2: "1", A3: "b", B3: "3" });
    render(<Harness id={id} />);
    const dialog = await insertChart(user, "Doughnut");
    await user.click(within(dialog).getByRole("button", { name: "Insert chart" }));
    expect(chartSvg().textContent).not.toContain("%");
    const model = workbookOf();
    act(() =>
      setModel(id, {
        ...model,
        sheets: model.sheets.map((sheet) => ({
          ...sheet,
          charts: sheet.charts.map((placed) => ({ ...placed, chart: { ...placed.chart, showLabels: true } })),
        })),
      } as Workbook),
    );
    expect(chartSvg().textContent).toContain("25%");
    expect(chartSvg().textContent).toContain("75%");
  });

  it("asks for a header row and data before it inserts anything", async () => {
    const user = userEvent.setup();
    render(<Harness id={seedWorkbook(DATA)} />);
    const dialog = await insertChart(user, "Scatter (XY)", "A1:B1");
    await user.click(within(dialog).getByRole("button", { name: "Insert chart" }));
    expect(workbookOf().sheets[0].charts).toHaveLength(0);
    expect(useToasts.getState().toasts.map((toast) => toast.title)).toContain(
      "Select a range with a header row and at least one data row.",
    );
  });
});
