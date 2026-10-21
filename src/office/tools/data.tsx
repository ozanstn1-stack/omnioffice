/** Data sheets: CSV/JSON import, grid editing and export. */
import { useEffect, useState } from "react";
import { Download, FileDown, Plus, Trash2 } from "lucide-react";
import { useT } from "../../lib/i18n";
import { reportError, useToasts } from "../../lib/store";
import { pickFileBytes, saveFileBytes } from "../../lib/mobile";
import { useDataSheets } from "../../lib/office-store";
import { Screen } from "../../components/layout";

// ---------------------------------------------------------------------------
// Data sheets
// ---------------------------------------------------------------------------

/**
 * Parses CSV text (RFC 4180 quoting: quoted delimiters, doubled quotes and
 * line breaks inside quotes). The delimiter is `;` when the header line has
 * more semicolons than commas outside quotes, `,` otherwise. Empty lines are
 * skipped and short rows are padded to the header width.
 */
export function parseCsv(text: string): { columns: string[]; rows: string[][] } {
  const source = text.replace(/^\uFEFF/, "");
  const firstLine = source.split(/\r?\n/, 1)[0] ?? "";
  const unquoted = firstLine.replace(/"[^"]*"/g, "");
  const delimiter = (unquoted.match(/;/g)?.length ?? 0) > (unquoted.match(/,/g)?.length ?? 0) ? ";" : ",";
  const records: string[][] = [];
  let record: string[] = [];
  let cell = "";
  let quoted = false;
  for (let index = 0; index < source.length; index += 1) {
    const char = source[index];
    if (quoted) {
      if (char === '"') {
        if (source[index + 1] === '"') {
          cell += '"';
          index += 1;
        } else {
          quoted = false;
        }
      } else {
        cell += char;
      }
    } else if (char === '"' && cell === "") {
      quoted = true;
    } else if (char === delimiter) {
      record.push(cell);
      cell = "";
    } else if (char === "\n" || char === "\r") {
      if (char === "\r" && source[index + 1] === "\n") index += 1;
      record.push(cell);
      records.push(record);
      record = [];
      cell = "";
    } else {
      cell += char;
    }
  }
  if (cell !== "" || record.length) {
    record.push(cell);
    records.push(record);
  }
  const nonEmpty = records.filter((row) => row.some((value) => value.trim() !== ""));
  const columns = nonEmpty[0] ?? [];
  const rows = nonEmpty
    .slice(1)
    .map((row) =>
      row.length < columns.length ? [...row, ...Array<string>(columns.length - row.length).fill("")] : row,
    );
  return { columns, rows };
}

/**
 * Turns JSON into a table: an array of objects (columns are the union of the
 * keys, in first-seen order), a single object (one row) or an array of
 * arrays (first row is the header). Anything else is rejected.
 */
export function parseJsonTable(text: string): { columns: string[]; rows: string[][] } {
  const parsed: unknown = JSON.parse(text);
  const cellText = (value: unknown): string =>
    value === null || value === undefined ? "" : typeof value === "object" ? JSON.stringify(value) : String(value);
  const items = Array.isArray(parsed) ? parsed : [parsed];
  if (items.length && items.every((item) => Array.isArray(item))) {
    const [header = [], ...body] = items as unknown[][];
    return { columns: header.map(cellText), rows: body.map((row) => row.map(cellText)) };
  }
  if (!items.every((item) => item !== null && typeof item === "object" && !Array.isArray(item))) {
    throw new Error("The JSON file must contain an object or a list of objects.");
  }
  const records = items as Record<string, unknown>[];
  const columns: string[] = [];
  for (const record of records) {
    for (const key of Object.keys(record)) if (!columns.includes(key)) columns.push(key);
  }
  return { columns, rows: records.map((record) => columns.map((column) => cellText(record[column]))) };
}

export function DataScreen() {
  const t = useT();
  const { sheets, loaded, load, create, update, remove } = useDataSheets();
  const [activeId, setActiveId] = useState<string | null>(null);
  const active = sheets.find((sheet) => sheet.id === activeId) ?? sheets[0] ?? null;

  useEffect(() => {
    if (!loaded) void load();
  }, [loaded, load]);

  const importFile = async () => {
    try {
      const picked = await pickFileBytes({
        name: "Data",
        extensions: ["csv", "json"],
        mimeTypes: ["text/csv", "text/comma-separated-values", "application/json", "text/plain", "*/*"],
      });
      if (!picked) return;
      const text = new TextDecoder().decode(picked.bytes);
      const { columns, rows } = picked.name.toLowerCase().endsWith(".json") ? parseJsonTable(text) : parseCsv(text);
      const sheet = create(picked.name.replace(/\.(csv|json)$/i, "") || "Imported");
      update(sheet.id, { columns, rows });
      setActiveId(sheet.id);
      useToasts.getState().push({ kind: "success", title: t("data.imported") });
    } catch (error) {
      reportError(error, t);
    }
  };

  const exportFile = async (format: "csv" | "json") => {
    if (!active) return;
    try {
      const content =
        format === "csv"
          ? [active.columns, ...active.rows]
              .map((row) =>
                row.map((cell) => (/[",;\r\n]/.test(cell) ? `"${cell.replace(/"/g, '""')}"` : cell)).join(","),
              )
              .join("\r\n")
          : JSON.stringify(
              active.rows.map((row) =>
                Object.fromEntries(active.columns.map((column, index) => [column, row[index] ?? ""])),
              ),
              null,
              2,
            );
      const saved = await saveFileBytes(new TextEncoder().encode(content), `${active.name}.${format}`, {
        name: format.toUpperCase(),
        extensions: [format],
      });
      if (!saved) return;
      useToasts.getState().push({ kind: "success", title: t("data.exported"), detail: saved });
    } catch (error) {
      reportError(error, t);
    }
  };

  return (
    <Screen
      title={t("data.title")}
      subtitle={t("data.subtitle")}
      actions={
        <div className="row">
          <button type="button" className="btn btn-soft" onClick={importFile}>
            <Download size={15} /> {t("data.import")}
          </button>
          <button type="button" className="btn btn-soft" onClick={() => exportFile("csv")} disabled={!active}>
            <FileDown size={15} /> CSV
          </button>
          <button type="button" className="btn btn-soft" onClick={() => exportFile("json")} disabled={!active}>
            <FileDown size={15} /> JSON
          </button>
          <button type="button" className="btn btn-primary" onClick={() => setActiveId(create().id)}>
            <Plus size={15} /> {t("data.newTable")}
          </button>
        </div>
      }
    >
      <div className="data-layout">
        <div className="card data-list">
          {sheets.map((sheet) => (
            <button
              key={sheet.id}
              type="button"
              className={`note-item${active?.id === sheet.id ? " is-active" : ""}`}
              onClick={() => setActiveId(sheet.id)}
              aria-label={sheet.name}
            >
              <div className="row">
                <strong>{sheet.name}</strong>
                <span className="spacer" />
                <span className="muted">{sheet.rows.length}</span>
              </div>
            </button>
          ))}
          {sheets.length === 0 ? <p className="muted">{t("data.none")}</p> : null}
        </div>
        <div className="card data-table-card">
          {active ? (
            <>
              <div className="row">
                <input
                  className="note-title"
                  value={active.name}
                  onChange={(event) => update(active.id, { name: event.target.value })}
                />
                <button
                  type="button"
                  className="btn btn-soft"
                  onClick={() => update(active.id, { rows: [...active.rows, active.columns.map(() => "")] })}
                >
                  <Plus size={14} /> {t("data.addRow")}
                </button>
                <button
                  type="button"
                  className="btn btn-soft"
                  onClick={() => {
                    const column = window.prompt(t("data.columnName"));
                    if (column)
                      update(active.id, {
                        columns: [...active.columns, column],
                        rows: active.rows.map((row) => [...row, ""]),
                      });
                  }}
                >
                  <Plus size={14} /> {t("data.addColumn")}
                </button>
                <button
                  type="button"
                  className="btn btn-soft"
                  onClick={() => {
                    remove(active.id);
                    setActiveId(null);
                  }}
                  aria-label={t("data.deleteTable")}
                >
                  <Trash2 size={14} />
                </button>
              </div>
              <div className="data-grid-wrap">
                <table className="table data-grid">
                  <thead>
                    <tr>
                      <th>
                        <span className="sr-only">{t("data.row")}</span>
                      </th>
                      {active.columns.map((column, columnIndex) => (
                        <th key={columnIndex}>
                          <input
                            aria-label={t("data.columnName", { index: columnIndex + 1 })}
                            value={column}
                            onChange={(event) =>
                              update(active.id, {
                                columns: active.columns.map((candidate, index) =>
                                  index === columnIndex ? event.target.value : candidate,
                                ),
                              })
                            }
                          />
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {active.rows.map((row, rowIndex) => (
                      <tr key={rowIndex}>
                        <td className="row-index">{rowIndex + 1}</td>
                        {active.columns.map((column, columnIndex) => (
                          <td key={columnIndex}>
                            <input
                              aria-label={`${column || t("data.columnName", { index: columnIndex + 1 })} — ${t("data.row")} ${rowIndex + 1}`}
                              value={row[columnIndex] ?? ""}
                              onChange={(event) =>
                                update(active.id, {
                                  rows: active.rows.map((candidate, index) =>
                                    index === rowIndex
                                      ? candidate.map((cell, cellIndex) =>
                                          cellIndex === columnIndex ? event.target.value : cell,
                                        )
                                      : candidate,
                                  ),
                                })
                              }
                            />
                          </td>
                        ))}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </>
          ) : (
            <p className="muted">{t("data.none")}</p>
          )}
        </div>
      </div>
    </Screen>
  );
}
