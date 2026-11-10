import { Sparkles } from "lucide-react";
import { ToolButton } from "../office-ui";
import { useT } from "../../lib/i18n";
import type { Animation, Slide, SlideLayout, SlideMaster } from "../../lib/office-types";

/** Master, layout and hidden flag for the current slide (3.3d). */
export function SlideSettings({
  slide,
  masters,
  layouts,
  onMaster,
  onLayout,
  onHidden,
}: {
  slide: Slide;
  masters: SlideMaster[];
  /** The selected master's layouts. */
  layouts: SlideLayout[];
  onMaster: (masterId: string) => void;
  onLayout: (layoutId: string) => void;
  onHidden: (hidden: boolean) => void;
}) {
  const t = useT();
  return (
    <div className="stack">
      <label className="field">
        <span>{t("impress.master")}</span>
        <select value={slide.masterId ?? masters[0]?.id ?? ""} onChange={(event) => onMaster(event.target.value)}>
          {masters.length === 0 ? <option value="">{t("impress.noMaster")}</option> : null}
          {masters.map((master) => (
            <option key={master.id} value={master.id}>
              {master.name}
            </option>
          ))}
        </select>
      </label>
      <label className="field">
        <span>{t("impress.layout")}</span>
        <select value={slide.layoutId ?? ""} onChange={(event) => onLayout(event.target.value)}>
          <option value="">{t("impress.noLayout")}</option>
          {layouts.map((layout) => (
            <option key={layout.id} value={layout.id}>
              {layout.name}
            </option>
          ))}
        </select>
      </label>
      <label className="field">
        <span>{t("impress.hiddenSlide")}</span>
        <input type="checkbox" checked={Boolean(slide.hidden)} onChange={(event) => onHidden(event.target.checked)} />
      </label>
    </div>
  );
}

/** Speaker notes and the slide's animation order, with reorder, edit and delete. */
export function SlideNotesAndAnimations({
  slide,
  onNotes,
  onReorder,
  onEdit,
  onDelete,
  onAdd,
}: {
  slide: Slide;
  onNotes: (notes: string) => void;
  onReorder: (id: string, delta: number) => void;
  onEdit: (animation: Animation) => void;
  onDelete: (id: string) => void;
  onAdd: () => void;
}) {
  const t = useT();
  return (
    <>
      <h4>{t("impress.notes")}</h4>
      <textarea
        className="notes-input"
        value={slide.notes}
        onChange={(event) => onNotes(event.target.value)}
        placeholder={t("impress.notesHint")}
      />
      <h4>{t("impress.animations")}</h4>
      <div className="stack">
        {[...(slide.animations ?? [])]
          .sort((a, b) => a.order - b.order)
          .map((animation, index, list) => {
            const target = slide.objects.find((object) => object.id === animation.objectId);
            return (
              <div key={animation.id} className="row" style={{ alignItems: "center", gap: 4 }}>
                <span
                  className="muted"
                  style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}
                >
                  {target?.name ?? t("impress.animationMissingObject")} · {animation.kind} · {animation.effect} ·{" "}
                  {animation.trigger}
                </span>
                <button
                  type="button"
                  className="icon-btn"
                  onClick={() => onReorder(animation.id, -1)}
                  disabled={index === 0}
                  title={t("impress.moveUp")}
                >
                  ↑
                </button>
                <button
                  type="button"
                  className="icon-btn"
                  onClick={() => onReorder(animation.id, 1)}
                  disabled={index === list.length - 1}
                  title={t("impress.moveDown")}
                >
                  ↓
                </button>
                <button type="button" className="icon-btn" onClick={() => onEdit(animation)} title={t("common.edit")}>
                  ✎
                </button>
                <button
                  type="button"
                  className="icon-btn"
                  onClick={() => onDelete(animation.id)}
                  title={t("common.delete")}
                >
                  ×
                </button>
              </div>
            );
          })}
        <ToolButton
          icon={<Sparkles size={14} />}
          label={t("impress.addAnimation")}
          onClick={onAdd}
          disabled={slide.objects.length === 0}
        />
      </div>
    </>
  );
}
