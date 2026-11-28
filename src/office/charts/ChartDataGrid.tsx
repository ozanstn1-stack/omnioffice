/**
 * Embedded spreadsheet-like Data Grid tab for the shared ChartDialog.
 */
import type { ChartData } from "../../lib/office-types";
import { useT } from "../../lib/i18n";
import { formatChartCellValue, type ChartGridCell } from "./chart-helpers";

export function ChartDataGrid({
  draft,
  rows,
  hasInitialCache,
  cellTexts,
  gridRef,
  onChangeRowCount,
  onUpdateSeries,
  onAddSeries,
  onRemoveSeries,
  onChangeCategory,
  onChangeValue,
  onFinishValue,
  onPaste,
  onSetPasteAnchor,
  onCellKeyDown,
}: {
  draft: ChartData;
  rows: number;
  hasInitialCache: boolean;
  cellTexts: Record<string, string>;
  gridRef: React.RefObject<HTMLDivElement | null>;
  onChangeRowCount: (next: number) => void;
  onUpdateSeries: (index: number, change: Partial<ChartData["series"][number]>) => void;
  onAddSeries: () => void;
  onRemoveSeries: (index: number) => void;
  onChangeCategory: (row: number, text: string) => void;
  onChangeValue: (seriesIndex: number, row: number, text: string) => void;
  onFinishValue: (seriesIndex: number, row: number) => void;
  onPaste: (event: React.ClipboardEvent<HTMLDivElement>) => void;
  onSetPasteAnchor: (cell: ChartGridCell) => void;
  onCellKeyDown: (event: React.KeyboardEvent<HTMLInputElement>, row: number, column: number) => void;
}) {
  const t = useT();
  const categories = draft.categoriesCache ?? [];
  const seriesValues = draft.seriesValuesCache ?? draft.series.map(() => []);

  return (
    <div className="stack">
      {hasInitialCache ? null : <p className="muted">{t("impress.chartRangeOnlyHint")}</p>}
      <div className="row" style={{ gap: 6, alignItems: "center", flexWrap: "wrap" }}>
        <button type="button" className="btn btn-soft" onClick={() => onChangeRowCount(rows + 1)}>
          + {t("impress.chartAddRow")}
        </button>
        <button type="button" className="btn btn-soft" onClick={() => onChangeRowCount(Math.max(1, rows - 1))}>
          − {t("impress.chartRemoveRow")}
        </button>
        <label className="field" style={{ flexDirection: "row", alignItems: "center", gap: 6 }}>
          <span>{t("impress.chartRowCount")}</span>
          <input
            type="number"
            min={1}
            max={1000}
            value={rows}
            style={{ width: 72 }}
            onChange={(event) => {
              const value = Number(event.target.value);
              if (Number.isFinite(value)) onChangeRowCount(value);
            }}
          />
        </label>
      </div>
      <div ref={gridRef} onPaste={onPaste} className="chart-data-grid-wrap">
        <table className="chart-data-grid">
          <thead>
            <tr>
              <th>{t("impress.chartCategoryColumn")}</th>
              {draft.series.map((entry, index) => (
                <th key={index}>
                  <div className="row" style={{ gap: 2, alignItems: "center" }}>
                    <input
                      data-series-name={index}
                      value={entry.name}
                      placeholder={t("impress.chartSeriesName")}
                      aria-label={`${t("impress.chartSeriesName")} ${index + 1}`}
                      onChange={(event) => onUpdateSeries(index, { name: event.target.value })}
                    />
                    <button
                      type="button"
                      className="icon-btn"
                      onClick={() => onRemoveSeries(index)}
                      title={t("common.delete")}
                    >
                      ×
                    </button>
                  </div>
                </th>
              ))}
              <th style={{ minWidth: 40 }}>
                <button type="button" className="icon-btn" onClick={onAddSeries} title={t("impress.chartAddSeries")}>
                  +
                </button>
              </th>
            </tr>
          </thead>
          <tbody>
            {Array.from({ length: rows }, (_, row) => (
              <tr key={row}>
                <td>
                  <input
                    data-cell={`${row}:0`}
                    value={categories[row] ?? ""}
                    placeholder={`${t("impress.chartCategoryColumn")} ${row + 1}`}
                    aria-label={`${t("impress.chartCategoryColumn")} ${row + 1}`}
                    onChange={(event) => onChangeCategory(row, event.target.value)}
                    onFocus={() => onSetPasteAnchor({ row, column: 0 })}
                    onPointerDown={() => onSetPasteAnchor({ row, column: 0 })}
                    onKeyDown={(event) => onCellKeyDown(event, row, 0)}
                  />
                </td>
                {draft.series.map((entry, seriesIndex) => {
                  const key = `${row}:${seriesIndex + 1}`;
                  const stored = seriesValues[seriesIndex]?.[row];
                  return (
                    <td key={seriesIndex}>
                      <input
                        data-cell={key}
                        value={cellTexts[key] ?? (stored === undefined ? "" : formatChartCellValue(stored))}
                        inputMode="decimal"
                        placeholder={entry.name}
                        aria-label={`${entry.name || `${t("impress.chartSeries")} ${seriesIndex + 1}`} ${row + 1}`}
                        onChange={(event) => onChangeValue(seriesIndex, row, event.target.value)}
                        onBlur={() => onFinishValue(seriesIndex, row)}
                        onFocus={() => onSetPasteAnchor({ row, column: seriesIndex + 1 })}
                        onPointerDown={() => onSetPasteAnchor({ row, column: seriesIndex + 1 })}
                        onKeyDown={(event) => onCellKeyDown(event, row, seriesIndex + 1)}
                      />
                    </td>
                  );
                })}
                <td>
                  <span className="sr-only">{t("common.actions")}</span>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {(["chartPasteHint", "chartDataHint", "chartDataGapNote"] as const).map((key) => (
        <p key={key} className="muted">
          {t(`impress.${key}`)}
        </p>
      ))}
    </div>
  );
}
