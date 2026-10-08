/** Paste special: what to bring along from the copied cells, and whether to transpose. */
import { useId, useState } from "react";
import { useT } from "../../../lib/i18n";
import { Dialog } from "../../office-ui";
import type { PasteContent, PasteOptions } from "../paste-special";

const CONTENT: PasteContent[] = ["all", "formulas", "values", "formats"];
const LABELS: Record<PasteContent, string> = {
  all: "calc.pasteAll",
  formulas: "calc.pasteFormulas",
  values: "calc.pasteValues",
  formats: "calc.pasteFormats",
};

export function PasteSpecialDialog({
  external,
  onClose,
  onApply,
}: {
  /** The clipboard holds plain text from another application: only values can come from it. */
  external: boolean;
  onClose: () => void;
  onApply: (options: PasteOptions) => void;
}) {
  const t = useT();
  const radioName = useId();
  const [content, setContent] = useState<PasteContent>(external ? "values" : "all");
  const [transpose, setTranspose] = useState(false);
  return (
    <Dialog title={t("calc.pasteSpecial")} onClose={onClose}>
      <div className="stack">
        <fieldset className="calc-tool-fieldset is-list">
          <legend>{t("calc.pasteContent")}</legend>
          {CONTENT.map((id) => (
            <label key={id} className="check">
              <input
                type="radio"
                name={radioName}
                checked={content === id}
                disabled={external && id !== "values"}
                onChange={() => setContent(id)}
              />
              {t(LABELS[id])}
            </label>
          ))}
        </fieldset>
        <label className="check">
          <input type="checkbox" checked={transpose} onChange={(event) => setTranspose(event.target.checked)} />
          {t("calc.pasteTranspose")}
        </label>
        {external ? <p className="muted small">{t("calc.pasteExternalNote")}</p> : null}
      </div>
      <div className="row" style={{ justifyContent: "flex-end", marginTop: 14 }}>
        <button type="button" className="btn btn-soft" onClick={onClose}>
          {t("common.cancel")}
        </button>
        <button type="button" className="btn btn-primary" onClick={() => onApply({ content, transpose })}>
          {t("common.apply")}
        </button>
      </div>
    </Dialog>
  );
}
