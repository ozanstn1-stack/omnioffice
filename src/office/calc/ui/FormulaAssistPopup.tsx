/** The argument hint and the autocomplete list shown under the focused formula input. */
import { useT } from "../../../lib/i18n";
import type { ArgumentHint, FormulaSuggestion, SuggestionList } from "../formula-assist";

export function FormulaAssistPopup({
  anchor,
  hint,
  suggestions,
  selectedIndex,
  android,
  onPick,
  onHover,
}: {
  anchor: { left: number; top: number; above?: boolean };
  hint: ArgumentHint | null;
  suggestions: SuggestionList | null;
  selectedIndex: number;
  android: boolean;
  onPick: (item: FormulaSuggestion) => void;
  onHover: (index: number) => void;
}) {
  const t = useT();
  return (
    <div
      className="calc-assist"
      style={{
        position: "fixed",
        left: anchor.left,
        top: anchor.top,
        transform: anchor.above ? "translateY(-100%)" : undefined,
        zIndex: 60,
        display: "flex",
        flexDirection: "column",
        gap: 4,
        maxWidth: 460,
      }}
    >
      {hint ? (
        <div
          style={{
            background: "var(--surface)",
            border: "1px solid var(--border)",
            borderRadius: 6,
            boxShadow: "var(--shadow)",
            padding: "4px 8px",
            fontFamily: "Consolas, monospace",
            fontSize: 11.5,
            display: "flex",
            gap: 2,
            flexWrap: "wrap",
          }}
        >
          <strong>{hint.name}</strong>
          <span>(</span>
          {hint.parts.map((part, index) => (
            <span key={`${index}:${part}`}>
              {index > 0 ? <span>, </span> : null}
              <span
                style={
                  index === hint.active
                    ? {
                        background: "var(--accent-weak)",
                        color: "var(--accent-text)",
                        borderRadius: 3,
                        padding: "0 3px",
                        fontWeight: 600,
                      }
                    : undefined
                }
              >
                {part}
              </span>
            </span>
          ))}
          <span>)</span>
        </div>
      ) : null}
      {suggestions && suggestions.items.length > 0 ? (
        <div
          role="listbox"
          aria-label={t("calc.suggestions")}
          style={{
            background: "var(--surface)",
            border: "1px solid var(--border)",
            borderRadius: 6,
            boxShadow: "var(--shadow)",
            maxHeight: 220,
            overflowY: "auto",
          }}
        >
          {suggestions.items.map((item, index) => (
            <div
              key={`${item.kind}:${item.label}`}
              role="option"
              tabIndex={-1}
              aria-selected={index === selectedIndex}
              // Applied on pointerdown, not click: on touch the input would
              // blur before a click ever fires, and preventDefault keeps the
              // caret (and the soft keyboard) in the editor.
              onPointerDown={(event) => {
                event.preventDefault();
                onPick(item);
              }}
              onMouseEnter={() => onHover(index)}
              style={{
                display: "flex",
                alignItems: "baseline",
                gap: 8,
                padding: android ? "9px 10px" : "3px 8px",
                minHeight: android ? 40 : undefined,
                cursor: "pointer",
                background: index === selectedIndex ? "var(--accent-weak)" : undefined,
                color: index === selectedIndex ? "var(--accent-text)" : undefined,
                fontSize: 12,
              }}
            >
              <span style={{ fontWeight: 600, fontFamily: "Consolas, monospace" }}>{item.label}</span>
              <span
                className="muted small"
                style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}
              >
                {item.detail}
              </span>
            </div>
          ))}
        </div>
      ) : null}
    </div>
  );
}
