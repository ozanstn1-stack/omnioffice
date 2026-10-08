/** The custom sort dialog: up to three keys, each ascending or descending. */
import { useState } from "react";
import { useT } from "../../../lib/i18n";
import { Dialog } from "../../office-ui";
import type { SortLevel } from "../sort";

export interface SortColumn {
  index: number;
  letter: string;
  /** The text in the first row of the range, used as the name when the data has headers. */
  header: string;
}

const LEVELS = 3;

interface LevelDraft {
  /** Column index, or null for "(none)" on the optional levels. */
  column: number | null;
  ascending: boolean;
}

export function SortDialog({
  columns,
  range,
  defaultColumn,
  headerGuess,
  onClose,
  onApply,
}: {
  columns: SortColumn[];
  /** The range being sorted, e.g. "A1:D20". */
  range: string;
  /** The column of the active cell, preselected as the first key. */
  defaultColumn: number;
  headerGuess: boolean;
  onClose: () => void;
  onApply: (levels: SortLevel[], hasHeaders: boolean) => void;
}) {
  const t = useT();
  const [hasHeaders, setHasHeaders] = useState(headerGuess);
  const [levels, setLevels] = useState<LevelDraft[]>(() =>
    Array.from({ length: LEVELS }, (_, level) => ({
      column: level === 0 ? (columns.find((column) => column.index === defaultColumn) ?? columns[0])?.index : null,
      ascending: true,
    })),
  );
  const patch = (level: number, change: Partial<LevelDraft>) =>
    setLevels((current) => current.map((entry, at) => (at === level ? { ...entry, ...change } : entry)));
  const nameOf = (column: SortColumn) =>
    hasHeaders && column.header ? column.header : t("calc.columnLabel", { column: column.letter });

  // A column serves one level only; a later level naming it again is dropped.
  const chosen: SortLevel[] = [];
  for (const level of levels) {
    if (level.column === null || chosen.some((entry) => entry.column === level.column)) continue;
    chosen.push({ column: level.column, ascending: level.ascending });
  }

  return (
    <Dialog title={t("calc.sort")} onClose={onClose}>
      <div className="stack">
        <p className="muted small">{t("calc.sortRange", { range })}</p>
        <label className="check">
          <input type="checkbox" checked={hasHeaders} onChange={(event) => setHasHeaders(event.target.checked)} />
          {t("calc.duplicatesHeaders")}
        </label>
        {levels.map((level, at) => {
          const label = at === 0 ? t("calc.sortBy") : t("calc.sortThenBy");
          return (
            <div key={at} className="row" role="group" aria-label={`${label} ${at + 1}`}>
              <label className="field">
                <span>{label}</span>
                <select
                  value={level.column ?? ""}
                  onChange={(event) =>
                    patch(at, { column: event.target.value === "" ? null : Number(event.target.value) })
                  }
                >
                  {at === 0 ? null : <option value="">{t("calc.sortNoColumn")}</option>}
                  {columns.map((column) => (
                    <option key={column.index} value={column.index}>
                      {nameOf(column)}
                    </option>
                  ))}
                </select>
              </label>
              <label className="field">
                <span>{t("calc.sortOrder")}</span>
                <select
                  value={level.ascending ? "asc" : "desc"}
                  disabled={level.column === null}
                  onChange={(event) => patch(at, { ascending: event.target.value === "asc" })}
                >
                  <option value="asc">{t("calc.sortAsc")}</option>
                  <option value="desc">{t("calc.sortDesc")}</option>
                </select>
              </label>
            </div>
          );
        })}
      </div>
      <div className="row" style={{ justifyContent: "flex-end", marginTop: 14 }}>
        <button type="button" className="btn btn-soft" onClick={onClose}>
          {t("common.cancel")}
        </button>
        <button
          type="button"
          className="btn btn-primary"
          disabled={chosen.length === 0}
          onClick={() => onApply(chosen, hasHeaders)}
        >
          {t("calc.sort")}
        </button>
      </div>
    </Dialog>
  );
}
