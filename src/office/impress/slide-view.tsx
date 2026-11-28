/**
 * Impress slide view: the object renderers, selection handles, object tree
 * and slide thumbnails.
 *
 * Extracted verbatim from ImpressEditor.tsx (B4 editor seam 3): these
 * components render the deck they are given and never reach into the editor
 * controller.
 */
import type { CSSProperties } from "react";
import { RotateCw } from "lucide-react";
import type { Deck, ShapeStyle, Slide, SlideFooter, SlideObject, TextParagraph } from "../../lib/office-types";
import { useT } from "../../lib/i18n";
import { recomputeConnectors, routeConnector } from "./connectors";
import { cropPreview, isFullCrop } from "./crop-preview";
import { inheritedObjects, pathKey, type SelectionPath } from "./selection";
import { ChartPreview } from "../charts/ChartPreview";
import { INHERITED_KEYS, MAX_GROUP_DEPTH, type Theme } from "./shared";
import { TextFrameEditor, type TextEditHandle, type TextSelectionInfo } from "./TextFrameEditor";

export function defaultShapeStyle(): ShapeStyle {
  return { fill: "#2563eb", stroke: null, strokeWidthPt: 1.5, opacity: 1, cornerRadiusPt: 0, shadow: false };
}

interface ObjectTreeProps {
  object: SlideObject;
  path: SelectionPath;
  depth: number;
  theme: Theme;
  scale: number;
  selectedKeys: Set<string>;
  editingKey: string | null;
  interactive: boolean;
  /** Sibling objects, so a connector line can resolve its glued endpoints. */
  objects?: SlideObject[];
  onObjectPointerDown?: (event: React.PointerEvent, path: SelectionPath) => void;
  onObjectDoubleClick?: (event: React.MouseEvent, path: SelectionPath) => void;
  onHandlePointerDown?: (event: React.PointerEvent, path: SelectionPath, mode: "resize" | "rotate") => void;
  onTextCommit?: (path: SelectionPath, paragraphs: TextParagraph[], recordUndo: boolean) => void;
  onTextDone?: () => void;
  onTextSelection?: (info: TextSelectionInfo | null) => void;
  textEditRef?: React.Ref<TextEditHandle>;
}

/** Crop handles over an image: four edges plus four corners. */
export function CropHandles({
  onHandlePointerDown,
}: {
  onHandlePointerDown: (event: React.PointerEvent, mode: string) => void;
}) {
  const modes = ["left", "right", "top", "bottom", "topleft", "topright", "bottomleft", "bottomright"];
  return (
    <>
      {modes.map((mode) => (
        <span
          key={mode}
          className={`crop-handle is-${mode}`}
          data-crop-handle={mode}
          onPointerDown={(event) => {
            event.stopPropagation();
            onHandlePointerDown(event, mode);
          }}
        />
      ))}
    </>
  );
}

export function SelectionHandles({
  onHandlePointerDown,
}: {
  onHandlePointerDown: (event: React.PointerEvent, mode: "resize" | "rotate") => void;
}) {
  return (
    <>
      <span
        className="resize-handle"
        onPointerDown={(event) => {
          event.stopPropagation();
          onHandlePointerDown(event, "resize");
        }}
      />
      <span
        className="rotate-handle"
        onPointerDown={(event) => {
          event.stopPropagation();
          onHandlePointerDown(event, "rotate");
        }}
      >
        <RotateCw size={10} />
      </span>
    </>
  );
}

export function ObjectTree(props: ObjectTreeProps) {
  const { object, path, depth, theme, scale, selectedKeys, editingKey, interactive } = props;
  if (object.kind === "group") {
    if (depth >= MAX_GROUP_DEPTH) return null;
    const children = [...(object.children ?? [])].sort((a, b) => a.z - b.z);
    return (
      <>
        {children.map((child) => {
          const childPath = [...path, child.id];
          const isSelected = selectedKeys.has(pathKey(childPath));
          return (
            <div
              key={child.id}
              className={`slide-object${isSelected ? " is-selected" : ""}`}
              style={{
                position: "absolute",
                left: (child.x - object.x) * scale,
                top: (child.y - object.y) * scale,
                width: child.w * scale,
                height: child.h * scale,
                transform: `rotate(${child.rotation}deg)`,
                pointerEvents: interactive ? "auto" : "none",
              }}
              onPointerDown={interactive ? (event) => props.onObjectPointerDown?.(event, childPath) : undefined}
              onDoubleClick={interactive ? (event) => props.onObjectDoubleClick?.(event, childPath) : undefined}
            >
              <ObjectTree {...props} object={child} path={childPath} depth={depth + 1} />
              {interactive && isSelected ? (
                <SelectionHandles
                  onHandlePointerDown={(event, mode) => props.onHandlePointerDown?.(event, childPath, mode)}
                />
              ) : null}
            </div>
          );
        })}
      </>
    );
  }
  return (
    <SlideObjectView
      object={object}
      objects={props.objects}
      theme={theme}
      scale={scale}
      editing={editingKey === pathKey(path)}
      onTextCommit={(paragraphs, recordUndo) => props.onTextCommit?.(path, paragraphs, recordUndo)}
      onTextDone={() => props.onTextDone?.()}
      onTextSelection={(info) => props.onTextSelection?.(info)}
      textEditRef={props.textEditRef}
    />
  );
}

export { ChartPreview };

function SlideObjectView({
  object,
  objects,
  theme,
  scale,
  editing,
  onTextCommit,
  onTextDone,
  onTextSelection,
  textEditRef,
}: {
  object: SlideObject;
  objects?: SlideObject[];
  theme: Theme;
  scale: number;
  editing: boolean;
  onTextCommit: (paragraphs: TextParagraph[], recordUndo: boolean) => void;
  onTextDone: () => void;
  onTextSelection?: (info: TextSelectionInfo | null) => void;
  textEditRef?: React.Ref<TextEditHandle>;
}) {
  const t = useT();
  if (object.kind === "group") return null;
  if (object.kind === "image") {
    if (!object.image || object.image.dataBase64 === "")
      return <div className="slide-image-placeholder">{object.placeholder ?? t("impress.emptyImage")}</div>;
    const crop = object.image.crop ?? null;
    const preview = cropPreview(crop);
    if (isFullCrop(crop)) {
      return (
        <img
          className="slide-image"
          src={`data:${object.image.mime};base64,${object.image.dataBase64}`}
          alt={object.image.alt}
          draggable={false}
        />
      );
    }
    // The full image stays in the model; the window only scales and shifts it.
    return (
      <div className="slide-image-crop">
        <img
          className="slide-image"
          src={`data:${object.image.mime};base64,${object.image.dataBase64}`}
          alt={object.image.alt}
          draggable={false}
          style={{
            position: "absolute",
            width: `${preview.widthPct}%`,
            height: `${preview.heightPct}%`,
            left: `${preview.offsetXPct}%`,
            top: `${preview.offsetYPct}%`,
          }}
        />
      </div>
    );
  }
  if (object.kind === "line" || object.kind === "arrow") {
    const line = object.line ?? { x2: object.w, y2: 0, beginArrow: false, endArrow: false, dash: "solid" };
    // A glued line draws between the routed anchors; a free line keeps its box.
    const routed = objects
      ? routeConnector(object, objects)
      : { x1: object.x, y1: object.y, x2: object.x + line.x2, y2: object.y + line.y2 };
    return (
      <svg width="100%" height="100%" viewBox={`0 0 ${object.w} ${object.h}`} preserveAspectRatio="none">
        <defs>
          <marker id={`arrow-${object.id}`} markerWidth="10" markerHeight="10" refX="7" refY="3" orient="auto">
            <path d="M0,0 L0,6 L8,3 z" fill={object.style?.stroke ?? theme.accent} />
          </marker>
        </defs>
        <line
          x1={routed.x1 - object.x}
          y1={routed.y1 - object.y}
          x2={routed.x2 - object.x}
          y2={routed.y2 - object.y}
          stroke={object.style?.stroke ?? theme.accent}
          strokeWidth={object.style?.strokeWidthPt ?? 2}
          strokeDasharray={line.dash === "dashed" ? "6 4" : line.dash === "dotted" ? "1 3" : undefined}
          markerEnd={line.endArrow ? `url(#arrow-${object.id})` : undefined}
        />
      </svg>
    );
  }
  if (object.kind === "table" && object.table) {
    return (
      <table className="slide-table">
        <tbody>
          {object.table.rows.map((row, rowIndex) => (
            <tr key={rowIndex}>
              {row.cells.map((cell, cellIndex) => (
                <td key={cellIndex} style={{ background: cell.background ?? undefined }}>
                  {cell.blocks.map((block) => blockTextOf(block)).join(" ")}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    );
  }
  if (object.kind === "chart") {
    return (
      <div style={{ width: "100%", height: "100%" }}>
        <ChartPreview chart={object.chart} theme={theme} scale={scale} />
      </div>
    );
  }
  const style = object.style ?? defaultShapeStyle();
  const isShape = object.kind === "rect" || object.kind === "ellipse" || object.kind === "roundRect";
  const emptyText = !object.text || object.text.paragraphs.every((paragraph) => paragraph.text.trim() === "");
  const placeholderLabel = object.placeholder && emptyText ? object.placeholder : null;
  return (
    <div
      className={`slide-shape ${object.kind}`}
      style={{
        background: isShape ? (style.fill ?? "transparent") : "transparent",
        border: isShape && style.stroke ? `${style.strokeWidthPt * scale}px solid ${style.stroke}` : undefined,
        borderRadius: object.kind === "ellipse" ? "50%" : `${(style.cornerRadiusPt ?? 0) * scale}px`,
        opacity: style.opacity || 1,
        boxShadow: style.shadow ? "0 6px 18px rgba(15,23,42,.25)" : undefined,
      }}
    >
      {placeholderLabel ? (
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            width: "100%",
            height: "100%",
            boxSizing: "border-box",
            border: "1px dashed currentColor",
            borderRadius: 4,
            padding: 4,
            fontStyle: "italic",
            opacity: 0.55,
            fontSize: Math.max(10, 12 * scale),
          }}
        >
          {placeholderLabel}
        </div>
      ) : object.text ? (
        editing ? (
          <TextFrameEditor
            ref={textEditRef}
            frame={object.text}
            scale={scale}
            color={theme.bodyColor}
            onCommit={onTextCommit}
            onDone={onTextDone}
            onSelection={(info) => onTextSelection?.(info)}
          />
        ) : (
          <div
            className="slide-text"
            style={{
              fontSize: (object.text.paragraphs[0]?.sizePt ?? object.text.sizePt ?? 18) * scale,
              color: object.text.paragraphs[0]?.color ?? theme.bodyColor,
              textAlign: (object.text.paragraphs[0]?.align ?? "left") as "left" | "center" | "right",
            }}
          >
            {object.text.paragraphs.map((paragraph, index) => (
              <p
                key={index}
                style={{
                  fontWeight: paragraph.bold ? 700 : undefined,
                  fontStyle: paragraph.italic ? "italic" : undefined,
                }}
              >
                {paragraph.bullet ? "• " : ""}
                {paragraph.text}
              </p>
            ))}
          </div>
        )
      ) : null}
    </div>
  );
}

function blockTextOf(block: { type: string; runs?: Array<{ text: string }> }): string {
  return (block.runs ?? []).map((run) => run.text).join("");
}

export function SlidePreview({
  deck,
  slide,
  theme,
  width,
  full,
  slideWidth,
  slideHeight,
  objectStyle,
}: {
  deck: Deck;
  slide: Slide;
  theme: Theme;
  width: number;
  full?: boolean;
  slideWidth?: number;
  slideHeight?: number;
  objectStyle?: (object: SlideObject) => CSSProperties | undefined;
}) {
  const actualWidth = full ? (slideWidth ?? deck.size.widthPt) : width;
  const scale = actualWidth / deck.size.widthPt;
  const height = full ? (slideHeight ?? deck.size.heightPt) : (deck.size.heightPt * width) / deck.size.widthPt;
  const inherited = inheritedObjects(deck, slide);
  // Imported decks may carry connectors whose box was never recomputed.
  const objects = recomputeConnectors(slide.objects);
  const slideNumber = deck.slides.indexOf(slide) + 1;
  return (
    <div
      className="slide-preview"
      style={{
        width: full ? "100%" : width,
        height: full ? "100%" : height,
        background: slide.background ?? theme.background,
        position: "relative",
        overflow: "hidden",
      }}
    >
      {inherited.map((object) => (
        <div
          key={object.id}
          className="slide-object is-inherited"
          style={{
            position: "absolute",
            left: object.x * scale,
            top: object.y * scale,
            width: object.w * scale,
            height: object.h * scale,
            transform: `rotate(${object.rotation}deg)`,
            pointerEvents: "none",
          }}
        >
          <ObjectTree
            object={object}
            path={[object.id]}
            depth={0}
            theme={theme}
            scale={full ? scale * 1.2 : scale}
            selectedKeys={INHERITED_KEYS}
            editingKey={null}
            interactive={false}
          />
        </div>
      ))}
      {[...objects]
        .sort((a, b) => a.z - b.z)
        .map((object) => (
          <div
            key={object.id}
            style={{
              position: "absolute",
              left: object.x * scale,
              top: object.y * scale,
              width: object.w * scale,
              height: object.h * scale,
              transform: `rotate(${object.rotation}deg)`,
              ...(objectStyle?.(object) ?? {}),
            }}
          >
            <ObjectTree
              object={object}
              objects={objects}
              path={[object.id]}
              depth={0}
              theme={theme}
              scale={full ? scale * 1.2 : scale}
              selectedKeys={INHERITED_KEYS}
              editingKey={null}
              interactive={false}
            />
          </div>
        ))}
      <SlideFooterBar
        footer={deck.footer}
        slideNumber={slideNumber > 0 ? slideNumber : 1}
        scale={full ? scale * 1.2 : scale}
        color={theme.bodyColor}
      />
    </div>
  );
}

/** The bottom line of the slide: footer text, date and slide number. */
export function SlideFooterBar({
  footer,
  slideNumber,
  scale,
  color,
}: {
  footer: SlideFooter | null | undefined;
  slideNumber: number;
  scale: number;
  color: string;
}) {
  if (!footer?.enabled) return null;
  return (
    <div className="slide-footer" style={{ fontSize: Math.max(7, 10 * scale), color }}>
      <span className="slide-footer-left">{footer.showText ? footer.text : ""}</span>
      <span className="slide-footer-center">{footer.showDate ? footer.dateText : ""}</span>
      <span className="slide-footer-right">{footer.showSlideNumber ? String(slideNumber) : ""}</span>
    </div>
  );
}
