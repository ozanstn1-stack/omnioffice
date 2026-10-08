/** The value-list filter dialog for one column. */
import { useT } from "../../../lib/i18n";
import { Dialog } from "../../office-ui";
import type { ConditionOp, FilterCondition, FilterDraft } from "../filter";

const TEXT_OPS: ConditionOp[] = ["contains", "begins", "ends", "equals"];
const NUMBER_OPS: ConditionOp[] = ["greater", "less", "between", "top", "bottom"];
const BLANK_OPS: ConditionOp[] = ["blank", "nonblank"];

/** The operator select and the inputs its condition needs. */
function ConditionEditor({
  condition,
  onChange,
}: {
  condition: FilterCondition;
  onChange: (condition: FilterCondition) => void;
}) {
  const t = useT();
  const { op } = condition;
  const counts = op === "top" || op === "bottom";
  return (
    <div className="stack">
      <label className="field">
        <span>{t("calc.filterCondition")}</span>
        <select value={op} onChange={(event) => onChange({ ...condition, op: event.target.value as ConditionOp })}>
          <option value="none">{t("calc.filterNoCondition")}</option>
          <optgroup label={t("calc.filterGroupText")}>
            {TEXT_OPS.map((entry) => (
              <option key={entry} value={entry}>
                {t(`calc.filterOp_${entry}`)}
              </option>
            ))}
          </optgroup>
          <optgroup label={t("calc.filterGroupNumber")}>
            {NUMBER_OPS.map((entry) => (
              <option key={entry} value={entry}>
                {t(`calc.filterOp_${entry}`)}
              </option>
            ))}
          </optgroup>
          <optgroup label={t("calc.filterGroupBlank")}>
            {BLANK_OPS.map((entry) => (
              <option key={entry} value={entry}>
                {t(`calc.filterOp_${entry}`)}
              </option>
            ))}
          </optgroup>
        </select>
      </label>
      {op === "none" || op === "blank" || op === "nonblank" ? null : (
        <div className="row">
          <label className="field">
            <span>{counts ? t("calc.filterCount") : t("calc.value")}</span>
            <input
              className="input"
              inputMode={op === "contains" || op === "begins" || op === "ends" || op === "equals" ? "text" : "decimal"}
              value={condition.value}
              onChange={(event) => onChange({ ...condition, value: event.target.value })}
            />
          </label>
          {op === "between" ? (
            <label className="field">
              <span>{t("calc.and")}</span>
              <input
                className="input"
                inputMode="decimal"
                value={condition.value2}
                onChange={(event) => onChange({ ...condition, value2: event.target.value })}
              />
            </label>
          ) : null}
        </div>
      )}
      {op === "none" ? null : <p className="muted small">{t("calc.filterAndHint")}</p>}
    </div>
  );
}

export function FilterDialog({
  draft,
  onChange,
  onApply,
  onClear,
  onClose,
}: {
  draft: FilterDraft;
  onChange: (draft: FilterDraft) => void;
  onApply: () => void;
  onClear: () => void;
  onClose: () => void;
}) {
  const t = useT();
  const setChecked = (index: number, checked: boolean) =>
    onChange({
      ...draft,
      values: draft.values.map((candidate, position) => (position === index ? { ...candidate, checked } : candidate)),
    });
  return (
    <Dialog title={draft.tableName ? `${t("calc.filter")} · ${draft.tableName}` : t("calc.filter")} onClose={onClose}>
      <ConditionEditor condition={draft.condition} onChange={(condition) => onChange({ ...draft, condition })} />
      <div className="stack filter-list">
        {draft.values.map((entry, index) => (
          <label key={entry.value} className="check">
            <input
              type="checkbox"
              checked={entry.checked}
              onChange={(event) => setChecked(index, event.target.checked)}
            />
            {entry.value || t("calc.filterBlank")}
          </label>
        ))}
      </div>
      <div className="row">
        <button
          type="button"
          className="btn btn-soft"
          onClick={() => onChange({ ...draft, values: draft.values.map((entry) => ({ ...entry, checked: true })) })}
        >
          {t("calc.selectAll")}
        </button>
        <button type="button" className="btn btn-primary" onClick={onApply}>
          {t("calc.applyFilter")}
        </button>
        <button type="button" className="btn btn-soft" onClick={onClear}>
          {t("calc.clearFilter")}
        </button>
      </div>
    </Dialog>
  );
}
