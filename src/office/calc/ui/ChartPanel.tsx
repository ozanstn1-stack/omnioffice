/** The chart overlay drawn over the sheet; the dialogs live in ChartDialogs.tsx. */
import { useMemo, useState, type PointerEvent } from "react";
import { Pencil, Trash2 } from "lucide-react";
import { useT } from "../../../lib/i18n";
import type { ChartData, Sheet, Workbook } from "../../../lib/office-types";
import { computeSheetValues } from "../cells";
import { addressesInRange } from "../formula";
import { CartesianPlot, PALETTE, RingPlot, ScatterPlot, colorOf, type Plot, type Series } from "./chart-plots";

/**
 * Height of the title bar above the drawing and of the legend row below it,
 * until they are measured (the buttons grow to touch size on a coarse pointer).
 */
const HEAD_HEIGHT = 26;
const LEGEND_HEIGHT = 22;
/** The box's top and bottom border, inside its height. */
const BORDERS = 2;

export function ChartBox({
  chart,
  sheet,
  workbook,
  x,
  y,
  onRemove,
  onEdit,
  onMoveStart,
  onResizeStart,
}: {
  chart: { id: string; chart: ChartData; anchor: string; widthPx: number; heightPx: number };
  sheet: Sheet;
  workbook: Workbook;
  x: number;
  y: number;
  onRemove: () => void;
  onEdit?: () => void;
  /** Pointer down on the title bar (not on its buttons) starts a move. */
  onMoveStart?: (event: PointerEvent) => void;
  /** Pointer down on the bottom-right handle starts a resize. */
  onResizeStart?: (event: PointerEvent) => void;
}) {
  const t = useT();
  const [headHeight, setHeadHeight] = useState(HEAD_HEIGHT);
  const [legendHeight, setLegendHeight] = useState(LEGEND_HEIGHT);
  const values = useMemo(() => computeSheetValues(workbook, sheet), [workbook, sheet]);
  const data = chart.chart;
  const categoryCells = addressesInRange(data.categories, 5000).map((address) => values.get(address) ?? "");
  const categories = categoryCells.map(String);
  const series: Series[] = data.series.map((entry) => ({
    name: entry.name,
    values: addressesInRange(entry.range, 5000).map((address) => Number(values.get(address) ?? 0)),
    color: entry.color,
  }));
  const width = chart.widthPx;
  const height = chart.heightPx;
  const svgHeight = Math.max(20, height - BORDERS - headHeight - (data.legend ? legendHeight : 0));
  const plot: Plot = { left: 40, top: 10, width: width - 48, height: Math.max(20, svgHeight - 10 - 22) };
  const ring = data.kind === "pie" || data.kind === "doughnut";

  return (
    <div className="chart-box" style={{ left: x, top: y, width, height }}>
      <div
        className="chart-head"
        ref={(node) => {
          if (node && node.offsetHeight > 0 && node.offsetHeight !== headHeight) setHeadHeight(node.offsetHeight);
        }}
        onPointerDown={(event) => {
          if (!(event.target as Element).closest("button")) onMoveStart?.(event);
        }}
      >
        <strong>{data.title}</strong>
        {onEdit ? (
          <button type="button" className="icon-btn" onClick={onEdit} title={t("calc.chartEdit")}>
            <Pencil size={12} />
          </button>
        ) : null}
        <button type="button" className="icon-btn" onClick={onRemove} title={t("calc.chartDelete")}>
          <Trash2 size={12} />
        </button>
      </div>
      <svg
        width={width}
        height={svgHeight}
        viewBox={`0 0 ${width} ${svgHeight}`}
        role="img"
        aria-label={`${t(`calc.chart_${data.kind}`)}: ${data.title}`}
        data-chart-kind={data.kind}
      >
        {data.kind === "scatter" ? (
          <ScatterPlot chart={data} plot={plot} categories={categoryCells} series={series} />
        ) : ring ? (
          <RingPlot chart={data} width={width} height={svgHeight} series={series} />
        ) : (
          <CartesianPlot kind={data.kind} plot={plot} categories={categories} series={series} />
        )}
      </svg>
      {data.legend ? (
        <div
          className="chart-legend"
          ref={(node) => {
            if (node && node.offsetHeight > 0 && node.offsetHeight !== legendHeight) setLegendHeight(node.offsetHeight);
          }}
        >
          {(ring
            ? categories.map((name, index) => ({ name, color: PALETTE[index % PALETTE.length] }))
            : series.map((entry, index) => ({ name: entry.name, color: colorOf(entry, index) }))
          ).map((entry, index) => (
            <span key={index}>
              <i style={{ background: entry.color }} />
              {entry.name}
            </span>
          ))}
        </div>
      ) : null}
      {onResizeStart ? <span className="chart-resize" aria-hidden="true" onPointerDown={onResizeStart} /> : null}
    </div>
  );
}
