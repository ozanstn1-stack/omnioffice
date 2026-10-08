/** The live pivot overlay and the dialog that configures a new pivot. */
import { useMemo, useState } from "react";
import { RefreshCw, Trash2 } from "lucide-react";
import { useT } from "../../../lib/i18n";
import type { PivotTable, PivotValueField, Sheet, Workbook } from "../../../lib/office-types";
import { Dialog } from "../../office-ui";
import { usedRange } from "../cells";
import { isError } from "../formula";
import { computePivot, pivotFields } from "../pivot";

/** The live pivot grid, rendered over the sheet at the pivot's anchor. */
export function PivotBox({
  pivot,
  sheet,
  workbook,
  x,
  y,
  onRemove,
}: {
  pivot: PivotTable;
  sheet: Sheet;
  workbook: Workbook;
  x: number;
  y: number;
  onRemove: () => void;
}) {
  const t = useT();
  const [refreshToken, setRefreshToken] = useState(0);
  // Recomputing on every relevant render keeps the pivot live; the refresh
  // button is for an explicit "show me the current data" action and bumps a
  // token so the memo is invalidated even when nothing else changed.
  // eslint-disable-next-line react-hooks/exhaustive-deps -- refreshToken forces the explicit refresh; sheet keeps the pivot live across sheet edits
  const result = useMemo(() => computePivot(workbook, pivot), [workbook, pivot, refreshToken, sheet]);
  return (
    <div className="pivot-box" style={{ left: x, top: y }}>
      <div className="chart-head">
        <strong>{pivot.name}</strong>
        <span className="spacer" />
        <button
          type="button"
          className="icon-btn"
          onClick={() => setRefreshToken((value) => value + 1)}
          title={t("calc.pivotRefresh")}
        >
          <RefreshCw size={12} />
        </button>
        <button type="button" className="icon-btn" onClick={onRemove} title={t("common.delete")}>
          <Trash2 size={12} />
        </button>
      </div>
      {result ? (
        <table className="pivot-grid">
          <tbody>
            {result.grid.map((line, rowIndex) => (
              <tr key={rowIndex}>
                {line.map((value, colIndex) => (
                  <td
                    key={colIndex}
                    className={colIndex < result.rowFieldCount || rowIndex === 0 ? "is-label" : undefined}
                  >
                    {isError(value) ? value.code : value === "" ? "" : String(value)}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      ) : (
        <p className="muted" style={{ padding: "6px 10px" }}>
          {t("calc.pivotNeedsData")}
        </p>
      )}
    </div>
  );
}

/** Configure a pivot over the sheet's used range. */
export function PivotDialog({
  workbook,
  sheet,
  onClose,
  onApply,
}: {
  workbook: Workbook;
  sheet: Sheet;
  onClose: () => void;
  onApply: (config: {
    rows: string[];
    columns: string[];
    values: PivotValueField[];
    filters: PivotTable["filters"];
  }) => void;
}) {
  const t = useT();
  const source = usedRange(sheet);
  const fields = pivotFields(workbook, sheet.name, source);
  const [row, setRow] = useState(fields[0] ?? "");
  const [column, setColumn] = useState("");
  const [value, setValue] = useState(fields[fields.length - 1] ?? fields[0] ?? "");
  const [aggregation, setAggregation] = useState<PivotValueField["aggregation"]>("sum");
  return (
    <Dialog title={t("calc.pivotTable")} onClose={onClose}>
      <div className="stack">
        <p className="muted">
          {t("calc.pivotHint")} · {source}
        </p>
        <label className="field">
          <span>{t("calc.pivotRows")}</span>
          <select value={row} onChange={(event) => setRow(event.target.value)}>
            {fields.map((field) => (
              <option key={field} value={field}>
                {field}
              </option>
            ))}
          </select>
        </label>
        <label className="field">
          <span>{t("calc.pivotColumns")}</span>
          <select value={column} onChange={(event) => setColumn(event.target.value)}>
            <option value="">—</option>
            {fields.map((field) => (
              <option key={field} value={field}>
                {field}
              </option>
            ))}
          </select>
        </label>
        <label className="field">
          <span>{t("calc.pivotValues")}</span>
          <select value={value} onChange={(event) => setValue(event.target.value)}>
            {fields.map((field) => (
              <option key={field} value={field}>
                {field}
              </option>
            ))}
          </select>
        </label>
        <label className="field">
          <span>{t("calc.pivotAggregation")}</span>
          <select
            value={aggregation}
            onChange={(event) => setAggregation(event.target.value as PivotValueField["aggregation"])}
          >
            {(["sum", "count", "average", "min", "max"] as const).map((kind) => (
              <option key={kind} value={kind}>
                {t(`calc.agg_${kind}`)}
              </option>
            ))}
          </select>
        </label>
        <button
          type="button"
          className="btn btn-primary"
          disabled={fields.length < 2}
          onClick={() =>
            onApply({
              rows: row ? [row] : [],
              columns: column ? [column] : [],
              values: value ? [{ field: value, aggregation }] : [],
              filters: [],
            })
          }
        >
          {t("calc.pivotInsert")}
        </button>
      </div>
    </Dialog>
  );
}
