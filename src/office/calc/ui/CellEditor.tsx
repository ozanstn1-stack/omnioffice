import type { KeyboardEvent, RefObject } from "react";

function caretOf(input: HTMLInputElement): number {
  return input.selectionStart ?? input.value.length;
}

/**
 * The editor drawn inside a Calc cell while it is being typed into (3.3h).
 * Like the formula bar, it turns keys into intents after the formula assist
 * has had its turn; CalcEditor decides what each one commits. Escape stops
 * there, so the grid's own Escape handling does not run as well.
 */
export function CellEditor({
  value,
  inputRef,
  onFocus,
  onCaret,
  onChange,
  onBlur,
  assistKey,
  onEnter,
  onTab,
  onEscape,
}: {
  value: string;
  inputRef: RefObject<HTMLInputElement | null>;
  onFocus: (caret: number) => void;
  onCaret: (caret: number) => void;
  onChange: (value: string, caret: number) => void;
  onBlur: () => void;
  assistKey: (event: KeyboardEvent<HTMLInputElement>) => boolean;
  onEnter: (shift: boolean) => void;
  onTab: (shift: boolean) => void;
  onEscape: () => void;
}) {
  return (
    <input
      className="cell-editor"
      ref={inputRef}
      value={value}
      // eslint-disable-next-line jsx-a11y/no-autofocus -- typing replaces the cell content; focusing the editor is the whole point of the interaction
      autoFocus
      onFocus={(event) => onFocus(caretOf(event.currentTarget))}
      onSelect={(event) => onCaret(caretOf(event.currentTarget))}
      onChange={(event) => onChange(event.target.value, caretOf(event.target))}
      onBlur={onBlur}
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
          event.stopPropagation();
          onEscape();
        }
      }}
    />
  );
}
