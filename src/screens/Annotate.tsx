/**
 * Annotate (v4.6): text boxes, sticky notes, highlights, underlines,
 * strikethroughs, freehand ink, image/signature stamps, rectangles and lines.
 *
 * New markings are written as real, editable annotations
 * (`pdf_annotate_editable`, the default) or flattened into the page content
 * (`annotate_pdf`). The document's own annotations are listed with
 * `pdf_list_annotations` and can be moved, resized, updated or deleted in the
 * same save run (`pdf_edit_annotations` runs first, against the input file).
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Highlighter,
  Image as ImageIcon,
  Minus,
  PenLine,
  Signature as SignatureIcon,
  Square,
  StickyNote,
  Strikethrough,
  Trash2,
  Type,
  Underline,
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { appDataDir } from "@tauri-apps/api/path";
import { open } from "@tauri-apps/plugin-dialog";
import { isAndroid, pickAndroidFiles } from "../lib/mobile";
import { Badge, Button, Card, ColorInput, Field, NumberInput, Segmented, Slider, Toggle } from "../components/ui";
import { DropZone, InfoStrip, OutputBar, ResultCard } from "../components/files";
import { PageCanvas, Pager } from "../components/pages";
import { OptionCard, TwoColumn } from "../components/layout";
import { useT } from "../lib/i18n";
import { useTool } from "../lib/useTool";
import { annotatePdf, pdfAnnotateEditable, pdfEditAnnotations, pdfListAnnotations, toAppError } from "../lib/api";
import { fileStem, joinPath, uid } from "../lib/format";
import type { Annotation, AnnotationEditItem, AnnotationKind, EditableAnnotation } from "../lib/types";
import { pointToDisplay, strokesToPaths } from "./annotate/geometry";
import { SignaturePad, dataUrlForBase64 } from "./annotate/signature-pad";

const TOOLS: { value: AnnotationKind; icon: React.ReactNode; labelKey: string }[] = [
  { value: "textbox", icon: <Type size={14} />, labelKey: "annotate.textboxTool" },
  { value: "note", icon: <StickyNote size={14} />, labelKey: "annotate.noteTool" },
  { value: "highlight", icon: <Highlighter size={14} />, labelKey: "annotate.highlightTool" },
  { value: "underline", icon: <Underline size={14} />, labelKey: "annotate.underlineTool" },
  { value: "strike", icon: <Strikethrough size={14} />, labelKey: "annotate.strikeTool" },
  { value: "ink", icon: <PenLine size={14} />, labelKey: "annotate.inkTool" },
  { value: "signature", icon: <SignatureIcon size={14} />, labelKey: "annotate.signatureTool" },
  { value: "image", icon: <ImageIcon size={14} />, labelKey: "annotate.imageTool" },
  { value: "rect", icon: <Square size={14} />, labelKey: "annotate.rectTool" },
  { value: "line", icon: <Minus size={14} />, labelKey: "annotate.lineTool" },
];

const DRAG_TOOLS: AnnotationKind[] = ["ink", "rect", "line"];
const BOX_KINDS: AnnotationKind[] = [
  "textbox",
  "note",
  "text",
  "highlight",
  "underline",
  "strike",
  "rect",
  "image",
  "signature",
];

function annotationKey(annotation: { page: number; index: number }): string {
  return `${annotation.page}:${annotation.index}`;
}

/** Percent-positioned overlay box for a display-space rectangle. */
function overlayStyle(
  x: number,
  y: number,
  w: number,
  h: number,
  displayW: number,
  displayH: number,
): React.CSSProperties {
  return {
    position: "absolute",
    left: `${(x / displayW) * 100}%`,
    top: `${(y / displayH) * 100}%`,
    width: `${(Math.max(0, w) / displayW) * 100}%`,
    height: `${(Math.max(0, h) / displayH) * 100}%`,
  };
}

/** Normalizes a backend colour to the `#rrggbb` the colour input needs. */
function normalizeColor(color: string): string {
  return /^#[0-9a-fA-F]{6}$/.test(color) ? color.toLowerCase() : "#e11d48";
}

/**
 * Stages the edit pass in the app data directory: `pdf_edit_annotations` gets
 * a file to write, and the annotate pass then reads that result. Kept out of
 * the user's folders on every platform (Android publishes the final file).
 */
async function stageEditedCopy(input: string): Promise<string> {
  const base = await appDataDir();
  const directory = joinPath(base, "AnnotateStaging");
  await invoke("ensure_dir", { path: directory }).catch(() => undefined);
  return joinPath(directory, `${fileStem(input)}-${uid("edit")}.pdf`);
}

export function Annotate({ initialFiles, dragging }: { initialFiles?: string[]; dragging: boolean }) {
  const t = useT();
  const session = useTool({ suffix: "_annotated", accept: "pdf", initialPaths: initialFiles });
  const [kind, setKind] = useState<AnnotationKind>("textbox");
  const [text, setText] = useState("");
  const [fontSize, setFontSize] = useState(16);
  const [bold, setBold] = useState(false);
  const [color, setColor] = useState("#e11d48");
  const [opacity, setOpacity] = useState(0.35);
  const [lineWidth, setLineWidth] = useState(2);
  const [imagePath, setImagePath] = useState<string | null>(null);
  const [signatureBase64, setSignatureBase64] = useState<string | null>(null);
  const [padOpen, setPadOpen] = useState(false);
  const [editable, setEditable] = useState(true);
  const [keepSignatures, setKeepSignatures] = useState(true);
  const [page, setPage] = useState(1);
  const [annotations, setAnnotations] = useState<Annotation[]>([]);
  const [existing, setExisting] = useState<EditableAnnotation[]>([]);
  const [existingLoaded, setExistingLoaded] = useState(false);
  const [existingError, setExistingError] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [pendingEdits, setPendingEdits] = useState<AnnotationEditItem[]>([]);
  const [draftW, setDraftW] = useState(0);
  const [draftH, setDraftH] = useState(0);
  const [movePreview, setMovePreview] = useState<{ key: string; dx: number; dy: number } | null>(null);
  const [inkPoints, setInkPoints] = useState<number[][] | null>(null);
  const [dragRect, setDragRect] = useState<{ x0: number; y0: number; x1: number; y1: number } | null>(null);

  const canvasRef = useRef<HTMLDivElement>(null);
  const inkRef = useRef<number[][] | null>(null);
  const dragRectRef = useRef<{ x0: number; y0: number; x1: number; y1: number } | null>(null);
  const moveRef = useRef<{ key: string; startX: number; startY: number } | null>(null);
  const originalsRef = useRef(new Map<string, EditableAnnotation>());

  const primaryPath = session.primary?.path;
  const primaryPassword = session.password;
  const pageCount = session.info?.pageCount ?? 1;
  const geometry = session.info?.pageGeometries.find((entry) => entry.page === page);
  const displayW = geometry?.display_width_pt ?? 0;
  const displayH = geometry?.display_height_pt ?? 0;
  const isDragTool = DRAG_TOOLS.includes(kind);

  const selectedAnnotation = useMemo(
    () => existing.find((annotation) => annotationKey(annotation) === selected) ?? null,
    [existing, selected],
  );

  const reloadExisting = useCallback(async () => {
    if (!primaryPath) {
      setExisting([]);
      setExistingLoaded(false);
      setExistingError(null);
      setSelected(null);
      setPendingEdits([]);
      return;
    }
    try {
      const list = await pdfListAnnotations(primaryPath, primaryPassword || undefined);
      const entries = list ?? [];
      originalsRef.current = new Map(entries.map((entry) => [annotationKey(entry), { ...entry }]));
      setExisting(entries);
      setExistingLoaded(true);
      setExistingError(null);
      setSelected(null);
      setPendingEdits([]);
    } catch (error) {
      setExisting([]);
      setExistingLoaded(true);
      setExistingError(toAppError(error).message);
    }
  }, [primaryPath, primaryPassword]);

  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect -- the async loader owns the annotation state
    void reloadExisting();
  }, [reloadExisting]);

  const addAtPoint = (nx: number, ny: number) => {
    if (!geometry) return;
    const created = makeAnnotation(kind, nx * geometry.display_width_pt, ny * geometry.display_height_pt);
    if (created) setAnnotations((previous) => [...previous, created]);
  };

  /** Builds the wire annotation for a tool at a display-space point. */
  const makeAnnotation = (
    tool: AnnotationKind,
    x: number,
    y: number,
    box?: { w: number; h: number; x2?: number; y2?: number },
  ): Annotation | null => {
    if (!geometry) return null;
    if (tool === "image" && !imagePath) return null;
    if (tool === "signature" && !signatureBase64) return null;
    const W = geometry.display_width_pt;
    const H = geometry.display_height_pt;
    const w = box?.w ?? (tool === "textbox" ? W * 0.4 : W * 0.3);
    const h = box?.h ?? (tool === "textbox" ? Math.max(fontSize * 2, 18) : H * 0.08);
    return {
      kind: tool,
      page,
      x,
      y,
      w,
      h,
      text,
      font_size_pt: fontSize,
      bold,
      color,
      opacity: tool === "highlight" ? opacity : 1,
      image_path: tool === "image" ? imagePath : null,
      line_width_pt: lineWidth,
      x2: box?.x2 ?? (tool === "line" ? x + W * 0.25 : null),
      y2: box?.y2 ?? (tool === "line" ? y + H * 0.1 : null),
      image_base64: tool === "signature" ? signatureBase64 : null,
    };
  };

  const pushEdit = (edit: AnnotationEditItem) => {
    setPendingEdits((previous) => {
      const kept = previous.filter((entry) => {
        if (entry.page !== edit.page || entry.index !== edit.index) return true;
        if (edit.action === "delete") return false;
        return entry.action !== edit.action;
      });
      return [...kept, edit];
    });
  };

  const selectExisting = (annotation: EditableAnnotation) => {
    setSelected(annotationKey(annotation));
    setDraftW(Math.round(annotation.w));
    setDraftH(Math.round(annotation.h));
    if (annotation.page !== page) setPage(annotation.page);
  };

  const queueUpdate = (patch: Partial<Pick<EditableAnnotation, "text" | "color" | "opacity">>) => {
    const annotation = selectedAnnotation;
    if (!annotation) return;
    const text = patch.text ?? annotation.text;
    const color = patch.color ?? annotation.color;
    const opacity = patch.opacity ?? annotation.opacity;
    setExisting((previous) =>
      previous.map((entry) => (annotationKey(entry) === selected ? { ...entry, text, color, opacity } : entry)),
    );
    pushEdit({ page: annotation.page, index: annotation.index, action: "update", text, color, opacity });
  };

  const applyResize = () => {
    const annotation = selectedAnnotation;
    if (!annotation) return;
    const w = Math.max(2, draftW);
    const h = Math.max(2, draftH);
    setExisting((previous) =>
      previous.map((entry) => (annotationKey(entry) === selected ? { ...entry, w, h } : entry)),
    );
    pushEdit({
      page: annotation.page,
      index: annotation.index,
      action: "resize",
      x: annotation.x,
      y: annotation.y,
      w,
      h,
    });
  };

  const deleteSelected = () => {
    const annotation = selectedAnnotation;
    if (!annotation) return;
    pushEdit({ page: annotation.page, index: annotation.index, action: "delete" });
    setExisting((previous) => previous.filter((entry) => annotationKey(entry) !== selected));
    setSelected(null);
  };

  /** Display point of a client coordinate, measured against the page image. */
  const displayPoint = (event: { clientX: number; clientY: number }): { x: number; y: number } | null => {
    const image = canvasRef.current?.querySelector("img");
    if (!image || !geometry) return null;
    return pointToDisplay(event, image.getBoundingClientRect(), geometry.display_width_pt, geometry.display_height_pt);
  };

  const startMove = (event: React.PointerEvent<Element>, annotation: EditableAnnotation) => {
    event.stopPropagation();
    selectExisting(annotation);
    event.currentTarget.setPointerCapture?.(event.pointerId);
    moveRef.current = { key: annotationKey(annotation), startX: event.clientX, startY: event.clientY };
  };

  const moveDelta = (event: { clientX: number; clientY: number }): { dx: number; dy: number } | null => {
    const drag = moveRef.current;
    const image = canvasRef.current?.querySelector("img");
    if (!drag || !image || !geometry) return null;
    const bounds = image.getBoundingClientRect();
    if (bounds.width < 1 || bounds.height < 1) return null;
    return {
      dx: ((event.clientX - drag.startX) * geometry.display_width_pt) / bounds.width,
      dy: ((event.clientY - drag.startY) * geometry.display_height_pt) / bounds.height,
    };
  };

  const handleMoveDrag = (event: React.PointerEvent<Element>) => {
    const drag = moveRef.current;
    if (!drag) return;
    event.stopPropagation();
    const delta = moveDelta(event);
    if (delta) setMovePreview({ key: drag.key, dx: delta.dx, dy: delta.dy });
  };

  const endMoveDrag = (event: React.PointerEvent<Element>) => {
    const drag = moveRef.current;
    if (!drag) return;
    event.stopPropagation();
    const delta = moveDelta(event);
    moveRef.current = null;
    setMovePreview(null);
    if (!delta || (Math.abs(delta.dx) < 0.5 && Math.abs(delta.dy) < 0.5)) return;
    const original = originalsRef.current.get(drag.key);
    if (!original) return;
    pushEdit({ page: original.page, index: original.index, action: "move", dx: delta.dx, dy: delta.dy });
    setExisting((previous) =>
      previous.map((entry) =>
        annotationKey(entry) === drag.key ? { ...entry, x: original.x + delta.dx, y: original.y + delta.dy } : entry,
      ),
    );
  };

  const addInkStroke = (stroke: number[][]) => {
    if (!geometry || stroke.length < 2) return;
    const xs = stroke.map(([x]) => x);
    const ys = stroke.map(([, y]) => y);
    const x = Math.min(...xs);
    const y = Math.min(...ys);
    setAnnotations((previous) => [
      ...previous,
      {
        kind: "ink",
        page,
        x,
        y,
        w: Math.max(1, Math.max(...xs) - x),
        h: Math.max(1, Math.max(...ys) - y),
        text: "",
        font_size_pt: fontSize,
        bold: false,
        color,
        opacity: 1,
        image_path: null,
        line_width_pt: lineWidth,
        x2: null,
        y2: null,
        strokes: [stroke],
      },
    ]);
  };

  const handleInkDown = (event: React.PointerEvent<HTMLDivElement>) => {
    event.stopPropagation();
    const point = displayPoint(event);
    if (!point) return;
    event.currentTarget.setPointerCapture?.(event.pointerId);
    inkRef.current = [[point.x, point.y]];
    setInkPoints(inkRef.current);
  };

  const handleInkMove = (event: React.PointerEvent<HTMLDivElement>) => {
    if (!inkRef.current) return;
    event.stopPropagation();
    const point = displayPoint(event);
    if (!point) return;
    inkRef.current = [...inkRef.current, [point.x, point.y]];
    setInkPoints(inkRef.current);
  };

  const handleInkUp = (event: React.PointerEvent<HTMLDivElement>) => {
    event.stopPropagation();
    const stroke = inkRef.current;
    inkRef.current = null;
    setInkPoints(null);
    if (stroke) addInkStroke(stroke);
  };

  const handleBoxDown = (event: React.PointerEvent<HTMLDivElement>) => {
    event.stopPropagation();
    const point = displayPoint(event);
    if (!point) return;
    event.currentTarget.setPointerCapture?.(event.pointerId);
    dragRectRef.current = { x0: point.x, y0: point.y, x1: point.x, y1: point.y };
    setDragRect(dragRectRef.current);
  };

  const handleBoxMove = (event: React.PointerEvent<HTMLDivElement>) => {
    const drag = dragRectRef.current;
    if (!drag) return;
    event.stopPropagation();
    const point = displayPoint(event);
    if (!point) return;
    dragRectRef.current = { ...drag, x1: point.x, y1: point.y };
    setDragRect(dragRectRef.current);
  };

  const handleBoxUp = (event: React.PointerEvent<HTMLDivElement>) => {
    event.stopPropagation();
    const drag = dragRectRef.current;
    dragRectRef.current = null;
    setDragRect(null);
    if (!drag || !geometry) return;
    const x = Math.min(drag.x0, drag.x1);
    const y = Math.min(drag.y0, drag.y1);
    const w = Math.abs(drag.x1 - drag.x0);
    const h = Math.abs(drag.y1 - drag.y0);
    const created =
      kind === "line"
        ? Math.hypot(w, h) < 6
          ? makeAnnotation("line", drag.x0, drag.y0)
          : makeAnnotation("line", x, y, { w, h, x2: drag.x1, y2: drag.y1 })
        : w < 6 || h < 6
          ? makeAnnotation("rect", drag.x0, drag.y0)
          : makeAnnotation("rect", x, y, { w, h });
    if (created) setAnnotations((previous) => [...previous, created]);
  };

  const pickImage = async () => {
    if (isAndroid()) {
      const paths = await pickAndroidFiles({ multiple: false, accept: "image" }).catch(() => []);
      if (paths.length) setImagePath(paths[0]);
      return;
    }
    const picked = await open({ multiple: false, filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg"] }] });
    if (picked) setImagePath(String(picked));
  };

  const run = () =>
    session.run(async (jobId, overwrite) => {
      const input = session.primary?.path ?? "";
      const spec = session.outputSpec(overwrite);
      const password = session.password || undefined;
      if (pendingEdits.length && !annotations.length) {
        const report = await pdfEditAnnotations(input, spec, pendingEdits, password);
        return { path: spec.path, message: report.warnings?.[0] ?? t("annotate.updated") };
      }
      let source = input;
      if (pendingEdits.length) {
        const staged = await stageEditedCopy(input);
        await pdfEditAnnotations(input, { path: staged, overwrite: "replace" }, pendingEdits, password);
        source = staged;
      }
      if (!annotations.length) return undefined;
      return editable
        ? pdfAnnotateEditable(source, spec, annotations, jobId, password, keepSignatures)
        : annotatePdf(source, spec, annotations, jobId, password, keepSignatures);
    });

  const newOnPage = annotations.filter((annotation) => annotation.page === page);
  const existingOnPage = existing.filter((annotation) => annotation.page === page);

  return (
    <>
      <TwoColumn
        main={
          !session.primary ? (
            <DropZone onPaths={(paths) => void session.addPaths(paths)} dragging={dragging} accept="pdf" />
          ) : (
            <>
              <Card className="p-4 flex items-center gap-3">
                <InfoStrip info={session.info} error={session.infoError} />
                <div className="ml-auto">
                  <Pager page={page} pageCount={pageCount || 1} onChange={setPage} />
                </div>
              </Card>
              <Card className="p-4">
                <p className="text-xs muted mb-2">{t("annotate.placeHint")}</p>
                <div className="mx-auto" style={{ maxWidth: 560 }} ref={canvasRef}>
                  <PageCanvas
                    path={session.primary.path}
                    page={page}
                    password={session.password || undefined}
                    maxWidth={1100}
                    dragging={!isDragTool}
                    onPointClick={addAtPoint}
                    overlay={
                      geometry ? (
                        <>
                          {newOnPage.map((annotation, index) => {
                            if (!BOX_KINDS.includes(annotation.kind)) return null;
                            const style = overlayStyle(
                              annotation.x,
                              annotation.y,
                              annotation.w,
                              annotation.h,
                              displayW,
                              displayH,
                            );
                            if (annotation.kind === "rect") {
                              return (
                                <div
                                  key={`new-${index}`}
                                  className="annotate-box"
                                  style={{
                                    ...style,
                                    border: `${Math.max(1, annotation.line_width_pt)}px solid ${annotation.color}`,
                                  }}
                                />
                              );
                            }
                            if (annotation.kind === "highlight") {
                              return (
                                <div
                                  key={`new-${index}`}
                                  className="annotate-box"
                                  style={{ ...style, background: annotation.color, opacity: annotation.opacity }}
                                />
                              );
                            }
                            if (annotation.kind === "underline" || annotation.kind === "strike") {
                              return (
                                <div key={`new-${index}`} className="annotate-box" style={style}>
                                  <div
                                    className={
                                      annotation.kind === "underline" ? "annotate-underline" : "annotate-strike"
                                    }
                                    style={{ background: annotation.color }}
                                  />
                                </div>
                              );
                            }
                            if (annotation.kind === "signature" && annotation.image_base64) {
                              return (
                                <img
                                  key={`new-${index}`}
                                  className="annotate-box"
                                  src={dataUrlForBase64(annotation.image_base64)}
                                  alt={t("annotate.signatureTool")}
                                  draggable={false}
                                />
                              );
                            }
                            return (
                              <div
                                key={`new-${index}`}
                                className={`annotate-box${annotation.kind === "note" ? " annotate-note" : ""}`}
                                style={{
                                  ...style,
                                  color: annotation.color,
                                  fontSize: `${Math.max(8, annotation.font_size_pt * 0.72)}px`,
                                  fontWeight: annotation.bold ? 700 : 400,
                                  borderColor: annotation.kind === "note" ? annotation.color : undefined,
                                }}
                              >
                                {annotation.kind === "image" ? "▣" : annotation.text}
                              </div>
                            );
                          })}
                          {existingOnPage.map((annotation) => {
                            if (!BOX_KINDS.includes(annotation.kind)) return null;
                            const key = annotationKey(annotation);
                            const active = selected === key;
                            const offset = movePreview?.key === key ? movePreview : null;
                            const style = overlayStyle(
                              annotation.x + (offset?.dx ?? 0),
                              annotation.y + (offset?.dy ?? 0),
                              annotation.w,
                              annotation.h,
                              displayW,
                              displayH,
                            );
                            const className = `annotate-box${active ? " annotate-box-selected" : ""}${
                              annotation.kind === "note" ? " annotate-note" : ""
                            }`;
                            const handlers = active
                              ? {
                                  onPointerDown: (event: React.PointerEvent<Element>) => startMove(event, annotation),
                                  onPointerMove: handleMoveDrag,
                                  onPointerUp: endMoveDrag,
                                  onPointerCancel: endMoveDrag,
                                }
                              : {};
                            if (annotation.kind === "rect") {
                              return (
                                <div
                                  key={`existing-${key}`}
                                  className={className}
                                  style={{
                                    ...style,
                                    border: `${Math.max(1, annotation.lineWidthPt)}px solid ${annotation.color}`,
                                  }}
                                  {...handlers}
                                />
                              );
                            }
                            if (annotation.kind === "highlight") {
                              return (
                                <div
                                  key={`existing-${key}`}
                                  className={className}
                                  style={{ ...style, background: annotation.color, opacity: annotation.opacity }}
                                  {...handlers}
                                />
                              );
                            }
                            if (annotation.kind === "underline" || annotation.kind === "strike") {
                              return (
                                <div key={`existing-${key}`} className={className} style={style} {...handlers}>
                                  <div
                                    className={
                                      annotation.kind === "underline" ? "annotate-underline" : "annotate-strike"
                                    }
                                    style={{ background: annotation.color }}
                                  />
                                </div>
                              );
                            }
                            return (
                              <div
                                key={`existing-${key}`}
                                className={className}
                                style={{
                                  ...style,
                                  color: annotation.color,
                                  fontSize: `${Math.max(8, annotation.fontSizePt * 0.72)}px`,
                                  fontWeight: annotation.bold ? 700 : 400,
                                  borderColor: annotation.kind === "note" ? annotation.color : undefined,
                                }}
                                {...handlers}
                              >
                                {annotation.kind === "image" || annotation.kind === "signature" ? "▣" : annotation.text}
                              </div>
                            );
                          })}
                          <svg
                            className="annotate-strokes"
                            viewBox={`0 0 ${displayW} ${displayH}`}
                            preserveAspectRatio="none"
                            aria-hidden
                          >
                            {newOnPage.map((annotation, index) => {
                              if (annotation.kind === "line") {
                                return (
                                  <line
                                    key={`new-line-${index}`}
                                    x1={annotation.x}
                                    y1={annotation.y}
                                    x2={annotation.x2 ?? annotation.x + annotation.w}
                                    y2={annotation.y2 ?? annotation.y + annotation.h}
                                    stroke={annotation.color}
                                    strokeWidth={annotation.line_width_pt}
                                  />
                                );
                              }
                              if (annotation.kind === "ink") {
                                return strokesToPaths(annotation.strokes).map((path, strokeIndex) => (
                                  <path
                                    key={`new-ink-${index}-${strokeIndex}`}
                                    d={path}
                                    fill="none"
                                    stroke={annotation.color}
                                    strokeWidth={annotation.line_width_pt}
                                    strokeLinecap="round"
                                    strokeLinejoin="round"
                                  />
                                ));
                              }
                              return null;
                            })}
                            {existingOnPage.map((annotation) => {
                              const key = annotationKey(annotation);
                              const active = selected === key;
                              const offset = movePreview?.key === key ? movePreview : null;
                              const shift = offset ? `translate(${offset.dx} ${offset.dy})` : undefined;
                              const handlers = active
                                ? {
                                    onPointerDown: (event: React.PointerEvent<Element>) => startMove(event, annotation),
                                    onPointerMove: handleMoveDrag,
                                    onPointerUp: endMoveDrag,
                                    onPointerCancel: endMoveDrag,
                                  }
                                : {};
                              if (annotation.kind === "line") {
                                // The list contract carries only the box; the
                                // line runs from its top-left to its bottom-right.
                                const x2 = annotation.x + annotation.w;
                                const y2 = annotation.y + annotation.h;
                                return (
                                  <g key={`existing-line-${key}`} transform={shift}>
                                    <line
                                      x1={annotation.x}
                                      y1={annotation.y}
                                      x2={x2}
                                      y2={y2}
                                      stroke={annotation.color}
                                      strokeWidth={annotation.lineWidthPt}
                                    />
                                    {active ? (
                                      <line
                                        x1={annotation.x}
                                        y1={annotation.y}
                                        x2={x2}
                                        y2={y2}
                                        stroke="transparent"
                                        strokeWidth={14}
                                        className="annotate-hit"
                                        {...handlers}
                                      />
                                    ) : null}
                                  </g>
                                );
                              }
                              if (annotation.kind === "ink") {
                                return strokesToPaths(annotation.strokes).map((path, strokeIndex) => (
                                  <g key={`existing-ink-${key}-${strokeIndex}`} transform={shift}>
                                    <path
                                      d={path}
                                      fill="none"
                                      stroke={annotation.color}
                                      strokeWidth={annotation.lineWidthPt}
                                      strokeLinecap="round"
                                      strokeLinejoin="round"
                                    />
                                    {active ? (
                                      <path
                                        d={path}
                                        fill="none"
                                        stroke="transparent"
                                        strokeWidth={14}
                                        className="annotate-hit"
                                        {...handlers}
                                      />
                                    ) : null}
                                  </g>
                                ));
                              }
                              return null;
                            })}
                          </svg>
                          {inkPoints ? (
                            <svg
                              className="annotate-strokes"
                              viewBox={`0 0 ${displayW} ${displayH}`}
                              preserveAspectRatio="none"
                              aria-hidden
                            >
                              <path
                                d={strokesToPaths([inkPoints])[0] ?? ""}
                                fill="none"
                                stroke={color}
                                strokeWidth={lineWidth}
                                strokeLinecap="round"
                                strokeLinejoin="round"
                              />
                            </svg>
                          ) : null}
                          {dragRect ? (
                            <div
                              className="annotate-drag-preview"
                              style={{
                                ...overlayStyle(
                                  Math.min(dragRect.x0, dragRect.x1),
                                  Math.min(dragRect.y0, dragRect.y1),
                                  Math.abs(dragRect.x1 - dragRect.x0),
                                  Math.abs(dragRect.y1 - dragRect.y0),
                                  displayW,
                                  displayH,
                                ),
                                borderColor: color,
                              }}
                            />
                          ) : null}
                          {isDragTool ? (
                            <div
                              className="annotate-draw-layer"
                              style={{ touchAction: "none" }}
                              onPointerDown={kind === "ink" ? handleInkDown : handleBoxDown}
                              onPointerMove={kind === "ink" ? handleInkMove : handleBoxMove}
                              onPointerUp={kind === "ink" ? handleInkUp : handleBoxUp}
                              onPointerCancel={kind === "ink" ? handleInkUp : handleBoxUp}
                            />
                          ) : null}
                        </>
                      ) : null
                    }
                  />
                </div>
              </Card>
            </>
          )
        }
        side={
          <>
            <OutputBar
              session={session}
              runLabel={editable ? t("annotate.applyEditable") : t("annotate.apply")}
              onRun={() => void run()}
              disabled={!session.primary || (!annotations.length && !pendingEdits.length)}
            />
            <OptionCard title={t("annotate.applyEditable")}>
              <Segmented
                value={editable ? "editable" : "flatten"}
                onChange={(value) => setEditable(value === "editable")}
                options={[
                  { value: "editable", label: t("annotate.editable") },
                  { value: "flatten", label: t("annotate.flatten") },
                ]}
              />
              <p className="muted small">{editable ? t("annotate.saveHint") : t("annotate.editableHint")}</p>
              <Toggle checked={keepSignatures} onChange={setKeepSignatures} label={t("metadata.keepSignatures")} />
              <p className="muted small">{t("annotate.keepSignaturesHint")}</p>
            </OptionCard>
            <OptionCard title={t("annotate.tools")}>
              <div className="flex flex-wrap gap-1.5">
                {TOOLS.map((entry) => (
                  <Button
                    key={entry.value}
                    size="sm"
                    variant={kind === entry.value ? "primary" : "default"}
                    icon={entry.icon}
                    onClick={() => setKind(entry.value)}
                  >
                    {t(entry.labelKey)}
                  </Button>
                ))}
              </div>
              {kind === "textbox" || kind === "note" ? (
                <>
                  <Field label={t("annotate.textPlaceholder")}>
                    <textarea
                      className="textarea"
                      rows={2}
                      value={text}
                      onChange={(event) => setText(event.target.value)}
                    />
                  </Field>
                  <Field label={t("annotate.fontSize")}>
                    <Slider value={fontSize} min={8} max={48} onChange={setFontSize} />
                  </Field>
                  <Toggle checked={bold} onChange={setBold} label={t("annotate.bold")} />
                </>
              ) : null}
              {kind === "image" ? (
                <Button size="sm" variant="ghost" icon={<ImageIcon size={14} />} onClick={() => void pickImage()}>
                  {imagePath ? imagePath.split(/[\\/]/).pop() : t("annotate.imageFile")}
                </Button>
              ) : null}
              {kind === "signature" ? (
                <>
                  <p className="muted small">{t("annotate.signatureHint")}</p>
                  <Button size="sm" variant="ghost" icon={<SignatureIcon size={14} />} onClick={() => setPadOpen(true)}>
                    {t("annotate.signaturePad")}
                  </Button>
                  {signatureBase64 ? (
                    <img
                      className="annotate-signature-preview"
                      src={dataUrlForBase64(signatureBase64)}
                      alt={t("annotate.signatureTool")}
                    />
                  ) : null}
                </>
              ) : null}
              {kind === "rect" || kind === "line" || kind === "ink" ? (
                <Field label={t("annotate.lineWidth")}>
                  <Slider value={lineWidth} min={1} max={10} onChange={setLineWidth} />
                </Field>
              ) : null}
              <Field label={t("common.color")}>
                <ColorInput value={color} onChange={setColor} />
              </Field>
              {kind === "highlight" ? (
                <Field label="Opacity">
                  <Slider
                    value={opacity * 100}
                    min={10}
                    max={90}
                    onChange={(value) => setOpacity(value / 100)}
                    format={(value) => `${value}%`}
                  />
                </Field>
              ) : null}
            </OptionCard>

            <OptionCard
              title={`${t("annotate.existing")}${existing.length ? ` · ${existing.length}` : ""}`}
              action={
                <Button size="sm" variant="ghost" onClick={() => void reloadExisting()} disabled={!session.primary}>
                  {t("annotate.loadExisting")}
                </Button>
              }
            >
              {existingError ? (
                <p className="text-xs" style={{ color: "var(--danger)" }}>
                  {existingError}
                </p>
              ) : null}
              {existingLoaded && !existing.length && !existingError ? (
                <p className="text-xs muted">{t("annotate.existingNone")}</p>
              ) : null}
              {existing.length ? (
                <div className="flex flex-col gap-1.5 max-h-[220px] overflow-y-auto">
                  {existing.map((annotation) => {
                    const key = annotationKey(annotation);
                    return (
                      <button
                        key={key}
                        type="button"
                        className={`annotate-existing-item${selected === key ? " annotate-existing-item-active" : ""}`}
                        onClick={() => selectExisting(annotation)}
                      >
                        <Badge tone="accent">{annotation.kind}</Badge>
                        <span className="truncate flex-1 text-left">
                          {t("common.page")} {annotation.page}
                          {annotation.text ? ` · ${annotation.text.slice(0, 24)}` : ""}
                        </span>
                      </button>
                    );
                  })}
                </div>
              ) : null}
              {selectedAnnotation ? (
                <div className="flex flex-col gap-2.5 border-t pt-2.5" style={{ borderColor: "var(--border)" }}>
                  <p className="text-[12px] font-semibold">{t("annotate.selected")}</p>
                  <Field label={t("annotate.editText")}>
                    <textarea
                      className="textarea"
                      rows={2}
                      value={selectedAnnotation.text}
                      onChange={(event) => queueUpdate({ text: event.target.value })}
                    />
                  </Field>
                  <Field label={t("annotate.editColor")}>
                    <ColorInput
                      value={normalizeColor(selectedAnnotation.color)}
                      onChange={(value) => queueUpdate({ color: value })}
                    />
                  </Field>
                  <Field label="Opacity">
                    <Slider
                      value={Math.round(selectedAnnotation.opacity * 100)}
                      min={5}
                      max={100}
                      onChange={(value) => queueUpdate({ opacity: value / 100 })}
                      format={(value) => `${value}%`}
                    />
                  </Field>
                  <Field label={t("common.size")}>
                    <div className="flex items-center gap-2">
                      <NumberInput value={draftW} onChange={setDraftW} min={2} suffix="pt" />
                      <NumberInput value={draftH} onChange={setDraftH} min={2} suffix="pt" />
                      <Button size="sm" variant="ghost" onClick={applyResize}>
                        {t("annotate.resize")}
                      </Button>
                    </div>
                  </Field>
                  <p className="muted small">{t("annotate.moveHint")}</p>
                  <div className="flex flex-wrap gap-2">
                    <Button size="sm" variant="danger" icon={<Trash2 size={13} />} onClick={deleteSelected}>
                      {t("annotate.delete")}
                    </Button>
                    <Button size="sm" variant="ghost" onClick={() => setSelected(null)}>
                      {t("annotate.clearSelection")}
                    </Button>
                  </div>
                </div>
              ) : null}
            </OptionCard>

            <OptionCard title={`${t("annotate.list")} · ${annotations.length}`}>
              {annotations.length === 0 ? (
                <p className="text-xs muted">{t("annotate.none")}</p>
              ) : (
                <div className="flex flex-col gap-1.5 max-h-[220px] overflow-y-auto">
                  {annotations.map((annotation, index) => (
                    <div
                      key={`${annotation.page}-${annotation.kind}-${index}`}
                      className="card-soft flex items-center gap-2 px-2.5 py-1.5 text-[12.5px]"
                    >
                      <Badge tone="accent">{annotation.kind}</Badge>
                      <span className="truncate flex-1">
                        {t("common.page")} {annotation.page}
                        {annotation.text ? ` · ${annotation.text.slice(0, 24)}` : ""}
                      </span>
                      <Button
                        size="sm"
                        variant="ghost"
                        icon={<Trash2 size={12} />}
                        aria-label={t("annotate.delete")}
                        title={t("annotate.delete")}
                        onClick={() => setAnnotations((previous) => previous.filter((_, i) => i !== index))}
                      />
                    </div>
                  ))}
                </div>
              )}
            </OptionCard>

            {session.result ? <ResultCard result={session.result} onReset={session.resetResult} /> : null}
          </>
        }
      />
      {padOpen ? (
        <SignaturePad
          onUse={(base64) => {
            setSignatureBase64(base64);
            setPadOpen(false);
          }}
          onClose={() => setPadOpen(false)}
        />
      ) : null}
    </>
  );
}
