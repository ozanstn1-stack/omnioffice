/**
 * Interactive and static Writer chart block views: preview card, selection,
 * proportional corner handles, alignment, inline caption, Edit data, and Delete.
 */
import { useState } from "react";
import type { Block, ChartData } from "../../lib/office-types";
import { useT } from "../../lib/i18n";
import { ChartDialog } from "../charts/ChartDialog";
import { ChartPreview } from "../charts/ChartPreview";
import { WRITER_CHART_THEME } from "../charts/chart-helpers";
import { ObjectSizeHandles } from "./object-handles";

export type ChartBlock = Extract<Block, { type: "chart" }>;

function ChartFigureFrame({
  block,
  widthPx,
  heightPx,
  zoom,
  selected,
  interactiveProps,
  handles,
  toolbar,
}: {
  block: ChartBlock;
  widthPx: number;
  heightPx: number;
  zoom: number;
  selected?: boolean;
  interactiveProps?: React.HTMLAttributes<HTMLSpanElement>;
  handles?: React.ReactNode;
  toolbar?: React.ReactNode;
}) {
  return (
    <figure
      className={`writer-chart${selected ? " is-selected" : ""}`}
      style={{ textAlign: (block.align || "center") as "left" | "center" | "right" }}
    >
      <span className="writer-chart-frame" style={{ width: widthPx, height: heightPx }} {...interactiveProps}>
        <ChartPreview chart={block.chart} theme={WRITER_CHART_THEME} scale={zoom} />
        {handles}
      </span>
      {block.caption ? <figcaption>{block.caption}</figcaption> : null}
      {toolbar}
    </figure>
  );
}

export function StaticChartBlock({ block, zoom }: { block: ChartBlock; zoom: number }) {
  return (
    <ChartFigureFrame
      block={block}
      widthPx={block.widthPt * (96 / 72) * zoom}
      heightPx={block.heightPt * (96 / 72) * zoom}
      zoom={zoom}
    />
  );
}

export function WriterChartBlock({
  block,
  index,
  zoom,
  selected,
  onSelect,
  onUpdate,
  onEditData,
  onDelete,
}: {
  block: ChartBlock;
  index: number;
  zoom: number;
  selected: boolean;
  onSelect: (index: number | null) => void;
  onUpdate: (block: Block) => void;
  onEditData?: (index: number) => void;
  onDelete?: (index: number) => void;
}) {
  const t = useT();
  const [sizePreview, setSizePreview] = useState<{ widthPt: number; heightPt: number } | null>(null);
  const widthPt = sizePreview?.widthPt ?? block.widthPt;
  const heightPt = sizePreview?.heightPt ?? block.heightPt;
  const pxScale = (96 / 72) * zoom;

  return (
    <ChartFigureFrame
      block={block}
      widthPx={widthPt * pxScale}
      heightPx={heightPt * pxScale}
      zoom={zoom}
      selected={selected}
      interactiveProps={{
        role: "button",
        tabIndex: 0,
        onClick: () => onSelect(index),
        onDoubleClick: () => onEditData?.(index),
        onKeyDown: (event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            onSelect(index);
          }
        },
      }}
      handles={
        selected ? (
          <ObjectSizeHandles
            widthPt={widthPt}
            heightPt={heightPt}
            zoom={zoom}
            onPreview={setSizePreview}
            onCommit={(next) => {
              setSizePreview(null);
              onUpdate({ ...block, widthPt: next.widthPt, heightPt: next.heightPt });
            }}
          />
        ) : null
      }
      toolbar={
        selected ? (
          <div className="writer-chart-toolbar" contentEditable={false}>
            <button type="button" className="btn btn-primary" onClick={() => onEditData?.(index)}>
              {t("writer.chartEditData")}
            </button>
            {(["left", "center", "right"] as const).map((align) => (
              <button
                key={align}
                type="button"
                className={`btn ${(block.align || "center") === align ? "btn-primary" : "btn-soft"}`}
                onClick={() => onUpdate({ ...block, align })}
              >
                {t(`writer.imageAlign${align[0].toUpperCase()}${align.slice(1)}`)}
              </button>
            ))}
            <input
              aria-label={t("writer.imageCaption")}
              value={block.caption}
              placeholder={t("writer.chartCaptionPlaceholder")}
              onChange={(event) => onUpdate({ ...block, caption: event.target.value })}
            />
            {onDelete ? (
              <button type="button" className="btn btn-soft" onClick={() => onDelete(index)}>
                {t("common.delete")}
              </button>
            ) : null}
          </div>
        ) : null
      }
    />
  );
}

export type WriterChartDialogState =
  { mode: "insert"; chart: ChartData } | { mode: "edit"; index: number; chart: ChartData } | null;

export function WriterChartModal({
  dialog,
  currentBlocks,
  insertBlockAfter,
  updateBlock,
  selectBlock,
  onClose,
}: {
  dialog: WriterChartDialogState;
  currentBlocks: () => Block[];
  insertBlockAfter: (index: number, block: Block) => void;
  updateBlock: (index: number, block: Block) => void;
  selectBlock: (index: number | null) => void;
  onClose: () => void;
}) {
  const t = useT();
  if (!dialog) return null;
  return (
    <ChartDialog
      chart={dialog.chart}
      title={t("writer.chart")}
      onClose={onClose}
      onSave={(chart) => {
        if (dialog.mode === "insert") {
          const active = window.document.activeElement as HTMLElement | null;
          const anchor = active?.dataset?.blockIndex ? Number(active.dataset.blockIndex) : currentBlocks().length - 1;
          insertBlockAfter(anchor, { type: "chart", chart, widthPt: 420, heightPt: 260, align: "center", caption: "" });
          selectBlock(anchor + 1);
        } else {
          const current = currentBlocks()[dialog.index];
          if (current?.type === "chart") updateBlock(dialog.index, { ...current, chart });
        }
        onClose();
      }}
    />
  );
}
