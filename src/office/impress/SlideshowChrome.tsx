import type { CSSProperties } from "react";
import { CircleDot, Eraser, PenLine, Pointer, Trash2 } from "lucide-react";
import { ToolButton } from "../office-ui";
import { useT } from "../../lib/i18n";
import type { Deck, Slide, SlideObject } from "../../lib/office-types";
import { formatClock } from "./selection";
import type { Theme } from "./shared";
import { SlidePreview } from "./slide-view";

export type ShowTool = "pointer" | "laser" | "pen" | "eraser";

const PANEL_STYLE: CSSProperties = {
  width: 320,
  minWidth: 320,
  maxHeight: "100%",
  overflow: "auto",
  display: "flex",
  flexDirection: "column",
  gap: 10,
  padding: 12,
  borderRadius: 10,
  background: "rgba(15, 23, 42, 0.94)",
  color: "#E2E8F0",
};

/** The presenter's side panel during a show: clock, current and next slide, notes (3.3c). */
export function PresenterPanel({
  deck,
  slide,
  nextSlide,
  theme,
  objectStyle,
  elapsedMs,
  onPrevious,
  onNext,
}: {
  deck: Deck;
  slide: Slide;
  nextSlide: Slide | undefined;
  theme: Theme;
  objectStyle: (object: SlideObject) => CSSProperties | undefined;
  elapsedMs: number;
  onPrevious: () => void;
  onNext: () => void;
}) {
  const t = useT();
  return (
    <aside className="presenter-panel" style={PANEL_STYLE}>
      <div className="row" style={{ justifyContent: "space-between", alignItems: "center" }}>
        <strong>{t("impress.presenterView")}</strong>
        <span style={{ fontVariantNumeric: "tabular-nums" }}>{formatClock(elapsedMs)}</span>
      </div>
      <div>
        <h4>{t("impress.currentSlide")}</h4>
        <SlidePreview deck={deck} slide={slide} theme={theme} width={280} objectStyle={objectStyle} />
      </div>
      {nextSlide ? (
        <div>
          <h4>{t("impress.nextSlide")}</h4>
          <SlidePreview deck={deck} slide={nextSlide} theme={theme} width={280} />
        </div>
      ) : null}
      <div>
        <h4>{t("impress.notes")}</h4>
        <div style={{ whiteSpace: "pre-wrap", fontSize: 12, opacity: 0.9 }}>{slide.notes || "—"}</div>
      </div>
      <div className="row">
        <button type="button" className="btn btn-soft" onClick={onPrevious}>
          ‹ {t("impress.previous")}
        </button>
        <button type="button" className="btn btn-soft" onClick={onNext}>
          {t("impress.next")} ›
        </button>
      </div>
    </aside>
  );
}

const TOOLS: { id: ShowTool; icon: typeof Pointer; titleKey: string }[] = [
  { id: "pointer", icon: Pointer, titleKey: "impress.pointer" },
  { id: "laser", icon: CircleDot, titleKey: "impress.laserPointer" },
  { id: "pen", icon: PenLine, titleKey: "impress.pen" },
  { id: "eraser", icon: Eraser, titleKey: "impress.eraser" },
];

/** The show's navigation bar: previous/next, slide counter, presenter toggle, ink tools, close. */
export function SlideshowNav({
  index,
  count,
  presenter,
  tool,
  onTool,
  onPrevious,
  onNext,
  onTogglePresenter,
  onClearInk,
  onClose,
}: {
  index: number;
  count: number;
  presenter: boolean;
  tool: ShowTool;
  onTool: (tool: ShowTool) => void;
  onPrevious: () => void;
  onNext: () => void;
  onTogglePresenter: () => void;
  onClearInk: () => void;
  onClose: () => void;
}) {
  const t = useT();
  return (
    <div
      className="slideshow-nav"
      role="presentation"
      onClick={(event) => event.stopPropagation()}
      style={presenter ? { position: "static" } : undefined}
    >
      <button type="button" className="btn btn-soft" onClick={onPrevious}>
        ‹
      </button>
      <span>
        {index + 1} / {count}
      </span>
      <button type="button" className="btn btn-soft" onClick={onNext}>
        ›
      </button>
      <button type="button" className="btn btn-soft" onClick={onTogglePresenter}>
        {t("impress.presenterView")}
      </button>
      <div className="slideshow-tools" role="toolbar" aria-label={t("impress.showTools")}>
        {TOOLS.map(({ id, icon: Icon, titleKey }) => (
          <ToolButton
            key={id}
            icon={<Icon size={14} />}
            title={t(titleKey)}
            active={tool === id}
            onClick={() => onTool(id)}
          />
        ))}
        <ToolButton icon={<Trash2 size={14} />} title={t("impress.clearInk")} onClick={onClearInk} />
      </div>
      <button type="button" className="btn btn-soft" onClick={onClose}>
        {t("common.close")}
      </button>
    </div>
  );
}
