import { useState } from "react";
import { useT } from "../../../lib/i18n";
import { Dialog } from "../../office-ui";

/** Adds a data-validation rule over the selection. */
export function ValidationDialog({
  onClose,
  onApply,
}: {
  onClose: () => void;
  onApply: (validation: {
    kind: string;
    values: string[];
    min: number | null;
    max: number | null;
    message: string;
  }) => void;
}) {
  const t = useT();
  const [kind, setKind] = useState("list");
  const [list, setList] = useState("Open,In progress,Done");
  const [min, setMin] = useState("0");
  const [max, setMax] = useState("100");
  const [message, setMessage] = useState("");
  return (
    <Dialog title={t("calc.dataValidation")} onClose={onClose}>
      <div className="stack">
        <label className="field">
          <span>{t("calc.validationType")}</span>
          <select value={kind} onChange={(event) => setKind(event.target.value)}>
            <option value="list">{t("calc.validationList")}</option>
            <option value="number">{t("calc.validationNumber")}</option>
          </select>
        </label>
        {kind === "list" ? (
          <label className="field">
            <span>{t("calc.validationValues")}</span>
            <input value={list} onChange={(event) => setList(event.target.value)} />
          </label>
        ) : (
          <div className="row">
            <label className="field">
              <span>{t("calc.minimum")}</span>
              <input value={min} onChange={(event) => setMin(event.target.value)} />
            </label>
            <label className="field">
              <span>{t("calc.maximum")}</span>
              <input value={max} onChange={(event) => setMax(event.target.value)} />
            </label>
          </div>
        )}
        <label className="field">
          <span>{t("calc.validationMessage")}</span>
          <input value={message} onChange={(event) => setMessage(event.target.value)} />
        </label>
        <button
          type="button"
          className="btn btn-primary"
          onClick={() =>
            onApply({
              kind,
              values: list.split(",").map((entry) => entry.trim()),
              min: kind === "number" ? Number(min) : null,
              max: kind === "number" ? Number(max) : null,
              message,
            })
          }
        >
          {t("common.apply")}
        </button>
      </div>
    </Dialog>
  );
}
