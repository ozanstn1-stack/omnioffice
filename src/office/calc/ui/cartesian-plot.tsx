/** Column, bar, line and area charts: values against categories, side by side or stacked (4.1c). */
import { formatTick } from "../chart-geometry";
import { STACKABLE, spanRange, valueSpans } from "../chart-stack";
import { AXIS_COLOR, LABEL_COLOR, colorOf, type Plot, type Series } from "./chart-plots";

/** Room an axis title takes beside the plot. */
export const AXIS_TITLE_ROOM = 14;

/**
 * The plot with room for its axis titles. The category title (`xTitle`) sits
 * along the category axis: below a column chart, left of a bar chart.
 */
export function titledPlot(plot: Plot, kind: string, xTitle: string, yTitle: string): Plot {
  const horizontal = kind === "bar";
  const left = (horizontal ? xTitle : yTitle) ? AXIS_TITLE_ROOM : 0;
  const below = (horizontal ? yTitle : xTitle) ? AXIS_TITLE_ROOM : 0;
  return { ...plot, left: plot.left + left, width: plot.width - left, height: Math.max(20, plot.height - below) };
}

export function CartesianPlot({
  kind,
  plot,
  categories,
  series,
  stacked = false,
  showLabels = false,
  xTitle = "",
  yTitle = "",
}: {
  kind: string;
  plot: Plot;
  categories: string[];
  series: Series[];
  stacked?: boolean;
  showLabels?: boolean;
  xTitle?: string;
  yTitle?: string;
}) {
  const stack = stacked && STACKABLE.has(kind);
  const spans = valueSpans(
    series.map((entry) => entry.values),
    stack,
  );
  const { min, max } = spanRange(spans);
  const count = Math.max(1, categories.length, ...series.map((entry) => entry.values.length));
  const horizontal = kind === "bar";
  const span = max - min || 1;
  const bottom = plot.top + plot.height;
  /** Pixel position of a value on the value axis. */
  const along = (value: number) =>
    horizontal ? plot.left + ((value - min) / span) * plot.width : bottom - ((value - min) / span) * plot.height;
  const zero = along(0);
  const band = (horizontal ? plot.height : plot.width) / count;
  const pointX = (index: number) => plot.left + (count === 1 ? plot.width / 2 : (index / (count - 1)) * plot.width);
  const bars = kind === "column" || kind === "bar";
  const thick = (bars && stack ? band * 0.7 : Math.max(2, (band * 0.7) / series.length)) || 2;
  /** Start of a bar across the category axis. */
  const barOffset = (index: number, seriesIndex: number) =>
    index * band + band * 0.15 + (stack ? 0 : seriesIndex * thick);
  const leftTitle = horizontal ? xTitle : yTitle;
  const bottomTitle = horizontal ? yTitle : xTitle;

  return (
    <>
      <line x1={plot.left} y1={bottom} x2={plot.left + plot.width} y2={bottom} stroke={AXIS_COLOR} />
      <line x1={plot.left} y1={plot.top} x2={plot.left} y2={bottom} stroke={AXIS_COLOR} />
      {bars
        ? series.map((entry, seriesIndex) =>
            spans[seriesIndex].map(({ from, to }, index) => {
              const start = along(from);
              const end = along(to);
              const offset = barOffset(index, seriesIndex);
              return (
                <rect
                  key={`${seriesIndex}-${index}`}
                  x={horizontal ? Math.min(start, end) : plot.left + offset}
                  y={horizontal ? plot.top + offset : Math.min(start, end)}
                  width={horizontal ? Math.abs(end - start) : thick}
                  height={horizontal ? thick : Math.abs(end - start)}
                  fill={colorOf(entry, seriesIndex)}
                  opacity={0.85}
                />
              );
            }),
          )
        : series.map((entry, seriesIndex) => {
            const upper = spans[seriesIndex].map(({ to }, index) => `${pointX(index)},${along(to)}`);
            const lower = spans[seriesIndex].map(({ from }, index) => `${pointX(index)},${along(from)}`).reverse();
            const base =
              stack || upper.length === 0 ? lower : [`${pointX(upper.length - 1)},${zero}`, `${pointX(0)},${zero}`];
            return (
              <g key={seriesIndex}>
                {kind === "area" ? (
                  <polygon points={[...upper, ...base].join(" ")} fill={colorOf(entry, seriesIndex)} opacity={0.25} />
                ) : null}
                <polyline points={upper.join(" ")} fill="none" stroke={colorOf(entry, seriesIndex)} strokeWidth={2} />
              </g>
            );
          })}
      {showLabels
        ? series.flatMap((entry, seriesIndex) =>
            spans[seriesIndex].map(({ from, to }, index) => {
              const value = entry.values[index];
              if (!Number.isFinite(value)) return null;
              const across = bars ? barOffset(index, seriesIndex) + thick / 2 : 0;
              // A stacked bar's value sits inside its own segment; otherwise past its end.
              const inside = bars && stack;
              const end = inside ? (along(from) + along(to)) / 2 : along(to);
              const x = bars ? (horizontal ? end + (inside ? 0 : 3) : plot.left + across) : pointX(index);
              const y = bars ? (horizontal ? plot.top + across + 3 : end + (inside ? 3 : -3)) : end - 5;
              return (
                <text
                  key={`label-${seriesIndex}-${index}`}
                  className="chart-value-label"
                  x={x}
                  y={y}
                  fontSize={8}
                  textAnchor={horizontal && !inside ? "start" : "middle"}
                  fill={inside ? "#ffffff" : LABEL_COLOR}
                >
                  {formatTick(value)}
                </text>
              );
            }),
          )
        : null}
      {categories.map((label, index) => (
        <text
          key={index}
          x={horizontal ? plot.left - 4 : plot.left + (index + 0.5) * band}
          y={horizontal ? plot.top + (index + 0.5) * band + 3 : bottom + 14}
          fontSize={9}
          textAnchor={horizontal ? "end" : "middle"}
          fill={LABEL_COLOR}
        >
          {label.length > 8 ? `${label.slice(0, 7)}…` : label}
        </text>
      ))}
      {bottomTitle ? (
        <text
          className="chart-axis-title"
          x={plot.left + plot.width / 2}
          y={bottom + 27}
          fontSize={9}
          textAnchor="middle"
          fill={LABEL_COLOR}
        >
          {bottomTitle}
        </text>
      ) : null}
      {leftTitle ? (
        <text
          className="chart-axis-title"
          transform={`translate(${plot.left - 30 - (horizontal ? 10 : 0)}, ${plot.top + plot.height / 2}) rotate(-90)`}
          fontSize={9}
          textAnchor="middle"
          fill={LABEL_COLOR}
        >
          {leftTitle}
        </text>
      ) : null}
    </>
  );
}
