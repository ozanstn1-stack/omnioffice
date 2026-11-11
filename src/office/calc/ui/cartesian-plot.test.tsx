import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { AXIS_TITLE_ROOM, CartesianPlot, titledPlot } from "./cartesian-plot";

const PLOT = { left: 40, top: 10, width: 200, height: 100 };
const SERIES = [
  { name: "A", values: [2, 3], color: null },
  { name: "B", values: [2, 1], color: null },
];

function draw(props: Partial<Parameters<typeof CartesianPlot>[0]> = {}) {
  const { container } = render(
    <svg>
      <CartesianPlot kind="column" plot={PLOT} categories={["Q1", "Q2"]} series={SERIES} {...props} />
    </svg>,
  );
  return container;
}

describe("Calc cartesian plot (4.1c)", () => {
  it("stacks columns so the tallest stack reaches the top of the plot", () => {
    const rects = [...draw({ stacked: true }).querySelectorAll("rect")];
    // Q1 is 2 + 2 = 4, the largest total: its upper bar ends at the plot top.
    const tops = rects.map((rect) => Number(rect.getAttribute("y")));
    expect(Math.min(...tops)).toBeCloseTo(PLOT.top);
    // One bar per value, stacked bars share the category's full width.
    expect(rects).toHaveLength(4);
    expect(rects[0].getAttribute("width")).toBe(rects[2].getAttribute("width"));
  });

  it("draws value labels only when asked", () => {
    expect(draw().querySelectorAll(".chart-value-label")).toHaveLength(0);
    const labels = [...draw({ showLabels: true }).querySelectorAll(".chart-value-label")].map((n) => n.textContent);
    expect(labels).toEqual(["2", "3", "2", "1"]);
  });

  it("puts the category title below a column chart and left of a bar chart", () => {
    const column = draw({ xTitle: "Quarter", yTitle: "Sales" });
    const titles = [...column.querySelectorAll(".chart-axis-title")];
    expect(titles.map((n) => n.textContent)).toEqual(["Quarter", "Sales"]);
    expect(titles[1].getAttribute("transform")).toContain("rotate(-90)");
    const bar = draw({ kind: "bar", xTitle: "Quarter", yTitle: "Sales" });
    expect(bar.querySelector(".chart-axis-title[transform]")?.textContent).toBe("Quarter");
  });

  it("makes room for the axis titles", () => {
    expect(titledPlot(PLOT, "column", "", "")).toEqual(PLOT);
    expect(titledPlot(PLOT, "column", "Quarter", "Sales")).toEqual({
      left: PLOT.left + AXIS_TITLE_ROOM,
      top: PLOT.top,
      width: PLOT.width - AXIS_TITLE_ROOM,
      height: PLOT.height - AXIS_TITLE_ROOM,
    });
  });
});
