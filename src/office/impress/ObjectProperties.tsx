import { Bold, IndentDecrease, IndentIncrease, Italic, List, Underline } from "lucide-react";
import { ToolButton, ToolNumber } from "../office-ui";
import { useT } from "../../lib/i18n";
import type { SlideObject } from "../../lib/office-types";
import type { TextEditHandle, TextSelectionInfo } from "./TextFrameEditor";

/** Axis labels; symbols, not translated. */
const AXIS = { x: "X", y: "Y", w: "W", h: "H", z: "Z" } as const;

/** Position, size, rotation and stacking of the selected object (3.3e). */
export function ObjectGeometry({
  object,
  onPosition,
  onSize,
  onPatch,
}: {
  object: SlideObject;
  onPosition: (axis: "x" | "y", value: number) => void;
  onSize: (axis: "w" | "h", value: number) => void;
  onPatch: (patch: Partial<SlideObject>) => void;
}) {
  const t = useT();
  return (
    <>
      <div className="row">
        <ToolNumber value={Math.round(object.x)} onChange={(x) => onPosition("x", x)} title={AXIS.x} width={64} />
        <ToolNumber value={Math.round(object.y)} onChange={(y) => onPosition("y", y)} title={AXIS.y} width={64} />
      </div>
      <div className="row">
        <ToolNumber value={Math.round(object.w)} onChange={(w) => onSize("w", w)} title={AXIS.w} width={64} />
        <ToolNumber value={Math.round(object.h)} onChange={(h) => onSize("h", h)} title={AXIS.h} width={64} />
      </div>
      <div className="row">
        <ToolNumber
          value={Math.round(object.rotation)}
          onChange={(rotation) => onPatch({ rotation })}
          min={-180}
          max={180}
          title={t("impress.rotation")}
          width={64}
        />
        <ToolNumber value={object.z} onChange={(z) => onPatch({ z })} min={1} max={99} title={AXIS.z} width={64} />
      </div>
    </>
  );
}

/**
 * Rich-text tools shown while a text frame is being edited. Every control
 * keeps the focus in the frame (`keepFocus`, mousedown prevented) so the
 * selection the command applies to survives the click.
 */
export function TextFormatTools({
  selection,
  fallbackSizePt,
  handle,
  colors,
}: {
  selection: TextSelectionInfo | null;
  /** The frame's first paragraph size, used when nothing is selected. */
  fallbackSizePt: number;
  handle: () => TextEditHandle | null;
  colors: readonly string[];
}) {
  const t = useT();
  return (
    <div className="slide-text-tools" data-keep-text-edit>
      <div className="row">
        <ToolButton
          keepFocus
          icon={<Bold size={14} />}
          title={t("impress.bold")}
          active={Boolean(selection?.bold)}
          onClick={() => handle()?.applyFormat({ bold: !selection?.bold })}
        />
        <ToolButton
          keepFocus
          icon={<Italic size={14} />}
          title={t("impress.italic")}
          active={Boolean(selection?.italic)}
          onClick={() => handle()?.applyFormat({ italic: !selection?.italic })}
        />
        <ToolButton
          keepFocus
          icon={<Underline size={14} />}
          title={t("impress.underline")}
          active={Boolean(selection?.underline)}
          onClick={() => handle()?.applyFormat({ underline: !selection?.underline })}
        />
        <ToolButton
          keepFocus
          icon={<List size={14} />}
          title={t("impress.bullets")}
          active={Boolean(selection?.bullet)}
          onClick={() => handle()?.toggleBullet()}
        />
      </div>
      <div className="row">
        <ToolButton
          keepFocus
          icon={<IndentDecrease size={14} />}
          title={t("impress.decreaseLevel")}
          onClick={() => handle()?.changeLevel(-1)}
        />
        <ToolButton
          keepFocus
          icon={<IndentIncrease size={14} />}
          title={t("impress.increaseLevel")}
          onClick={() => handle()?.changeLevel(1)}
        />
        <ToolNumber
          value={selection?.sizePt ?? fallbackSizePt}
          onChange={(sizePt) => handle()?.applyFormat({ sizePt })}
          min={4}
          max={400}
          title={t("impress.fontSize")}
          width={64}
        />
      </div>
      <div className="row slide-text-colors">
        {colors.map((color) => (
          <button
            key={color}
            type="button"
            className="color-swatch"
            style={{ background: color }}
            title={color}
            aria-label={color}
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => handle()?.applyFormat({ color })}
          />
        ))}
        <input
          type="color"
          className="tool-color"
          title={t("impress.textColor")}
          onMouseDown={(event) => event.preventDefault()}
          onChange={(event) => handle()?.applyFormat({ color: event.target.value })}
        />
      </div>
    </div>
  );
}
