/**
 * Shared chrome for the office editors: ribbon with tabs, ribbon groups,
 * tool buttons, dialogs and small inputs. Styling comes from CSS variables in
 * styles.css so all themes (light/dark/midnight/paper) work unchanged.
 */
import { type ReactNode, useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { ChevronDown, ChevronUp } from "lucide-react";
import { useT } from "../lib/i18n";

export interface RibbonTab {
  id: string;
  label: string;
}

/** Phone-width windows fold the ribbon instead of scrolling it sideways. */
export const NARROW_RIBBON_QUERY = "(max-width: 760px)";

function useMediaQuery(query: string): boolean {
  const [matches, setMatches] = useState(() =>
    typeof window.matchMedia === "function" ? window.matchMedia(query).matches : false,
  );
  useEffect(() => {
    if (typeof window.matchMedia !== "function") return;
    const list = window.matchMedia(query);
    const update = () => setMatches(list.matches);
    list.addEventListener?.("change", update);
    return () => list.removeEventListener?.("change", update);
  }, [query]);
  return matches;
}

/**
 * Height that shows only the first row of a wrapped ribbon body (including
 * its padding and border), or 0 when every group fits on one row.
 */
export function firstRowHeight(body: HTMLElement): number {
  const items = Array.from(body.children).filter(
    (child): child is HTMLElement => child instanceof HTMLElement && child.offsetHeight > 0,
  );
  if (items.length === 0) return 0;
  const top = Math.min(...items.map((item) => item.offsetTop));
  const firstRow = items.filter((item) => item.offsetTop < top + 4);
  if (firstRow.length === items.length) return 0;
  const style = window.getComputedStyle(body);
  const chrome = ["paddingTop", "paddingBottom", "borderTopWidth", "borderBottomWidth"]
    .map((key) => parseFloat(style[key as "paddingTop"]) || 0)
    .reduce((sum, value) => sum + value, 0);
  return Math.max(...firstRow.map((item) => item.offsetTop - top + item.offsetHeight)) + chrome;
}

export function Ribbon({
  tabs,
  active,
  onSelect,
  children,
}: {
  tabs: RibbonTab[];
  active: string;
  onSelect: (id: string) => void;
  children: ReactNode;
}) {
  const t = useT();
  const bodyId = useId();
  const narrow = useMediaQuery(NARROW_RIBBON_QUERY);
  const bodyRef = useRef<HTMLDivElement>(null);
  // 0 = the groups fit (or the window is wide): nothing to fold.
  const [collapsedHeight, setCollapsedHeight] = useState(0);
  // Expansion belongs to one tab; switching tabs starts folded again.
  const [expandedTab, setExpandedTab] = useState<string | null>(null);
  const expanded = expandedTab === active;

  useLayoutEffect(() => {
    const body = bodyRef.current;
    if (!narrow || !body) {
      setCollapsedHeight(0);
      return;
    }
    const measure = () => setCollapsedHeight(firstRowHeight(body));
    measure();
    // Width changes re-wrap the groups; tab switches and state-driven
    // buttons change them, so watch both.
    const resize = new ResizeObserver(measure);
    resize.observe(body);
    const mutation = new MutationObserver(measure);
    mutation.observe(body, { childList: true, subtree: true });
    return () => {
      resize.disconnect();
      mutation.disconnect();
    };
  }, [narrow]);

  const folds = narrow && collapsedHeight > 0;

  return (
    <div className={`ribbon${narrow ? " is-narrow" : ""}${folds && expanded ? " is-expanded" : ""}`}>
      <div className="ribbon-tabs">
        {tabs.map((tab) => (
          <button
            key={tab.id}
            type="button"
            className={`ribbon-tab${active === tab.id ? " is-active" : ""}`}
            onClick={() => onSelect(tab.id)}
          >
            {tab.label}
          </button>
        ))}
        {folds ? (
          <button
            type="button"
            className="ribbon-tab ribbon-more"
            aria-expanded={expanded}
            aria-controls={bodyId}
            onClick={() => setExpandedTab(expanded ? null : active)}
          >
            {expanded ? <ChevronUp size={14} aria-hidden /> : <ChevronDown size={14} aria-hidden />}
            {expanded ? t("office.ribbonLess") : t("office.ribbonMore")}
          </button>
        ) : null}
      </div>
      <div
        ref={bodyRef}
        id={bodyId}
        className="ribbon-body"
        style={folds && !expanded ? { maxHeight: collapsedHeight } : undefined}
      >
        {children}
      </div>
    </div>
  );
}

export function RibbonGroup({ label, children }: { label?: string; children: ReactNode }) {
  return (
    <div className="ribbon-group">
      <div className="ribbon-group-items">{children}</div>
      {label ? <div className="ribbon-group-label">{label}</div> : null}
    </div>
  );
}

export function ToolButton({
  icon,
  label,
  onClick,
  active,
  disabled,
  title,
  keepFocus,
}: {
  icon?: ReactNode;
  label?: string;
  onClick?: () => void;
  /** Makes the button a toggle: highlighted and exposed as pressed when true. Leave it out for plain actions. */
  active?: boolean;
  disabled?: boolean;
  title?: string;
  /** Keeps the text selection / focus of the editing surface when pressed. */
  keepFocus?: boolean;
}) {
  return (
    <button
      type="button"
      className={`tool-btn${active ? " is-active" : ""}${label ? "" : " is-icon-only"}`}
      onMouseDown={keepFocus ? (event) => event.preventDefault() : undefined}
      onClick={onClick}
      disabled={disabled}
      title={title ?? label}
      // A button given an `active` state is a toggle and says whether it is on.
      aria-pressed={active}
    >
      {icon}
      {label ? <span>{label}</span> : null}
    </button>
  );
}

export function ToolDivider() {
  return <div className="tool-divider" />;
}

export function ToolSelect({
  value,
  onChange,
  options,
  title,
  width,
}: {
  value: string;
  onChange: (value: string) => void;
  options: Array<{ value: string; label: string }>;
  title?: string;
  width?: number;
}) {
  return (
    <select
      className="tool-select"
      value={value}
      title={title}
      style={width ? { width } : undefined}
      onChange={(event) => onChange(event.target.value)}
    >
      {options.map((option) => (
        <option key={option.value} value={option.value}>
          {option.label}
        </option>
      ))}
    </select>
  );
}

export function ToolNumber({
  value,
  onChange,
  min,
  max,
  step = 1,
  title,
  width = 64,
}: {
  value: number;
  onChange: (value: number) => void;
  min?: number;
  max?: number;
  step?: number;
  title?: string;
  width?: number;
}) {
  return (
    <input
      type="number"
      className="tool-input"
      value={Number.isFinite(value) ? value : 0}
      min={min}
      max={max}
      step={step}
      title={title}
      style={{ width }}
      onChange={(event) => onChange(Number(event.target.value))}
    />
  );
}

export function ToolColor({
  value,
  onChange,
  title,
}: {
  value: string;
  onChange: (value: string) => void;
  title: string;
}) {
  return (
    <input
      type="color"
      className="tool-color"
      value={value}
      title={title}
      onChange={(event) => onChange(event.target.value)}
    />
  );
}

export function Dialog({
  title,
  onClose,
  children,
  wide,
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
  wide?: boolean;
}) {
  return (
    <div
      className="modal-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className={`modal${wide ? " modal-wide" : ""}`} role="dialog" aria-modal="true" aria-label={title}>
        <div className="modal-head">
          <h3>{title}</h3>
          <button type="button" className="icon-btn" onClick={onClose} aria-label="Close">
            ×
          </button>
        </div>
        <div className="modal-body">{children}</div>
      </div>
    </div>
  );
}

export function TextField({
  label,
  value,
  onChange,
  placeholder,
  type = "text",
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  type?: string;
}) {
  return (
    <label className="field">
      <span>{label}</span>
      <input type={type} value={value} placeholder={placeholder} onChange={(event) => onChange(event.target.value)} />
    </label>
  );
}

export function useTablePicker(): {
  open: boolean;
  openPicker: () => void;
  close: () => void;
  grid: ReactNode;
} {
  const [open, setOpen] = useState(false);
  const [hover, setHover] = useState<{ rows: number; cols: number }>({ rows: 1, cols: 1 });
  const rows = 8;
  const cols = 10;
  const grid = (
    <div className="table-picker">
      <p className="muted">
        {hover.rows} × {hover.cols} table
      </p>
      <div className="table-picker-grid" onMouseLeave={() => setHover({ rows: 1, cols: 1 })}>
        {Array.from({ length: rows }, (_, rowIndex) => (
          <div key={rowIndex} className="table-picker-row">
            {Array.from({ length: cols }, (_, colIndex) => (
              <button
                key={colIndex}
                type="button"
                className={`table-picker-cell${rowIndex < hover.rows && colIndex < hover.cols ? " is-on" : ""}`}
                onMouseEnter={() => setHover({ rows: rowIndex + 1, cols: colIndex + 1 })}
                onClick={() => {
                  window.dispatchEvent(
                    new CustomEvent("oswk-insert-table", { detail: { rows: rowIndex + 1, cols: colIndex + 1 } }),
                  );
                  setOpen(false);
                }}
                aria-label={`${rowIndex + 1} by ${colIndex + 1}`}
              />
            ))}
          </div>
        ))}
      </div>
    </div>
  );
  return { open, openPicker: () => setOpen(true), close: () => setOpen(false), grid };
}
