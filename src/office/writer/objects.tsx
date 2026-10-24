/**
 * Writer object panels: image sizing, watermark options and the page overlay.
 */
import { useState } from "react";
import { useT } from "../../lib/i18n";
import type { Block, Watermark } from "../../lib/office-types";

function ImageOptions({
  block,
  onChange,
}: {
  block: Block;
  onChange: (width: number, height: number, align: string, caption: string, wrap: string) => void;
}) {
  const t = useT();
  if (block.type !== "image") return null;
  const aspect = block.widthPt / Math.max(1, block.heightPt);
  const wrap = block.wrap || "inline";
  const commit = (width: number, height: number, align: string, caption: string, wrapValue: string) =>
    onChange(width, height, align, caption, wrapValue);
  return (
    <div className="stack">
      <label className="field">
        <span>{t("writer.imageWidth")}</span>
        <input
          type="number"
          value={Math.round(block.widthPt)}
          onChange={(event) => {
            const width = Number(event.target.value);
            commit(width, Math.round(width / aspect), block.align, block.caption, wrap);
          }}
        />
      </label>
      <label className="field">
        <span>{t("writer.imageCaption")}</span>
        <input
          value={block.caption}
          onChange={(event) => commit(block.widthPt, block.heightPt, block.align, event.target.value, wrap)}
        />
      </label>
      <label className="field">
        <span>{t("writer.imageAlign")}</span>
        <select
          value={block.align}
          onChange={(event) => commit(block.widthPt, block.heightPt, event.target.value, block.caption, wrap)}
        >
          <option value="left">{t("writer.imageAlignLeft")}</option>
          <option value="center">{t("writer.imageAlignCenter")}</option>
          <option value="right">{t("writer.imageAlignRight")}</option>
        </select>
      </label>
      <label className="field">
        <span>{t("writer.imageWrap")}</span>
        <select
          value={wrap}
          onChange={(event) => commit(block.widthPt, block.heightPt, block.align, block.caption, event.target.value)}
        >
          <option value="inline">{t("writer.imageWrapInline")}</option>
          <option value="square">{t("writer.imageWrapSquare")}</option>
          <option value="topBottom">{t("writer.imageWrapTopBottom")}</option>
        </select>
      </label>
    </div>
  );
}

/** Watermark defaults: a large translucent diagonal stamp. */
const WATERMARK_DEFAULTS: Watermark = { text: "", color: null, opacity: 0.2, rotation: -45, fontPt: 72, bold: true };

function WatermarkOptions({
  watermark,
  onApply,
  onRemove,
}: {
  watermark: Watermark | null;
  onApply: (watermark: Watermark) => void;
  onRemove: () => void;
}) {
  const t = useT();
  const [draft, setDraft] = useState<Watermark>(watermark ? { ...watermark } : { ...WATERMARK_DEFAULTS });
  const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));
  return (
    <div className="stack">
      <div className="row">
        <button
          type="button"
          className="btn btn-soft"
          onClick={() => setDraft((current) => ({ ...current, text: t("writer.watermarkDraft"), bold: true }))}
        >
          {t("writer.watermarkDraft")}
        </button>
        <button
          type="button"
          className="btn btn-soft"
          onClick={() => setDraft((current) => ({ ...current, text: t("writer.watermarkConfidential"), bold: true }))}
        >
          {t("writer.watermarkConfidential")}
        </button>
      </div>
      <label className="field">
        <span>{t("writer.watermarkText")}</span>
        <input value={draft.text} onChange={(event) => setDraft({ ...draft, text: event.target.value })} />
      </label>
      <div className="row">
        <label className="field">
          <span>{t("writer.watermarkOpacity")}</span>
          <input
            type="number"
            min={0}
            max={1}
            step={0.05}
            value={draft.opacity}
            onChange={(event) => setDraft({ ...draft, opacity: clamp(Number(event.target.value), 0, 1) })}
          />
        </label>
        <label className="field">
          <span>{t("writer.watermarkRotation")}</span>
          <input
            type="number"
            min={-180}
            max={180}
            step={5}
            value={draft.rotation}
            onChange={(event) => setDraft({ ...draft, rotation: clamp(Number(event.target.value), -180, 180) })}
          />
        </label>
        <label className="field">
          <span>{t("writer.watermarkSize")}</span>
          <input
            type="number"
            min={8}
            max={200}
            step={4}
            value={draft.fontPt}
            onChange={(event) => setDraft({ ...draft, fontPt: clamp(Number(event.target.value), 8, 200) })}
          />
        </label>
      </div>
      <div className="row">
        <button
          type="button"
          className="btn btn-primary"
          disabled={!draft.text.trim()}
          onClick={() => onApply({ ...draft, text: draft.text.trim() })}
        >
          {t("writer.watermarkApply")}
        </button>
        <button type="button" className="btn btn-soft" onClick={onRemove}>
          {t("writer.watermarkRemove")}
        </button>
      </div>
    </div>
  );
}

/** The translucent, rotated watermark drawn behind a page's content. */
function WatermarkLayer({
  watermark,
  widthPx,
  heightPx,
  zoom,
}: {
  watermark: Watermark;
  widthPx: number;
  heightPx: number;
  zoom: number;
}) {
  if (!watermark.text.trim()) return null;
  return (
    <div className="writer-watermark-layer" aria-hidden="true" style={{ width: widthPx, height: heightPx }}>
      <span
        className="writer-watermark"
        style={{
          color: watermark.color ?? "#94a3b8",
          opacity: watermark.opacity,
          fontSize: `${watermark.fontPt * zoom}pt`,
          fontWeight: watermark.bold ? 700 : 400,
          transform: `rotate(${watermark.rotation}deg)`,
        }}
      >
        {watermark.text}
      </span>
    </div>
  );
}

export { ImageOptions, WatermarkLayer, WatermarkOptions };
