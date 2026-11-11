/** The charts floating over the sheet: edit, delete, move and resize (4.1a, 4.1b). */
import { useEffect, useRef, useState, type PointerEvent } from "react";
import type { ChartPlacement, Sheet, Workbook } from "../../../lib/office-types";
import { formatAddress, parseAddress } from "../addresses";
import { anchorCell, columnLeft, resizedChart, type ChartPatch } from "../chart-drag";
import { HEADER_HEIGHT } from "../grid-metrics";
import type { RowLayout } from "../row-layout";
import { ChartBox } from "./ChartPanel";
import { ChartProperties } from "./ChartDialogs";

/** A drag in progress; dx/dy are canvas units (the pointer delta over the zoom). */
interface Drag {
  id: string;
  mode: "move" | "resize";
  startX: number;
  startY: number;
  dx: number;
  dy: number;
}

/** Below this many canvas pixels a pointer press is a click, not a drag. */
const DRAG_SLOP = 3;

export function ChartLayer({
  sheet,
  workbook,
  rowLayout,
  zoom,
  onChange,
}: {
  sheet: Sheet;
  workbook: Workbook;
  rowLayout: Pick<RowLayout, "offsetOf" | "rowAtY">;
  /** The grid zoom: pointer pixels are divided by it to get canvas units. */
  zoom: number;
  /** New placement fields for a chart, or null to delete it. */
  onChange: (id: string, patch: ChartPatch) => void;
}) {
  const [editing, setEditing] = useState<ChartPlacement | null>(null);
  const [drag, setDrag] = useState<Drag | null>(null);
  const dragRef = useRef<Drag | null>(null);
  const place = (anchor: string) => {
    const position = parseAddress(anchor) ?? { row: 0, col: 0 };
    return { x: columnLeft(sheet.colWidths, position.col), y: HEADER_HEIGHT + rowLayout.offsetOf(position.row) };
  };

  const commit = (done: Drag) => {
    const chart = sheet.charts.find((entry) => entry.id === done.id);
    if (!chart || Math.hypot(done.dx, done.dy) < DRAG_SLOP) return;
    if (done.mode === "resize") {
      onChange(done.id, resizedChart(chart.widthPx, chart.heightPx, done.dx, done.dy));
      return;
    }
    const { x, y } = place(chart.anchor);
    const cell = anchorCell(sheet.colWidths, rowLayout.rowAtY, x + done.dx, y + done.dy);
    const anchor = formatAddress(cell.row, cell.col);
    if (anchor !== chart.anchor) onChange(done.id, { anchor });
  };
  // The window listeners call the latest commit without re-binding.
  const commitRef = useRef(commit);
  useEffect(() => {
    commitRef.current = commit;
  });

  const dragging = drag !== null;
  useEffect(() => {
    if (!dragging) return;
    const move = (event: globalThis.PointerEvent) => {
      const current = dragRef.current;
      if (!current) return;
      const next = {
        ...current,
        dx: (event.clientX - current.startX) / zoom,
        dy: (event.clientY - current.startY) / zoom,
      };
      dragRef.current = next;
      setDrag(next);
    };
    const end = () => {
      const done = dragRef.current;
      dragRef.current = null;
      setDrag(null);
      if (done) commitRef.current(done);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", end);
    window.addEventListener("pointercancel", end);
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", end);
      window.removeEventListener("pointercancel", end);
    };
  }, [dragging, zoom]);

  const begin = (id: string, mode: Drag["mode"]) => (event: PointerEvent) => {
    if (event.button !== 0) return;
    // The grid underneath must not start a cell selection.
    event.preventDefault();
    event.stopPropagation();
    const start = { id, mode, startX: event.clientX, startY: event.clientY, dx: 0, dy: 0 };
    dragRef.current = start;
    setDrag(start);
  };

  return (
    <>
      {sheet.charts.map((chart) => {
        const { x, y } = place(chart.anchor);
        const active = drag?.id === chart.id ? drag : null;
        const size =
          active?.mode === "resize" ? resizedChart(chart.widthPx, chart.heightPx, active.dx, active.dy) : chart;
        const shift = active?.mode === "move" ? { dx: active.dx, dy: active.dy } : { dx: 0, dy: 0 };
        return (
          <ChartBox
            key={chart.id}
            chart={{ ...chart, widthPx: size.widthPx, heightPx: size.heightPx }}
            sheet={sheet}
            workbook={workbook}
            x={x + shift.dx}
            y={y + shift.dy}
            onRemove={() => onChange(chart.id, null)}
            onEdit={() => setEditing(chart)}
            onMoveStart={begin(chart.id, "move")}
            onResizeStart={begin(chart.id, "resize")}
          />
        );
      })}
      {editing ? (
        <ChartProperties
          chart={editing.chart}
          onClose={() => setEditing(null)}
          onSave={(chart) => {
            onChange(editing.id, { chart });
            setEditing(null);
          }}
        />
      ) : null}
    </>
  );
}
