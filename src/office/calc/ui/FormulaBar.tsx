import type { KeyboardEvent, RefObject } from "react";
import { Check, X } from "lucide-react";
import { useT } from "../../../lib/i18n";

/** The function symbol before the formula input; the same in every language. */
const FX = "fx";

/** Where the caret sits in an input, or its end when the browser does not say. */
function caretOf(input: HTMLInputElement): number {
  return input.selectionStart ?? input.value.length;
}

/**
 * Calc's name box and formula input (3.3f). The editor keeps the draft, the
 * open cell editor and what Enter, Tab and Escape commit; this draws the bar
 * and turns keys into those three intents. On phones the bar is docked above
 * the keyboard and gains cancel / apply buttons.
 */
export function FormulaBar({
  docked,
  nameBox,
  onNameBox,
  value,
  inputRef,
  onFocus,
  onCaret,
  onBlur,
  onChange,
  assistKey,
  onEnter,
  onTab,
  onEscape,
  onCancel,
  onApply,
}: {
  docked: boolean;
  nameBox: string;
  /** The typed name box text, already trimmed. */
  onNameBox: (text: string) => void;
  value: string;
  inputRef: RefObject<HTMLInputElement | null>;
  onFocus: (caret: number) => void;
  onCaret: (caret: number) => void;
  onBlur: () => void;
  onChange: (value: string, caret: number) => void;
  /** The formula-assist popup; returns true when it consumed the key. */
  assistKey: (event: KeyboardEvent<HTMLInputElement>) => boolean;
  onEnter: (shift: boolean) => void;
  onTab: (shift: boolean) => void;
  onEscape: () => void;
  onCancel: () => void;
  onApply: () => void;
}) {
  const t = useT();
  return (
    <div className={`calc-formula-bar${docked ? " is-docked" : ""}`}>
      <input className="name-box" value={nameBox} onChange={(event) => onNameBox(event.target.value.trim())} />
      <span className="fx">{FX}</span>
      <input
        className="formula-input"
        ref={inputRef}
        value={value}
        placeholder={t("calc.formulaHint")}
        onFocus={(event) => onFocus(caretOf(event.currentTarget))}
        onSelect={(event) => onCaret(caretOf(event.currentTarget))}
        onBlur={onBlur}
        onChange={(event) => onChange(event.target.value, caretOf(event.target))}
        onKeyDown={(event) => {
          if (event.nativeEvent.isComposing) return;
          // The popup owns Tab/Enter/Escape/arrows while it is open.
          if (assistKey(event)) return;
          if (event.key === "Enter") {
            event.preventDefault();
            onEnter(event.shiftKey);
          }
          if (event.key === "Tab") {
            event.preventDefault();
            onTab(event.shiftKey);
          }
          if (event.key === "Escape") {
            event.preventDefault();
            onEscape();
          }
        }}
      />
      {docked ? (
        <>
          <button type="button" className="icon-btn calc-bar-action" aria-label={t("common.cancel")} onClick={onCancel}>
            <X size={17} />
          </button>
          <button
            type="button"
            className="icon-btn calc-bar-action is-primary"
            aria-label={t("common.apply")}
            onClick={onApply}
          >
            <Check size={17} />
          </button>
        </>
      ) : null}
    </div>
  );
}
