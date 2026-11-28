/**
 * Shared modal Chart editor (Data grid + Chart formatting across all 7 chart kinds),
 * used by both Impress and Writer.
 */
import { useRef, useState } from "react";
import type { ChartData } from "../../lib/office-types";
import { useT } from "../../lib/i18n";
import { Dialog, TextField } from "../office-ui";
import { ChartDataGrid } from "./ChartDataGrid";
import { ChartPreview } from "./ChartPreview";
import {
  CHART_KINDS,
  CHART_PALETTE,
  SCATTER_STYLES,
  WRITER_CHART_THEME,
  chartDataRowCount,
  chartHasCachedData,
  mergeChartPaste,
  normalizeChartDraft,
  parseChartCellNumber,
  parseChartClipboard,
  syncChartDataRanges,
  type ChartGridCell,
  type ChartPreviewTheme,
} from "./chart-helpers";

export function ChartDialog({
  chart,
  theme = WRITER_CHART_THEME,
  title,
  onClose,
  onSave,
}: {
  chart: ChartData;
  theme?: ChartPreviewTheme;
  title?: string;
  onClose: () => void;
  onSave: (chart: ChartData) => void;
}) {
  const t = useT();
  const [draft, setDraft] = useState<ChartData>(() => normalizeChartDraft(chart));
  const [tab, setTab] = useState<"data" | "chart">("data");
  const [rowCount, setRowCount] = useState(() => chartDataRowCount(chart));
  const [cellTexts, setCellTexts] = useState<Record<string, string>>({});
  const [pasteAnchor, setPasteAnchor] = useState<ChartGridCell>({ row: 0, column: 1 });
  const [dataTouched, setDataTouched] = useState(false);
  const gridRef = useRef<HTMLDivElement>(null);
  const patch = (change: Partial<ChartData>) => setDraft((current) => ({ ...current, ...change }));

  const categories = draft.categoriesCache ?? [];
  const seriesValues = draft.seriesValuesCache ?? draft.series.map(() => []);
  const rows = Math.max(rowCount, categories.length, ...seriesValues.map((values) => values.length), 1);

  const updateSeries = (index: number, change: Partial<ChartData["series"][number]>) => {
    setDraft((current) => ({
      ...current,
      series: current.series.map((entry, position) => (position === index ? { ...entry, ...change } : entry)),
    }));
  };

  const addSeries = () => {
    setDraft((current) => {
      const values = current.seriesValuesCache ?? current.series.map(() => []);
      return {
        ...current,
        series: [
          ...current.series,
          { name: `${t("impress.chartSeries")} ${current.series.length + 1}`, range: "", color: null },
        ],
        seriesValuesCache: [...values, []],
      };
    });
  };

  const removeSeries = (index: number) => {
    setDataTouched(true);
    setCellTexts({});
    setDraft((current) => {
      const values = current.seriesValuesCache ?? current.series.map(() => []);
      return {
        ...current,
        series: current.series.filter((_, position) => position !== index),
        seriesValuesCache: values.filter((_, position) => position !== index),
      };
    });
  };

  const changeRowCount = (next: number) => {
    const target = Math.max(1, Math.min(1000, Math.floor(next) || 1));
    setDataTouched(true);
    setRowCount(target);
    setDraft((current) => ({
      ...current,
      categoriesCache: (current.categoriesCache ?? []).filter((_, row) => row < target),
      seriesValuesCache: current.series.map((_, index) =>
        (current.seriesValuesCache?.[index] ?? []).filter((_, row) => row < target),
      ),
    }));
    setCellTexts((current) => {
      const kept: Record<string, string> = {};
      for (const [key, text] of Object.entries(current)) {
        if (Number(key.split(":")[0]) < target) kept[key] = text;
      }
      return kept;
    });
  };

  const commitValue = (seriesIndex: number, row: number, value: number) => {
    setDataTouched(true);
    setDraft((current) => {
      const values = (current.seriesValuesCache ?? []).map((list) => [...list]);
      const target = values[seriesIndex] ?? [];
      while (target.length < row) target.push(0);
      target[row] = value;
      values[seriesIndex] = target;
      return { ...current, seriesValuesCache: values };
    });
  };

  const clearValue = (seriesIndex: number, row: number) => {
    setDataTouched(true);
    setDraft((current) => {
      const values = (current.seriesValuesCache ?? []).map((list) => [...list]);
      const target = values[seriesIndex];
      if (!target || row >= target.length) return current;
      if (row === target.length - 1) target.pop();
      else target[row] = 0;
      values[seriesIndex] = target;
      return { ...current, seriesValuesCache: values };
    });
  };

  const changeValue = (seriesIndex: number, row: number, text: string) => {
    const key = `${row}:${seriesIndex + 1}`;
    const parsed = parseChartCellNumber(text);
    if (parsed === null) {
      setCellTexts((current) => ({ ...current, [key]: text }));
      return;
    }
    setCellTexts((current) => {
      if (!(key in current)) return current;
      const next = { ...current };
      delete next[key];
      return next;
    });
    commitValue(seriesIndex, row, parsed);
  };

  const finishValue = (seriesIndex: number, row: number) => {
    const key = `${row}:${seriesIndex + 1}`;
    const text = cellTexts[key];
    if (text === undefined) return;
    setCellTexts((current) => {
      const next = { ...current };
      delete next[key];
      return next;
    });
    if (parseChartCellNumber(text) === null && text.trim() === "") clearValue(seriesIndex, row);
  };

  const changeCategory = (row: number, text: string) => {
    setDataTouched(true);
    setDraft((current) => {
      const next = [...(current.categoriesCache ?? [])];
      while (next.length <= row) next.push("");
      next[row] = text;
      while (next.length > 0 && next[next.length - 1] === "") next.pop();
      return { ...current, categoriesCache: next };
    });
  };

  const handlePaste = (event: React.ClipboardEvent<HTMLDivElement>) => {
    const text = event.clipboardData?.getData("text/plain") ?? "";
    if (text.trim() === "") return;
    const block = parseChartClipboard(text);
    if (block.length === 0) return;
    event.preventDefault();
    setDataTouched(true);
    setDraft((current) => mergeChartPaste(current, pasteAnchor, block));
    setRowCount((current) => Math.max(current, pasteAnchor.row + block.length, 1));
  };

  const handleCellKeyDown = (event: React.KeyboardEvent<HTMLInputElement>, row: number, column: number) => {
    const input = event.currentTarget;
    const atStart = input.selectionStart === 0 && input.selectionEnd === 0;
    const atEnd = input.selectionStart === input.value.length && input.selectionEnd === input.value.length;
    let next: ChartGridCell | null = null;
    if (event.key === "ArrowUp" && row > 0) next = { row: row - 1, column };
    else if ((event.key === "ArrowDown" || event.key === "Enter") && row < rows - 1) next = { row: row + 1, column };
    else if (event.key === "ArrowLeft" && column > 0 && atStart) next = { row, column: column - 1 };
    else if (event.key === "ArrowRight" && column < draft.series.length && atEnd) next = { row, column: column + 1 };
    if (!next) return;
    event.preventDefault();
    const target = gridRef.current?.querySelector<HTMLInputElement>(`[data-cell="${next.row}:${next.column}"]`);
    if (target) {
      target.focus();
      target.select();
    }
  };

  return (
    <Dialog title={title ?? t("impress.chartData")} onClose={onClose} wide>
      <div className="stack">
        <div className="chart-dialog-preview" style={{ background: theme.background ?? "#ffffff" }}>
          <ChartPreview chart={draft} theme={theme} scale={1} />
        </div>
        <div className="row" style={{ gap: 4 }}>
          <button
            type="button"
            className={`btn ${tab === "data" ? "btn-primary" : "btn-soft"}`}
            onClick={() => setTab("data")}
          >
            {t("impress.chartTabData")}
          </button>
          <button
            type="button"
            className={`btn ${tab === "chart" ? "btn-primary" : "btn-soft"}`}
            onClick={() => setTab("chart")}
          >
            {t("impress.chartTabChart")}
          </button>
        </div>

        {tab === "data" ? (
          <ChartDataGrid
            draft={draft}
            rows={rows}
            hasInitialCache={chartHasCachedData(chart)}
            cellTexts={cellTexts}
            gridRef={gridRef}
            onChangeRowCount={changeRowCount}
            onUpdateSeries={updateSeries}
            onAddSeries={addSeries}
            onRemoveSeries={removeSeries}
            onChangeCategory={changeCategory}
            onChangeValue={changeValue}
            onFinishValue={finishValue}
            onPaste={handlePaste}
            onSetPasteAnchor={setPasteAnchor}
            onCellKeyDown={handleCellKeyDown}
          />
        ) : (
          <div className="stack">
            <label className="field">
              <span>{t("impress.chartKind")}</span>
              <select value={draft.kind} onChange={(event) => patch({ kind: event.target.value })}>
                {CHART_KINDS.map((kind) => (
                  <option key={kind} value={kind}>
                    {t(`calc.chart_${kind}`)}
                  </option>
                ))}
              </select>
            </label>
            {draft.kind === "doughnut" ? (
              <label className="field">
                <span>{t("impress.chartHoleSize")}</span>
                <input
                  type="number"
                  min={10}
                  max={90}
                  value={draft.holeSize ?? 50}
                  onChange={(event) =>
                    patch({ holeSize: Math.min(90, Math.max(10, Number(event.target.value) || 50)) })
                  }
                />
              </label>
            ) : null}
            {draft.kind === "scatter" ? (
              <label className="field">
                <span>{t("impress.chartScatterStyle")}</span>
                <select
                  value={draft.scatterStyle ?? ""}
                  onChange={(event) => patch({ scatterStyle: event.target.value || null })}
                >
                  {SCATTER_STYLES.map((opt) => (
                    <option key={opt.key} value={opt.value}>
                      {t(opt.key)}
                    </option>
                  ))}
                </select>
              </label>
            ) : null}
            <TextField
              label={t("impress.chartTitle")}
              value={draft.title}
              onChange={(titleVal) => patch({ title: titleVal })}
            />
            <TextField
              label={t("impress.chartCategories")}
              value={draft.categories}
              onChange={(cats) => patch({ categories: cats })}
            />
            <h4>{t("impress.chartSeries")}</h4>
            {draft.series.map((entry, index) => (
              <div key={index} className="row" style={{ gap: 4, alignItems: "center" }}>
                <input
                  value={entry.name}
                  placeholder={t("impress.chartSeriesName")}
                  onChange={(event) => updateSeries(index, { name: event.target.value })}
                />
                <input
                  value={entry.range}
                  placeholder={t("impress.chartRange")}
                  onChange={(event) => updateSeries(index, { range: event.target.value })}
                />
                <input
                  type="color"
                  value={entry.color ?? CHART_PALETTE[index % CHART_PALETTE.length]}
                  onChange={(event) => updateSeries(index, { color: event.target.value })}
                />
                <button
                  type="button"
                  className="icon-btn"
                  onClick={() => removeSeries(index)}
                  title={t("common.delete")}
                >
                  ×
                </button>
              </div>
            ))}
            <button type="button" className="btn btn-soft" onClick={addSeries}>
              + {t("impress.chartAddSeries")}
            </button>
            <div className="row">
              {(
                [
                  ["legend", "chartLegend"],
                  ["stacked", "chartStacked"],
                  ["showLabels", "chartShowLabels"],
                ] as const
              ).map(([field, labelKey]) => (
                <label key={field} className="check">
                  <input
                    type="checkbox"
                    checked={draft[field]}
                    onChange={(event) => patch({ [field]: event.target.checked })}
                  />
                  {t(`impress.${labelKey}`)}
                </label>
              ))}
            </div>
            <TextField label={t("impress.chartXTitle")} value={draft.xTitle} onChange={(xTitle) => patch({ xTitle })} />
            <TextField label={t("impress.chartYTitle")} value={draft.yTitle} onChange={(yTitle) => patch({ yTitle })} />
          </div>
        )}

        <div className="row">
          <button type="button" className="btn btn-soft" onClick={onClose}>
            {t("common.cancel")}
          </button>
          <button
            type="button"
            className="btn btn-primary"
            onClick={() => onSave(dataTouched && chartHasCachedData(draft) ? syncChartDataRanges(draft) : draft)}
          >
            {t("common.save")}
          </button>
        </div>
      </div>
    </Dialog>
  );
}
