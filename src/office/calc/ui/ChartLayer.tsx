/** The charts floating over the sheet, with their delete and edit actions (4.1a). */
import { useState } from "react";
import type { ChartData, ChartPlacement, Sheet, Workbook } from "../../../lib/office-types";
import { parseAddress } from "../addresses";
import { ChartBox } from "./ChartPanel";
import { ChartProperties } from "./ChartDialogs";

export function ChartLayer({
  sheet,
  workbook,
  place,
  onChange,
}: {
  sheet: Sheet;
  workbook: Workbook;
  /** Canvas position of a cell's top-left corner. */
  place: (row: number, col: number) => { x: number; y: number };
  /** A chart's new data, or null to delete it. */
  onChange: (id: string, chart: ChartData | null) => void;
}) {
  const [editing, setEditing] = useState<ChartPlacement | null>(null);
  return (
    <>
      {sheet.charts.map((chart) => {
        const position = parseAddress(chart.anchor) ?? { row: 0, col: 0 };
        const { x, y } = place(position.row, position.col);
        return (
          <ChartBox
            key={chart.id}
            chart={chart}
            sheet={sheet}
            workbook={workbook}
            x={x}
            y={y}
            onRemove={() => onChange(chart.id, null)}
            onEdit={() => setEditing(chart)}
          />
        );
      })}
      {editing ? (
        <ChartProperties
          chart={editing.chart}
          onClose={() => setEditing(null)}
          onSave={(chart) => {
            onChange(editing.id, chart);
            setEditing(null);
          }}
        />
      ) : null}
    </>
  );
}
