/** The value-list filter dialog for one column. */
import { useT } from "../../../lib/i18n";
import { Dialog } from "../../office-ui";
import type { FilterDraft } from "../filter";

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
