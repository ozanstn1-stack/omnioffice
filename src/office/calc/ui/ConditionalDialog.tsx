import { useState } from "react";
import { useT } from "../../../lib/i18n";
import { Dialog } from "../../office-ui";

/** Adds a conditional-format rule over the sheet's used range. */
export function ConditionalDialog({
  onClose,
  onApply,
}: {
  onClose: () => void;
  onApply: (rule: { kind: string; values: string[]; fill: string; topN?: number }) => void;
}) {
  const t = useT();
  const [kind, setKind] = useState("greater");
  const [first, setFirst] = useState("100");
  const [second, setSecond] = useState("0");
  const [fill, setFill] = useState("#FEE2E2");
  return (
    <Dialog title={t("calc.conditionalFormatting")} onClose={onClose}>
      <div className="stack">
        <label className="field">
          <span>{t("calc.rule")}</span>
          <select
            value={kind}
            onChange={(event) => {
              setKind(event.target.value);
              if (event.target.value === "dataBar") setFill("#638EC6");
            }}
          >
            <option value="greater">{t("calc.ruleGreater")}</option>
            <option value="less">{t("calc.ruleLess")}</option>
            <option value="between">{t("calc.ruleBetween")}</option>
            <option value="equal">{t("calc.ruleEqual")}</option>
            <option value="textContains">{t("calc.ruleText")}</option>
            <option value="duplicate">{t("calc.ruleDuplicate")}</option>
            <option value="top">{t("calc.ruleTop")}</option>
            <option value="dataBar">{t("calc.ruleDataBar")}</option>
          </select>
        </label>
        {kind === "duplicate" || kind === "dataBar" ? null : (
          <div className="row">
            <label className="field">
              <span>{t("calc.value")}</span>
              <input value={first} onChange={(event) => setFirst(event.target.value)} />
            </label>
            {kind === "between" ? (
              <label className="field">
                <span>{t("calc.and")}</span>
                <input value={second} onChange={(event) => setSecond(event.target.value)} />
              </label>
            ) : null}
          </div>
        )}
        <label className="field">
          <span>{t("calc.fillColor")}</span>
          <input type="color" value={fill} onChange={(event) => setFill(event.target.value)} />
        </label>
        <button
          type="button"
          className="btn btn-primary"
          onClick={() =>
            onApply({ kind, values: [first, second], fill, topN: kind === "top" ? Number(first) || 10 : undefined })
          }
        >
          {t("common.apply")}
        </button>
      </div>
    </Dialog>
  );
}
