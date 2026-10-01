/**
 * Shared chrome for the office editors: ribbon with tabs, ribbon groups,
 * tool buttons, dialogs and small inputs. Styling comes from CSS variables in
 * styles.css so all themes (light/dark/midnight/paper) work unchanged.
 */
import { type ReactNode, useState } from "react";

export interface RibbonTab {
  id: string;
  label: string;
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
  return (
    <div className="ribbon">
      <div className="ribbon-tabs">
        {tabs.map((tab) => (
          <button key={tab.id} type="button" className={`ribbon-tab${active === tab.id ? " is-active" : ""}`} onClick={() => onSelect(tab.id)}>
            {tab.label}
          </button>
        ))}
      </div>
      <div className="ribbon-body">{children}</div>
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
}: {
  icon?: ReactNode;
  label?: string;
  onClick?: () => void;
  active?: boolean;
  disabled?: boolean;
  title?: string;
}) {
  return (
    <button
      type="button"
      className={`tool-btn${active ? " is-active" : ""}${label ? "" : " is-icon-only"}`}
      onClick={onClick}
      disabled={disabled}
      title={title ?? label}
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
    <select className="tool-select" value={value} title={title} style={width ? { width } : undefined} onChange={(event) => onChange(event.target.value)}>
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

export function ToolColor({ value, onChange, title }: { value: string; onChange: (value: string) => void; title: string }) {
  return <input type="color" className="tool-color" value={value} title={title} onChange={(event) => onChange(event.target.value)} />;
}

export function Dialog({ title, onClose, children, wide }: { title: string; onClose: () => void; children: ReactNode; wide?: boolean }) {
  return (
    <div className="modal-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}>
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
                  window.dispatchEvent(new CustomEvent("oswk-insert-table", { detail: { rows: rowIndex + 1, cols: colIndex + 1 } }));
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
