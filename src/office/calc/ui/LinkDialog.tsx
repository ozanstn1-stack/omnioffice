/** Insert link: the text shown in the cell and where the link goes. */
import { useState } from "react";
import { useT } from "../../../lib/i18n";
import { Dialog } from "../../office-ui";
import { parseAddress } from "../formula";
import {
  linkKind,
  parsePlaceTarget,
  placeTarget,
  safeLinkTarget,
  targetFromInput,
  type LinkDraft,
  type LinkKind,
} from "../links";

/** What the dialog starts from: the cell's current text and its link, if it has one. */
export interface LinkInitial {
  /** The text the cell shows. */
  text: string;
  /** The cell's link target; null for a cell without one. */
  target: string | null;
  tooltip: string;
}

export function LinkDialog({
  initial,
  sheetNames,
  currentSheet,
  names,
  onSubmit,
  onRemove,
  onClose,
}: {
  initial: LinkInitial;
  sheetNames: string[];
  /** The sheet pre-selected for a link inside the document. */
  currentSheet: string;
  /** Defined names a link inside the document can point at. */
  names: string[];
  onSubmit: (draft: LinkDraft) => void;
  onRemove: () => void;
  onClose: () => void;
}) {
  const t = useT();
  const place = initial.target && linkKind(initial.target) === "place" ? parsePlaceTarget(initial.target) : null;
  const [kind, setKind] = useState<LinkKind>(initial.target ? linkKind(initial.target) : "web");
  const [text, setText] = useState(initial.text);
  const [address, setAddress] = useState(
    initial.target && kind !== "place" ? initial.target.replace(/^mailto:/i, "") : "",
  );
  const [sheet, setSheet] = useState(place?.sheet ?? currentSheet);
  const [reference, setReference] = useState(place?.reference ?? "A1");
  const [tooltip, setTooltip] = useState(initial.tooltip);
  const [error, setError] = useState<string | null>(null);

  const submit = () => {
    let target: string;
    if (kind === "place") {
      const cell = reference.trim().replace(/\$/g, "");
      const name = names.find((candidate) => candidate.toLowerCase() === cell.toLowerCase());
      if (parseAddress(cell.split(":")[0]) !== null) target = placeTarget(sheet, cell);
      else if (name) target = `#${name}`;
      else return setError(t("calc.linkBadPlace"));
    } else {
      target = targetFromInput(kind, address);
    }
    const safe = safeLinkTarget(target);
    if (!safe) return setError(t("calc.linkInvalid"));
    // Unchanged text leaves the cell as it is (a number stays a number, a formula stays a formula).
    onSubmit({ target: safe, text: text === initial.text ? null : text, tooltip });
  };

  const kinds: Array<[LinkKind, string]> = [
    ["web", t("calc.linkWeb")],
    ["mail", t("calc.linkMail")],
    ["place", t("calc.linkPlace")],
  ];
  return (
    <Dialog title={initial.target ? t("calc.linkEdit") : t("calc.linkInsert")} onClose={onClose}>
      <form
        className="stack"
        onSubmit={(event) => {
          event.preventDefault();
          submit();
        }}
      >
        <div className="link-kinds" role="radiogroup" aria-label={t("calc.linkKind")}>
          {kinds.map(([id, label]) => (
            <button
              key={id}
              type="button"
              role="radio"
              aria-checked={kind === id}
              className={`btn ${kind === id ? "btn-primary" : "btn-soft"}`}
              onClick={() => {
                setKind(id);
                setError(null);
              }}
            >
              {label}
            </button>
          ))}
        </div>
        <label className="field">
          <span>{t("calc.linkText")}</span>
          <input value={text} onChange={(event) => setText(event.target.value)} />
        </label>
        {kind === "place" ? (
          <div className="row">
            <label className="field">
              <span>{t("calc.linkSheet")}</span>
              <select value={sheet} onChange={(event) => setSheet(event.target.value)}>
                {sheetNames.map((name) => (
                  <option key={name} value={name}>
                    {name}
                  </option>
                ))}
              </select>
            </label>
            <label className="field">
              <span>{t("calc.linkCell")}</span>
              <input
                value={reference}
                list="link-names"
                onChange={(event) => {
                  setReference(event.target.value);
                  setError(null);
                }}
              />
              <datalist id="link-names">
                {names.map((name) => (
                  <option key={name} value={name}>
                    {name}
                  </option>
                ))}
              </datalist>
            </label>
          </div>
        ) : (
          <label className="field">
            <span>{kind === "mail" ? t("calc.linkMail") : t("calc.linkAddress")}</span>
            <input
              inputMode={kind === "mail" ? "email" : "url"}
              value={address}
              placeholder={kind === "mail" ? "name@example.com" : "https://"}
              // eslint-disable-next-line jsx-a11y/no-autofocus -- the dialog opens to type the address
              autoFocus
              onChange={(event) => {
                setAddress(event.target.value);
                setError(null);
              }}
            />
          </label>
        )}
        <label className="field">
          <span>{t("calc.linkTip")}</span>
          <input value={tooltip} onChange={(event) => setTooltip(event.target.value)} />
        </label>
        {error ? (
          <p className="find-error" role="alert">
            {error}
          </p>
        ) : null}
        <div className="row">
          <button type="submit" className="btn btn-primary">
            {t("common.apply")}
          </button>
          {initial.target ? (
            <button type="button" className="btn btn-soft" onClick={onRemove}>
              {t("calc.linkRemove")}
            </button>
          ) : null}
          <button type="button" className="btn btn-soft" onClick={onClose}>
            {t("common.cancel")}
          </button>
        </div>
      </form>
    </Dialog>
  );
}
