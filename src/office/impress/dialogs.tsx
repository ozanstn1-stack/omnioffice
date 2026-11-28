/**
 * Impress dialogs: the footer, master, chart and animation editors.
 *
 * Extracted verbatim from ImpressEditor.tsx (B4 editor seam 3): each dialog
 * takes the model, the current value and the callbacks it needs as props.
 */
import { useState } from "react";
import type { Animation, Deck, Slide, SlideFooter, SlideLayout, SlideObject } from "../../lib/office-types";
import { newSlideMaster, uid } from "../../lib/office-types";
import { useT } from "../../lib/i18n";
import { Dialog, TextField, ToolNumber } from "../office-ui";
import { ANIMATION_EFFECTS } from "./shared";

/** Today as YYYY-MM-DD; the default date text when a footer is first enabled. */
function todayIso(): string {
  const now = new Date();
  const month = String(now.getMonth() + 1).padStart(2, "0");
  const day = String(now.getDate()).padStart(2, "0");
  return `${now.getFullYear()}-${month}-${day}`;
}

function defaultFooter(): SlideFooter {
  return { enabled: false, text: "", showText: true, showSlideNumber: false, showDate: false, dateText: todayIso() };
}

/** Deck-wide header & footer settings; changes apply to every slide. */
export function FooterDialog({
  footer,
  onChange,
  onClose,
}: {
  footer: SlideFooter | null | undefined;
  onChange: (footer: SlideFooter) => void;
  onClose: () => void;
}) {
  const t = useT();
  const current: SlideFooter = { ...defaultFooter(), ...(footer ?? {}) };
  const patch = (change: Partial<SlideFooter>) => onChange({ ...current, ...change });
  return (
    <Dialog title={t("impress.headerFooter")} onClose={onClose}>
      <div className="stack">
        <label className="field">
          <span>{t("impress.footerEnabled")}</span>
          <input
            type="checkbox"
            checked={current.enabled}
            onChange={(event) => patch({ enabled: event.target.checked })}
          />
        </label>
        <label className="field">
          <span>{t("impress.showFooterText")}</span>
          <input
            type="checkbox"
            checked={current.showText}
            onChange={(event) => patch({ showText: event.target.checked })}
          />
        </label>
        <TextField label={t("impress.footerText")} value={current.text} onChange={(text) => patch({ text })} />
        <label className="field">
          <span>{t("impress.showSlideNumber")}</span>
          <input
            type="checkbox"
            checked={current.showSlideNumber}
            onChange={(event) => patch({ showSlideNumber: event.target.checked })}
          />
        </label>
        <label className="field">
          <span>{t("impress.showDate")}</span>
          <input
            type="checkbox"
            checked={current.showDate}
            onChange={(event) => patch({ showDate: event.target.checked })}
          />
        </label>
        <TextField
          label={t("impress.dateText")}
          value={current.dateText}
          onChange={(dateText) => patch({ dateText })}
        />
        <p className="muted">{t("impress.footerHint")}</p>
      </div>
    </Dialog>
  );
}

export function MasterDialog({
  deck,
  slide,
  update,
  onClose,
  onUseLayout,
}: {
  deck: Deck;
  slide: Slide;
  update: (mutate: (deck: Deck) => Deck, recordUndo?: boolean) => void;
  onClose: () => void;
  onUseLayout: (masterId: string, layout: SlideLayout) => void;
}) {
  const t = useT();
  const masters = deck.masters ?? [];
  const [activeId, setActiveId] = useState(
    slide.masterId && masters.some((master) => master.id === slide.masterId) ? slide.masterId : (masters[0]?.id ?? ""),
  );
  const active = masters.find((master) => master.id === activeId) ?? masters[0];

  const addMaster = () => {
    const master = newSlideMaster(`Master ${masters.length + 1}`);
    setActiveId(master.id);
    update((current) => ({ ...current, masters: [...(current.masters ?? []), master] }));
  };
  const renameMaster = (masterId: string, name: string) =>
    update(
      (current) => ({
        ...current,
        masters: (current.masters ?? []).map((master) => (master.id === masterId ? { ...master, name } : master)),
      }),
      false,
    );
  const addLayout = (masterId: string) => {
    const layout: SlideLayout = {
      id: uid(),
      name: `Layout ${(masters.find((master) => master.id === masterId)?.layouts.length ?? 0) + 1}`,
      kind: "blank",
      objects: [],
    };
    update((current) => ({
      ...current,
      masters: (current.masters ?? []).map((master) =>
        master.id === masterId ? { ...master, layouts: [...master.layouts, layout] } : master,
      ),
    }));
  };
  const renameLayout = (masterId: string, layoutId: string, name: string) =>
    update(
      (current) => ({
        ...current,
        masters: (current.masters ?? []).map((master) =>
          master.id === masterId
            ? {
                ...master,
                layouts: master.layouts.map((layout) => (layout.id === layoutId ? { ...layout, name } : layout)),
              }
            : master,
        ),
      }),
      false,
    );
  const removeLayout = (masterId: string, layoutId: string) =>
    update((current) => ({
      ...current,
      masters: (current.masters ?? []).map((master) =>
        master.id === masterId
          ? { ...master, layouts: master.layouts.filter((layout) => layout.id !== layoutId) }
          : master,
      ),
    }));

  return (
    <Dialog title={t("impress.masters")} onClose={onClose} wide>
      <div className="row" style={{ alignItems: "flex-start", gap: 12 }}>
        <div className="stack" style={{ minWidth: 170 }}>
          {masters.map((master) => (
            <button
              key={master.id}
              type="button"
              className={`btn ${master.id === (active?.id ?? "") ? "btn-primary" : "btn-soft"}`}
              onClick={() => setActiveId(master.id)}
            >
              {master.name}
            </button>
          ))}
          <button type="button" className="btn btn-soft" onClick={addMaster}>
            + {t("impress.addMaster")}
          </button>
        </div>
        {active ? (
          <div className="stack" style={{ flex: 1 }}>
            <label className="field">
              <span>{t("impress.masterName")}</span>
              <input value={active.name} onChange={(event) => renameMaster(active.id, event.target.value)} />
            </label>
            <h4>{t("impress.layouts")}</h4>
            {active.layouts.map((layout) => (
              <div key={layout.id} className="row" style={{ gap: 4, alignItems: "center" }}>
                <input
                  value={layout.name}
                  onChange={(event) => renameLayout(active.id, layout.id, event.target.value)}
                />
                <button
                  type="button"
                  className={`btn ${slide.masterId === active.id && slide.layoutId === layout.id ? "btn-primary" : "btn-soft"}`}
                  onClick={() => onUseLayout(active.id, layout)}
                >
                  {t("impress.useLayout")}
                </button>
                <button
                  type="button"
                  className="icon-btn"
                  onClick={() => removeLayout(active.id, layout.id)}
                  title={t("common.delete")}
                >
                  ×
                </button>
              </div>
            ))}
            <button type="button" className="btn btn-soft" onClick={() => addLayout(active.id)}>
              + {t("impress.addLayout")}
            </button>
            <p className="muted">{t("impress.masterHint")}</p>
          </div>
        ) : null}
      </div>
    </Dialog>
  );
}

export { ChartDialog } from "../charts/ChartDialog";

export function AnimationDialog({
  animation,
  objects,
  onClose,
  onSave,
}: {
  animation: Animation;
  objects: SlideObject[];
  onClose: () => void;
  onSave: (animation: Animation) => void;
}) {
  const t = useT();
  const [draft, setDraft] = useState<Animation>({ ...animation });
  const effects = ANIMATION_EFFECTS[draft.kind] ?? ANIMATION_EFFECTS.entrance;
  const effect = effects.includes(draft.effect) ? draft.effect : effects[0];
  const missingObject = !objects.some((object) => object.id === draft.objectId);

  return (
    <Dialog title={t("impress.animation")} onClose={onClose}>
      <div className="stack">
        <label className="field">
          <span>{t("impress.animationObject")}</span>
          <select value={draft.objectId} onChange={(event) => setDraft({ ...draft, objectId: event.target.value })}>
            {missingObject ? <option value={draft.objectId}>{t("impress.animationMissingObject")}</option> : null}
            {objects.map((object, index) => (
              <option key={object.id} value={object.id}>
                {object.name || `${object.kind} ${index + 1}`}
              </option>
            ))}
          </select>
        </label>
        <label className="field">
          <span>{t("impress.animationKind")}</span>
          <select
            value={draft.kind}
            onChange={(event) => {
              const kind = event.target.value;
              const list = ANIMATION_EFFECTS[kind] ?? [];
              setDraft({ ...draft, kind, effect: list.includes(draft.effect) ? draft.effect : (list[0] ?? "fade") });
            }}
          >
            {["entrance", "emphasis", "exit"].map((kind) => (
              <option key={kind} value={kind}>
                {t(`impress.animationKind_${kind}`)}
              </option>
            ))}
          </select>
        </label>
        <label className="field">
          <span>{t("impress.animationEffect")}</span>
          <select value={effect} onChange={(event) => setDraft({ ...draft, effect: event.target.value })}>
            {effects.map((candidate) => (
              <option key={candidate} value={candidate}>
                {candidate}
              </option>
            ))}
          </select>
        </label>
        <label className="field">
          <span>{t("impress.animationTrigger")}</span>
          <select value={draft.trigger} onChange={(event) => setDraft({ ...draft, trigger: event.target.value })}>
            <option value="onClick">{t("impress.triggerOnClick")}</option>
            <option value="withPrevious">{t("impress.triggerWithPrevious")}</option>
            <option value="afterPrevious">{t("impress.triggerAfterPrevious")}</option>
          </select>
        </label>
        <div className="row">
          <ToolNumber
            value={draft.durationMs}
            onChange={(durationMs) => setDraft({ ...draft, durationMs })}
            min={0}
            max={10000}
            step={50}
            title={t("impress.animationDuration")}
            width={80}
          />
          <ToolNumber
            value={draft.delayMs}
            onChange={(delayMs) => setDraft({ ...draft, delayMs })}
            min={0}
            max={10000}
            step={50}
            title={t("impress.animationDelay")}
            width={80}
          />
        </div>
        <div className="row">
          <button type="button" className="btn btn-soft" onClick={onClose}>
            {t("common.cancel")}
          </button>
          <button type="button" className="btn btn-primary" onClick={() => onSave({ ...draft, effect })}>
            {t("common.save")}
          </button>
        </div>
      </div>
    </Dialog>
  );
}
