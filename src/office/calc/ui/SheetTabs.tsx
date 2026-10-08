/** The strip of sheet tabs under the grid, with add / rename / delete. */
import { Plus, Trash2 } from "lucide-react";
import { useT } from "../../../lib/i18n";
import type { Workbook } from "../../../lib/office-types";

export function SheetTabs({
  sheets,
  activeIndex,
  path,
  dirty,
  onSelect,
  onAdd,
  onRename,
  onRemove,
}: {
  sheets: Workbook["sheets"];
  activeIndex: number;
  /** The document path shown at the right edge; null for an unsaved document. */
  path: string | null;
  dirty: boolean;
  onSelect: (index: number) => void;
  onAdd: () => void;
  onRename: (index: number) => void;
  onRemove: (index: number) => void;
}) {
  const t = useT();
  return (
    <div className="calc-sheet-tabs">
      <button type="button" className="icon-btn" onClick={onAdd} title={t("calc.addSheet")}>
        <Plus size={14} />
      </button>
      {sheets.map((candidate, index) => (
        <div
          key={candidate.id}
          className={`sheet-tab${index === activeIndex ? " is-active" : ""}`}
          role="tab"
          tabIndex={index === activeIndex ? 0 : -1}
          aria-selected={index === activeIndex}
          onClick={() => onSelect(index)}
          onKeyDown={(event) => {
            if (event.key === "Enter" || event.key === " ") {
              event.preventDefault();
              onSelect(index);
            }
          }}
        >
          <span onDoubleClick={() => onRename(index)}>{candidate.name}</span>
          {sheets.length > 1 ? (
            <button
              type="button"
              className="icon-btn"
              onClick={(event) => {
                event.stopPropagation();
                onRemove(index);
              }}
              title={t("common.delete")}
            >
              <Trash2 size={11} />
            </button>
          ) : null}
        </div>
      ))}
      <span className="spacer" />
      <span className="muted">
        {path ?? t("writer.unsaved")} {dirty ? "•" : ""}
      </span>
    </div>
  );
}
