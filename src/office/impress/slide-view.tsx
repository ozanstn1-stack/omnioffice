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
import type {
  ChartData,
  Deck,
  ShapeStyle,
  Slide,
  SlideFooter,
  SlideObject,
  TextParagraph,
} from "../../lib/office-types";
import { useT } from "../../lib/i18n";
import { recomputeConnectors, routeConnector } from "./connectors";
import { cropPreview, isFullCrop } from "./crop-preview";
import { inheritedObjects, pathKey, type SelectionPath } from "./selection";
import { CHART_PALETTE, INHERITED_KEYS, MAX_GROUP_DEPTH, formatChartCellValue, type Theme } from "./shared";
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

export function ChartPreview({ chart, theme, scale }: { chart: ChartData | null; theme: Theme; scale: number }) {
  const t = useT();
  if (!chart) return <div className="slide-chart-placeholder">{t("calc.chart")}</div>;
  const series = chart.series ?? [];
  const colorOf = (index: number) => series[index]?.color ?? CHART_PALETTE[index % CHART_PALETTE.length];
  const categories = chart.categoriesCache ?? [];
  const values = series.map((_, index) => chart.seriesValuesCache?.[index] ?? []);
  const hasValues = values.some((list) => list.length > 0);
  const rowCount = Math.max(categories.length, ...values.map((list) => list.length), 0);
  // Bars scale against the largest magnitude so negative series still get a
  // baseline; pies only use positive slices.
  const maxValue = Math.max(1e-9, ...values.flatMap((list) => list.map((value) => Math.abs(value))));
  const labelOf = (row: number) => categories[row] || `${row + 1}`;
  const heights = series.map((_, index) => 45 + ((index * 37) % 50));
  const smallFont = Math.max(7, 8 * scale);

  // The caches are what the exporter embeds, so the preview is data-driven when
  // they carry values; range-only charts keep the schematic index-based look.
  let plot: React.ReactNode = null;
  if (hasValues && series.length > 0) {
    if (chart.kind === "pie") {
      const pieIndex = values.findIndex((list) => list.some((value) => value > 0));
      const pieValues = pieIndex >= 0 ? values[pieIndex] : [];
      const pieTotal = pieValues.reduce((sum, value) => sum + Math.max(0, value), 0);
      if (pieTotal > 0) {
        const fractions = pieValues.map((value) => Math.max(0, value) / pieTotal);
        const slices = fractions.map((fraction, row) => {
          const start = fractions.slice(0, row).reduce((sum, value) => sum + value, 0);
          return `${CHART_PALETTE[row % CHART_PALETTE.length]} ${start * 100}% ${(start + fraction) * 100}%`;
        });
        plot = (
          <div style={{ display: "flex", alignItems: "center", gap: 8 * scale, width: "100%", height: "100%" }}>
            <div
              style={{
                height: "90%",
                aspectRatio: "1 / 1",
                borderRadius: "50%",
                background: `conic-gradient(${slices.join(", ")})`,
                border: `1px solid ${theme.accent}`,
              }}
            />
            <div style={{ flex: 1, display: "flex", flexDirection: "column", gap: 2 * scale, overflow: "hidden" }}>
              {pieValues.map((value, row) => (
                <span
                  key={row}
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 4 * scale,
                    whiteSpace: "nowrap",
                    overflow: "hidden",
                  }}
                >
                  <i
                    style={{
                      width: 8 * scale,
                      height: 8 * scale,
                      background: CHART_PALETTE[row % CHART_PALETTE.length],
                      borderRadius: 2,
                      flex: "0 0 auto",
                    }}
                  />
                  <span style={{ overflow: "hidden", textOverflow: "ellipsis" }}>{labelOf(row)}</span>
                  <span style={{ opacity: 0.7, marginLeft: "auto" }}>{formatChartCellValue(value)}</span>
                </span>
              ))}
            </div>
          </div>
        );
      }
    } else if (chart.kind === "line" || chart.kind === "area") {
      const pointX = (row: number) => (rowCount <= 1 ? 50 : (row / (rowCount - 1)) * 100);
      const pointY = (value: number) => 50 - (value / maxValue) * 45;
      plot = (
        <svg
          width="100%"
          height="100%"
          viewBox="0 0 100 100"
          preserveAspectRatio="none"
          style={{ display: "block", width: "100%", height: "100%" }}
        >
          <line
            x1="0"
            y1="50"
            x2="100"
            y2="50"
            stroke={theme.accent}
            strokeWidth="1"
            opacity="0.5"
            vectorEffect="non-scaling-stroke"
          />
          {series.map((entry, seriesIndex) => {
            const points = values[seriesIndex].map((value, row) => `${pointX(row)},${pointY(value)}`).join(" ");
            return (
              <g key={seriesIndex}>
                {chart.kind === "area" ? (
                  <polygon points={`0,50 ${points} 100,50`} fill={colorOf(seriesIndex)} opacity="0.35" />
                ) : null}
                <polyline
                  points={points}
                  fill="none"
                  stroke={colorOf(seriesIndex)}
                  strokeWidth="1.6"
                  vectorEffect="non-scaling-stroke"
                >
                  <title>{entry.name}</title>
                </polyline>
              </g>
            );
          })}
        </svg>
      );
    } else if (chart.kind === "bar") {
      plot = (
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            justifyContent: "center",
            gap: 3 * scale,
            width: "100%",
            height: "100%",
          }}
        >
          {Array.from({ length: rowCount }, (_, row) => (
            <div key={row} style={{ display: "flex", alignItems: "center", gap: 4 * scale }}>
              <span
                style={{
                  width: 56 * scale,
                  flex: "0 0 auto",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                  opacity: 0.8,
                }}
              >
                {labelOf(row)}
              </span>
              <div style={{ flex: 1, display: "flex", flexDirection: "column", gap: 1 }}>
                {series.map((entry, seriesIndex) => {
                  const value = values[seriesIndex]?.[row] ?? 0;
                  return (
                    <div
                      key={seriesIndex}
                      title={`${entry.name}: ${formatChartCellValue(value)}`}
                      style={{ display: "flex", alignItems: "center", gap: 3 * scale }}
                    >
                      <div
                        style={{
                          width: `${(Math.max(0, value) / maxValue) * 100}%`,
                          height: Math.max(4, 7 * scale),
                          background: colorOf(seriesIndex),
                          borderRadius: 2,
                        }}
                      />
                      {chart.showLabels ? (
                        <span style={{ fontSize: smallFont, opacity: 0.75 }}>{formatChartCellValue(value)}</span>
                      ) : null}
                    </div>
                  );
                })}
              </div>
            </div>
          ))}
        </div>
      );
    } else {
      plot = (
        <div
          style={{
            display: "flex",
            alignItems: "flex-end",
            justifyContent: "space-around",
            gap: 2 * scale,
            width: "100%",
            height: "100%",
          }}
        >
          {Array.from({ length: rowCount }, (_, row) => {
            const total = series.reduce((sum, _, seriesIndex) => sum + Math.max(0, values[seriesIndex]?.[row] ?? 0), 0);
            return (
              <div
                key={row}
                style={{
                  flex: 1,
                  maxWidth: 64 * scale,
                  height: "100%",
                  display: "flex",
                  flexDirection: "column",
                  justifyContent: "flex-end",
                  alignItems: "center",
                  gap: 1,
                }}
              >
                {chart.showLabels && (chart.stacked || series.length === 1) ? (
                  <span style={{ fontSize: smallFont, opacity: 0.75 }}>
                    {formatChartCellValue(chart.stacked ? total : (values[0]?.[row] ?? 0))}
                  </span>
                ) : null}
                <div
                  style={{
                    width: "100%",
                    height: `${(total / maxValue) * 100}%`,
                    display: "flex",
                    flexDirection: chart.stacked ? "column-reverse" : "row",
                    alignItems: "flex-end",
                    justifyContent: "center",
                    gap: 1,
                  }}
                >
                  {series.map((entry, seriesIndex) => {
                    const raw = values[seriesIndex]?.[row] ?? 0;
                    const value = Math.max(0, raw);
                    return (
                      <div
                        key={seriesIndex}
                        title={`${entry.name}: ${formatChartCellValue(raw)}`}
                        style={{
                          flex: chart.stacked ? "none" : 1,
                          width: chart.stacked ? "100%" : undefined,
                          height: chart.stacked ? `${total > 0 ? (value / total) * 100 : 0}%` : "100%",
                          background: colorOf(seriesIndex),
                          borderRadius: chart.stacked ? 0 : 2,
                        }}
                      />
                    );
                  })}
                </div>
              </div>
            );
          })}
        </div>
      );
    }
  }

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        width: "100%",
        height: "100%",
        padding: 6 * scale,
        gap: 3 * scale,
        fontSize: Math.max(8, 10 * scale),
        color: theme.bodyColor,
        overflow: "hidden",
        boxSizing: "border-box",
      }}
    >
      <div style={{ display: "flex", alignItems: "baseline", gap: 6 * scale, flexWrap: "wrap" }}>
        {chart.title ? <strong style={{ fontSize: Math.max(9, 12 * scale) }}>{chart.title}</strong> : null}
        <span style={{ opacity: 0.7 }}>{t(`calc.chart_${chart.kind}`)}</span>
        {chart.stacked ? <span style={{ opacity: 0.7 }}>· {t("impress.chartStacked")}</span> : null}
        {chart.showLabels ? <span style={{ opacity: 0.7 }}>· {t("impress.chartShowLabels")}</span> : null}
      </div>
      <div
        style={{
          position: "relative",
          flex: 1,
          minHeight: 40 * scale,
          border: `1px dashed ${theme.accent}`,
          borderRadius: 4,
          display: "flex",
          alignItems: "flex-end",
          justifyContent: "center",
          gap: 4 * scale,
          padding: 4 * scale,
          overflow: "hidden",
        }}
      >
        {series.length === 0 ? (
          <span style={{ opacity: 0.6, textAlign: "center" }}>{t("impress.chartNoSeries")}</span>
        ) : plot ? (
          plot
        ) : chart.kind === "pie" ? (
          <div style={{ display: "flex", width: "100%", height: "100%" }}>
            {series.map((entry, index) => (
              <div
                key={index}
                title={`${entry.name}${entry.range ? ` · ${entry.range}` : ""}`}
                style={{ flex: 1, background: colorOf(index), opacity: 0.85 }}
              />
            ))}
          </div>
        ) : (
          series.map((entry, index) => (
            <div
              key={index}
              title={`${entry.name}${entry.range ? ` · ${entry.range}` : ""}`}
              style={{
                flex: 1,
                maxWidth: 48 * scale,
                height: `${heights[index]}%`,
                background: colorOf(index),
                opacity: 0.85,
                borderRadius: 2,
              }}
            />
          ))
        )}
      </div>
      {hasValues && chart.kind !== "pie" && chart.kind !== "bar" ? (
        <div
          style={{ display: "flex", justifyContent: "space-around", gap: 2 * scale, opacity: 0.8, overflow: "hidden" }}
        >
          {Array.from({ length: rowCount }, (_, row) => (
            <span
              key={row}
              style={{
                flex: 1,
                maxWidth: 64 * scale,
                textAlign: "center",
                overflow: "hidden",
                textOverflow: "ellipsis",
                whiteSpace: "nowrap",
              }}
            >
              {labelOf(row)}
            </span>
          ))}
        </div>
      ) : null}
      <div style={{ display: "flex", justifyContent: "space-between", opacity: 0.75, gap: 6 * scale }}>
        <span>{chart.yTitle || ""}</span>
        <span style={{ textAlign: "center", flex: 1 }}>
          {hasValues ? t("impress.chartCachedData") : chart.categories || t("impress.chartNoCategories")}
        </span>
        <span>{chart.xTitle || ""}</span>
      </div>
      {chart.legend && series.length > 0 ? (
        <div style={{ display: "flex", flexWrap: "wrap", gap: 4 * scale, opacity: 0.9 }}>
          {series.map((entry, index) => (
            <span key={index} style={{ display: "inline-flex", alignItems: "center", gap: 3 * scale }}>
              <i
                style={{
                  width: 8 * scale,
                  height: 8 * scale,
                  background: colorOf(index),
                  display: "inline-block",
                  borderRadius: 2,
                }}
              />
              {entry.name || `${t("impress.chartSeries")} ${index + 1}`}
              {!hasValues && entry.range ? ` · ${entry.range}` : ""}
            </span>
          ))}
        </div>
      ) : null}
      {hasValues ? null : (
        <div style={{ opacity: 0.55, fontSize: Math.max(7, 8 * scale) }}>{t("impress.chartSchematicNote")}</div>
      )}
    </div>
  );
}

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
  if (object.kind === "group") return null;
  if (object.kind === "image") {
    if (!object.image || object.image.dataBase64 === "")
      return <div className="slide-image-placeholder">{object.placeholder ?? "Double-click to add an image"}</div>;
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
