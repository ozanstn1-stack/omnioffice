import { Grid2X2, MonitorPlay, Play, Rows3, Sparkles } from "lucide-react";
import { RibbonGroup, ToolButton, ToolNumber, ToolSelect } from "../office-ui";
import { useT } from "../../lib/i18n";
import type { Deck, Slide } from "../../lib/office-types";

/** The duration unit; a symbol, not translated. */
const MS = "ms";

/** The Transitions ribbon tab: effect, duration and apply-to-all (3.3c). */
export function TransitionsTab({
  slide,
  updateSlide,
  update,
}: {
  slide: Slide;
  updateSlide: (mutate: (slide: Slide) => Slide) => void;
  update: (mutate: (deck: Deck) => Deck) => void;
}) {
  const t = useT();
  return (
    <RibbonGroup label={t("impress.transition")}>
      <ToolSelect
        value={slide.transition ?? "none"}
        onChange={(transition) =>
          updateSlide((current) => ({ ...current, transition: transition === "none" ? null : transition }))
        }
        options={[
          { value: "none", label: t("impress.transitionNone") },
          { value: "fade", label: t("impress.transitionFade") },
          { value: "slide", label: t("impress.transitionSlide") },
          { value: "push", label: t("impress.transitionPush") },
          { value: "wipe", label: t("impress.transitionWipe") },
        ]}
        width={130}
      />
      <ToolNumber
        value={slide.transitionMs}
        onChange={(transitionMs) => updateSlide((current) => ({ ...current, transitionMs }))}
        min={100}
        max={3000}
        step={100}
        title={MS}
        width={80}
      />
      <ToolButton
        label={t("impress.applyToAll")}
        onClick={() =>
          update((current) => ({
            ...current,
            slides: current.slides.map((candidate) => ({
              ...candidate,
              transition: slide.transition,
              transitionMs: slide.transitionMs,
            })),
          }))
        }
      />
    </RibbonGroup>
  );
}

/** The Animations ribbon tab. */
export function AnimationsTab({ onAdd, disabled }: { onAdd: () => void; disabled: boolean }) {
  const t = useT();
  return (
    <RibbonGroup label={t("impress.animations")}>
      <ToolButton icon={<Sparkles size={16} />} label={t("impress.addAnimation")} onClick={onAdd} disabled={disabled} />
    </RibbonGroup>
  );
}

/** The View ribbon tab: normal or sorter view, and starting a show (with or without presenter view). */
export function ViewTab({
  view,
  onView,
  onStart,
}: {
  view: "normal" | "sorter";
  onView: (view: "normal" | "sorter") => void;
  onStart: (presenter: boolean) => void;
}) {
  const t = useT();
  return (
    <>
      <RibbonGroup label={t("impress.view")}>
        <ToolButton
          icon={<Rows3 size={16} />}
          label={t("impress.viewNormal")}
          onClick={() => onView("normal")}
          active={view === "normal"}
        />
        <ToolButton
          icon={<Grid2X2 size={16} />}
          label={t("impress.slideSorter")}
          onClick={() => onView("sorter")}
          active={view === "sorter"}
        />
      </RibbonGroup>
      <RibbonGroup label={t("impress.present")}>
        <ToolButton icon={<Play size={16} />} label={t("impress.startShow")} onClick={() => onStart(false)} />
        <ToolButton icon={<MonitorPlay size={16} />} label={t("impress.presenter")} onClick={() => onStart(true)} />
      </RibbonGroup>
    </>
  );
}
