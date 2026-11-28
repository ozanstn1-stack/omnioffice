/**
 * Shared interactive chart preview component for Impress and Writer across
 * all 7 supported chart kinds (column, bar, line, area, pie, doughnut, scatter).
 */
import type { ChartData } from "../../lib/office-types";
import { useT } from "../../lib/i18n";
import { PieOrDoughnutPlot, ScatterPlot, polarPoint } from "./ChartPlots";
import { CHART_PALETTE, formatChartCellValue, type ChartPreviewTheme } from "./chart-helpers";

export function ChartPreview({
  chart,
  theme,
  scale,
}: {
  chart: ChartData | null;
  theme: ChartPreviewTheme;
  scale: number;
}) {
  const t = useT();
  if (!chart) return <div className="slide-chart-placeholder">{t("calc.chart")}</div>;
  const series = chart.series ?? [];
  const colorOf = (index: number) => series[index]?.color ?? CHART_PALETTE[index % CHART_PALETTE.length];
  const categories = chart.categoriesCache ?? [];
  const values = series.map((_, index) => chart.seriesValuesCache?.[index] ?? []);
  const hasValues = values.some((list) => list.length > 0);
  const rowCount = Math.max(categories.length, ...values.map((list) => list.length), 0);
  const maxValue = Math.max(1e-9, ...values.flatMap((list) => list.map((value) => Math.abs(value))));
  const labelOf = (row: number) => categories[row] || `${row + 1}`;
  const heights = series.map((_, index) => 45 + ((index * 37) % 50));
  const smallFont = Math.max(7, 8 * scale);

  let plot: React.ReactNode = null;
  if (hasValues && series.length > 0) {
    if (chart.kind === "pie" || chart.kind === "doughnut") {
      const hole = chart.kind === "doughnut" ? Math.min(90, Math.max(10, chart.holeSize ?? 50)) : null;
      plot = (
        <PieOrDoughnutPlot
          values={values}
          labelOf={labelOf}
          holePct={hole}
          bgColor={theme.background ?? "#ffffff"}
          borderColor={theme.accent}
          scale={scale}
        />
      );
    } else if (chart.kind === "scatter") {
      plot = (
        <ScatterPlot chart={chart} categories={categories} values={values} colorOf={colorOf} accent={theme.accent} />
      );
    } else if (chart.kind === "line" || chart.kind === "area") {
      const pointX = (row: number) => (rowCount <= 1 ? 50 : (row / (rowCount - 1)) * 100);
      const pointY = (value: number) => 50 - (value / maxValue) * 45;
      plot = (
        <svg width="100%" height="100%" viewBox="0 0 100 100" preserveAspectRatio="none" style={{ display: "block" }}>
          <line
            x1="0"
            y1="50"
            x2="100"
            y2="50"
            stroke={theme.accent}
            strokeWidth="1"
            opacity="0.5"
            vectorEffect="non-scaling-stroke"
          />
          {series.map((entry, seriesIndex) => {
            const points = values[seriesIndex].map((value, row) => `${pointX(row)},${pointY(value)}`).join(" ");
            return (
              <g key={seriesIndex}>
                {chart.kind === "area" ? (
                  <polygon points={`0,50 ${points} 100,50`} fill={colorOf(seriesIndex)} opacity="0.35" />
                ) : null}
                <polyline
                  points={points}
                  fill="none"
                  stroke={colorOf(seriesIndex)}
                  strokeWidth="1.6"
                  vectorEffect="non-scaling-stroke"
                >
                  <title>{entry.name}</title>
                </polyline>
              </g>
            );
          })}
        </svg>
      );
    } else if (chart.kind === "bar") {
      plot = (
        <div className="chart-bar-wrap" style={{ gap: 3 * scale }}>
          {Array.from({ length: rowCount }, (_, row) => (
            <div key={row} className="chart-pie-row" style={{ gap: 4 * scale }}>
              <span className="chart-ellipsis" style={{ width: 56 * scale, flex: "0 0 auto", opacity: 0.8 }}>
                {labelOf(row)}
              </span>
              <div className="chart-bar-tracks">
                {series.map((entry, seriesIndex) => {
                  const value = values[seriesIndex]?.[row] ?? 0;
                  return (
                    <div
                      key={seriesIndex}
                      title={`${entry.name}: ${formatChartCellValue(value)}`}
                      className="chart-pie-row"
                      style={{ gap: 3 * scale }}
                    >
                      <div
                        style={{
                          width: `${(Math.max(0, value) / maxValue) * 100}%`,
                          height: Math.max(4, 7 * scale),
                          background: colorOf(seriesIndex),
                          borderRadius: 2,
                        }}
                      />
                      {chart.showLabels ? (
                        <span style={{ fontSize: smallFont, opacity: 0.75 }}>{formatChartCellValue(value)}</span>
                      ) : null}
                    </div>
                  );
                })}
              </div>
            </div>
          ))}
        </div>
      );
    } else {
      plot = (
        <div className="chart-col-wrap" style={{ gap: 2 * scale }}>
          {Array.from({ length: rowCount }, (_, row) => {
            const total = series.reduce((sum, _, seriesIndex) => sum + Math.max(0, values[seriesIndex]?.[row] ?? 0), 0);
            return (
              <div key={row} className="chart-col" style={{ maxWidth: 64 * scale }}>
                {chart.showLabels && (chart.stacked || series.length === 1) ? (
                  <span style={{ fontSize: smallFont, opacity: 0.75 }}>
                    {formatChartCellValue(chart.stacked ? total : (values[0]?.[row] ?? 0))}
                  </span>
                ) : null}
                <div
                  className="chart-col-stack"
                  style={{
                    height: `${(total / maxValue) * 100}%`,
                    flexDirection: chart.stacked ? "column-reverse" : "row",
                  }}
                >
                  {series.map((entry, seriesIndex) => {
                    const raw = values[seriesIndex]?.[row] ?? 0;
                    const value = Math.max(0, raw);
                    return (
                      <div
                        key={seriesIndex}
                        title={`${entry.name}: ${formatChartCellValue(raw)}`}
                        style={{
                          flex: chart.stacked ? "none" : 1,
                          width: chart.stacked ? "100%" : undefined,
                          height: chart.stacked ? `${total > 0 ? (value / total) * 100 : 0}%` : "100%",
                          background: colorOf(seriesIndex),
                          borderRadius: chart.stacked ? 0 : 2,
                        }}
                      />
                    );
                  })}
                </div>
              </div>
            );
          })}
        </div>
      );
    }
  }

  return (
    <div
      className="chart-preview"
      style={{ padding: 6 * scale, gap: 3 * scale, fontSize: Math.max(8, 10 * scale), color: theme.bodyColor }}
    >
      <div className="chart-preview-head" style={{ gap: 6 * scale }}>
        {chart.title ? <strong style={{ fontSize: Math.max(9, 12 * scale) }}>{chart.title}</strong> : null}
        <span style={{ opacity: 0.7 }}>{t(`calc.chart_${chart.kind}`)}</span>
        {chart.stacked ? <span style={{ opacity: 0.7 }}>· {t("impress.chartStacked")}</span> : null}
        {chart.showLabels ? <span style={{ opacity: 0.7 }}>· {t("impress.chartShowLabels")}</span> : null}
      </div>
      <div
        className="chart-preview-plot"
        style={{ minHeight: 40 * scale, border: `1px dashed ${theme.accent}`, gap: 4 * scale, padding: 4 * scale }}
      >
        {series.length === 0 ? (
          <span style={{ opacity: 0.6, textAlign: "center" }}>{t("impress.chartNoSeries")}</span>
        ) : plot ? (
          plot
        ) : chart.kind === "pie" || chart.kind === "doughnut" ? (
          <svg width="80" height="80" viewBox="0 0 40 40" aria-hidden="true">
            <polygon
              points={`20,20 ${polarPoint(20, 20, 16, 0)} ${polarPoint(20, 20, 16, 0.45)}`}
              fill={colorOf(0)}
              opacity="0.85"
            />
            <circle cx="20" cy="20" r="16" fill="none" stroke={colorOf(0)} strokeWidth="1.5" />
          </svg>
        ) : (
          series.map((entry, index) => (
            <div
              key={index}
              title={`${entry.name}${entry.range ? ` · ${entry.range}` : ""}`}
              style={{
                flex: 1,
                maxWidth: 48 * scale,
                height: `${heights[index]}%`,
                background: colorOf(index),
                opacity: 0.85,
                borderRadius: 2,
              }}
            />
          ))
        )}
      </div>
      {hasValues && chart.kind !== "pie" && chart.kind !== "doughnut" && chart.kind !== "bar" ? (
        <div className="chart-preview-cats" style={{ gap: 2 * scale }}>
          {Array.from({ length: rowCount }, (_, row) => (
            <span key={row} className="chart-ellipsis" style={{ flex: 1, maxWidth: 64 * scale, textAlign: "center" }}>
              {labelOf(row)}
            </span>
          ))}
        </div>
      ) : null}
      <div className="chart-preview-foot" style={{ gap: 6 * scale }}>
        <span>{chart.yTitle || ""}</span>
        <span style={{ textAlign: "center", flex: 1 }}>
          {hasValues ? t("impress.chartCachedData") : chart.categories || t("impress.chartNoCategories")}
        </span>
        <span>{chart.xTitle || ""}</span>
      </div>
      {chart.legend && series.length > 0 ? (
        <div className="chart-preview-legend" style={{ gap: 4 * scale }}>
          {series.map((entry, index) => (
            <span key={index} className="chart-preview-legend-item" style={{ gap: 3 * scale }}>
              <i
                style={{
                  width: 8 * scale,
                  height: 8 * scale,
                  background: colorOf(index),
                  display: "inline-block",
                  borderRadius: 2,
                }}
              />
              {entry.name || `${t("impress.chartSeries")} ${index + 1}`}
              {!hasValues && entry.range ? ` · ${entry.range}` : ""}
            </span>
          ))}
        </div>
      ) : null}
      {hasValues ? null : (
        <div style={{ opacity: 0.55, fontSize: Math.max(7, 8 * scale) }}>{t("impress.chartSchematicNote")}</div>
      )}
    </div>
  );
}
