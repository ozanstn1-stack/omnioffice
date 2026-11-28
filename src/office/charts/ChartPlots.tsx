/**
 * Specialized chart sub-plot renderers (pie/doughnut wedges and X/Y scatter plots)
 * shared by ChartPreview across Impress and Writer.
 */
import type { ChartData } from "../../lib/office-types";
import { CHART_PALETTE, formatChartCellValue } from "./chart-helpers";

export function polarPoint(cx: number, cy: number, r: number, frac: number): string {
  const angle = (frac * 360 - 90) * (Math.PI / 180);
  return `${(cx + r * Math.cos(angle)).toFixed(2)},${(cy + r * Math.sin(angle)).toFixed(2)}`;
}

export function PieOrDoughnutPlot({
  values,
  labelOf,
  holePct,
  bgColor,
  borderColor,
  scale,
}: {
  values: number[][];
  labelOf: (row: number) => string;
  holePct: number | null;
  bgColor: string;
  borderColor: string;
  scale: number;
}) {
  const pieIndex = values.findIndex((list) => list.some((value) => value > 0));
  const pieValues = pieIndex >= 0 ? values[pieIndex] : [];
  const pieTotal = pieValues.reduce((sum, value) => sum + Math.max(0, value), 0);
  if (pieTotal <= 0) return null;

  const fractions = pieValues.map((value) => Math.max(0, value) / pieTotal);
  const slices = fractions.map((fraction, row) => {
    const start = fractions.slice(0, row).reduce((sum, value) => sum + value, 0);
    return `${CHART_PALETTE[row % CHART_PALETTE.length]} ${start * 100}% ${(start + fraction) * 100}%`;
  });
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 8 * scale, width: "100%", height: "100%" }}>
      <div
        className="chart-pie-circle"
        style={{ background: `conic-gradient(${slices.join(", ")})`, border: `1px solid ${borderColor}` }}
      >
        {holePct !== null ? (
          <div
            style={{
              width: `${holePct}%`,
              height: `${holePct}%`,
              borderRadius: "50%",
              background: bgColor,
              border: `1px solid ${borderColor}`,
            }}
          />
        ) : null}
      </div>
      <div className="chart-pie-list" style={{ gap: 2 * scale }}>
        {pieValues.map((value, row) => (
          <span key={row} className="chart-pie-row" style={{ gap: 4 * scale }}>
            <i
              style={{
                width: 8 * scale,
                height: 8 * scale,
                background: CHART_PALETTE[row % CHART_PALETTE.length],
                borderRadius: 2,
                flex: "0 0 auto",
              }}
            />
            <span className="chart-ellipsis">{labelOf(row)}</span>
            <span style={{ opacity: 0.7, marginLeft: "auto" }}>{formatChartCellValue(value)}</span>
          </span>
        ))}
      </div>
    </div>
  );
}

export function ScatterPlot({
  chart,
  categories,
  values,
  colorOf,
  accent,
}: {
  chart: ChartData;
  categories: string[];
  values: number[][];
  colorOf: (index: number) => string;
  accent: string;
}) {
  const xsRaw = Array.from({ length: Math.max(categories.length, ...values.map((v) => v.length)) }, (_, row) => {
    const parsed = Number(categories[row]?.trim());
    return Number.isFinite(parsed) && categories[row]?.trim() !== "" ? parsed : row + 1;
  });
  const svgGrid = (xs: number[], ys: number[]) => {
    const [minX, maxX] = [Math.min(...xs, 0), Math.max(...xs, Math.min(...xs, 0) + 1)];
    const [minY, maxY] = [Math.min(...ys, 0), Math.max(...ys, Math.min(...ys, 0) + 1)];
    const spanX = Math.max(1e-9, maxX - minX);
    const spanY = Math.max(1e-9, maxY - minY);
    return {
      mapX: (v: number) => 8 + ((v - minX) / spanX) * 84,
      mapY: (v: number) => 88 - ((v - minY) / spanY) * 76,
    };
  };
  const { mapX, mapY } = svgGrid(xsRaw, values.flat());
  const style = chart.scatterStyle ?? "";
  const showLines = style === "lineMarker" || style === "line" || style === "smoothMarker" || style === "smooth";
  const showMarkers = style !== "line" && style !== "smooth";

  return (
    <svg width="100%" height="100%" viewBox="0 0 100 100" preserveAspectRatio="none" style={{ display: "block" }}>
      <polyline
        points="8,10 8,88 94,88"
        fill="none"
        stroke={accent}
        strokeWidth="0.8"
        opacity="0.5"
        vectorEffect="non-scaling-stroke"
      />
      {chart.series.map((entry, sIdx) => {
        const pts = (values[sIdx] ?? []).map((y, row) => ({
          x: mapX(xsRaw[row] ?? row + 1),
          y: mapY(y),
          label: `${entry.name}: (${xsRaw[row] ?? row + 1}, ${y})`,
        }));
        const poly = pts.map((p) => `${p.x.toFixed(2)},${p.y.toFixed(2)}`).join(" ");
        return (
          <g key={sIdx}>
            {showLines && pts.length > 1 ? (
              <polyline
                points={poly}
                fill="none"
                stroke={colorOf(sIdx)}
                strokeWidth="1.6"
                vectorEffect="non-scaling-stroke"
              />
            ) : null}
            {showMarkers
              ? pts.map((p, idx) => (
                  <circle key={idx} cx={p.x} cy={p.y} r="2.6" fill={colorOf(sIdx)}>
                    <title>{p.label}</title>
                  </circle>
                ))
              : null}
          </g>
        );
      })}
    </svg>
  );
}
