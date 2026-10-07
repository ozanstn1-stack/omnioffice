/** Structured tables: the insert dialog and the side panel listing them. */
import { useState } from "react";
import { Tag, Trash2 } from "lucide-react";
import { useT } from "../../../lib/i18n";
import type { SpreadsheetTable } from "../../../lib/office-types";
import { Dialog } from "../../office-ui";
import { parseRange } from "../formula";

/** Configures a new structured table over a cell range. */
export function InsertTableDialog({
  defaultName,
  defaultRange,
  onClose,
  onApply,
}: {
  defaultName: string;
  defaultRange: string;
  onClose: () => void;
  onApply: (config: {
    name: string;
    range: string;
    hasHeaders: boolean;
    hasTotals: boolean;
    bandedRows: boolean;
  }) => void;
}) {
  const t = useT();
  const [name, setName] = useState(defaultName);
  const [range, setRange] = useState(defaultRange);
  const [hasHeaders, setHasHeaders] = useState(true);
  const [hasTotals, setHasTotals] = useState(false);
  const [bandedRows, setBandedRows] = useState(true);
  const valid = name.trim() !== "" && parseRange(range) !== null;
  return (
    <Dialog title={t("calc.insertTable")} onClose={onClose}>
      <div className="stack">
        <label className="field">
          <span>{t("calc.tableName")}</span>
          <input className="input" value={name} onChange={(event) => setName(event.target.value)} />
        </label>
        <label className="field">
          <span>{t("calc.tableRange")}</span>
          <input className="input" value={range} onChange={(event) => setRange(event.target.value)} />
        </label>
        <label className="check">
          <input type="checkbox" checked={hasHeaders} onChange={(event) => setHasHeaders(event.target.checked)} />
          {t("calc.tableHeaders")}
        </label>
        <label className="check">
          <input type="checkbox" checked={hasTotals} onChange={(event) => setHasTotals(event.target.checked)} />
          {t("calc.tableTotals")}
        </label>
        <label className="check">
          <input type="checkbox" checked={bandedRows} onChange={(event) => setBandedRows(event.target.checked)} />
          {t("calc.tableBanded")}
        </label>
        <p className="muted small">=SUM(Name[Column])</p>
        <button
          type="button"
          className="btn btn-primary"
          disabled={!valid}
          onClick={() => onApply({ name, range, hasHeaders, hasTotals, bandedRows })}
        >
          {t("common.apply")}
        </button>
      </div>
    </Dialog>
  );
}

interface TablePanelDraft {
  name: string;
  formula: string;
  filter: string;
}

/**
 * Side panel listing the active sheet's structured tables.
 *
 * Selecting a table jumps to it; the panel renames, deletes, toggles the
 * totals/banding flags, appends a calculated column, and opens the shared
 * filter dialog scoped to one table column.
 */
export function TablesPanel({
  tables,
  onClose,
  onInsert,
  onJump,
  onRename,
  onDelete,
  onToggleTotals,
  onToggleBanded,
  onAddColumn,
  onFilter,
}: {
  tables: SpreadsheetTable[];
  onClose: () => void;
  onInsert: () => void;
  onJump: (table: SpreadsheetTable) => void;
  onRename: (table: SpreadsheetTable) => void;
  onDelete: (table: SpreadsheetTable) => void;
  onToggleTotals: (table: SpreadsheetTable) => void;
  onToggleBanded: (table: SpreadsheetTable) => void;
  onAddColumn: (table: SpreadsheetTable, name: string, formula: string) => void;
  onFilter: (table: SpreadsheetTable, column: string) => void;
}) {
  const t = useT();
  const [drafts, setDrafts] = useState<Record<string, TablePanelDraft>>({});
  const draftFor = (table: SpreadsheetTable): TablePanelDraft =>
    drafts[table.id] ?? { name: "", formula: "", filter: table.columns[0]?.name ?? "" };
  const patchDraft = (table: SpreadsheetTable, patch: Partial<TablePanelDraft>) => {
    setDrafts((current) => ({
      ...current,
      [table.id]: {
        ...(current[table.id] ?? { name: "", formula: "", filter: table.columns[0]?.name ?? "" }),
        ...patch,
      },
    }));
  };

  return (
    <aside
      className="calc-tables-panel"
      style={{
        position: "fixed",
        top: 150,
        right: 14,
        width: 320,
        maxHeight: "62vh",
        overflowY: "auto",
        background: "var(--surface)",
        border: "1px solid var(--border)",
        borderRadius: 10,
        boxShadow: "var(--shadow)",
        padding: 10,
        zIndex: 30,
      }}
    >
      <div className="row" style={{ alignItems: "center" }}>
        <strong>{t("calc.tableList")}</strong>
        <span className="spacer" />
        <button type="button" className="btn btn-soft" onClick={onInsert}>
          {t("calc.insertTable")}
        </button>
        <button type="button" className="icon-btn" onClick={onClose} aria-label={t("common.close")}>
          ×
        </button>
      </div>
      {tables.length === 0 ? <p className="muted small">{t("calc.noTables")}</p> : null}
      <div className="stack">
        {tables.map((table) => {
          const draft = draftFor(table);
          return (
            <div
              key={table.id}
              className="stack"
              style={{ border: "1px solid var(--border)", borderRadius: 8, padding: 8 }}
            >
              <div className="row" style={{ alignItems: "center", gap: 6 }}>
                <button
                  type="button"
                  className="btn btn-soft"
                  onClick={() => onJump(table)}
                  title={t("calc.tableJump")}
                >
                  {table.name}
                </button>
                <span className="muted small">
                  {table.range} · {table.columns.length}
                </span>
                <span className="spacer" />
                <button
                  type="button"
                  className="icon-btn"
                  onClick={() => onRename(table)}
                  title={t("calc.tableRename")}
                >
                  <Tag size={13} />
                </button>
                <button type="button" className="icon-btn" onClick={() => onDelete(table)} title={t("common.delete")}>
                  <Trash2 size={13} />
                </button>
              </div>
              <div className="row wrap" style={{ gap: 10 }}>
                <label className="check">
                  <input type="checkbox" checked={table.hasTotals} onChange={() => onToggleTotals(table)} />
                  {t("calc.tableTotals")}
                </label>
                <label className="check">
                  <input type="checkbox" checked={table.bandedRows} onChange={() => onToggleBanded(table)} />
                  {t("calc.tableBanded")}
                </label>
              </div>
              <div className="row wrap" style={{ gap: 6, alignItems: "flex-end" }}>
                <label className="field" style={{ flex: 1 }}>
                  <span>{t("calc.tableFilter")}</span>
                  <select
                    className="input"
                    value={draft.filter}
                    onChange={(event) => patchDraft(table, { filter: event.target.value })}
                  >
                    {table.columns.map((column) => (
                      <option key={column.name} value={column.name}>
                        {column.name}
                      </option>
                    ))}
                  </select>
                </label>
                <button
                  type="button"
                  className="btn btn-soft"
                  onClick={() => onFilter(table, draft.filter)}
                  disabled={draft.filter === ""}
                >
                  {t("calc.filter")}
                </button>
              </div>
              <div className="row wrap" style={{ gap: 6, alignItems: "flex-end" }}>
                <label className="field" style={{ flex: 1 }}>
                  <span>{t("calc.tableNewColumn")}</span>
                  <input
                    className="input"
                    value={draft.name}
                    onChange={(event) => patchDraft(table, { name: event.target.value })}
                  />
                </label>
                <label className="field" style={{ flex: 1.4 }}>
                  <span>{t("calc.tableFormula")}</span>
                  <input
                    className="input"
                    placeholder={`=${table.name}[${table.columns[0]?.name ?? "Column"}]`}
                    value={draft.formula}
                    onChange={(event) => patchDraft(table, { formula: event.target.value })}
                  />
                </label>
                <button
                  type="button"
                  className="btn btn-primary"
                  disabled={draft.name.trim() === "" || draft.formula.trim() === ""}
                  onClick={() => {
                    onAddColumn(table, draft.name, draft.formula);
                    patchDraft(table, { name: "", formula: "" });
                  }}
                >
                  {t("common.add")}
                </button>
              </div>
            </div>
          );
        })}
      </div>
    </aside>
  );
}
