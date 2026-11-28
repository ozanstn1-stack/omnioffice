/**
 * Corner drag resize handles shared by selected Writer images and charts.
 */
import { useEffect, useRef } from "react";

export function ObjectSizeHandles({
  widthPt,
  heightPt,
  zoom,
  onPreview,
  onCommit,
}: {
  widthPt: number;
  heightPt: number;
  zoom: number;
  onPreview: (size: { widthPt: number; heightPt: number }) => void;
  onCommit: (size: { widthPt: number; heightPt: number }) => void;
}) {
  const dragRef = useRef<{
    startX: number;
    startWidthPt: number;
    directionX: 1 | -1;
    widthPt: number;
    heightPt: number;
    aspect: number;
  } | null>(null);

  useEffect(() => {
    const move = (event: PointerEvent) => {
      const drag = dragRef.current;
      if (!drag) return;
      const dx = ((event.clientX - drag.startX) * 72) / 96 / zoom;
      let width = Math.max(24, drag.startWidthPt + drag.directionX * dx);
      let height = width / drag.aspect;
      if (height < 24) {
        height = 24;
        width = height * drag.aspect;
      }
      drag.widthPt = width;
      drag.heightPt = height;
      onPreview({ widthPt: width, heightPt: height });
    };
    const up = () => {
      const drag = dragRef.current;
      dragRef.current = null;
      if (!drag) return;
      onCommit({ widthPt: drag.widthPt, heightPt: drag.heightPt });
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", up);
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", up);
    };
  });

  const start = (event: React.PointerEvent<HTMLSpanElement>, directionX: 1 | -1) => {
    event.preventDefault();
    event.stopPropagation();
    dragRef.current = {
      startX: event.clientX,
      startWidthPt: widthPt,
      directionX,
      widthPt,
      heightPt,
      aspect: widthPt / Math.max(1, heightPt),
    };
    try {
      event.currentTarget.setPointerCapture?.(event.pointerId);
    } catch {
      // Pointer capture unavailable in jsdom or older webviews.
    }
  };

  const corners: Array<{ corner: string; directionX: 1 | -1 }> = [
    { corner: "nw", directionX: -1 },
    { corner: "ne", directionX: 1 },
    { corner: "sw", directionX: -1 },
    { corner: "se", directionX: 1 },
  ];
  return (
    <>
      {corners.map(({ corner, directionX }) => (
        <span
          key={corner}
          className={`writer-image-handle is-${corner}`}
          data-image-handle={corner}
          onPointerDown={(event) => start(event, directionX)}
        />
      ))}
    </>
  );
}
