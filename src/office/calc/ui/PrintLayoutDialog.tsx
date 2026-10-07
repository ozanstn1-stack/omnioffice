import { useState } from "react";
import { useT } from "../../../lib/i18n";
import type { PrintSettings } from "../../../lib/office-types";
import { Dialog } from "../../office-ui";

/** Paper, orientation, scaling and header/footer for printing and PDF export. */
export function PrintLayoutDialog({
  print,
  sheetName,
  onClose,
  onApply,
}: {
  print: PrintSettings;
  sheetName: string;
  onClose: () => void;
  onApply: (print: PrintSettings) => void;
}) {
  const t = useT();
  const [draft, setDraft] = useState<PrintSettings>({ ...print });
  const patch = (next: Partial<PrintSettings>) => setDraft((current) => ({ ...current, ...next }));

  return (
    <Dialog title={t("calc.printSetup")} onClose={onClose} wide>
      <div className="stack">
        <label className="field">
          <span>{t("calc.paperSize")}</span>
          <select
            className="input"
            value={draft.paperSize}
            onChange={(event) => patch({ paperSize: Number(event.target.value) })}
          >
            <option value={9}>A4</option>
            <option value={1}>Letter</option>
            <option value={5}>Legal</option>
            <option value={8}>A3</option>
            <option value={9}>A4</option>
            <option value={11}>A5</option>
          </select>
        </label>
        <label className="check">
          <input
            type="checkbox"
            checked={draft.landscape}
            onChange={(event) => patch({ landscape: event.target.checked })}
          />
          {t("calc.landscape")}
        </label>
        <label className="field">
          <span>{t("calc.scale")}</span>
          <input
            className="input"
            type="number"
            min={10}
            max={400}
            value={draft.scale}
            onChange={(event) => patch({ scale: Math.min(400, Math.max(10, Number(event.target.value) || 100)) })}
          />
        </label>
        <label className="field">
          <span>{t("calc.fitToWidth")}</span>
          <input
            className="input"
            type="number"
            min={0}
            max={10}
            value={draft.fitToWidth}
            onChange={(event) => patch({ fitToWidth: Math.max(0, Number(event.target.value) || 0) })}
          />
        </label>
        <label className="field">
          <span>{t("calc.printTitlesRows")}</span>
          <input
            className="input"
            placeholder="1:1"
            value={draft.printTitlesRows ?? ""}
            onChange={(event) =>
              patch({ printTitlesRows: event.target.value.trim() === "" ? null : event.target.value.trim() })
            }
          />
        </label>
        <label className="check">
          <input
            type="checkbox"
            checked={draft.printGridlines}
            onChange={(event) => patch({ printGridlines: event.target.checked })}
          />
          {t("calc.printGridlines")}
        </label>
        <label className="check">
          <input
            type="checkbox"
            checked={draft.printHeadings}
            onChange={(event) => patch({ printHeadings: event.target.checked })}
          />
          {t("calc.printHeadings")}
        </label>
        <label className="check">
          <input
            type="checkbox"
            checked={draft.centerHorizontally}
            onChange={(event) => patch({ centerHorizontally: event.target.checked })}
          />
          {t("calc.centerHorizontally")}
        </label>
        <label className="field">
          <span>{t("calc.header")}</span>
          <input className="input" value={draft.header} onChange={(event) => patch({ header: event.target.value })} />
        </label>
        <p className="muted small">{t("calc.printSheetNote", { sheet: sheetName })}</p>
      </div>
      <div className="row" style={{ justifyContent: "flex-end", marginTop: 14 }}>
        <button type="button" className="btn btn-soft" onClick={onClose}>
          {t("common.cancel")}
        </button>
        <button type="button" className="btn btn-primary" onClick={() => onApply(draft)}>
          {t("common.apply")}
        </button>
      </div>
    </Dialog>
  );
}
