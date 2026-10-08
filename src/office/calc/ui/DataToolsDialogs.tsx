/** Text to columns and remove duplicates. */
import { useId, useState } from "react";
import { useT } from "../../../lib/i18n";
import { Dialog } from "../../office-ui";
import { planTextToColumns, type ColumnSplitPlan, type DuplicateOptions, type SplitDelimiter } from "../data-tools";
import { columnLabel } from "../formula";

/**
 * Text to columns: splits the selected column on a delimiter into itself and
 * the columns to its right. The preview shows the first rows; replacing data
 * already in the target cells needs a second, explicit confirmation.
 */
export function TextToColumnsDialog({
  texts,
  startColumn,
  holdsData,
  onClose,
  onApply,
}: {
  texts: string[];
  startColumn: number;
  /** True when the target cell at this offset from the first source cell holds data. */
  holdsData: (rowOffset: number, colOffset: number) => boolean;
  onClose: () => void;
  onApply: (plan: ColumnSplitPlan) => void;
}) {
  const t = useT();
  const radioName = useId();
  const [delimiter, setDelimiter] = useState<SplitDelimiter>("comma");
  const [custom, setCustom] = useState("");
  const [mergeConsecutive, setMergeConsecutive] = useState(false);
  // How many target cells would be replaced; non-null while asking about it.
  const [overwrites, setOverwrites] = useState<number | null>(null);
  const plan = planTextToColumns(texts, { delimiter, custom, mergeConsecutive });
  const splits = plan.rows.some((pieces) => pieces !== null);
  const preview = plan.rows.slice(0, 5).map((pieces, index) => pieces ?? [texts[index]]);
  const previewWidth = Math.max(1, ...preview.map((pieces) => pieces.length));

  const split = () => {
    if (overwrites === null) {
      let count = 0;
      plan.rows.forEach((pieces, rowOffset) => {
        for (let colOffset = 1; colOffset < (pieces?.length ?? 0); colOffset += 1) {
          if (holdsData(rowOffset, colOffset)) count += 1;
        }
      });
      if (count > 0) {
        setOverwrites(count);
        return;
      }
    }
    onApply(plan);
  };

  return (
    <Dialog title={t("calc.textToColumns")} onClose={onClose} wide>
      <div className="stack">
        <fieldset className="calc-tool-fieldset">
          <legend>{t("calc.delimiter")}</legend>
          {(["comma", "semicolon", "tab", "space", "custom"] as const).map((id) => (
            <label key={id} className="check">
              <input
                type="radio"
                name={radioName}
                checked={delimiter === id}
                onChange={() => {
                  setDelimiter(id);
                  setOverwrites(null);
                }}
              />
              {t(`calc.delimiter_${id}`)}
            </label>
          ))}
          <input
            className="input calc-split-custom"
            aria-label={t("calc.delimiterCustomValue")}
            value={custom}
            maxLength={8}
            onChange={(event) => {
              setCustom(event.target.value);
              setDelimiter("custom");
              setOverwrites(null);
            }}
          />
        </fieldset>
        <label className="check">
          <input
            type="checkbox"
            checked={mergeConsecutive}
            onChange={(event) => {
              setMergeConsecutive(event.target.checked);
              setOverwrites(null);
            }}
          />
          {t("calc.mergeDelimiters")}
        </label>
        <div className="calc-split-preview">
          <table aria-label={t("calc.preview")}>
            <thead>
              <tr>
                {Array.from({ length: previewWidth }, (_, index) => (
                  <th key={index}>{columnLabel(startColumn + index)}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {preview.map((pieces, row) => (
                <tr key={row}>
                  {Array.from({ length: previewWidth }, (_, col) => (
                    <td key={col}>{pieces[col] ?? ""}</td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {splits ? null : <p className="muted small">{t("calc.splitNothing")}</p>}
        {overwrites !== null ? (
          <p className="calc-tool-warning" role="alert">
            {t("calc.splitOverwrite", { count: overwrites })}
          </p>
        ) : null}
      </div>
      <div className="row" style={{ justifyContent: "flex-end", marginTop: 14 }}>
        <button
          type="button"
          className="btn btn-soft"
          onClick={overwrites === null ? onClose : () => setOverwrites(null)}
        >
          {t("common.cancel")}
        </button>
        <button type="button" className="btn btn-primary" disabled={!splits} onClick={split}>
          {overwrites === null ? t("calc.split") : t("calc.splitReplace")}
        </button>
      </div>
    </Dialog>
  );
}

/**
 * Remove duplicates: picks the columns two rows must agree on. Text compares
 * case-insensitively, and the header row, when there is one, always stays.
 */
export function RemoveDuplicatesDialog({
  columns,
  onClose,
  onApply,
}: {
  columns: Array<{ letter: string; header: string }>;
  onClose: () => void;
  onApply: (options: DuplicateOptions) => void;
}) {
  const t = useT();
  const [hasHeaders, setHasHeaders] = useState(false);
  const [checked, setChecked] = useState(() => columns.map(() => true));
  const chosen = checked.flatMap((on, index) => (on ? [index] : []));
  return (
    <Dialog title={t("calc.removeDuplicates")} onClose={onClose}>
      <div className="stack">
        <label className="check">
          <input type="checkbox" checked={hasHeaders} onChange={(event) => setHasHeaders(event.target.checked)} />
          {t("calc.duplicatesHeaders")}
        </label>
        <fieldset className="calc-tool-fieldset is-list">
          <legend>{t("calc.duplicatesColumns")}</legend>
          {columns.map((column, index) => (
            <label key={column.letter} className="check">
              <input
                type="checkbox"
                checked={checked[index]}
                onChange={(event) =>
                  setChecked((current) => current.map((on, at) => (at === index ? event.target.checked : on)))
                }
              />
              {hasHeaders && column.header ? column.header : t("calc.columnLabel", { column: column.letter })}
            </label>
          ))}
        </fieldset>
        <p className="muted small">{t("calc.duplicatesHint")}</p>
      </div>
      <div className="row" style={{ justifyContent: "flex-end", marginTop: 14 }}>
        <button type="button" className="btn btn-soft" onClick={onClose}>
          {t("common.cancel")}
        </button>
        <button
          type="button"
          className="btn btn-primary"
          disabled={chosen.length === 0}
          onClick={() => onApply({ columns: chosen, hasHeaders })}
        >
          {t("calc.removeDuplicates")}
        </button>
      </div>
    </Dialog>
  );
}
