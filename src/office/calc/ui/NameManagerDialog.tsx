import { useState } from "react";
import { Trash2 } from "lucide-react";
import { useT } from "../../../lib/i18n";
import type { NamedRange } from "../../../lib/office-types";
import { Dialog } from "../../office-ui";
import { parseAddress } from "../formula";

/**
 * Workbook- and sheet-scoped name manager.
 *
 * A name points at a range, a cell, a constant or a formula; the "this sheet
 * only" checkbox decides whether other sheets can see it. The scope rules match
 * what the formula evaluator does, so what is listed here is what resolves.
 */
export function NameManagerDialog({
  names,
  currentSheet,
  selection,
  onClose,
  onChange,
}: {
  names: NamedRange[];
  currentSheet: string;
  selection: string;
  onClose: () => void;
  onChange: (names: NamedRange[]) => void;
}) {
  const t = useT();
  const [entryName, setEntryName] = useState("");
  const [entryTarget, setEntryTarget] = useState(selection);
  const [entryScope, setEntryScope] = useState<"workbook" | "sheet">("workbook");

  const problem =
    entryName.trim() === ""
      ? t("calc.nameRequired")
      : !isValidDefinedName(entryName.trim())
        ? t("calc.nameInvalid")
        : "";

  const save = () => {
    if (problem) return;
    const definition = entryTarget.trim();
    const next: NamedRange = {
      name: entryName.trim().toUpperCase(),
      definition,
      sheet: entryScope === "sheet" ? currentSheet : null,
    };
    // Replace an existing name with the same identifier and scope.
    const without = names.filter((entry) => !(entry.name === next.name && entry.sheet === next.sheet));
    onChange([...without, next]);
  };

  return (
    <Dialog title={t("calc.nameManager")} onClose={onClose} wide>
      <div className="stack">
        <div className="row wrap" style={{ gap: 8 }}>
          <input
            className="input"
            value={entryName}
            placeholder={t("calc.namePlaceholder")}
            onChange={(event) => setEntryName(event.target.value)}
          />
          <input
            className="input"
            value={entryTarget}
            placeholder={t("calc.nameTarget")}
            onChange={(event) => setEntryTarget(event.target.value)}
          />
          <label className="check">
            <input
              type="checkbox"
              checked={entryScope === "sheet"}
              onChange={(event) => setEntryScope(event.target.checked ? "sheet" : "workbook")}
            />
            {t("calc.nameThisSheetOnly")}
          </label>
          <button type="button" className="btn btn-primary" onClick={save} disabled={problem !== ""}>
            {t("common.add")}
          </button>
        </div>
        {problem ? <p className="muted small">{problem}</p> : null}

        <table className="data-table">
          <thead>
            <tr>
              <th>{t("calc.nameColumn")}</th>
              <th>{t("calc.nameTarget")}</th>
              <th>{t("calc.nameScope")}</th>
              <th>
                <span className="sr-only">{t("common.actions")}</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {names.length === 0 ? (
              <tr>
                <td colSpan={4} className="muted">
                  {t("calc.noNames")}
                </td>
              </tr>
            ) : null}
            {names.map((entry) => (
              <tr key={`${entry.sheet ?? ""}:${entry.name}`}>
                <td>
                  <input
                    className="input"
                    aria-label={t("calc.nameColumn")}
                    defaultValue={entry.name}
                    onBlur={(event) => {
                      const next = event.target.value.trim().toUpperCase();
                      if (!isValidDefinedName(next) || next === entry.name) return;
                      onChange(
                        names.map((candidate) => (candidate === entry ? { ...candidate, name: next } : candidate)),
                      );
                    }}
                  />
                </td>
                <td>
                  <input
                    className="input"
                    aria-label={t("calc.nameTarget")}
                    defaultValue={entry.definition}
                    onBlur={(event) =>
                      onChange(
                        names.map((candidate) =>
                          candidate === entry ? { ...candidate, definition: event.target.value.trim() } : candidate,
                        ),
                      )
                    }
                  />
                </td>
                <td className="muted">{entry.sheet ?? t("calc.nameWorkbookScope")}</td>
                <td>
                  <button
                    type="button"
                    className="icon-btn"
                    aria-label={t("common.remove")}
                    onClick={() => onChange(names.filter((candidate) => candidate !== entry))}
                  >
                    <Trash2 size={13} />
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </Dialog>
  );
}

/**
 * Excel's rule for a legal name: it must start with a letter or underscore,
 * may contain letters, digits, dots and underscores, and must not look like a
 * cell reference (otherwise the formula parser reads `A1` as a cell).
 */
export function isValidDefinedName(name: string): boolean {
  if (!/^[A-Za-z_][A-Za-z0-9_.]*$/.test(name)) return false;
  return parseAddress(name) === null;
}
