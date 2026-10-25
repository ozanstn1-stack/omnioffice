/**
 * Impress dialogs: the footer, master, chart and animation editors.
 *
 * Extracted verbatim from ImpressEditor.tsx (B4 editor seam 3): each dialog
 * takes the model, the current value and the callbacks it needs as props.
 */
import { useRef, useState, type CSSProperties } from "react";
import type { Animation, ChartData, Deck, Slide, SlideFooter, SlideLayout, SlideObject } from "../../lib/office-types";
import { newSlideMaster, uid } from "../../lib/office-types";
import { useT } from "../../lib/i18n";
import { Dialog, TextField, ToolNumber } from "../office-ui";
import {
  ANIMATION_EFFECTS,
  CHART_PALETTE,
  chartDataRowCount,
  chartHasCachedData,
  formatChartCellValue,
  mergeChartPaste,
  normalizeChartDraft,
  parseChartCellNumber,
  parseChartClipboard,
  syncChartDataRanges,
  type ChartGridCell,
  type Theme,
} from "./shared";
import { ChartPreview } from "./slide-view";

/** Today as YYYY-MM-DD; the default date text when a footer is first enabled. */
function todayIso(): string {
  const now = new Date();
  const month = String(now.getMonth() + 1).padStart(2, "0");
  const day = String(now.getDate()).padStart(2, "0");
  return `${now.getFullYear()}-${month}-${day}`;
}

function defaultFooter(): SlideFooter {
  return { enabled: false, text: "", showText: true, showSlideNumber: false, showDate: false, dateText: todayIso() };
}

/** Deck-wide header & footer settings; changes apply to every slide. */
export function FooterDialog({
  footer,
  onChange,
  onClose,
}: {
  footer: SlideFooter | null | undefined;
  onChange: (footer: SlideFooter) => void;
  onClose: () => void;
}) {
  const t = useT();
  const current: SlideFooter = { ...defaultFooter(), ...(footer ?? {}) };
  const patch = (change: Partial<SlideFooter>) => onChange({ ...current, ...change });
  return (
    <Dialog title={t("impress.headerFooter")} onClose={onClose}>
      <div className="stack">
        <label className="field">
          <span>{t("impress.footerEnabled")}</span>
          <input
            type="checkbox"
            checked={current.enabled}
            onChange={(event) => patch({ enabled: event.target.checked })}
          />
        </label>
        <label className="field">
          <span>{t("impress.showFooterText")}</span>
          <input
            type="checkbox"
            checked={current.showText}
            onChange={(event) => patch({ showText: event.target.checked })}
          />
        </label>
        <TextField label={t("impress.footerText")} value={current.text} onChange={(text) => patch({ text })} />
        <label className="field">
          <span>{t("impress.showSlideNumber")}</span>
          <input
            type="checkbox"
            checked={current.showSlideNumber}
            onChange={(event) => patch({ showSlideNumber: event.target.checked })}
          />
        </label>
        <label className="field">
          <span>{t("impress.showDate")}</span>
          <input
            type="checkbox"
            checked={current.showDate}
            onChange={(event) => patch({ showDate: event.target.checked })}
          />
        </label>
        <TextField
          label={t("impress.dateText")}
          value={current.dateText}
          onChange={(dateText) => patch({ dateText })}
        />
        <p className="muted">{t("impress.footerHint")}</p>
      </div>
    </Dialog>
  );
}

export function MasterDialog({
  deck,
  slide,
  update,
  onClose,
  onUseLayout,
}: {
  deck: Deck;
  slide: Slide;
  update: (mutate: (deck: Deck) => Deck, recordUndo?: boolean) => void;
  onClose: () => void;
  onUseLayout: (masterId: string, layout: SlideLayout) => void;
}) {
  const t = useT();
  const masters = deck.masters ?? [];
  const [activeId, setActiveId] = useState(
    slide.masterId && masters.some((master) => master.id === slide.masterId) ? slide.masterId : (masters[0]?.id ?? ""),
  );
  const active = masters.find((master) => master.id === activeId) ?? masters[0];

  const addMaster = () => {
    const master = newSlideMaster(`Master ${masters.length + 1}`);
    setActiveId(master.id);
    update((current) => ({ ...current, masters: [...(current.masters ?? []), master] }));
  };
  const renameMaster = (masterId: string, name: string) =>
    update(
      (current) => ({
        ...current,
        masters: (current.masters ?? []).map((master) => (master.id === masterId ? { ...master, name } : master)),
      }),
      false,
    );
  const addLayout = (masterId: string) => {
    const layout: SlideLayout = {
      id: uid(),
      name: `Layout ${(masters.find((master) => master.id === masterId)?.layouts.length ?? 0) + 1}`,
      kind: "blank",
      objects: [],
    };
    update((current) => ({
      ...current,
      masters: (current.masters ?? []).map((master) =>
        master.id === masterId ? { ...master, layouts: [...master.layouts, layout] } : master,
      ),
    }));
  };
  const renameLayout = (masterId: string, layoutId: string, name: string) =>
    update(
      (current) => ({
        ...current,
        masters: (current.masters ?? []).map((master) =>
          master.id === masterId
            ? {
                ...master,
                layouts: master.layouts.map((layout) => (layout.id === layoutId ? { ...layout, name } : layout)),
              }
            : master,
        ),
      }),
      false,
    );
  const removeLayout = (masterId: string, layoutId: string) =>
    update((current) => ({
      ...current,
      masters: (current.masters ?? []).map((master) =>
        master.id === masterId
          ? { ...master, layouts: master.layouts.filter((layout) => layout.id !== layoutId) }
          : master,
      ),
    }));

  return (
    <Dialog title={t("impress.masters")} onClose={onClose} wide>
      <div className="row" style={{ alignItems: "flex-start", gap: 12 }}>
        <div className="stack" style={{ minWidth: 170 }}>
          {masters.map((master) => (
            <button
              key={master.id}
              type="button"
              className={`btn ${master.id === (active?.id ?? "") ? "btn-primary" : "btn-soft"}`}
              onClick={() => setActiveId(master.id)}
            >
              {master.name}
            </button>
          ))}
          <button type="button" className="btn btn-soft" onClick={addMaster}>
            + {t("impress.addMaster")}
          </button>
        </div>
        {active ? (
          <div className="stack" style={{ flex: 1 }}>
            <label className="field">
              <span>{t("impress.masterName")}</span>
              <input value={active.name} onChange={(event) => renameMaster(active.id, event.target.value)} />
            </label>
            <h4>{t("impress.layouts")}</h4>
            {active.layouts.map((layout) => (
              <div key={layout.id} className="row" style={{ gap: 4, alignItems: "center" }}>
                <input
                  value={layout.name}
                  onChange={(event) => renameLayout(active.id, layout.id, event.target.value)}
                />
                <button
                  type="button"
                  className={`btn ${slide.masterId === active.id && slide.layoutId === layout.id ? "btn-primary" : "btn-soft"}`}
                  onClick={() => onUseLayout(active.id, layout)}
                >
                  {t("impress.useLayout")}
                </button>
                <button
                  type="button"
                  className="icon-btn"
                  onClick={() => removeLayout(active.id, layout.id)}
                  title={t("common.delete")}
                >
                  ×
                </button>
              </div>
            ))}
            <button type="button" className="btn btn-soft" onClick={() => addLayout(active.id)}>
              + {t("impress.addLayout")}
            </button>
            <p className="muted">{t("impress.masterHint")}</p>
          </div>
        ) : null}
      </div>
    </Dialog>
  );
}

export function ChartDialog({
  chart,
  theme,
  onClose,
  onSave,
}: {
  chart: ChartData;
  theme: Theme;
  onClose: () => void;
  onSave: (chart: ChartData) => void;
}) {
  const t = useT();
  const [draft, setDraft] = useState<ChartData>(() => normalizeChartDraft(chart));
  const [tab, setTab] = useState<"data" | "chart">("data");
  // Displayed rows; caches may grow past this through paste, the derived count
  // below always covers every stored value.
  const [rowCount, setRowCount] = useState(() => chartDataRowCount(chart));
  // Text of cells that are being edited but do not parse (or were emptied).
  // The draft only ever receives parsed numbers, so invalid input is never
  // committed; blurring discards the text and restores the stored value.
  const [cellTexts, setCellTexts] = useState<Record<string, string>>({});
  // Paste anchor = the focused cell; paste starts there.
  const [pasteAnchor, setPasteAnchor] = useState<ChartGridCell>({ row: 0, column: 1 });
  const [dataTouched, setDataTouched] = useState(false);
  const gridRef = useRef<HTMLDivElement>(null);
  const patch = (change: Partial<ChartData>) => setDraft((current) => ({ ...current, ...change }));

  const categories = draft.categoriesCache ?? [];
  const seriesValues = draft.seriesValuesCache ?? draft.series.map(() => []);
  const rows = Math.max(rowCount, categories.length, ...seriesValues.map((values) => values.length), 1);
  const cellKey = (row: number, column: number) => `${row}:${column}`;

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
    // Uncommitted cell text is keyed by column, so drop it instead of letting
    // it attach to a different series after the shift.
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

  /** Shrinks or grows the visible rows; shrinking trims caches to fit. */
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

  /** Commits a parsed value; rows beyond the current length are padded with 0
   * because caches are dense lists (the Rust reader zero-fills sparse points). */
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

  /** Clearing a cell drops the trailing value or, inside a series, stores 0 so
   * the later points keep their category alignment. */
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
    const key = cellKey(row, seriesIndex + 1);
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

  /** Blur commit: an emptied cell clears its value, text that never parsed is
   * dropped (invalid input is not committed). */
  const finishValue = (seriesIndex: number, row: number) => {
    const key = cellKey(row, seriesIndex + 1);
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
      // Trailing empty labels do not carry data; keep the cache short.
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

  const focusCell = (row: number, column: number) => {
    const input = gridRef.current?.querySelector<HTMLInputElement>(`[data-cell="${row}:${column}"]`);
    if (input) {
      input.focus();
      input.select();
    }
  };

  /** Spreadsheet-style navigation: up/down/Enter move between rows, left/right
   * only when the caret is already at the edge of the cell text. */
  const handleCellKeyDown = (event: React.KeyboardEvent<HTMLInputElement>, row: number, column: number) => {
    const input = event.currentTarget;
    const atStart = input.selectionStart === 0 && input.selectionEnd === 0;
    const atEnd = input.selectionStart === input.value.length && input.selectionEnd === input.value.length;
    let next: ChartGridCell | null = null;
    if (event.key === "ArrowUp" && row > 0) next = { row: row - 1, column };
    else if (event.key === "ArrowDown" && row < rows - 1) next = { row: row + 1, column };
    else if (event.key === "Enter" && row < rows - 1) next = { row: row + 1, column };
    else if (event.key === "ArrowLeft" && column > 0 && atStart) next = { row, column: column - 1 };
    else if (event.key === "ArrowRight" && column < draft.series.length && atEnd) next = { row, column: column + 1 };
    if (!next) return;
    event.preventDefault();
    focusCell(next.row, next.column);
  };

  const gridInputStyle: CSSProperties = {
    width: "100%",
    minWidth: 84,
    boxSizing: "border-box",
    border: "none",
    background: "transparent",
    padding: "6px 8px",
    font: "inherit",
    color: "inherit",
  };
  const gridCellStyle: CSSProperties = {
    padding: 0,
    borderBottom: "1px solid var(--border)",
    borderRight: "1px solid var(--border)",
  };
  const gridHeadStyle: CSSProperties = {
    position: "sticky",
    top: 0,
    zIndex: 1,
    background: "var(--surface)",
    borderBottom: "1px solid var(--border)",
    borderRight: "1px solid var(--border)",
    padding: 2,
    textAlign: "left",
    minWidth: 110,
  };

  return (
    <Dialog title={t("impress.chartData")} onClose={onClose} wide>
      <div className="stack">
        <div
          style={{
            height: 190,
            flex: "0 0 auto",
            border: "1px solid var(--border)",
            borderRadius: 6,
            overflow: "hidden",
            background: theme.background,
          }}
        >
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
          <div className="stack">
            {chartHasCachedData(chart) ? null : <p className="muted">{t("impress.chartRangeOnlyHint")}</p>}
            <div className="row" style={{ gap: 6, alignItems: "center", flexWrap: "wrap" }}>
              <button type="button" className="btn btn-soft" onClick={() => changeRowCount(rows + 1)}>
                + {t("impress.chartAddRow")}
              </button>
              <button type="button" className="btn btn-soft" onClick={() => changeRowCount(Math.max(1, rows - 1))}>
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
                    if (Number.isFinite(value)) changeRowCount(value);
                  }}
                />
              </label>
            </div>
            <div
              ref={gridRef}
              onPaste={handlePaste}
              style={{
                overflowX: "auto",
                overflowY: "auto",
                maxHeight: 280,
                border: "1px solid var(--border)",
                borderRadius: 6,
              }}
            >
              <table style={{ borderCollapse: "collapse", width: "100%" }}>
                <thead>
                  <tr>
                    <th style={gridHeadStyle}>{t("impress.chartCategoryColumn")}</th>
                    {draft.series.map((entry, index) => (
                      <th key={index} style={gridHeadStyle}>
                        <div className="row" style={{ gap: 2, alignItems: "center" }}>
                          <input
                            data-series-name={index}
                            value={entry.name}
                            placeholder={t("impress.chartSeriesName")}
                            aria-label={`${t("impress.chartSeriesName")} ${index + 1}`}
                            style={gridInputStyle}
                            onChange={(event) => updateSeries(index, { name: event.target.value })}
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
                      </th>
                    ))}
                    <th style={{ ...gridHeadStyle, minWidth: 40 }}>
                      <button
                        type="button"
                        className="icon-btn"
                        onClick={addSeries}
                        title={t("impress.chartAddSeries")}
                      >
                        +
                      </button>
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {Array.from({ length: rows }, (_, row) => (
                    <tr key={row}>
                      <td style={gridCellStyle}>
                        <input
                          data-cell={`${row}:0`}
                          value={categories[row] ?? ""}
                          placeholder={`${t("impress.chartCategoryColumn")} ${row + 1}`}
                          aria-label={`${t("impress.chartCategoryColumn")} ${row + 1}`}
                          style={gridInputStyle}
                          onChange={(event) => changeCategory(row, event.target.value)}
                          onFocus={() => setPasteAnchor({ row, column: 0 })}
                          onPointerDown={() => setPasteAnchor({ row, column: 0 })}
                          onKeyDown={(event) => handleCellKeyDown(event, row, 0)}
                        />
                      </td>
                      {draft.series.map((entry, seriesIndex) => {
                        const key = cellKey(row, seriesIndex + 1);
                        const stored = seriesValues[seriesIndex]?.[row];
                        return (
                          <td key={seriesIndex} style={gridCellStyle}>
                            <input
                              data-cell={key}
                              value={cellTexts[key] ?? (stored === undefined ? "" : formatChartCellValue(stored))}
                              inputMode="decimal"
                              placeholder={entry.name}
                              aria-label={`${entry.name || `${t("impress.chartSeries")} ${seriesIndex + 1}`} ${row + 1}`}
                              style={gridInputStyle}
                              onChange={(event) => changeValue(seriesIndex, row, event.target.value)}
                              onBlur={() => finishValue(seriesIndex, row)}
                              onFocus={() => setPasteAnchor({ row, column: seriesIndex + 1 })}
                              onPointerDown={() => setPasteAnchor({ row, column: seriesIndex + 1 })}
                              onKeyDown={(event) => handleCellKeyDown(event, row, seriesIndex + 1)}
                            />
                          </td>
                        );
                      })}
                      <td style={{ borderBottom: "1px solid var(--border)" }}>
                        <span className="sr-only">{t("common.actions")}</span>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <p className="muted">{t("impress.chartPasteHint")}</p>
            <p className="muted">{t("impress.chartDataHint")}</p>
            <p className="muted">{t("impress.chartDataGapNote")}</p>
          </div>
        ) : (
          <div className="stack">
            <label className="field">
              <span>{t("impress.chartKind")}</span>
              <select value={draft.kind} onChange={(event) => patch({ kind: event.target.value })}>
                {["column", "bar", "line", "pie", "area"].map((kind) => (
                  <option key={kind} value={kind}>
                    {t(`calc.chart_${kind}`)}
                  </option>
                ))}
              </select>
            </label>
            <TextField label={t("impress.chartTitle")} value={draft.title} onChange={(title) => patch({ title })} />
            <TextField
              label={t("impress.chartCategories")}
              value={draft.categories}
              onChange={(categories) => patch({ categories })}
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
              <label className="check">
                <input
                  type="checkbox"
                  checked={draft.legend}
                  onChange={(event) => patch({ legend: event.target.checked })}
                />
                {t("impress.chartLegend")}
              </label>
              <label className="check">
                <input
                  type="checkbox"
                  checked={draft.stacked}
                  onChange={(event) => patch({ stacked: event.target.checked })}
                />
                {t("impress.chartStacked")}
              </label>
              <label className="check">
                <input
                  type="checkbox"
                  checked={draft.showLabels}
                  onChange={(event) => patch({ showLabels: event.target.checked })}
                />
                {t("impress.chartShowLabels")}
              </label>
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

export function AnimationDialog({
  animation,
  objects,
  onClose,
  onSave,
}: {
  animation: Animation;
  objects: SlideObject[];
  onClose: () => void;
  onSave: (animation: Animation) => void;
}) {
  const t = useT();
  const [draft, setDraft] = useState<Animation>({ ...animation });
  const effects = ANIMATION_EFFECTS[draft.kind] ?? ANIMATION_EFFECTS.entrance;
  const effect = effects.includes(draft.effect) ? draft.effect : effects[0];
  const missingObject = !objects.some((object) => object.id === draft.objectId);

  return (
    <Dialog title={t("impress.animation")} onClose={onClose}>
      <div className="stack">
        <label className="field">
          <span>{t("impress.animationObject")}</span>
          <select value={draft.objectId} onChange={(event) => setDraft({ ...draft, objectId: event.target.value })}>
            {missingObject ? <option value={draft.objectId}>{t("impress.animationMissingObject")}</option> : null}
            {objects.map((object, index) => (
              <option key={object.id} value={object.id}>
                {object.name || `${object.kind} ${index + 1}`}
              </option>
            ))}
          </select>
        </label>
        <label className="field">
          <span>{t("impress.animationKind")}</span>
          <select
            value={draft.kind}
            onChange={(event) => {
              const kind = event.target.value;
              const list = ANIMATION_EFFECTS[kind] ?? [];
              setDraft({ ...draft, kind, effect: list.includes(draft.effect) ? draft.effect : (list[0] ?? "fade") });
            }}
          >
            {["entrance", "emphasis", "exit"].map((kind) => (
              <option key={kind} value={kind}>
                {t(`impress.animationKind_${kind}`)}
              </option>
            ))}
          </select>
        </label>
        <label className="field">
          <span>{t("impress.animationEffect")}</span>
          <select value={effect} onChange={(event) => setDraft({ ...draft, effect: event.target.value })}>
            {effects.map((candidate) => (
              <option key={candidate} value={candidate}>
                {candidate}
              </option>
            ))}
          </select>
        </label>
        <label className="field">
          <span>{t("impress.animationTrigger")}</span>
          <select value={draft.trigger} onChange={(event) => setDraft({ ...draft, trigger: event.target.value })}>
            <option value="onClick">{t("impress.triggerOnClick")}</option>
            <option value="withPrevious">{t("impress.triggerWithPrevious")}</option>
            <option value="afterPrevious">{t("impress.triggerAfterPrevious")}</option>
          </select>
        </label>
        <div className="row">
          <ToolNumber
            value={draft.durationMs}
            onChange={(durationMs) => setDraft({ ...draft, durationMs })}
            min={0}
            max={10000}
            step={50}
            title={t("impress.animationDuration")}
            width={80}
          />
          <ToolNumber
            value={draft.delayMs}
            onChange={(delayMs) => setDraft({ ...draft, delayMs })}
            min={0}
            max={10000}
            step={50}
            title={t("impress.animationDelay")}
            width={80}
          />
        </div>
        <div className="row">
          <button type="button" className="btn btn-soft" onClick={onClose}>
            {t("common.cancel")}
          </button>
          <button type="button" className="btn btn-primary" onClick={() => onSave({ ...draft, effect })}>
            {t("common.save")}
          </button>
        </div>
      </div>
    </Dialog>
  );
}
