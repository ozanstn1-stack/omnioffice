/**
 * Impress editor: slide thumbnails, drag/resize/rotate canvas, properties
 * panel, notes, layouts, themes and transitions, plus a real slideshow.
 *
 * V3 adds masters/layouts (inherited placeholders), real shape groups with
 * nested children, schematic chart previews, per-object animations and a
 * presenter view. The pure helpers below (grouping, animation ordering) are
 * exported so they can be unit tested without rendering the editor.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  AlignCenterHorizontal,
  AlignCenterVertical,
  AlignEndHorizontal,
  AlignEndVertical,
  AlignStartHorizontal,
  AlignStartVertical,
  ArrowRight,
  Bold,
  Braces,
  Circle,
  CircleDot,
  Copy,
  Crop,
  Eraser,
  FileDown,
  FolderOpen,
  Grid2X2,
  Group,
  Image as ImageIcon,
  IndentDecrease,
  IndentIncrease,
  Italic,
  LayoutTemplate,
  LineChart,
  Link2,
  List,
  Minus,
  MonitorPlay,
  Move,
  PenLine,
  Play,
  Plus,
  Pointer,
  Printer,
  Redo2,
  Rows3,
  Save,
  Sparkles,
  Square,
  Table as TableIcon,
  Trash2,
  Type,
  Underline,
  Undo2,
  Ungroup,
} from "lucide-react";
import type { OfficeTab } from "../lib/office-store";
import { useOfficeTabs } from "../lib/office-store";
import type {
  Animation,
  ChartData,
  Deck,
  ImageCrop,
  LineSpec,
  Slide,
  SlideObject,
  TextParagraph,
} from "../lib/office-types";
import { newAnimation, newSlideMaster, newSlideObject, uid } from "../lib/office-types";
import {
  animationObjectStyle,
  animationTimeline,
  cloneObject,
  cloneObjects,
  formatClock,
  groupSelection,
  inheritedObjects,
  isInheritedId,
  objectAtPath,
  objectBounds,
  objectsInRect,
  pathKey,
  refreshGroupBounds,
  replaceObjectAtPath,
  scaleObject,
  translateObject,
  removeSelectionPaths,
  ungroupSelection,
  type AnimationStep,
  type SelectionPath,
} from "./impress/selection";
export {
  animationEffectStyle,
  animationObjectStyle,
  animationTimeline,
  cloneObjects,
  formatClock,
  groupSelection,
  inheritedObjects,
  isInheritedId,
  objectAtPath,
  objectBounds,
  objectsInRect,
  pathKey,
  refreshGroupBounds,
  removeSelectionPaths,
  replaceObjectAtPath,
  scaleObject,
  translateObject,
  ungroupSelection,
  type AnimationStep,
  type SelectionPath,
} from "./impress/selection";
import { useT } from "../lib/i18n";
import { mimeForName, pickFileBytes } from "../lib/mobile";
import { reportError, useToasts } from "../lib/store";
import type { AiOutlineSlide } from "../lib/types";
import { useAiStatus } from "./ai/editor-ai";
import { alignBox, distributePositions } from "./impress/arrange";
import { connectorPatch, nearestSite, recomputeConnectors, remapConnectorTargets } from "./impress/connectors";
import { clampCrop, isFullCrop } from "./impress/crop-preview";
import { AnimationDialog, ChartDialog, FooterDialog, MasterDialog } from "./impress/dialogs";
import { ImpressAiDialog } from "./impress/ImpressAiDialog";
import { INHERITED_KEYS, type Theme } from "./impress/shared";
import { simplifyStroke, strokeHit, strokePath, type StrokePoint } from "./impress/show-drawing";
import {
  CropHandles,
  ObjectTree,
  SelectionHandles,
  SlideFooterBar,
  SlidePreview,
  defaultShapeStyle,
} from "./impress/slide-view";
import { withFrameAlign, withFrameSize } from "./impress/textFrame";
import type { TextEditHandle, TextSelectionInfo } from "./impress/TextFrameEditor";
import { Ribbon, RibbonGroup, ToolButton, ToolColor, ToolNumber, ToolSelect } from "./office-ui";
import { openIntoWorkspace, useEditorShortcuts, useOfficeSession } from "./useOfficeSession";
import { documentLocationLabel } from "./location-label";

type ImpressTab = OfficeTab & { model: Deck };
/** Path of object ids from the slide root; deeper paths are children of groups. */

export interface RunningAnimation {
  effect: Animation;
  phase: "to" | "back";
}

export type AnimationRunState = Record<string, RunningAnimation>;
export type AnimationDoneState = Record<string, boolean>;

export { ANIMATION_EFFECTS } from "./impress/shared";

/** Quick text colours offered by the frame editing toolbar. */
const TEXT_COLORS = ["#111827", "#DC2626", "#EA580C", "#16A34A", "#2563EB", "#7C3AED", "#DB2777", "#64748B"];

const THEMES: Theme[] = [
  {
    id: "minimal",
    name: "Minimal",
    background: "#FFFFFF",
    title: "Segoe UI",
    body: "Segoe UI",
    accent: "#2563EB",
    titleColor: "#111827",
    bodyColor: "#334155",
  },
  {
    id: "business",
    name: "Business",
    background: "#F8FAFC",
    title: "Segoe UI",
    body: "Segoe UI",
    accent: "#1D4ED8",
    titleColor: "#0F172A",
    bodyColor: "#334155",
  },
  {
    id: "dark",
    name: "Dark",
    background: "#0F172A",
    title: "Segoe UI",
    body: "Segoe UI",
    accent: "#60A5FA",
    titleColor: "#F8FAFC",
    bodyColor: "#CBD5E1",
  },
  {
    id: "modern",
    name: "Modern",
    background: "#FFFFFF",
    title: "Segoe UI",
    body: "Segoe UI",
    accent: "#14B8A6",
    titleColor: "#0B1220",
    bodyColor: "#334155",
  },
  {
    id: "education",
    name: "Education",
    background: "#FFFDF5",
    title: "Georgia",
    body: "Georgia",
    accent: "#B45309",
    titleColor: "#1E293B",
    bodyColor: "#44403C",
  },
  {
    id: "simple",
    name: "Simple",
    background: "#FFFFFF",
    title: "Arial",
    body: "Arial",
    accent: "#444444",
    titleColor: "#111111",
    bodyColor: "#444444",
  },
];

const LAYOUTS: Array<{ id: string; name: string; build: (deck: Deck) => SlideObject[] }> = [
  {
    id: "title",
    name: "Title",
    build: () => [textObject("Click to add a title", 120, 200, 720, 120, 40, "center")],
  },
  {
    id: "titleContent",
    name: "Title + content",
    build: () => [
      textObject("Click to add a title", 80, 60, 800, 80, 32, "left"),
      bulletObject("Click to add text", 80, 180, 800, 280),
    ],
  },
  {
    id: "twoColumns",
    name: "Two columns",
    build: () => [
      textObject("Title", 80, 50, 800, 70, 30, "left"),
      bulletObject("Left column", 80, 160, 380, 300),
      bulletObject("Right column", 500, 160, 380, 300),
    ],
  },
  {
    id: "imageText",
    name: "Image + text",
    build: () => [
      textObject("Title", 80, 50, 800, 70, 30, "left"),
      { ...textObject("Add an image", 80, 160, 400, 300, 16, "center") },
      bulletObject("Describe the image", 510, 170, 370, 280),
    ],
  },
  {
    id: "section",
    name: "Section header",
    build: () => [textObject("Section title", 80, 220, 800, 100, 36, "center")],
  },
  { id: "blank", name: "Blank", build: () => [] },
  {
    id: "quote",
    name: "Quote",
    build: () => [textObject("“An important quote goes here.”", 140, 180, 680, 180, 28, "center")],
  },
  {
    id: "comparison",
    name: "Comparison",
    build: () => [
      textObject("Option A", 80, 80, 380, 60, 24, "left"),
      textObject("Option B", 500, 80, 380, 60, 24, "left"),
      bulletObject("Details", 80, 170, 380, 280),
      bulletObject("Details", 500, 170, 380, 280),
    ],
  },
];

function textObject(
  text: string,
  x: number,
  y: number,
  w: number,
  h: number,
  size: number,
  align: string,
): SlideObject {
  return {
    id: uid(),
    kind: "text",
    x,
    y,
    w,
    h,
    rotation: 0,
    z: 1,
    text: {
      paragraphs: [
        {
          text,
          level: 0,
          bold: size >= 30,
          italic: false,
          underline: false,
          sizePt: size,
          color: null,
          align,
          bullet: false,
          runs: [],
        },
      ],
      valign: "top",
      font: null,
      sizePt: size,
      color: null,
      align,
    },
    image: null,
    style: null,
    line: null,
    table: null,
    chart: null,
    groupId: null,
    name: "Text",
  };
}

/**
 * Title + bullet slides for a validated AI outline, built with the same
 * "Title + content" layout as New slide. A model reply never becomes markup:
 * titles and bullets are plain paragraph text.
 */
export function outlineSlides(
  deck: Deck,
  outline: AiOutlineSlide[],
  inherit: { masterId?: string | null; layoutId?: string | null } = {},
): Slide[] {
  const layout = LAYOUTS.find((candidate) => candidate.id === "titleContent")!;
  return outline.map((entry) => {
    const objects = layout.build(deck).map((object, index) => ({ ...object, z: index + 1 }));
    const [title, body] = objects;
    title.text!.paragraphs[0].text = entry.title;
    const bullet = body.text!.paragraphs[0];
    body.text!.paragraphs = (entry.bullets.length > 0 ? entry.bullets : [""]).map((text) => ({ ...bullet, text }));
    return {
      id: uid(),
      layout: "titleContent",
      masterId: inherit.masterId ?? null,
      layoutId: inherit.layoutId ?? null,
      background: null,
      transition: null,
      transitionMs: 500,
      objects,
      animations: [],
      notes: "",
    };
  });
}

function bulletObject(text: string, x: number, y: number, w: number, h: number): SlideObject {
  const object = textObject(text, x, y, w, h, 20, "left");
  object.text!.paragraphs[0].bullet = true;
  object.name = "Bullets";
  return object;
}

function emptyChart(): ChartData {
  return {
    kind: "column",
    title: "Chart",
    categories: "",
    series: [],
    legend: true,
    xTitle: "",
    yTitle: "",
    stacked: false,
    showLabels: false,
    categoriesCache: [],
    seriesValuesCache: [],
  };
}

// ---------------------------------------------------------------------------
// Chart data helpers
//
// Moved verbatim to ./impress/shared (B4 editor seam 3) so the chart dialog
// and the chart preview can share them without importing this module back;
// re-exported here so this module's public API is unchanged.
// ---------------------------------------------------------------------------

export {
  chartDataRowCount,
  chartHasCachedData,
  chartWorkbookRange,
  formatChartCellValue,
  mergeChartPaste,
  normalizeChartDraft,
  parseChartCellNumber,
  parseChartClipboard,
  syncChartDataRanges,
  type ChartGridCell,
} from "./impress/shared";

// ---------------------------------------------------------------------------
// Pure helpers: paths, groups, inheritance, animation ordering
// ---------------------------------------------------------------------------

export function ImpressEditor({ tab }: { tab: ImpressTab }) {
  const t = useT();
  const deck = tab.model;
  const edit = useOfficeTabs((state) => state.edit);
  const session = useOfficeSession(tab);
  const [ribbon, setRibbon] = useState("home");
  const [slideIndex, setSlideIndex] = useState(0);
  const [selected, setSelected] = useState<SelectionPath[]>([]);
  const [editingText, setEditingText] = useState<string | null>(null);
  const [slideshow, setSlideshow] = useState<number | null>(null);
  const [presenterView, setPresenterView] = useState(false);
  const [elapsedMs, setElapsedMs] = useState(0);
  const [stepIndex, setStepIndex] = useState(0);
  const [animRunning, setAnimRunning] = useState<AnimationRunState>({});
  const [animDone, setAnimDone] = useState<AnimationDoneState>({});
  const [masterDialog, setMasterDialog] = useState(false);
  const [chartPath, setChartPath] = useState<SelectionPath | null>(null);
  const [animationEditing, setAnimationEditing] = useState<Animation | null>(null);
  const aiStatus = useAiStatus();
  const [outlineDialog, setOutlineDialog] = useState(false);
  const [footerDialog, setFooterDialog] = useState(false);
  const [view, setView] = useState<"normal" | "sorter">("normal");
  const [connectorMode, setConnectorMode] = useState(false);
  const [connectorStart, setConnectorStart] = useState<string | null>(null);
  const [cropPath, setCropPath] = useState<SelectionPath | null>(null);
  const [cropDraft, setCropDraft] = useState<ImageCrop | null>(null);
  const [undoStack, setUndoStack] = useState<Deck[]>([]);
  const [redoStack, setRedoStack] = useState<Deck[]>([]);
  const canvasRef = useRef<HTMLDivElement>(null);
  const [canvasWidth, setCanvasWidth] = useState(0);
  const textHandleRef = useRef<TextEditHandle | null>(null);
  const [textSelection, setTextSelection] = useState<TextSelectionInfo | null>(null);
  const clipboardRef = useRef<SlideObject[]>([]);
  const sorterDragRef = useRef<number | null>(null);
  const dropIndexRef = useRef<number | null>(null);
  const [dropIndex, setDropIndex] = useState<number | null>(null);
  const [showTool, setShowTool] = useState<"pointer" | "laser" | "pen" | "eraser">("pointer");
  const [strokes, setStrokes] = useState<StrokePoint[][]>([]);
  const [draftStroke, setDraftStroke] = useState<StrokePoint[] | null>(null);
  const [laser, setLaser] = useState<StrokePoint | null>(null);
  const strokeRef = useRef<StrokePoint[] | null>(null);
  const laserRef = useRef<StrokePoint | null>(null);
  const slideAreaRef = useRef<HTMLDivElement>(null);
  const dragState = useRef<{
    path: SelectionPath;
    mode: "move" | "resize" | "rotate";
    startX: number;
    startY: number;
    object: SlideObject;
  } | null>(null);
  const lastTapRef = useRef<{ key: string; time: number } | null>(null);
  const [marquee, setMarquee] = useState<{ x: number; y: number; w: number; h: number } | null>(null);
  const showTimersRef = useRef<number[]>([]);
  const showStartRef = useRef<number | null>(null);
  const showEndRef = useRef<number | null>(null);
  const stepIndexRef = useRef(0);
  const advanceRef = useRef<() => void>(() => undefined);

  // Resets the slideshow state exactly on the boundaries (entering/leaving the
  // show), derived during render so no effect has to call setState.
  const showActive = slideshow !== null;
  const [lastShowActive, setLastShowActive] = useState(showActive);
  if (lastShowActive !== showActive) {
    setLastShowActive(showActive);
    setStepIndex(0);
    setElapsedMs(0);
    setAnimRunning({});
    setAnimDone({});
    if (!showActive) {
      setPresenterView(false);
      // Ink and the laser are session-only; each show starts clean.
      setStrokes([]);
      setDraftStroke(null);
      setLaser(null);
      setShowTool("pointer");
    }
  }

  const slide = deck.slides[Math.min(slideIndex, deck.slides.length - 1)] ?? deck.slides[0];

  useEditorShortcuts(session);
  const theme = useMemo(() => THEMES.find((candidate) => candidate.id === deck.theme) ?? THEMES[0], [deck.theme]);
  const masters = useMemo(() => deck.masters ?? [], [deck.masters]);
  const selectedMaster = useMemo(
    () => masters.find((candidate) => candidate.id === slide.masterId) ?? masters[0] ?? null,
    [masters, slide.masterId],
  );
  const inherited = useMemo(() => inheritedObjects(deck, slide), [deck, slide]);
  useEffect(() => {
    const element = canvasRef.current;
    if (!element) return;
    const observer = new ResizeObserver(() => setCanvasWidth(element.clientWidth));
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  const scale = useMemo(
    () => Math.min(1.4, Math.max(0.2, (canvasWidth - 48) / deck.size.widthPt)),
    [canvasWidth, deck.size.widthPt],
  );

  const update = useCallback(
    (mutate: (deck: Deck) => Deck, recordUndo = true) => {
      if (recordUndo) {
        setUndoStack((stack) => [...stack.slice(-40), deck]);
        setRedoStack([]);
      }
      edit(tab.id, (model) => mutate(model as Deck));
    },
    [deck, edit, tab.id],
  );

  const updateSlide = useCallback(
    (mutate: (slide: Slide) => Slide, recordUndo = true) =>
      update(
        (current) => ({
          ...current,
          // Connectors follow the shapes they are glued to on every change.
          slides: current.slides.map((candidate, index) => {
            if (index !== slideIndex) return candidate;
            const next = mutate(candidate);
            return { ...next, objects: recomputeConnectors(next.objects) };
          }),
        }),
        recordUndo,
      ),
    [slideIndex, update],
  );

  const updatePath = useCallback(
    (path: SelectionPath, patch: Partial<SlideObject>, recordUndo = true) =>
      updateSlide((current) => {
        const next = replaceObjectAtPath(current.objects, path, patch);
        return { ...current, objects: path.length > 1 ? refreshGroupBounds(next) : next };
      }, recordUndo),
    [updateSlide],
  );

  // -------------------------------------------------------------------------
  // Selection
  // -------------------------------------------------------------------------

  const selectedKeys = useMemo(() => new Set(selected.map((path) => pathKey(path))), [selected]);
  const selectedObjects = useMemo(
    () =>
      selected
        .map((path) => objectAtPath(slide.objects, path))
        .filter((object): object is SlideObject => Boolean(object)),
    [selected, slide.objects],
  );
  const primary = selectedObjects[0];
  const primaryPath = selected[0];
  const hasGroupSelection = selected.some(
    (path) => path.length === 1 && objectAtPath(slide.objects, path)?.kind === "group",
  );

  const selectFromPointer = (event: React.PointerEvent, path: SelectionPath) => {
    if (isInheritedId(path[0])) return;
    const target: SelectionPath = event.altKey ? path : [path[0]];
    const key = pathKey(target);
    if (event.shiftKey) {
      setSelected((current) =>
        current.some((item) => pathKey(item) === key)
          ? current.filter((item) => pathKey(item) !== key)
          : [...current, target],
      );
    } else {
      setSelected([target]);
    }
  };

  // -------------------------------------------------------------------------
  // Pointer interaction (mouse, pen and touch share one code path)
  // -------------------------------------------------------------------------

  /** Opens whatever a double-click (or touch double-tap) means for an object. */
  const openObjectEditor = (path: SelectionPath, object: SlideObject) => {
    if (object.kind === "chart") {
      setChartPath(path);
      return;
    }
    if (object.text) setEditingText(pathKey(path));
    if (object.kind === "image") void replaceImage(path);
  };

  const beginDrag = (event: React.PointerEvent, path: SelectionPath, mode: "move" | "resize" | "rotate") => {
    event.stopPropagation();
    if (isInheritedId(path[0])) return;
    const object = objectAtPath(slide.objects, path);
    if (!object) return;
    const snapshot = JSON.parse(JSON.stringify(object)) as SlideObject;
    dragState.current = { path, mode, startX: event.clientX, startY: event.clientY, object: snapshot };
    const pointerId = event.pointerId;
    const startX = event.clientX;
    const startY = event.clientY;
    const capture = event.currentTarget as Element | null;
    // Touch keeps receiving events after the finger leaves the object; the
    // mouse does not capture so double-click still reaches the object.
    try {
      if (event.pointerType !== "mouse") capture?.setPointerCapture?.(pointerId);
    } catch {
      // Pointer capture is unavailable (older webviews, jsdom).
    }
    const onMove = (move: PointerEvent) => {
      if (move.pointerId !== pointerId) return;
      const state = dragState.current;
      if (!state) return;
      const dx = (move.clientX - state.startX) / scale;
      const dy = (move.clientY - state.startY) / scale;
      const children = state.object.children;
      if (state.mode === "move") {
        const nextX = Math.round(state.object.x + dx);
        const nextY = Math.round(state.object.y + dy);
        const moveDx = nextX - state.object.x;
        const moveDy = nextY - state.object.y;
        updatePath(
          state.path,
          {
            x: nextX,
            y: nextY,
            children: children ? children.map((child) => translateObject(child, moveDx, moveDy)) : undefined,
          },
          false,
        );
      } else if (state.mode === "resize") {
        const w = Math.max(24, Math.round(state.object.w + dx));
        const h = Math.max(24, Math.round(state.object.h + dy));
        const sx = state.object.w > 0 ? w / state.object.w : 1;
        const sy = state.object.h > 0 ? h / state.object.h : 1;
        updatePath(
          state.path,
          {
            w,
            h,
            children: children
              ? children.map((child) => scaleObject(child, sx, sy, state.object.x, state.object.y))
              : undefined,
          },
          false,
        );
      } else {
        const centerX = state.object.x + state.object.w / 2;
        const centerY = state.object.y + state.object.h / 2;
        const angle = (Math.atan2(move.clientY / scale - centerY, move.clientX / scale - centerX) * 180) / Math.PI + 90;
        updatePath(state.path, { rotation: Math.round(angle) }, false);
      }
    };
    const onUp = (up: PointerEvent) => {
      if (up.pointerId !== pointerId) return;
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
      try {
        capture?.releasePointerCapture?.(pointerId);
      } catch {
        // Not captured.
      }
      dragState.current = null;
      // Touch never produces a native dblclick once the object owns the
      // gesture, so a double-tap opens the text editor by hand.
      if (
        mode === "move" &&
        up.pointerType !== "mouse" &&
        Math.abs(up.clientX - startX) + Math.abs(up.clientY - startY) < 6
      ) {
        const now = Date.now();
        const key = pathKey(path);
        const previous = lastTapRef.current;
        if (previous && previous.key === key && now - previous.time < 350) {
          lastTapRef.current = null;
          const current = objectAtPath(slide.objects, path);
          if (current) openObjectEditor(path, current);
        } else {
          lastTapRef.current = { key, time: now };
        }
      }
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
  };

  /** Empty-canvas drag: a marquee that selects every object it touches. */
  const beginMarquee = (event: React.PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    const surface = event.currentTarget;
    const pointerId = event.pointerId;
    const origin = surface.getBoundingClientRect();
    const startX = event.clientX;
    const startY = event.clientY;
    // The rectangle lives in slide units; the canvas is laid out at `scale`.
    const rectAt = (clientX: number, clientY: number) => {
      const bounds = surface.getBoundingClientRect();
      const endX = (clientX - bounds.left) / scale;
      const endY = (clientY - bounds.top) / scale;
      const startDeckX = (startX - origin.left) / scale;
      const startDeckY = (startY - origin.top) / scale;
      return {
        x: Math.min(startDeckX, endX),
        y: Math.min(startDeckY, endY),
        w: Math.abs(endX - startDeckX),
        h: Math.abs(endY - startDeckY),
      };
    };
    let moved = false;
    setSelected([]);
    try {
      if (event.pointerType !== "mouse") surface.setPointerCapture?.(pointerId);
    } catch {
      // Pointer capture is unavailable (older webviews, jsdom).
    }
    const onMove = (move: PointerEvent) => {
      if (move.pointerId !== pointerId) return;
      if (!moved && Math.abs(move.clientX - startX) + Math.abs(move.clientY - startY) < 4) return;
      moved = true;
      setMarquee(rectAt(move.clientX, move.clientY));
    };
    const onUp = (up: PointerEvent) => {
      if (up.pointerId !== pointerId) return;
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
      try {
        surface.releasePointerCapture?.(pointerId);
      } catch {
        // Not captured.
      }
      setMarquee(null);
      if (!moved) return;
      const hits = objectsInRect(slide.objects, rectAt(up.clientX, up.clientY));
      setSelected(hits.map((object) => [object.id]));
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
  };

  const handleObjectPointerDown = (event: React.PointerEvent, path: SelectionPath) => {
    event.stopPropagation();
    if (connectorMode) {
      const object = objectAtPath(slide.objects, path);
      if (!object || isInheritedId(path[0]) || object.kind === "line" || object.kind === "arrow") return;
      if (!connectorStart) {
        setConnectorStart(object.id);
        setSelected([[path[0]]]);
        return;
      }
      if (connectorStart === object.id) {
        setConnectorStart(null);
        return;
      }
      const from = slide.objects.find((candidate) => candidate.id === connectorStart);
      setConnectorStart(null);
      setConnectorMode(false);
      if (from) createConnector(from, object);
      return;
    }
    selectFromPointer(event, path);
    beginDrag(event, event.altKey ? path : [path[0]], "move");
  };

  const handleObjectDoubleClick = (event: React.MouseEvent, path: SelectionPath) => {
    event.stopPropagation();
    const object = objectAtPath(slide.objects, path);
    if (!object) return;
    // Editing a frame also makes it the primary selection, so its properties
    // (and the text toolbar) are on screen.
    setSelected([path.length === 1 ? path : [path[0]]]);
    openObjectEditor(path, object);
  };

  const handleTextCommit = (path: SelectionPath, paragraphs: TextParagraph[], recordUndo: boolean) => {
    const object = objectAtPath(slide.objects, path);
    if (!object?.text) return;
    if (paragraphs === object.text.paragraphs) return;
    updatePath(path, { text: { ...object.text, paragraphs } }, recordUndo);
  };

  // -------------------------------------------------------------------------
  // Slide operations
  // -------------------------------------------------------------------------

  const addSlide = () => {
    const next = LAYOUTS.find((layout) => layout.id === "titleContent")!;
    const created: Slide = {
      id: uid(),
      layout: "titleContent",
      masterId: slide.masterId ?? null,
      layoutId: slide.layoutId ?? null,
      background: null,
      transition: null,
      transitionMs: 500,
      objects: next.build(deck).map((object, index) => ({ ...object, z: index + 1 })),
      animations: [],
      notes: "",
    };
    update((current) => ({
      ...current,
      slides: [...current.slides.slice(0, slideIndex + 1), created, ...current.slides.slice(slideIndex + 1)],
    }));
    setSlideIndex(slideIndex + 1);
  };

  /** Appends the AI outline as slides: one undo step. */
  const addOutlineSlides = (outline: AiOutlineSlide[]) => {
    setOutlineDialog(false);
    const created = outlineSlides(deck, outline, { masterId: slide.masterId, layoutId: slide.layoutId });
    const firstNew = deck.slides.length;
    update((current) => ({ ...current, slides: [...current.slides, ...created] }));
    setSlideIndex(firstNew);
    setSelected([]);
    useToasts.getState().push({ kind: "success", title: t("ai.edit.slidesAdded", { count: created.length }) });
  };

  const duplicateSlide = () => {
    const { objects, idMap } = cloneObjects(slide.objects);
    const copy: Slide = JSON.parse(JSON.stringify(slide));
    copy.id = uid();
    copy.objects = objects;
    copy.animations = (slide.animations ?? []).map((animation) => ({
      ...animation,
      id: uid(),
      objectId: idMap.get(animation.objectId) ?? animation.objectId,
    }));
    update((current) => ({
      ...current,
      slides: [...current.slides.slice(0, slideIndex + 1), copy, ...current.slides.slice(slideIndex + 1)],
    }));
    setSlideIndex(slideIndex + 1);
  };

  const deleteSlide = () => {
    if (deck.slides.length <= 1) return;
    update((current) => ({ ...current, slides: current.slides.filter((_, index) => index !== slideIndex) }));
    setSlideIndex(Math.max(0, slideIndex - 1));
  };

  const moveSlide = useCallback(
    (from: number, to: number) => {
      if (to < 0 || to >= deck.slides.length) return;
      update((current) => {
        const slides = [...current.slides];
        const [moved] = slides.splice(from, 1);
        slides.splice(to, 0, moved);
        return { ...current, slides };
      });
      setSlideIndex(to);
    },
    [deck.slides.length, update],
  );

  const applyLayout = (layoutId: string) => {
    const layout = LAYOUTS.find((candidate) => candidate.id === layoutId);
    if (!layout) return;
    updateSlide((current) => ({
      ...current,
      layout: layoutId,
      objects: layout.build(deck).map((object, index) => ({ ...object, z: index + 1 })),
    }));
  };

  const selectMaster = (masterId: string) => {
    updateSlide((current) => ({ ...current, masterId: masterId || null, layoutId: null }));
  };

  const selectLayout = (layoutId: string) => {
    if (!layoutId) {
      updateSlide((current) => ({ ...current, layoutId: null }));
      return;
    }
    const layout = selectedMaster?.layouts.find((candidate) => candidate.id === layoutId);
    if (!layout) return;
    updateSlide((current) => ({ ...current, layoutId, layout: layout.kind }));
  };

  const openMasterDialog = () => {
    if (masters.length === 0) {
      const master = newSlideMaster();
      update((current) => ({ ...current, masters: [master] }));
    }
    setMasterDialog(true);
  };

  const addObject = (kind: string) => {
    const base = { x: 120, y: 140, w: 240, h: 140 };
    let object: SlideObject;
    if (kind === "text") object = textObject("New text", base.x, base.y, 320, 100, 20, "left");
    else if (kind === "image") {
      object = { ...textObject("Add an image", base.x, base.y, 320, 200, 16, "center"), kind: "image" };
    } else if (kind === "line" || kind === "arrow") {
      object = {
        ...textObject("", base.x, base.y, 200, 40, 16, "left"),
        kind,
        line: { x2: 200, y2: 40, beginArrow: false, endArrow: kind === "arrow", dash: "solid" },
        style: { fill: null, stroke: theme.accent, strokeWidthPt: 2, opacity: 1, cornerRadiusPt: 0, shadow: false },
      };
    } else if (kind === "table") {
      object = {
        ...textObject("", base.x, base.y, 460, 200, 14, "left"),
        kind: "table",
        table: {
          rows: Array.from({ length: 3 }, (): import("../lib/office-types").TableRow => ({
            cells: Array.from({ length: 3 }, (): import("../lib/office-types").TableCell => ({
              blocks: [
                {
                  type: "paragraph" as const,
                  props: {
                    style: "Normal",
                    align: "left",
                    lineSpacing: 1.15,
                    spaceBeforePt: 0,
                    spaceAfterPt: 0,
                    indentLeftPt: 0,
                    indentRightPt: 0,
                    firstLinePt: 0,
                    list: null,
                    pageBreakBefore: false,
                  },
                  runs: [
                    {
                      text: "",
                      bold: false,
                      italic: false,
                      underline: false,
                      strike: false,
                      color: null,
                      highlight: null,
                      font: null,
                      sizePt: null,
                      link: null,
                      comment: null,
                      superscript: false,
                      subscript: false,
                    },
                  ],
                },
              ],
              colspan: 1,
              rowspan: 1,
              background: null,
              align: "left",
              valign: "top",
              widthPt: null,
            })),
            heightPt: null,
            header: false,
          })),
          columnWidthsPt: [153, 153, 153],
          borders: true,
          borderColor: "#94A3B8",
          align: "left",
        },
      };
    } else if (kind === "chart") {
      object = { ...textObject("", base.x, base.y, 420, 260, 14, "left"), kind: "chart", chart: emptyChart() };
    } else {
      object = {
        ...textObject("", base.x, base.y, base.w, base.h, 16, "left"),
        kind,
        style: {
          fill: theme.accent,
          stroke: null,
          strokeWidthPt: 1.5,
          opacity: 1,
          cornerRadiusPt: kind === "roundRect" ? 12 : 0,
          shadow: false,
        },
      };
    }
    object.z = slide.objects.length + 1;
    updateSlide((current) => ({ ...current, objects: [...current.objects, object] }));
    setSelected([[object.id]]);
  };

  const deleteSelected = () => {
    if (selected.length === 0) return;
    updateSlide((current) => ({ ...current, objects: removeSelectionPaths(current.objects, selected) }));
    setSelected([]);
  };

  const duplicateSelected = () => {
    const members = selected
      .filter((path) => path.length === 1)
      .map((path) => slide.objects.find((object) => object.id === path[0]))
      .filter((object): object is SlideObject => Boolean(object));
    if (members.length === 0) return;
    const copies = members.map((object) => ({ ...cloneObject(object), x: object.x + 16, y: object.y + 16 }));
    updateSlide((current) => ({
      ...current,
      objects: [...current.objects, ...copies].map((object, index) => ({ ...object, z: index + 1 })),
    }));
    setSelected(copies.map((object) => [object.id]));
  };

  // -------------------------------------------------------------------------
  // Internal object clipboard and connectors
  // -------------------------------------------------------------------------

  const copySelected = () => {
    const members = selected
      .filter((path) => path.length === 1)
      .map((path) => slide.objects.find((object) => object.id === path[0]))
      .filter((object): object is SlideObject => Boolean(object));
    if (members.length === 0) return;
    clipboardRef.current = JSON.parse(JSON.stringify(members)) as SlideObject[];
  };

  const pasteClipboard = () => {
    if (clipboardRef.current.length === 0) return;
    const { objects: copies, idMap } = cloneObjects(clipboardRef.current);
    const placed = remapConnectorTargets(copies, idMap).map((object) => ({
      ...object,
      x: object.x + 16,
      y: object.y + 16,
    }));
    updateSlide((current) => ({
      ...current,
      objects: [...current.objects, ...placed].map((object, index) => ({ ...object, z: index + 1 })),
    }));
    setSelected(placed.map((object) => [object.id]));
  };

  const cutSelected = () => {
    copySelected();
    deleteSelected();
  };

  /** A connector line glued to the two shapes, routed site to site. */
  const createConnector = (from: SlideObject, to: SlideObject) => {
    const fromCenter = { x: from.x + from.w / 2, y: from.y + from.h / 2 };
    const toCenter = { x: to.x + to.w / 2, y: to.y + to.h / 2 };
    const line: LineSpec = {
      x2: Math.round(toCenter.x - fromCenter.x),
      y2: Math.round(toCenter.y - fromCenter.y),
      beginArrow: false,
      endArrow: true,
      dash: "solid",
      beginObject: from.id,
      endObject: to.id,
      beginSite: nearestSite(from, toCenter),
      endSite: nearestSite(to, fromCenter),
    };
    const base: SlideObject = {
      ...newSlideObject("line", from.x, from.y, 40, 40),
      line,
      style: { fill: null, stroke: theme.accent, strokeWidthPt: 2, opacity: 1, cornerRadiusPt: 0, shadow: false },
      name: "Connector",
    };
    const created = connectorPatch(base, [...slide.objects, base]);
    created.z = slide.objects.length + 1;
    updateSlide((current) => ({ ...current, objects: [...current.objects, created] }));
    setSelected([[created.id]]);
  };

  const alignSelected = (mode: "left" | "center" | "right" | "top" | "middle" | "bottom") => {
    if (selected.length === 0) return;
    updateSlide((current) => {
      const targets = selected
        .map((path) => ({ path, object: objectAtPath(current.objects, path) }))
        .filter((entry): entry is { path: SelectionPath; object: SlideObject } => Boolean(entry.object));
      if (targets.length === 0) return current;
      // A single object aligns to the slide; several align to their bounding box.
      const bounds =
        targets.length === 1
          ? { x: 0, y: 0, w: deck.size.widthPt, h: deck.size.heightPt }
          : objectBounds(targets.map((entry) => entry.object));
      let objects = current.objects;
      for (const target of targets) {
        objects = replaceObjectAtPath(objects, target.path, alignBox(target.object, bounds, mode));
      }
      return { ...current, objects: refreshGroupBounds(objects) };
    });
  };

  /** Equal spacing between edge pairs; needs at least three top-level objects. */
  const distributeSelected = (axis: "x" | "y") => {
    const targets = selected
      .filter((path) => path.length === 1)
      .map((path) => slide.objects.find((object) => object.id === path[0]))
      .filter((object): object is SlideObject => Boolean(object));
    if (targets.length < 3) return;
    updateSlide((current) => {
      const boxes = targets.map((object) => ({
        id: object.id,
        x: object.x,
        y: object.y,
        w: object.w,
        h: object.h,
      }));
      const positions = distributePositions(boxes, axis);
      let objects = current.objects;
      for (const box of boxes) {
        const value = positions.get(box.id);
        if (value === undefined) continue;
        objects = replaceObjectAtPath(objects, [box.id], axis === "x" ? { x: value } : { y: value });
      }
      return { ...current, objects };
    });
  };

  const groupSelected = () => {
    const ids = selected.filter((path) => path.length === 1).map((path) => path[0]);
    if (ids.length < 2) return;
    const groupId = uid();
    updateSlide((current) => ({ ...current, objects: groupSelection(current.objects, ids, groupId) }));
    setSelected([[groupId]]);
  };

  const ungroupSelected = () => {
    const ids = selected
      .filter((path) => path.length === 1)
      .map((path) => path[0])
      .filter((id) => slide.objects.find((object) => object.id === id)?.kind === "group");
    if (ids.length === 0) return;
    updateSlide((current) => ({ ...current, objects: ungroupSelection(current.objects, ids) }));
    setSelected([]);
  };

  const bringForward = (delta: number) => {
    const ids = new Set(selected.map((path) => path[0]));
    if (ids.size === 0) return;
    updateSlide((current) => {
      const objects = [...current.objects].sort((a, b) => a.z - b.z);
      for (const id of ids) {
        const index = objects.findIndex((object) => object.id === id);
        if (index < 0) continue;
        const target = Math.max(0, Math.min(objects.length - 1, index + delta));
        const [moved] = objects.splice(index, 1);
        objects.splice(target, 0, moved);
      }
      return { ...current, objects: objects.map((object, index) => ({ ...object, z: index + 1 })) };
    });
  };

  const setPrimaryPosition = (axis: "x" | "y", value: number) => {
    if (!primary || !primaryPath) return;
    const delta = value - primary[axis];
    if (!delta) return;
    const patch: Partial<SlideObject> = axis === "x" ? { x: value } : { y: value };
    if (primary.children && primary.children.length > 0) {
      patch.children = primary.children.map((child) =>
        translateObject(child, axis === "x" ? delta : 0, axis === "y" ? delta : 0),
      );
    }
    updatePath(primaryPath, patch);
  };

  const setPrimarySize = (axis: "w" | "h", value: number) => {
    if (!primary || !primaryPath) return;
    const next = Math.max(4, Math.round(value));
    const patch: Partial<SlideObject> = axis === "w" ? { w: next } : { h: next };
    if (primary.children && primary.children.length > 0) {
      const sx = axis === "w" && primary.w > 0 ? next / primary.w : 1;
      const sy = axis === "h" && primary.h > 0 ? next / primary.h : 1;
      patch.children = primary.children.map((child) => scaleObject(child, sx, sy, primary.x, primary.y));
    }
    updatePath(primaryPath, patch);
  };

  /** Drags one crop handle: fractions move with the pointer, committed once. */
  const beginCropDrag = (event: React.PointerEvent, mode: string) => {
    if (!primary || !primaryPath || !primary.image) return;
    event.stopPropagation();
    const base = cropDraft ?? clampCrop(primary.image.crop);
    const startX = event.clientX;
    const startY = event.clientY;
    const width = Math.max(1, primary.w);
    const height = Math.max(1, primary.h);
    const pointerId = event.pointerId;
    let latest = base;
    const onMove = (move: PointerEvent) => {
      if (move.pointerId !== pointerId) return;
      const dx = (move.clientX - startX) / scale / width;
      const dy = (move.clientY - startY) / scale / height;
      const next = { ...base };
      if (mode.includes("left")) next.left = base.left + dx;
      if (mode.includes("right")) next.right = base.right - dx;
      if (mode.includes("top")) next.top = base.top + dy;
      if (mode.includes("bottom")) next.bottom = base.bottom - dy;
      latest = clampCrop(next);
      setCropDraft(latest);
    };
    const onUp = (up: PointerEvent) => {
      if (up.pointerId !== pointerId) return;
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
      setCropDraft(null);
      const committed = clampCrop(latest);
      updatePath(primaryPath, {
        image: { ...primary.image!, crop: isFullCrop(committed) ? null : committed },
      });
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
  };

  const undo = () => {
    setUndoStack((stack) => {
      const previous = stack[stack.length - 1];
      if (!previous) return stack;
      setRedoStack((redos) => [...redos, deck]);
      edit(tab.id, () => previous);
      return stack.slice(0, -1);
    });
  };

  const redo = () => {
    setRedoStack((stack) => {
      const next = stack[stack.length - 1];
      if (!next) return stack;
      setUndoStack((undos) => [...undos, deck]);
      edit(tab.id, () => next);
      return stack.slice(0, -1);
    });
  };

  // -------------------------------------------------------------------------
  // Animations + slideshow
  // -------------------------------------------------------------------------

  const reorderAnimation = (id: string, delta: number) => {
    updateSlide((current) => {
      const list = [...(current.animations ?? [])].sort((a, b) => a.order - b.order);
      const index = list.findIndex((animation) => animation.id === id);
      const target = index + delta;
      if (index < 0 || target < 0 || target >= list.length) return current;
      const [moved] = list.splice(index, 1);
      list.splice(target, 0, moved);
      return { ...current, animations: list.map((animation, position) => ({ ...animation, order: position + 1 })) };
    });
  };

  const saveAnimation = (animation: Animation) => {
    updateSlide((current) => {
      const list = current.animations ?? [];
      return {
        ...current,
        animations: list.some((candidate) => candidate.id === animation.id)
          ? list.map((candidate) => (candidate.id === animation.id ? animation : candidate))
          : [...list, animation],
      };
    });
    setAnimationEditing(null);
  };

  const addAnimation = () => {
    const target = selectedObjects[0] ?? slide.objects[0];
    if (!target) return;
    const order = (slide.animations ?? []).reduce((max, animation) => Math.max(max, animation.order), 0) + 1;
    setAnimationEditing(newAnimation(target.id, "entrance", "fade", "onClick", order));
  };

  const clearShowTimers = useCallback(() => {
    for (const timer of showTimersRef.current) window.clearTimeout(timer);
    showTimersRef.current = [];
  }, []);

  const runStep = useCallback((step: AnimationStep) => {
    const startedAt = Date.now();
    let endAt = startedAt;
    for (const animation of step.animations) {
      const beginsAfter = Math.max(0, animation.delayMs);
      const lasting = Math.max(0, animation.durationMs);
      endAt = Math.max(endAt, startedAt + beginsAfter + lasting);
      const startTimer = window.setTimeout(() => {
        const running: RunningAnimation = { effect: animation, phase: "to" };
        setAnimRunning((current) => ({ ...current, [animation.id]: running }));
        if (animation.kind === "emphasis" && lasting > 0) {
          const backTimer = window.setTimeout(
            () => {
              setAnimRunning((current) =>
                current[animation.id] ? { ...current, [animation.id]: { effect: animation, phase: "back" } } : current,
              );
            },
            Math.round(lasting / 2),
          );
          showTimersRef.current.push(backTimer);
        }
        const endTimer = window.setTimeout(() => {
          setAnimRunning((current) => {
            const next = { ...current };
            delete next[animation.id];
            return next;
          });
          setAnimDone((current) => ({ ...current, [animation.id]: true }));
        }, lasting);
        showTimersRef.current.push(endTimer);
      }, beginsAfter);
      showTimersRef.current.push(startTimer);
    }
    showStartRef.current = startedAt;
    showEndRef.current = endAt;
    stepIndexRef.current += 1;
    setStepIndex(stepIndexRef.current);
  }, []);

  /** Nearest visible slide in a direction; stays put when there is none. */
  const nextVisibleIndex = useCallback(
    (index: number, direction: 1 | -1) => {
      let next = index;
      while (next >= 0 && next < deck.slides.length && deck.slides[next]?.hidden) next += direction;
      return next >= 0 && next < deck.slides.length ? next : index;
    },
    [deck.slides],
  );

  const goToSlide = useCallback(
    (index: number) => {
      clearShowTimers();
      showStartRef.current = null;
      showEndRef.current = null;
      stepIndexRef.current = 0;
      setStepIndex(0);
      setAnimRunning({});
      setAnimDone({});
      const clamped = Math.max(0, Math.min(index, deck.slides.length - 1));
      // The slideshow skips hidden slides in both directions.
      setSlideshow(deck.slides[clamped]?.hidden ? nextVisibleIndex(clamped, 1) : clamped);
    },
    [clearShowTimers, deck.slides, nextVisibleIndex],
  );

  const showSlide = slideshow === null ? undefined : deck.slides[Math.min(slideshow, deck.slides.length - 1)];
  const nextSlide = slideshow === null ? undefined : deck.slides[slideshow + 1];
  const showAnimations = useMemo(() => showSlide?.animations ?? [], [showSlide]);
  const stepList = useMemo(() => animationTimeline(showAnimations), [showAnimations]);

  const advanceShow = useCallback(() => {
    if (slideshow === null) return;
    if (stepIndexRef.current < stepList.length) {
      const step = stepList[stepIndexRef.current];
      if (step.waitForClick) runStep(step);
      return;
    }
    const next = nextVisibleIndex(slideshow + 1, 1);
    if (next > slideshow) goToSlide(next);
  }, [goToSlide, nextVisibleIndex, runStep, slideshow, stepList]);

  useEffect(() => {
    advanceRef.current = advanceShow;
  }, [advanceShow]);

  useEffect(() => {
    if (slideshow === null || stepIndex >= stepList.length) return;
    const step = stepList[stepIndex];
    if (step.waitForClick) return;
    const now = Date.now();
    const base =
      step.relativeTo === "previousStart"
        ? (showStartRef.current ?? now)
        : step.relativeTo === "previousEnd"
          ? (showEndRef.current ?? now)
          : now;
    const timer = window.setTimeout(() => runStep(step), Math.max(0, base - now));
    showTimersRef.current.push(timer);
    return () => window.clearTimeout(timer);
  }, [runStep, slideshow, stepIndex, stepList]);

  useEffect(() => {
    if (showActive) return;
    clearShowTimers();
    stepIndexRef.current = 0;
  }, [clearShowTimers, showActive]);

  useEffect(() => {
    if (slideshow === null) return;
    const handler = (event: KeyboardEvent) => {
      const target = event.target;
      if (target instanceof Element && target.closest("input,textarea,select,[contenteditable=true]")) return;
      if (event.key === "Escape") {
        setSlideshow(null);
        setPresenterView(false);
        return;
      }
      if (event.key === "ArrowRight" || event.key === " ") {
        event.preventDefault();
        advanceRef.current();
        return;
      }
      if (event.key === "ArrowLeft") {
        event.preventDefault();
        goToSlide(nextVisibleIndex(slideshow - 1, -1));
        return;
      }
      const key = event.key.toLowerCase();
      if (key === "p") {
        setPresenterView((value) => !value);
        return;
      }
      // Presentation tools: laser, pen and eraser toggle; C clears the ink.
      if (key === "l") setShowTool((value) => (value === "laser" ? "pointer" : "laser"));
      if (key === "b") setShowTool((value) => (value === "pen" ? "pointer" : "pen"));
      if (key === "e") setShowTool((value) => (value === "eraser" ? "pointer" : "eraser"));
      if (key === "c") setStrokes([]);
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [goToSlide, nextVisibleIndex, slideshow]);

  useEffect(() => {
    if (!showActive) return;
    const startedAt = Date.now();
    const interval = window.setInterval(() => setElapsedMs(Date.now() - startedAt), 500);
    return () => window.clearInterval(interval);
  }, [showActive]);

  useEffect(() => () => clearShowTimers(), [clearShowTimers]);

  // -------------------------------------------------------------------------
  // Keyboard shortcuts
  // -------------------------------------------------------------------------

  const shortcutsRef = useRef<(event: KeyboardEvent) => void>(() => undefined);
  const slideshowRef = useRef<number | null>(null);
  useEffect(() => {
    slideshowRef.current = slideshow;
  }, [slideshow]);
  useEffect(() => {
    shortcutsRef.current = (event: KeyboardEvent) => {
      // The slideshow owns the keyboard while it is open.
      if (slideshowRef.current !== null) return;
      const target = event.target;
      if (target instanceof Element && target.closest("input,textarea,select,[contenteditable=true]")) return;
      if (event.key === "Escape") {
        if (editingText) {
          setEditingText(null);
          setTextSelection(null);
        }
        setSelected([]);
        setConnectorMode(false);
        setConnectorStart(null);
        setCropPath(null);
        setCropDraft(null);
        return;
      }
      if (event.ctrlKey || event.metaKey) {
        switch (event.key.toLowerCase()) {
          case "z":
            event.preventDefault();
            if (event.shiftKey) redo();
            else undo();
            return;
          case "y":
            event.preventDefault();
            redo();
            return;
          case "c":
            event.preventDefault();
            copySelected();
            return;
          case "x":
            event.preventDefault();
            cutSelected();
            return;
          case "v":
            event.preventDefault();
            pasteClipboard();
            return;
          case "d":
            event.preventDefault();
            duplicateSelected();
            return;
          default:
            return;
        }
      }
      if (event.key === "Delete" || event.key === "Backspace") {
        if (selected.length > 0) {
          event.preventDefault();
          deleteSelected();
        }
        return;
      }
      if (event.key.startsWith("Arrow")) {
        const stage = document.querySelector<HTMLElement>(".slide-stage");
        if (!stage) return;
        event.preventDefault();
        if (event.key === "ArrowRight") stage.scrollLeft += 60;
        if (event.key === "ArrowLeft") stage.scrollLeft = Math.max(0, stage.scrollLeft - 60);
        if (event.key === "ArrowDown") stage.scrollTop += 60;
        if (event.key === "ArrowUp") stage.scrollTop = Math.max(0, stage.scrollTop - 60);
      }
    };
  });
  useEffect(() => {
    const listener = (event: KeyboardEvent) => shortcutsRef.current(event);
    window.addEventListener("keydown", listener);
    return () => window.removeEventListener("keydown", listener);
  }, []);

  // Sorter drag: pointerup commits the reorder at the last hovered cell.
  useEffect(() => {
    const up = () => {
      const from = sorterDragRef.current;
      sorterDragRef.current = null;
      const to = dropIndexRef.current;
      dropIndexRef.current = null;
      setDropIndex(null);
      if (from === null || to === null || to === from) return;
      moveSlide(from, to);
    };
    window.addEventListener("pointerup", up);
    return () => window.removeEventListener("pointerup", up);
  }, [moveSlide]);

  const showObjectStyle = useCallback(
    (object: SlideObject) => animationObjectStyle(object.id, showAnimations, animRunning, animDone),
    [animDone, animRunning, showAnimations],
  );

  /** Pointer position in slide units, for ink and the laser dot. */
  const showPoint = (event: { clientX: number; clientY: number }): StrokePoint | null => {
    const element = slideAreaRef.current;
    if (!element) return null;
    const rect = element.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) return { x: event.clientX, y: event.clientY };
    return {
      x: ((event.clientX - rect.left) / rect.width) * deck.size.widthPt,
      y: ((event.clientY - rect.top) / rect.height) * deck.size.heightPt,
    };
  };

  const eraseAt = (point: StrokePoint) => {
    setStrokes((current) => {
      const hit = strokeHit(current, point, 14);
      return hit < 0 ? current : current.filter((_, index) => index !== hit);
    });
  };

  const slideshowSlideArea = showSlide ? (
    <div
      ref={slideAreaRef}
      className="slideshow-slide"
      data-transition={showSlide.transition ?? "fade"}
      style={{
        width: presenterView ? "100%" : "90vw",
        height: presenterView ? "100%" : `${(90 * deck.size.heightPt) / deck.size.widthPt}vh`,
        background: showSlide.background ?? theme.background,
        touchAction: showTool === "pointer" ? undefined : "none",
      }}
      onPointerDown={(event) => {
        if (showTool === "pointer") return;
        event.stopPropagation();
        const point = showPoint(event);
        if (!point) return;
        if (showTool === "pen") {
          strokeRef.current = [point];
          setDraftStroke([point]);
        } else if (showTool === "eraser") {
          eraseAt(point);
        }
      }}
      onPointerMove={(event) => {
        if (showTool === "pointer") return;
        const point = showPoint(event);
        if (!point) return;
        if (showTool === "laser") {
          laserRef.current = point;
          setLaser(point);
          return;
        }
        if (showTool === "eraser") {
          if (event.buttons) eraseAt(point);
          return;
        }
        if (showTool === "pen" && strokeRef.current) {
          strokeRef.current = [...strokeRef.current, point];
          setDraftStroke(strokeRef.current);
        }
      }}
      onPointerUp={(event) => {
        if (showTool !== "pen") return;
        event.stopPropagation();
        const points = strokeRef.current;
        strokeRef.current = null;
        setDraftStroke(null);
        if (!points || points.length === 0) return;
        setStrokes((current) => [...current, simplifyStroke(points)]);
      }}
      onPointerLeave={() => {
        if (showTool === "laser" && laserRef.current) {
          laserRef.current = null;
          setLaser(null);
        }
      }}
    >
      <SlidePreview
        deck={deck}
        slide={showSlide}
        theme={theme}
        width={0}
        full
        slideWidth={deck.size.widthPt}
        slideHeight={deck.size.heightPt}
        objectStyle={showObjectStyle}
      />
      <svg
        className="slideshow-ink"
        viewBox={`0 0 ${deck.size.widthPt} ${deck.size.heightPt}`}
        preserveAspectRatio="none"
        aria-hidden="true"
      >
        {strokes.map((stroke, index) => (
          <path
            key={index}
            d={strokePath(stroke)}
            fill="none"
            stroke="#ef4444"
            strokeWidth={3}
            vectorEffect="non-scaling-stroke"
          />
        ))}
        {draftStroke ? (
          <path
            d={strokePath(draftStroke)}
            fill="none"
            stroke="#ef4444"
            strokeWidth={3}
            vectorEffect="non-scaling-stroke"
          />
        ) : null}
      </svg>
      {laser ? (
        <span
          className="slideshow-laser"
          style={{
            left: `${(laser.x / deck.size.widthPt) * 100}%`,
            top: `${(laser.y / deck.size.heightPt) * 100}%`,
          }}
        />
      ) : null}
    </div>
  ) : null;

  const presenterPanel =
    presenterView && showSlide ? (
      <aside
        className="presenter-panel"
        style={{
          width: 320,
          minWidth: 320,
          maxHeight: "100%",
          overflow: "auto",
          display: "flex",
          flexDirection: "column",
          gap: 10,
          padding: 12,
          borderRadius: 10,
          background: "rgba(15, 23, 42, 0.94)",
          color: "#E2E8F0",
        }}
      >
        <div className="row" style={{ justifyContent: "space-between", alignItems: "center" }}>
          <strong>{t("impress.presenterView")}</strong>
          <span style={{ fontVariantNumeric: "tabular-nums" }}>{formatClock(elapsedMs)}</span>
        </div>
        <div>
          <h4>{t("impress.currentSlide")}</h4>
          <SlidePreview deck={deck} slide={showSlide} theme={theme} width={280} objectStyle={showObjectStyle} />
        </div>
        {nextSlide ? (
          <div>
            <h4>{t("impress.nextSlide")}</h4>
            <SlidePreview deck={deck} slide={nextSlide} theme={theme} width={280} />
          </div>
        ) : null}
        <div>
          <h4>{t("impress.notes")}</h4>
          <div style={{ whiteSpace: "pre-wrap", fontSize: 12, opacity: 0.9 }}>{showSlide.notes || "—"}</div>
        </div>
        <div className="row">
          <button type="button" className="btn btn-soft" onClick={() => goToSlide(Math.max(0, (slideshow ?? 0) - 1))}>
            ‹ {t("impress.previous")}
          </button>
          <button type="button" className="btn btn-soft" onClick={advanceShow}>
            {t("impress.next")} ›
          </button>
        </div>
      </aside>
    ) : null;

  const slideshowNav = (
    <div
      className="slideshow-nav"
      role="presentation"
      onClick={(event) => event.stopPropagation()}
      style={presenterView ? { position: "static" } : undefined}
    >
      <button type="button" className="btn btn-soft" onClick={() => goToSlide(Math.max(0, (slideshow ?? 0) - 1))}>
        ‹
      </button>
      <span>
        {(slideshow ?? 0) + 1} / {deck.slides.length}
      </span>
      <button type="button" className="btn btn-soft" onClick={advanceShow}>
        ›
      </button>
      <button type="button" className="btn btn-soft" onClick={() => setPresenterView((value) => !value)}>
        {t("impress.presenterView")}
      </button>
      <div className="slideshow-tools" role="toolbar" aria-label={t("impress.showTools")}>
        <ToolButton
          icon={<Pointer size={14} />}
          title={t("impress.pointer")}
          active={showTool === "pointer"}
          onClick={() => setShowTool("pointer")}
        />
        <ToolButton
          icon={<CircleDot size={14} />}
          title={t("impress.laserPointer")}
          active={showTool === "laser"}
          onClick={() => setShowTool("laser")}
        />
        <ToolButton
          icon={<PenLine size={14} />}
          title={t("impress.pen")}
          active={showTool === "pen"}
          onClick={() => setShowTool("pen")}
        />
        <ToolButton
          icon={<Eraser size={14} />}
          title={t("impress.eraser")}
          active={showTool === "eraser"}
          onClick={() => setShowTool("eraser")}
        />
        <ToolButton icon={<Trash2 size={14} />} title={t("impress.clearInk")} onClick={() => setStrokes([])} />
      </div>
      <button
        type="button"
        className="btn btn-soft"
        onClick={() => {
          setSlideshow(null);
          setPresenterView(false);
        }}
      >
        {t("common.close")}
      </button>
    </div>
  );

  return (
    <div className="editor impress-editor">
      <Ribbon
        tabs={[
          { id: "home", label: t("impress.tabHome") },
          { id: "insert", label: t("impress.tabInsert") },
          { id: "design", label: t("impress.tabDesign") },
          { id: "transitions", label: t("impress.tabTransitions") },
          { id: "animations", label: t("impress.tabAnimations") },
          { id: "view", label: t("impress.tabView") },
        ]}
        active={ribbon}
        onSelect={setRibbon}
      >
        {ribbon === "home" ? (
          <>
            <RibbonGroup label={t("writer.clipboard")}>
              <ToolButton
                icon={<Undo2 size={16} />}
                onClick={undo}
                disabled={undoStack.length === 0}
                title={t("common.undo")}
              />
              <ToolButton
                icon={<Redo2 size={16} />}
                onClick={redo}
                disabled={redoStack.length === 0}
                title={t("common.redo")}
              />
              <ToolButton
                icon={<Copy size={16} />}
                label={t("impress.duplicate")}
                onClick={duplicateSelected}
                disabled={selected.length === 0}
              />
              <ToolButton
                icon={<Trash2 size={16} />}
                label={t("common.delete")}
                onClick={deleteSelected}
                disabled={selected.length === 0}
              />
            </RibbonGroup>
            <RibbonGroup label={t("impress.objects")}>
              <ToolButton icon={<Type size={16} />} label={t("impress.text")} onClick={() => addObject("text")} />
              <ToolButton icon={<Square size={16} />} label={t("impress.rect")} onClick={() => addObject("rect")} />
              <ToolButton
                icon={<Circle size={16} />}
                label={t("impress.ellipse")}
                onClick={() => addObject("ellipse")}
              />
              <ToolButton icon={<Minus size={16} />} label={t("impress.line")} onClick={() => addObject("line")} />
              <ToolButton
                icon={<ArrowRight size={16} />}
                label={t("impress.arrow")}
                onClick={() => addObject("arrow")}
              />
              <ToolButton
                icon={<Link2 size={16} />}
                label={t("impress.connector")}
                onClick={() => {
                  setConnectorMode((value) => !value);
                  setConnectorStart(null);
                }}
                active={connectorMode}
              />
              <ToolButton icon={<ImageIcon size={16} />} label={t("writer.image")} onClick={() => addObject("image")} />
              <ToolButton icon={<TableIcon size={16} />} label={t("writer.table")} onClick={() => addObject("table")} />
              <ToolButton icon={<LineChart size={16} />} label={t("calc.chart")} onClick={() => addObject("chart")} />
            </RibbonGroup>
            <RibbonGroup label={t("impress.arrange")}>
              <ToolButton
                icon={<AlignStartHorizontal size={16} />}
                onClick={() => alignSelected("left")}
                title={t("impress.alignLeft")}
              />
              <ToolButton
                icon={<AlignCenterHorizontal size={16} />}
                onClick={() => alignSelected("center")}
                title={t("impress.alignCenter")}
              />
              <ToolButton
                icon={<AlignEndHorizontal size={16} />}
                onClick={() => alignSelected("right")}
                title={t("impress.alignRight")}
              />
              <ToolButton
                icon={<AlignStartVertical size={16} />}
                onClick={() => alignSelected("top")}
                title={t("impress.alignTop")}
              />
              <ToolButton
                icon={<AlignCenterVertical size={16} />}
                onClick={() => alignSelected("middle")}
                title={t("impress.alignMiddle")}
              />
              <ToolButton
                icon={<AlignEndVertical size={16} />}
                onClick={() => alignSelected("bottom")}
                title={t("impress.alignBottom")}
              />
              <ToolButton
                icon={<Rows3 size={16} />}
                onClick={() => distributeSelected("y")}
                title={t("impress.distributeVertical")}
              />
              <ToolButton
                icon={<Grid2X2 size={16} />}
                onClick={() => distributeSelected("x")}
                title={t("impress.distributeHorizontal")}
              />
              <ToolButton icon={<Move size={16} />} onClick={() => bringForward(1)} title={t("impress.bringForward")} />
              <ToolButton
                icon={<Group size={16} />}
                label={t("impress.group")}
                onClick={groupSelected}
                disabled={selected.filter((path) => path.length === 1).length < 2}
              />
              <ToolButton
                icon={<Ungroup size={16} />}
                label={t("impress.ungroup")}
                onClick={ungroupSelected}
                disabled={!hasGroupSelection}
              />
            </RibbonGroup>
            <RibbonGroup label={t("impress.slides")}>
              <ToolButton icon={<Plus size={16} />} label={t("impress.newSlide")} onClick={addSlide} />
              <ToolButton icon={<Copy size={16} />} label={t("impress.duplicateSlide")} onClick={duplicateSlide} />
              <ToolButton
                icon={<Trash2 size={16} />}
                label={t("impress.deleteSlide")}
                onClick={deleteSlide}
                disabled={deck.slides.length <= 1}
              />
            </RibbonGroup>
          </>
        ) : null}

        {ribbon === "insert" ? (
          <>
            <RibbonGroup label={t("impress.slides")}>
              <ToolButton icon={<Plus size={16} />} label={t("impress.newSlide")} onClick={addSlide} />
            </RibbonGroup>
            <RibbonGroup label={t("ai.edit.group")}>
              <ToolButton
                icon={<Sparkles size={16} />}
                label={t("ai.edit.btn.outlineToSlides")}
                onClick={() => setOutlineDialog(true)}
                disabled={!aiStatus.configured}
                title={
                  aiStatus.configured
                    ? undefined
                    : `${t("ai.edit.btn.outlineToSlides")} - ${t("ai.edit.notConfigured")}`
                }
              />
            </RibbonGroup>
            <RibbonGroup label={t("impress.objects")}>
              <ToolButton icon={<Type size={16} />} label={t("impress.text")} onClick={() => addObject("text")} />
              <ToolButton icon={<ImageIcon size={16} />} label={t("writer.image")} onClick={() => addObject("image")} />
              <ToolButton icon={<TableIcon size={16} />} label={t("writer.table")} onClick={() => addObject("table")} />
              <ToolButton icon={<LineChart size={16} />} label={t("calc.chart")} onClick={() => addObject("chart")} />
              <ToolButton
                icon={<Braces size={16} />}
                label={t("impress.rect")}
                onClick={() => addObject("roundRect")}
              />
            </RibbonGroup>
          </>
        ) : null}

        {ribbon === "design" ? (
          <>
            <RibbonGroup label={t("impress.theme")}>
              <ToolSelect
                value={deck.theme}
                onChange={(themeId) => update((current) => ({ ...current, theme: themeId }))}
                options={THEMES.map((candidate) => ({ value: candidate.id, label: candidate.name }))}
                width={130}
              />
            </RibbonGroup>
            <RibbonGroup label={t("impress.layout")}>
              <ToolSelect
                value={slide.layout}
                onChange={applyLayout}
                options={LAYOUTS.map((layout) => ({ value: layout.id, label: layout.name }))}
                width={150}
              />
            </RibbonGroup>
            <RibbonGroup label={t("impress.master")}>
              <ToolButton icon={<LayoutTemplate size={16} />} label={t("impress.masters")} onClick={openMasterDialog} />
            </RibbonGroup>
            <RibbonGroup label={t("impress.background")}>
              <ToolColor
                value={slide.background ?? theme.background}
                onChange={(background) => updateSlide((current) => ({ ...current, background }))}
                title={t("impress.background")}
              />
              <ToolButton
                label={t("impress.clearBackground")}
                onClick={() => updateSlide((current) => ({ ...current, background: null }))}
              />
            </RibbonGroup>
            <RibbonGroup label={t("impress.headerFooter")}>
              <ToolButton
                icon={<Rows3 size={16} />}
                label={t("impress.headerFooter")}
                onClick={() => setFooterDialog(true)}
              />
            </RibbonGroup>
          </>
        ) : null}

        {ribbon === "transitions" ? (
          <RibbonGroup label={t("impress.transition")}>
            <ToolSelect
              value={slide.transition ?? "none"}
              onChange={(transition) =>
                updateSlide((current) => ({ ...current, transition: transition === "none" ? null : transition }))
              }
              options={[
                { value: "none", label: t("impress.transitionNone") },
                { value: "fade", label: t("impress.transitionFade") },
                { value: "slide", label: t("impress.transitionSlide") },
                { value: "push", label: t("impress.transitionPush") },
                { value: "wipe", label: t("impress.transitionWipe") },
              ]}
              width={130}
            />
            <ToolNumber
              value={slide.transitionMs}
              onChange={(transitionMs) => updateSlide((current) => ({ ...current, transitionMs }))}
              min={100}
              max={3000}
              step={100}
              title="ms"
              width={80}
            />
            <ToolButton
              label={t("impress.applyToAll")}
              onClick={() =>
                update((current) => ({
                  ...current,
                  slides: current.slides.map((candidate) => ({
                    ...candidate,
                    transition: slide.transition,
                    transitionMs: slide.transitionMs,
                  })),
                }))
              }
            />
          </RibbonGroup>
        ) : null}

        {ribbon === "animations" ? (
          <RibbonGroup label={t("impress.animations")}>
            <ToolButton
              icon={<Sparkles size={16} />}
              label={t("impress.addAnimation")}
              onClick={addAnimation}
              disabled={slide.objects.length === 0}
            />
          </RibbonGroup>
        ) : null}

        {ribbon === "view" ? (
          <>
            <RibbonGroup label={t("impress.view")}>
              <ToolButton
                icon={<Rows3 size={16} />}
                label={t("impress.viewNormal")}
                onClick={() => setView("normal")}
                active={view === "normal"}
              />
              <ToolButton
                icon={<Grid2X2 size={16} />}
                label={t("impress.slideSorter")}
                onClick={() => setView("sorter")}
                active={view === "sorter"}
              />
            </RibbonGroup>
            <RibbonGroup label={t("impress.present")}>
              <ToolButton
                icon={<Play size={16} />}
                label={t("impress.startShow")}
                onClick={() => {
                  setPresenterView(false);
                  goToSlide(slideIndex);
                }}
              />
              <ToolButton
                icon={<MonitorPlay size={16} />}
                label={t("impress.presenter")}
                onClick={() => {
                  setPresenterView(true);
                  goToSlide(slideIndex);
                }}
              />
            </RibbonGroup>
          </>
        ) : null}

        <div className="ribbon-spacer" />
        <RibbonGroup>
          <ToolButton
            icon={<FolderOpen size={16} />}
            label={t("common.open")}
            onClick={() => void openIntoWorkspace()}
          />
          <ToolButton
            icon={<Save size={16} />}
            label={t("common.save")}
            onClick={() => void session.save()}
            disabled={session.busy}
          />
          <ToolButton label={t("common.saveAs")} onClick={() => void session.saveAs()} disabled={session.busy} />
          <ToolButton
            icon={<FileDown size={16} />}
            label={t("writer.exportPdf")}
            onClick={() => void session.exportPdf()}
          />
          <ToolButton icon={<Printer size={16} />} label={t("common.print")} onClick={() => void session.print()} />
        </RibbonGroup>
      </Ribbon>

      <div className={`impress-layout${view === "sorter" ? " is-sorter" : ""}`}>
        {view === "sorter" ? (
          <div className="slide-sorter">
            {deck.slides.map((candidate, index) => (
              <div
                key={candidate.id}
                className={`sorter-item${index === slideIndex ? " is-active" : ""}${
                  candidate.hidden ? " is-hidden" : ""
                }${dropIndex === index ? " is-drop" : ""}`}
                data-slide-index={index}
                role="button"
                tabIndex={0}
                aria-label={`${t("impress.slide")} ${index + 1}`}
                onPointerDown={(event) => {
                  event.stopPropagation();
                  setSlideIndex(index);
                  setSelected([]);
                  sorterDragRef.current = index;
                  dropIndexRef.current = index;
                  setDropIndex(index);
                }}
                onPointerEnter={() => {
                  if (sorterDragRef.current === null) return;
                  dropIndexRef.current = index;
                  setDropIndex(index);
                }}
                onKeyDown={(event) => {
                  if (event.key === "Enter" || event.key === " ") {
                    event.preventDefault();
                    setSlideIndex(index);
                    setSelected([]);
                  }
                }}
              >
                <span className="slide-number">{index + 1}</span>
                {candidate.hidden ? (
                  <span className="slide-hidden-badge" title={t("impress.hiddenSlide")}>
                    {t("impress.hiddenSlide")}
                  </span>
                ) : null}
                <SlidePreview deck={deck} slide={candidate} theme={theme} width={210} />
              </div>
            ))}
            <button type="button" className="btn btn-soft slide-add" onClick={addSlide}>
              <Plus size={14} /> {t("impress.newSlide")}
            </button>
          </div>
        ) : (
          <>
            <div className="slide-list">
              {deck.slides.map((candidate, index) => (
                <div
                  key={candidate.id}
                  className={`slide-thumb${index === slideIndex ? " is-active" : ""}${
                    candidate.hidden ? " is-hidden" : ""
                  }`}
                  role="button"
                  tabIndex={0}
                  aria-label={`${t("impress.slide")} ${index + 1}`}
                  onClick={() => {
                    setSlideIndex(index);
                    setSelected([]);
                  }}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" || event.key === " ") {
                      event.preventDefault();
                      setSlideIndex(index);
                      setSelected([]);
                    }
                  }}
                >
                  <span className="slide-number">{index + 1}</span>
                  {candidate.hidden ? (
                    <span className="slide-hidden-badge" title={t("impress.hiddenSlide")}>
                      {t("impress.hiddenSlide")}
                    </span>
                  ) : null}
                  <SlidePreview deck={deck} slide={candidate} theme={theme} width={148} />
                  <div className="slide-thumb-actions">
                    <button
                      type="button"
                      className="icon-btn"
                      onClick={(event) => {
                        event.stopPropagation();
                        moveSlide(index, index - 1);
                      }}
                      title={t("common.moveUp")}
                    >
                      ↑
                    </button>
                    <button
                      type="button"
                      className="icon-btn"
                      onClick={(event) => {
                        event.stopPropagation();
                        moveSlide(index, index + 1);
                      }}
                      title={t("common.moveDown")}
                    >
                      ↓
                    </button>
                  </div>
                </div>
              ))}
              <button type="button" className="btn btn-soft slide-add" onClick={addSlide}>
                <Plus size={14} /> {t("impress.newSlide")}
              </button>
            </div>

            <div
              className="slide-stage"
              ref={canvasRef}
              role="presentation"
              onPointerDown={(event) => {
                if (event.target === event.currentTarget) setSelected([]);
              }}
            >
              <div
                className="slide-canvas"
                role="presentation"
                style={{
                  width: deck.size.widthPt * scale,
                  height: deck.size.heightPt * scale,
                  background: slide.background ?? theme.background,
                }}
                onClick={(event) => event.stopPropagation()}
                onPointerDown={beginMarquee}
              >
                {marquee ? (
                  <div
                    className="slide-marquee"
                    style={{
                      left: marquee.x * scale,
                      top: marquee.y * scale,
                      width: marquee.w * scale,
                      height: marquee.h * scale,
                    }}
                  />
                ) : null}
                {inherited.map((object) => (
                  <div
                    key={object.id}
                    className="slide-object is-inherited"
                    style={{
                      left: object.x * scale,
                      top: object.y * scale,
                      width: object.w * scale,
                      height: object.h * scale,
                      transform: `rotate(${object.rotation}deg)`,
                      zIndex: 0,
                      pointerEvents: "none",
                    }}
                  >
                    <ObjectTree
                      object={object}
                      path={[object.id]}
                      depth={0}
                      theme={theme}
                      scale={scale}
                      selectedKeys={INHERITED_KEYS}
                      editingKey={null}
                      interactive={false}
                    />
                  </div>
                ))}
                {[...slide.objects]
                  .sort((a, b) => a.z - b.z)
                  .map((object) => {
                    const path: SelectionPath = [object.id];
                    const isSelected = selectedKeys.has(pathKey(path));
                    const isCropTarget = cropPath !== null && pathKey(cropPath) === pathKey(path);
                    // While a crop drag runs, the canvas previews the draft crop.
                    const rendered =
                      isCropTarget && cropDraft && object.image
                        ? { ...object, image: { ...object.image, crop: cropDraft } }
                        : object;
                    return (
                      <div
                        key={object.id}
                        className={`slide-object${isSelected ? " is-selected" : ""}${
                          connectorStart === object.id ? " is-connector-start" : ""
                        }`}
                        style={{
                          left: rendered.x * scale,
                          top: rendered.y * scale,
                          width: rendered.w * scale,
                          height: rendered.h * scale,
                          transform: `rotate(${rendered.rotation}deg)`,
                          zIndex: rendered.z,
                        }}
                        onPointerDown={(event) => handleObjectPointerDown(event, path)}
                        onDoubleClick={(event) => handleObjectDoubleClick(event, path)}
                      >
                        <ObjectTree
                          object={rendered}
                          objects={slide.objects}
                          path={path}
                          depth={0}
                          theme={theme}
                          scale={scale}
                          selectedKeys={selectedKeys}
                          editingKey={editingText}
                          interactive
                          onObjectPointerDown={handleObjectPointerDown}
                          onObjectDoubleClick={handleObjectDoubleClick}
                          onHandlePointerDown={beginDrag}
                          onTextCommit={handleTextCommit}
                          onTextDone={() => {
                            setEditingText(null);
                            setTextSelection(null);
                          }}
                          onTextSelection={setTextSelection}
                          textEditRef={textHandleRef}
                        />
                        {isSelected ? (
                          <SelectionHandles onHandlePointerDown={(event, mode) => beginDrag(event, path, mode)} />
                        ) : null}
                        {isCropTarget ? <CropHandles onHandlePointerDown={beginCropDrag} /> : null}
                      </div>
                    );
                  })}
                <SlideFooterBar
                  footer={deck.footer}
                  slideNumber={slideIndex + 1}
                  scale={scale}
                  color={theme.bodyColor}
                />
              </div>
            </div>

            <div className="slide-properties">
              <h4>{t("impress.properties")}</h4>
              <div className="stack">
                <label className="field">
                  <span>{t("impress.master")}</span>
                  <select
                    value={slide.masterId ?? masters[0]?.id ?? ""}
                    onChange={(event) => selectMaster(event.target.value)}
                  >
                    {masters.length === 0 ? <option value="">{t("impress.noMaster")}</option> : null}
                    {masters.map((master) => (
                      <option key={master.id} value={master.id}>
                        {master.name}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="field">
                  <span>{t("impress.layout")}</span>
                  <select value={slide.layoutId ?? ""} onChange={(event) => selectLayout(event.target.value)}>
                    <option value="">{t("impress.noLayout")}</option>
                    {(selectedMaster?.layouts ?? []).map((layout) => (
                      <option key={layout.id} value={layout.id}>
                        {layout.name}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="field">
                  <span>{t("impress.hiddenSlide")}</span>
                  <input
                    type="checkbox"
                    checked={Boolean(slide.hidden)}
                    onChange={(event) => updateSlide((current) => ({ ...current, hidden: event.target.checked }))}
                  />
                </label>
              </div>
              {primary && primaryPath ? (
                <div className="stack">
                  <div className="row">
                    <ToolNumber
                      value={Math.round(primary.x)}
                      onChange={(x) => setPrimaryPosition("x", x)}
                      title="X"
                      width={64}
                    />
                    <ToolNumber
                      value={Math.round(primary.y)}
                      onChange={(y) => setPrimaryPosition("y", y)}
                      title="Y"
                      width={64}
                    />
                  </div>
                  <div className="row">
                    <ToolNumber
                      value={Math.round(primary.w)}
                      onChange={(w) => setPrimarySize("w", w)}
                      title="W"
                      width={64}
                    />
                    <ToolNumber
                      value={Math.round(primary.h)}
                      onChange={(h) => setPrimarySize("h", h)}
                      title="H"
                      width={64}
                    />
                  </div>
                  <div className="row">
                    <ToolNumber
                      value={Math.round(primary.rotation)}
                      onChange={(rotation) => updatePath(primaryPath, { rotation })}
                      min={-180}
                      max={180}
                      title={t("impress.rotation")}
                      width={64}
                    />
                    <ToolNumber
                      value={primary.z}
                      onChange={(z) => updatePath(primaryPath, { z })}
                      min={1}
                      max={99}
                      title="Z"
                      width={64}
                    />
                  </div>
                  {primary.kind === "rect" || primary.kind === "ellipse" || primary.kind === "roundRect" ? (
                    <>
                      <label className="field">
                        <span>{t("impress.fill")}</span>
                        <input
                          type="color"
                          value={primary.style?.fill ?? "#2563eb"}
                          onChange={(event) =>
                            updatePath(primaryPath, {
                              style: { ...(primary.style ?? defaultShapeStyle()), fill: event.target.value },
                            })
                          }
                        />
                      </label>
                      <label className="field">
                        <span>{t("impress.cornerRadius")}</span>
                        <input
                          type="number"
                          value={primary.style?.cornerRadiusPt ?? 0}
                          onChange={(event) =>
                            updatePath(primaryPath, {
                              style: {
                                ...(primary.style ?? defaultShapeStyle()),
                                cornerRadiusPt: Number(event.target.value),
                              },
                            })
                          }
                        />
                      </label>
                    </>
                  ) : null}
                  {primary.text ? (
                    <label className="field">
                      <span>{t("impress.fontSize")}</span>
                      <input
                        type="number"
                        value={primary.text.paragraphs[0]?.sizePt ?? primary.text.sizePt ?? 18}
                        onChange={(event) =>
                          updatePath(primaryPath, { text: withFrameSize(primary.text!, Number(event.target.value)) })
                        }
                      />
                    </label>
                  ) : null}
                  {primary.kind === "image" ? (
                    <div className="row">
                      <ToolButton
                        icon={<Crop size={14} />}
                        label={t("impress.crop")}
                        active={cropPath !== null}
                        onClick={() => {
                          if (cropPath) {
                            setCropPath(null);
                            setCropDraft(null);
                            return;
                          }
                          setCropPath(primaryPath);
                          setCropDraft(clampCrop(primary.image?.crop));
                        }}
                      />
                      {primary.image?.crop ? (
                        <ToolButton
                          label={t("impress.resetCrop")}
                          onClick={() => updatePath(primaryPath, { image: { ...primary.image!, crop: null } })}
                        />
                      ) : null}
                    </div>
                  ) : null}
                  {editingText ? (
                    <div className="slide-text-tools" data-keep-text-edit>
                      <div className="row">
                        <ToolButton
                          keepFocus
                          icon={<Bold size={14} />}
                          title={t("impress.bold")}
                          active={Boolean(textSelection?.bold)}
                          onClick={() => textHandleRef.current?.applyFormat({ bold: !textSelection?.bold })}
                        />
                        <ToolButton
                          keepFocus
                          icon={<Italic size={14} />}
                          title={t("impress.italic")}
                          active={Boolean(textSelection?.italic)}
                          onClick={() => textHandleRef.current?.applyFormat({ italic: !textSelection?.italic })}
                        />
                        <ToolButton
                          keepFocus
                          icon={<Underline size={14} />}
                          title={t("impress.underline")}
                          active={Boolean(textSelection?.underline)}
                          onClick={() => textHandleRef.current?.applyFormat({ underline: !textSelection?.underline })}
                        />
                        <ToolButton
                          keepFocus
                          icon={<List size={14} />}
                          title={t("impress.bullets")}
                          active={Boolean(textSelection?.bullet)}
                          onClick={() => textHandleRef.current?.toggleBullet()}
                        />
                      </div>
                      <div className="row">
                        <ToolButton
                          keepFocus
                          icon={<IndentDecrease size={14} />}
                          title={t("impress.decreaseLevel")}
                          onClick={() => textHandleRef.current?.changeLevel(-1)}
                        />
                        <ToolButton
                          keepFocus
                          icon={<IndentIncrease size={14} />}
                          title={t("impress.increaseLevel")}
                          onClick={() => textHandleRef.current?.changeLevel(1)}
                        />
                        <ToolNumber
                          value={
                            textSelection?.sizePt ?? primary.text!.paragraphs[0]?.sizePt ?? primary.text!.sizePt ?? 18
                          }
                          onChange={(sizePt) => textHandleRef.current?.applyFormat({ sizePt })}
                          min={4}
                          max={400}
                          title={t("impress.fontSize")}
                          width={64}
                        />
                      </div>
                      <div className="row slide-text-colors">
                        {TEXT_COLORS.map((color) => (
                          <button
                            key={color}
                            type="button"
                            className="color-swatch"
                            style={{ background: color }}
                            title={color}
                            aria-label={color}
                            onMouseDown={(event) => event.preventDefault()}
                            onClick={() => textHandleRef.current?.applyFormat({ color })}
                          />
                        ))}
                        <input
                          type="color"
                          className="tool-color"
                          title={t("impress.textColor")}
                          onMouseDown={(event) => event.preventDefault()}
                          onChange={(event) => textHandleRef.current?.applyFormat({ color: event.target.value })}
                        />
                      </div>
                    </div>
                  ) : null}
                  <label className="field">
                    <span>{t("writer.paragraph")}</span>
                    <select
                      value={primary.text?.paragraphs[0]?.align ?? "left"}
                      onChange={(event) =>
                        updatePath(primaryPath, {
                          text: primary.text ? withFrameAlign(primary.text, event.target.value) : null,
                        })
                      }
                    >
                      <option value="left">{t("writer.alignLeft")}</option>
                      <option value="center">{t("writer.alignCenter")}</option>
                      <option value="right">{t("writer.alignRight")}</option>
                    </select>
                  </label>
                  {primary.kind === "chart" ? (
                    <ToolButton
                      icon={<LineChart size={14} />}
                      label={t("impress.chartData")}
                      onClick={() => setChartPath(primaryPath)}
                    />
                  ) : null}
                  <ToolButton
                    label={t("impress.editText")}
                    onClick={() =>
                      primary.text
                        ? setEditingText(pathKey(primaryPath))
                        : updatePath(primaryPath, {
                            text: {
                              paragraphs: [
                                {
                                  text: "New text",
                                  level: 0,
                                  bold: false,
                                  italic: false,
                                  underline: false,
                                  sizePt: 20,
                                  color: null,
                                  align: "left",
                                  bullet: false,
                                  runs: [],
                                },
                              ],
                              valign: "top",
                              font: null,
                              sizePt: 20,
                              color: null,
                              align: "left",
                            },
                          })
                    }
                  />
                </div>
              ) : (
                <p className="muted">{t("impress.noSelection")}</p>
              )}
              <h4>{t("impress.notes")}</h4>
              <textarea
                className="notes-input"
                value={slide.notes}
                onChange={(event) => updateSlide((current) => ({ ...current, notes: event.target.value }))}
                placeholder={t("impress.notesHint")}
              />
              <h4>{t("impress.animations")}</h4>
              <div className="stack">
                {[...(slide.animations ?? [])]
                  .sort((a, b) => a.order - b.order)
                  .map((animation, index, list) => {
                    const target = slide.objects.find((object) => object.id === animation.objectId);
                    return (
                      <div key={animation.id} className="row" style={{ alignItems: "center", gap: 4 }}>
                        <span
                          className="muted"
                          style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}
                        >
                          {target?.name ?? t("impress.animationMissingObject")} · {animation.kind} · {animation.effect}{" "}
                          · {animation.trigger}
                        </span>
                        <button
                          type="button"
                          className="icon-btn"
                          onClick={() => reorderAnimation(animation.id, -1)}
                          disabled={index === 0}
                          title={t("impress.moveUp")}
                        >
                          ↑
                        </button>
                        <button
                          type="button"
                          className="icon-btn"
                          onClick={() => reorderAnimation(animation.id, 1)}
                          disabled={index === list.length - 1}
                          title={t("impress.moveDown")}
                        >
                          ↓
                        </button>
                        <button
                          type="button"
                          className="icon-btn"
                          onClick={() => setAnimationEditing(animation)}
                          title={t("common.edit")}
                        >
                          ✎
                        </button>
                        <button
                          type="button"
                          className="icon-btn"
                          onClick={() =>
                            updateSlide((current) => ({
                              ...current,
                              animations: (current.animations ?? []).filter(
                                (candidate) => candidate.id !== animation.id,
                              ),
                            }))
                          }
                          title={t("common.delete")}
                        >
                          ×
                        </button>
                      </div>
                    );
                  })}
                <ToolButton
                  icon={<Sparkles size={14} />}
                  label={t("impress.addAnimation")}
                  onClick={addAnimation}
                  disabled={slide.objects.length === 0}
                />
              </div>
            </div>
          </>
        )}
      </div>

      <div className="editor-status">
        <span>
          {slideIndex + 1} / {deck.slides.length} {t("impress.slides")}
        </span>
        <span className="spacer" />
        <span>
          {documentLocationLabel(tab.path) ?? t("writer.unsaved")} {tab.dirty ? "•" : ""}
        </span>
      </div>

      {slideshow !== null && showSlide ? (
        <div
          className="slideshow"
          role="presentation"
          onClick={(event) => {
            // Clicks inside the presenter control panel must not advance the
            // show to the next slide, and a drawing tool owns the click.
            if (event.target instanceof Element && event.target.closest(".presenter-panel")) return;
            if (showTool !== "pointer") return;
            advanceShow();
          }}
          style={
            presenterView
              ? {
                  display: "flex",
                  flexDirection: "column",
                  alignItems: "stretch",
                  justifyContent: "flex-start",
                  padding: 12,
                  gap: 8,
                }
              : undefined
          }
        >
          {presenterView ? (
            <div
              style={{
                display: "flex",
                gap: 12,
                flex: 1,
                minHeight: 0,
                alignItems: "stretch",
                justifyContent: "center",
                width: "100%",
              }}
            >
              {slideshowSlideArea}
              {presenterPanel}
            </div>
          ) : (
            slideshowSlideArea
          )}
          {slideshowNav}
        </div>
      ) : null}

      {outlineDialog ? (
        <ImpressAiDialog
          docId={tab.id}
          status={aiStatus}
          onAccept={addOutlineSlides}
          onClose={() => setOutlineDialog(false)}
        />
      ) : null}

      {masterDialog ? (
        <MasterDialog
          deck={deck}
          slide={slide}
          update={update}
          onClose={() => setMasterDialog(false)}
          onUseLayout={(masterId, layout) => {
            updateSlide((current) => ({ ...current, masterId, layoutId: layout.id, layout: layout.kind }));
          }}
        />
      ) : null}

      {footerDialog ? (
        <FooterDialog
          footer={deck.footer}
          onChange={(footer) => update((current) => ({ ...current, footer }), false)}
          onClose={() => setFooterDialog(false)}
        />
      ) : null}

      {chartPath ? (
        <ChartDialog
          key={pathKey(chartPath)}
          chart={objectAtPath(slide.objects, chartPath)?.chart ?? emptyChart()}
          theme={theme}
          onClose={() => setChartPath(null)}
          onSave={(chart) => {
            updatePath(chartPath, { chart });
            setChartPath(null);
          }}
        />
      ) : null}

      {animationEditing ? (
        <AnimationDialog
          key={animationEditing.id}
          animation={animationEditing}
          objects={slide.objects}
          onClose={() => setAnimationEditing(null)}
          onSave={saveAnimation}
        />
      ) : null}
    </div>
  );

  async function replaceImage(path: SelectionPath) {
    try {
      // The Android system picker replaces the desktop dialog there.
      const picked = await pickFileBytes({
        name: "Images",
        extensions: ["png", "jpg", "jpeg", "gif", "webp", "bmp"],
        mimeTypes: ["image/*"],
      });
      if (!picked) return;
      const { bytes, name } = picked;
      let base64 = "";
      for (let index = 0; index < bytes.length; index += 0x8000)
        base64 += String.fromCharCode(...bytes.subarray(index, index + 0x8000));
      // GIF/WebP/BMP used to be labelled image/png, which mislabels the part
      // in the exported presentation.
      const mime = mimeForName(name).startsWith("image/") ? mimeForName(name) : "image/png";
      updatePath(path, { image: { name, mime, dataBase64: btoa(base64), alt: "" }, kind: "image" });
    } catch (error) {
      reportError(error, t);
    }
  }
}

export { SlidePreview } from "./impress/slide-view";
