/**
 * Paragraph ruler for the Writer's paginated view.
 *
 * The ruler is a view over the active paragraph: it draws the page margins and
 * the paragraph's left/right/first-line indents, and turns clicks and marker
 * drags into `ParaProps.tabs` edits. All tab-stop math lives in `tab-stops.ts`,
 * so this component only converts points to screen pixels and back.
 */
import { useRef, useState } from "react";
import { useT } from "../../lib/i18n";
import type { ParaProps } from "../../lib/office-types";
import { addTabStop, moveTabStop, nextTabStop, removeTabStop } from "./tab-stops";

export function WriterRuler({
  pageWidthPt,
  marginLeftPt,
  marginRightPt,
  zoom,
  props,
  onChange,
}: {
  pageWidthPt: number;
  marginLeftPt: number;
  marginRightPt: number;
  zoom: number;
  /** The active paragraph's properties, or null when nothing is focused. */
  props: ParaProps | null;
  /** Commits a tab-stop change as one model edit. */
  onChange: (patch: Partial<ParaProps>) => void;
}) {
  const t = useT();
  const ref = useRef<HTMLDivElement>(null);
  const dragRef = useRef<{ index: number; startX: number; startPosPt: number } | null>(null);
  const dragPosRef = useRef<number | null>(null);
  const [dragPreview, setDragPreview] = useState<{ index: number; posPt: number } | null>(null);
  const contentWidthPt = Math.max(0, pageWidthPt - marginLeftPt - marginRightPt);
  const pxPerPt = (96 / 72) * zoom;
  const tabs = props?.tabs ?? [];
  const tabAt = (index: number) => (dragPreview?.index === index ? dragPreview.posPt : tabs[index].posPt);

  /** Click/drag x position as points from the left margin, clamped to the text area. */
  const positionFromEvent = (clientX: number): number => {
    const rect = ref.current?.getBoundingClientRect();
    const position = (clientX - (rect?.left ?? 0)) / pxPerPt - marginLeftPt;
    return Math.max(0, Math.min(contentWidthPt, position));
  };

  const startDrag = (event: React.PointerEvent<HTMLElement>, index: number) => {
    event.preventDefault();
    event.stopPropagation();
    dragRef.current = { index, startX: event.clientX, startPosPt: tabs[index].posPt };
    dragPosRef.current = tabs[index].posPt;
    try {
      event.currentTarget.setPointerCapture?.(event.pointerId);
    } catch {
      // Pointer capture is unavailable (jsdom, older webviews).
    }
  };

  const moveDrag = (event: React.PointerEvent<HTMLElement>) => {
    const drag = dragRef.current;
    if (!drag) return;
    const position = positionFromEvent(event.clientX);
    dragPosRef.current = position;
    setDragPreview({ index: drag.index, posPt: position });
  };

  const endDrag = () => {
    const drag = dragRef.current;
    const position = dragPosRef.current;
    dragRef.current = null;
    dragPosRef.current = null;
    setDragPreview(null);
    if (!drag || position === null) return;
    // A plain click (no movement) is not an edit, so it must not push an undo
    // step; only a real move commits.
    if (Math.abs(position - drag.startPosPt) < 0.5) return;
    onChange({ tabs: moveTabStop(tabs, drag.index, position) });
  };

  const removeAt = (index: number) => onChange({ tabs: removeTabStop(tabs, index) });
  const lastStopPt = tabs.length > 0 ? Math.max(...tabs.map((stop) => stop.posPt)) : 0;

  const indentLeftPx = (marginLeftPt + (props?.indentLeftPt ?? 0)) * pxPerPt;
  const firstLinePx = indentLeftPx + (props?.firstLinePt ?? 0) * pxPerPt;
  const indentRightPx = (pageWidthPt - marginRightPt - (props?.indentRightPt ?? 0)) * pxPerPt;

  return (
    <div
      ref={ref}
      className="writer-ruler"
      data-testid="writer-ruler"
      role="button"
      tabIndex={0}
      aria-label={t("writer.ruler")}
      style={{ width: pageWidthPt * pxPerPt }}
      onClick={(event) => {
        if (!props) return;
        onChange({ tabs: addTabStop(tabs, positionFromEvent(event.clientX)) });
      }}
      onKeyDown={(event) => {
        if (!props || (event.key !== "Enter" && event.key !== " ")) return;
        event.preventDefault();
        onChange({ tabs: addTabStop(tabs, nextTabStop(tabs, lastStopPt)) });
      }}
    >
      <div className="writer-ruler-content" style={{ left: marginLeftPt * pxPerPt, width: contentWidthPt * pxPerPt }} />
      {props ? (
        <>
          <span className="writer-ruler-indent" data-indent="left" style={{ left: indentLeftPx }} />
          <span className="writer-ruler-indent" data-indent="first" style={{ left: firstLinePx }} />
          <span className="writer-ruler-indent" data-indent="right" style={{ left: indentRightPx }} />
        </>
      ) : null}
      {tabs.map((stop, index) => (
        <span
          key={index}
          className={`writer-ruler-tab is-${stop.align || "left"}`}
          data-tab-index={index}
          data-tab-align={stop.align}
          role="button"
          tabIndex={0}
          aria-label={`${t("writer.tabStop")} ${index + 1}`}
          style={{ left: (marginLeftPt + tabAt(index)) * pxPerPt }}
          onPointerDown={(event) => startDrag(event, index)}
          onPointerMove={moveDrag}
          onPointerUp={endDrag}
          onPointerCancel={endDrag}
          onClick={(event) => event.stopPropagation()}
          onDoubleClick={(event) => {
            event.stopPropagation();
            removeAt(index);
          }}
          onContextMenu={(event) => {
            event.preventDefault();
            event.stopPropagation();
            removeAt(index);
          }}
          onKeyDown={(event) => {
            if (event.key === "Delete" || event.key === "Backspace") {
              event.preventDefault();
              removeAt(index);
            } else if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
              event.preventDefault();
              const delta = event.key === "ArrowLeft" ? -12 : 12;
              onChange({ tabs: moveTabStop(tabs, index, Math.max(0, stop.posPt + delta)) });
            }
          }}
        />
      ))}
    </div>
  );
}
