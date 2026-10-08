/** The floating find & replace panel (Ctrl+F / Ctrl+H); see useFindReplace for its state. */
import { useEffect, useRef } from "react";
import { ChevronDown, ChevronRight, X } from "lucide-react";
import { useT } from "../../../lib/i18n";
import type { FindReplacePanelModel } from "./useFindReplace";

export function FindReplacePanel({ panel }: { panel: FindReplacePanelModel }) {
  const t = useT();
  const searchRef = useRef<HTMLInputElement>(null);
  const panelRef = useRef<HTMLElement>(null);
  const replacing = panel.mode === "replace";
  const { focusRequest, onClose } = panel;
  useEffect(() => {
    searchRef.current?.focus();
    searchRef.current?.select();
  }, [focusRequest]);
  // Escape closes the panel from any of its fields. It is a native listener:
  // a non-interactive container may not carry a JSX key handler.
  useEffect(() => {
    const element = panelRef.current;
    if (!element) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopPropagation();
      onClose();
    };
    element.addEventListener("keydown", onKeyDown);
    return () => element.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  return (
    <aside
      ref={panelRef}
      className="calc-find-panel"
      role="dialog"
      aria-label={replacing ? t("writer.findReplace") : t("writer.find")}
    >
      <div className="row">
        <button
          type="button"
          className="icon-btn"
          aria-expanded={replacing}
          aria-label={t("writer.replaceWith")}
          title={t("writer.replaceWith")}
          onClick={() => panel.onMode(replacing ? "find" : "replace")}
        >
          {replacing ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        </button>
        <strong>{replacing ? t("writer.findReplace") : t("writer.find")}</strong>
        <span className="spacer" />
        <button type="button" className="icon-btn" aria-label={t("common.close")} onClick={panel.onClose}>
          <X size={14} />
        </button>
      </div>
      <label className="field">
        <span>{t("writer.findWhat")}</span>
        <input
          ref={searchRef}
          value={panel.query}
          aria-invalid={panel.error !== null}
          onChange={(event) => panel.onQuery(event.target.value)}
          onKeyDown={(event) => {
            if (event.key !== "Enter" || event.nativeEvent.isComposing) return;
            event.preventDefault();
            if (panel.canStep) panel.onStep(event.shiftKey ? -1 : 1);
          }}
        />
      </label>
      {panel.error ? (
        <p className="find-error" role="alert">
          {panel.error}
        </p>
      ) : (
        <p className="find-count muted" role="status">
          {panel.status}
        </p>
      )}
      {replacing ? (
        <label className="field">
          <span>{t("writer.replaceWith")}</span>
          <input value={panel.replacement} onChange={(event) => panel.onReplacement(event.target.value)} />
        </label>
      ) : null}
      <div className="row">
        <label className="field">
          <span>{t("calc.findWithin")}</span>
          <select
            value={panel.options.scope}
            onChange={(event) => panel.onOptions({ scope: event.target.value as "sheet" | "workbook" })}
          >
            <option value="sheet">{t("calc.findWithinSheet")}</option>
            <option value="workbook">{t("calc.findWithinWorkbook")}</option>
          </select>
        </label>
        <label className="field">
          <span>{t("calc.findLookIn")}</span>
          <select
            value={panel.options.lookIn}
            onChange={(event) => panel.onOptions({ lookIn: event.target.value as "formulas" | "values" })}
          >
            <option value="formulas">{t("calc.findLookInFormulas")}</option>
            <option value="values">{t("calc.findLookInValues")}</option>
          </select>
        </label>
      </div>
      <div className="row">
        <label className="check">
          <input
            type="checkbox"
            checked={panel.options.matchCase}
            onChange={(event) => panel.onOptions({ matchCase: event.target.checked })}
          />{" "}
          {t("writer.matchCase")}
        </label>
        <label className="check">
          <input
            type="checkbox"
            checked={panel.options.wholeCell}
            onChange={(event) => panel.onOptions({ wholeCell: event.target.checked })}
          />{" "}
          {t("calc.matchEntireCell")}
        </label>
        <label className="check">
          <input
            type="checkbox"
            checked={panel.options.regex}
            onChange={(event) => panel.onOptions({ regex: event.target.checked })}
          />{" "}
          {t("writer.regex")}
        </label>
      </div>
      {panel.notice ? <p className="muted small">{panel.notice}</p> : null}
      <div className="row">
        <button type="button" className="btn btn-soft" disabled={!panel.canStep} onClick={() => panel.onStep(-1)}>
          {t("writer.findPrevious")}
        </button>
        <button type="button" className="btn btn-soft" disabled={!panel.canStep} onClick={() => panel.onStep(1)}>
          {t("writer.findNext")}
        </button>
        {replacing ? (
          <>
            <button type="button" className="btn btn-soft" disabled={!panel.canReplace} onClick={panel.onReplace}>
              {t("writer.replace")}
            </button>
            <button type="button" className="btn btn-primary" disabled={!panel.canReplace} onClick={panel.onReplaceAll}>
              {t("writer.replaceAll")}
            </button>
          </>
        ) : null}
      </div>
    </aside>
  );
}
