/**
 * Impress editor: slide thumbnails, drag/resize/rotate canvas, properties
 * panel, notes, layouts, themes and transitions, plus a real slideshow.
 *
 * V3 adds masters/layouts (inherited placeholders), real shape groups with
 * nested children, schematic chart previews, per-object animations and a
 * presenter view. The pure helpers below (grouping, animation ordering) are
 * exported so they can be unit tested without rendering the editor.
 */
import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import {
  AlignCenterHorizontal,
  AlignEndHorizontal,
  AlignStartHorizontal,
  ArrowRight,
  Braces,
  Circle,
  Copy,
  FileDown,
  FolderOpen,
  Group,
  Image as ImageIcon,
  LayoutTemplate,
  LineChart,
  Minus,
  MonitorPlay,
  Move,
  Play,
  Plus,
  Printer,
  Redo2,
  RotateCw,
  Save,
  Sparkles,
  Square,
  Table as TableIcon,
  Trash2,
  Type,
  Undo2,
  Ungroup,
} from "lucide-react";
import type { OfficeTab } from "../lib/office-store";
import { useOfficeTabs } from "../lib/office-store";
import type { Animation, ChartData, Deck, Slide, SlideLayout, SlideObject } from "../lib/office-types";
import { newAnimation, newSlideMaster, uid, type ShapeStyle } from "../lib/office-types";
import { useT } from "../lib/i18n";
import { reportError } from "../lib/store";
import { Dialog, Ribbon, RibbonGroup, TextField, ToolButton, ToolColor, ToolNumber, ToolSelect } from "./office-ui";
import { openIntoWorkspace, useEditorShortcuts, useOfficeSession } from "./useOfficeSession";

type ImpressTab = OfficeTab & { model: Deck };
type Theme = (typeof THEMES)[number];
/** Path of object ids from the slide root; deeper paths are children of groups. */
export type SelectionPath = string[];

/** Animations that start together, with how their start time is anchored. */
export interface AnimationStep {
  animations: Animation[];
  waitForClick: boolean;
  relativeTo: "start" | "previousStart" | "previousEnd";
  durationMs: number;
}

export interface RunningAnimation {
  effect: Animation;
  phase: "to" | "back";
}

export type AnimationRunState = Record<string, RunningAnimation>;
export type AnimationDoneState = Record<string, boolean>;

const MAX_GROUP_DEPTH = 8;
const INHERITED_KEYS: Set<string> = new Set();
const CHART_PALETTE = ["#2563EB", "#F97316", "#10B981", "#8B5CF6", "#EF4444", "#14B8A6"];

export const ANIMATION_EFFECTS: Record<string, string[]> = {
  entrance: ["appear", "fade", "flyIn", "zoom"],
  emphasis: ["pulse", "spin", "grow", "shrink"],
  exit: ["disappear", "fadeOut", "flyOut"],
};

const THEMES: Array<{ id: string; name: string; background: string; title: string; body: string; accent: string; titleColor: string; bodyColor: string }> = [
  { id: "minimal", name: "Minimal", background: "#FFFFFF", title: "Segoe UI", body: "Segoe UI", accent: "#2563EB", titleColor: "#111827", bodyColor: "#334155" },
  { id: "business", name: "Business", background: "#F8FAFC", title: "Segoe UI", body: "Segoe UI", accent: "#1D4ED8", titleColor: "#0F172A", bodyColor: "#334155" },
  { id: "dark", name: "Dark", background: "#0F172A", title: "Segoe UI", body: "Segoe UI", accent: "#60A5FA", titleColor: "#F8FAFC", bodyColor: "#CBD5E1" },
  { id: "modern", name: "Modern", background: "#FFFFFF", title: "Segoe UI", body: "Segoe UI", accent: "#14B8A6", titleColor: "#0B1220", bodyColor: "#334155" },
  { id: "education", name: "Education", background: "#FFFDF5", title: "Georgia", body: "Georgia", accent: "#B45309", titleColor: "#1E293B", bodyColor: "#44403C" },
  { id: "simple", name: "Simple", background: "#FFFFFF", title: "Arial", body: "Arial", accent: "#444444", titleColor: "#111111", bodyColor: "#444444" },
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
    build: () => [textObject("Click to add a title", 80, 60, 800, 80, 32, "left"), bulletObject("Click to add text", 80, 180, 800, 280)],
  },
  {
    id: "twoColumns",
    name: "Two columns",
    build: () => [textObject("Title", 80, 50, 800, 70, 30, "left"), bulletObject("Left column", 80, 160, 380, 300), bulletObject("Right column", 500, 160, 380, 300)],
  },
  {
    id: "imageText",
    name: "Image + text",
    build: () => [textObject("Title", 80, 50, 800, 70, 30, "left"), { ...textObject("Add an image", 80, 160, 400, 300, 16, "center") }, bulletObject("Describe the image", 510, 170, 370, 280)],
  },
  {
    id: "section",
    name: "Section header",
    build: () => [textObject("Section title", 80, 220, 800, 100, 36, "center")],
  },
  { id: "blank", name: "Blank", build: () => [] },
  { id: "quote", name: "Quote", build: () => [textObject("“An important quote goes here.”", 140, 180, 680, 180, 28, "center")] },
  {
    id: "comparison",
    name: "Comparison",
    build: () => [textObject("Option A", 80, 80, 380, 60, 24, "left"), textObject("Option B", 500, 80, 380, 60, 24, "left"), bulletObject("Details", 80, 170, 380, 280), bulletObject("Details", 500, 170, 380, 280)],
  },
];

function textObject(text: string, x: number, y: number, w: number, h: number, size: number, align: string): SlideObject {
  return {
    id: uid(),
    kind: "text",
    x,
    y,
    w,
    h,
    rotation: 0,
    z: 1,
    text: { paragraphs: [{ text, level: 0, bold: size >= 30, italic: false, underline: false, sizePt: size, color: null, align, bullet: false, runs: [] }], valign: "top", font: null, sizePt: size, color: null, align },
    image: null,
    style: null,
    line: null,
    table: null,
    chart: null,
    groupId: null,
    name: "Text",
  };
}

function bulletObject(text: string, x: number, y: number, w: number, h: number): SlideObject {
  const object = textObject(text, x, y, w, h, 20, "left");
  object.text!.paragraphs[0].bullet = true;
  object.name = "Bullets";
  return object;
}

function emptyChart(): ChartData {
  return { kind: "column", title: "Chart", categories: "", series: [], legend: true, xTitle: "", yTitle: "", stacked: false, showLabels: false, categoriesCache: [], seriesValuesCache: [] };
}

// ---------------------------------------------------------------------------
// Chart data helpers
//
// A presentation carries no workbook, so a chart that only has ranges cannot
// render outside the editor. The chart dialog therefore edits the V3.1 caches
// (`categoriesCache` / `seriesValuesCache`), which the PPTX exporter writes as
// `c:strCache` / `c:numCache` plus an embedded Excel workbook. The pure helpers
// below are exported for unit tests; the dialog binds them to the grid.
// ---------------------------------------------------------------------------

/** Sheet name the Rust exporter uses for the embedded chart workbook. */
const CHART_DATA_SHEET = "Sheet1";

/** Parses one cell as a locale-independent decimal; anything else is null. */
export function parseChartCellNumber(text: string): number | null {
  const trimmed = text.trim();
  if (!/^[+-]?(\d+(\.\d+)?|\.\d+)([eE][+-]?\d+)?$/.test(trimmed)) return null;
  const value = Number(trimmed);
  return Number.isFinite(value) ? value : null;
}

/** Text a cached number is edited as: plain decimal, no locale formatting. */
export function formatChartCellValue(value: number): string {
  return String(value);
}

/**
 * Splits pasted text into rows of cells. A row that contains a tab is split on
 * tabs only, so comma decimals inside spreadsheet exports are kept together;
 * otherwise commas are the separator. Line endings may be CRLF, CR or LF and a
 * single trailing newline is ignored. Blank rows are kept so pasted columns
 * stay aligned with the row they start at.
 */
export function parseChartClipboard(text: string): string[][] {
  const lines = text.replace(/\r\n?/g, "\n").split("\n");
  while (lines.length > 0 && lines[lines.length - 1] === "") lines.pop();
  return lines.map((line) => line.split(line.includes("\t") ? "\t" : ",").map((cell) => cell.trim()));
}

/**
 * A1 range for one column of the embedded workbook the exporter builds:
 * row 1 is the header, so the first cached value lives on row 2.
 */
export function chartWorkbookRange(columnIndex: number, rowCount: number): string {
  let value = columnIndex + 1;
  let letters = "";
  while (value > 0) {
    const remainder = (value - 1) % 26;
    letters = String.fromCharCode(65 + remainder) + letters;
    value = Math.floor((value - 1) / 26);
  }
  return `${CHART_DATA_SHEET}!$${letters}$2:$${letters}$${rowCount + 1}`;
}

/** True when any chart cache carries data; false for range-only charts. */
export function chartHasCachedData(chart: ChartData): boolean {
  return (chart.categoriesCache ?? []).length > 0 || (chart.seriesValuesCache ?? []).some((values) => values.length > 0);
}

/** Display rows: enough for every cache plus at least one empty row. */
export function chartDataRowCount(chart: ChartData): number {
  const valueRows = (chart.seriesValuesCache ?? []).reduce((max, values) => Math.max(max, values.length), 0);
  return Math.max(1, (chart.categoriesCache ?? []).length, valueRows);
}

/** One grid cell: row index and column index (0 is the category column). */
export interface ChartGridCell {
  row: number;
  column: number;
}

/**
 * Deep clone for the dialog draft plus cache arrays aligned with `series`, so
 * grid edits never mutate the document until Save is pressed.
 */
export function normalizeChartDraft(chart: ChartData): ChartData {
  const clone = JSON.parse(JSON.stringify(chart)) as ChartData;
  clone.categoriesCache = [...(chart.categoriesCache ?? [])];
  clone.seriesValuesCache = chart.series.map((_, index) => [...(chart.seriesValuesCache?.[index] ?? [])]);
  return clone;
}

/**
 * Writes a parsed clipboard block into the caches starting at `target`.
 * Column 0 holds category labels (any text), columns 1..n the series values;
 * cells that do not parse as numbers are skipped, mirroring the rule that
 * invalid cell input is never committed. Leading rows beyond the current
 * series length are padded with 0 because the cache is a dense list.
 */
export function mergeChartPaste(chart: ChartData, target: ChartGridCell, block: string[][]): ChartData {
  const categories = [...(chart.categoriesCache ?? [])];
  const seriesValues = (chart.seriesValuesCache ?? []).map((values) => [...values]);
  block.forEach((line, rowOffset) => {
    line.forEach((cell, columnOffset) => {
      const row = target.row + rowOffset;
      const column = target.column + columnOffset;
      if (column === 0) {
        while (categories.length <= row) categories.push("");
        categories[row] = cell;
        return;
      }
      const values = seriesValues[column - 1];
      if (!values) return;
      const parsed = parseChartCellNumber(cell);
      if (parsed === null) return;
      while (values.length < row) values.push(0);
      values[row] = parsed;
    });
  });
  while (categories.length > 0 && categories[categories.length - 1] === "") categories.pop();
  return { ...chart, categoriesCache: categories, seriesValuesCache: seriesValues };
}

/**
 * Rewrites the range fields to point at the embedded workbook the PPTX
 * exporter builds from the caches: category labels in column A, series 1..n in
 * B, C, ... This makes the exported chart self-consistent ("ranges that
 * reference nothing" become real). Called only when the user edited data in
 * this dialog session, so untouched imported charts keep their ranges.
 */
export function syncChartDataRanges(chart: ChartData): ChartData {
  const categories = chart.categoriesCache ?? [];
  const values = chart.seriesValuesCache ?? [];
  const dataRows = Math.max(categories.length, ...values.map((list) => list.length));
  if (dataRows === 0) return chart;
  return {
    ...chart,
    categories: chartWorkbookRange(0, categories.length > 0 ? categories.length : dataRows),
    series: chart.series.map((entry, index) => {
      const length = values[index]?.length ?? 0;
      return length > 0 ? { ...entry, range: chartWorkbookRange(index + 1, length) } : entry;
    }),
  };
}

// ---------------------------------------------------------------------------
// Pure helpers: paths, groups, inheritance, animation ordering
// ---------------------------------------------------------------------------

export function pathKey(path: SelectionPath): string {
  return path.join("/");
}

export function isInheritedId(id: string): boolean {
  return id.startsWith("master:") || id.startsWith("layout:");
}

/** Deep clone that gives every object (and group child) a fresh id. */
function cloneObject(object: SlideObject): SlideObject {
  return {
    ...(JSON.parse(JSON.stringify(object)) as SlideObject),
    id: uid(),
    children: object.children?.map(cloneObject),
  };
}

/** Clone a list of objects and report the old to new id mapping. */
function cloneObjects(objects: SlideObject[]): { objects: SlideObject[]; idMap: Map<string, string> } {
  const idMap = new Map<string, string>();
  const clone = (list: SlideObject[]): SlideObject[] =>
    list.map((object) => {
      const id = uid();
      idMap.set(object.id, id);
      return {
        ...(JSON.parse(JSON.stringify(object)) as SlideObject),
        id,
        children: object.children ? clone(object.children) : undefined,
      };
    });
  return { objects: clone(objects), idMap };
}

/** Bounding box of a set of objects (rotation is ignored, as in PowerPoint). */
export function objectBounds(objects: SlideObject[]): { x: number; y: number; w: number; h: number } {
  if (objects.length === 0) return { x: 0, y: 0, w: 0, h: 0 };
  const left = Math.min(...objects.map((object) => object.x));
  const top = Math.min(...objects.map((object) => object.y));
  const right = Math.max(...objects.map((object) => object.x + object.w));
  const bottom = Math.max(...objects.map((object) => object.y + object.h));
  return { x: left, y: top, w: Math.max(1, right - left), h: Math.max(1, bottom - top) };
}

export function objectAtPath(objects: SlideObject[], path: SelectionPath): SlideObject | undefined {
  let list = objects;
  let found: SlideObject | undefined;
  for (const id of path) {
    found = list.find((object) => object.id === id);
    if (!found) return undefined;
    list = found.children ?? [];
  }
  return found;
}

export function replaceObjectAtPath(objects: SlideObject[], path: SelectionPath, patch: Partial<SlideObject>): SlideObject[] {
  const [head, ...rest] = path;
  return objects.map((object) => {
    if (object.id !== head) return object;
    if (rest.length === 0) return { ...object, ...patch };
    return { ...object, children: object.children ? replaceObjectAtPath(object.children, rest, patch) : object.children };
  });
}

function removeAtPath(objects: SlideObject[], path: SelectionPath): SlideObject[] {
  const [head, ...rest] = path;
  if (rest.length === 0) return objects.filter((object) => object.id !== head);
  return objects.map((object) => (object.id === head && object.children ? { ...object, children: removeAtPath(object.children, rest) } : object));
}

/** Translate an object and every group child by the same delta. */
export function translateObject(object: SlideObject, dx: number, dy: number): SlideObject {
  return {
    ...object,
    x: object.x + dx,
    y: object.y + dy,
    children: object.children?.map((child) => translateObject(child, dx, dy)),
  };
}

/** Scale an object and every group child about an origin (absolute child coords). */
export function scaleObject(object: SlideObject, sx: number, sy: number, originX: number, originY: number): SlideObject {
  return {
    ...object,
    x: originX + (object.x - originX) * sx,
    y: originY + (object.y - originY) * sy,
    w: Math.max(4, object.w * sx),
    h: Math.max(4, object.h * sy),
    children: object.children?.map((child) => scaleObject(child, sx, sy, originX, originY)),
  };
}

/** Recompute every group's bounding box from its children (deepest first). */
export function refreshGroupBounds(objects: SlideObject[]): SlideObject[] {
  return objects.map((object) => {
    if (object.kind !== "group" || !object.children || object.children.length === 0) return object;
    const children = refreshGroupBounds(object.children);
    return { ...object, ...objectBounds(children), children };
  });
}

/**
 * Group top-level objects into a real `{ kind: "group", children }` hierarchy.
 * Child coordinates stay absolute; the group's box is their bounding box.
 */
export function groupSelection(objects: SlideObject[], ids: string[], groupId = uid()): SlideObject[] {
  const selected = new Set(ids);
  const members = objects.filter((object) => selected.has(object.id)).sort((a, b) => a.z - b.z);
  if (members.length < 2) return objects;
  const bounds = objectBounds(members);
  const before = objects.findIndex((object) => selected.has(object.id));
  const insertAt = objects.slice(0, before < 0 ? objects.length : before).filter((object) => !selected.has(object.id)).length;
  const group: SlideObject = {
    id: groupId,
    kind: "group",
    x: bounds.x,
    y: bounds.y,
    w: bounds.w,
    h: bounds.h,
    rotation: 0,
    z: 0,
    text: null,
    image: null,
    style: null,
    line: null,
    table: null,
    chart: null,
    groupId: null,
    children: members.map((member, index) => ({ ...(JSON.parse(JSON.stringify(member)) as SlideObject), z: index + 1 })),
    placeholder: null,
    name: "Group",
  };
  const rest = objects.filter((object) => !selected.has(object.id));
  rest.splice(insertAt, 0, group);
  return rest.map((object, index) => ({ ...object, z: index + 1 }));
}

/** Dissolve one level of grouping: selected groups return their children. */
export function ungroupSelection(objects: SlideObject[], ids: string[]): SlideObject[] {
  const selected = new Set(ids);
  const result: SlideObject[] = [];
  for (const object of [...objects].sort((a, b) => a.z - b.z)) {
    if (selected.has(object.id) && object.kind === "group" && object.children && object.children.length > 0) {
      result.push(...[...object.children].sort((a, b) => a.z - b.z).map((child) => ({ ...child, groupId: null })));
    } else {
      result.push(object);
    }
  }
  return result.map((object, index) => ({ ...object, z: index + 1 }));
}

/** Remove selected paths; a selected parent wins over its selected children. */
export function removeSelectionPaths(objects: SlideObject[], paths: SelectionPath[]): SlideObject[] {
  const topLevel = new Set(paths.filter((path) => path.length === 1).map((path) => path[0]));
  let next = objects.filter((object) => !topLevel.has(object.id));
  for (const path of paths.filter((item) => item.length > 1 && !topLevel.has(item[0]))) {
    next = removeAtPath(next, path);
  }
  return refreshGroupBounds(next);
}

/**
 * Master objects followed by layout objects, ids prefixed with `master:` /
 * `layout:` (which marks them non-selectable). A placeholder role that the
 * slide already fills is suppressed, exactly like PowerPoint.
 */
export function inheritedObjects(deck: Deck, slide: Slide): SlideObject[] {
  const masters = deck.masters ?? [];
  if (masters.length === 0) return [];
  const master = masters.find((candidate) => candidate.id === slide.masterId) ?? masters[0];
  const layout = master.layouts.find((candidate) => candidate.id === slide.layoutId) ?? master.layouts.find((candidate) => candidate.kind === slide.layout) ?? null;
  const filled = new Set(slide.objects.map((object) => object.placeholder).filter((role): role is string => Boolean(role)));
  const inherit = (objects: SlideObject[], prefix: string): SlideObject[] =>
    objects
      .filter((object) => !object.placeholder || !filled.has(object.placeholder))
      .sort((a, b) => a.z - b.z)
      .map((object) => ({ ...cloneObject(object), id: `${prefix}:${object.id}` }));
  return [...inherit(master.objects, "master"), ...(layout ? inherit(layout.objects, "layout") : [])];
}

/**
 * Order animations into steps that start together.
 *
 * - `withPrevious` merges into the current step.
 * - `afterPrevious` starts a step when the previous step ends.
 * - `onClick` waits for the presenter to click or press space.
 */
export function animationTimeline(animations: Animation[]): AnimationStep[] {
  const sorted = [...animations].sort((a, b) => a.order - b.order);
  const steps: AnimationStep[] = [];
  for (const animation of sorted) {
    const own = Math.max(0, animation.delayMs) + Math.max(0, animation.durationMs);
    const current = steps[steps.length - 1];
    if (animation.trigger === "withPrevious" && current) {
      current.animations.push(animation);
      current.durationMs = Math.max(current.durationMs, own);
      continue;
    }
    steps.push({
      animations: [animation],
      waitForClick: animation.trigger === "onClick",
      relativeTo: animation.trigger === "afterPrevious" ? "previousEnd" : animation.trigger === "withPrevious" ? "previousStart" : "start",
      durationMs: own,
    });
  }
  return steps;
}

/** The style one animation phase paints on its object (transitions are added by the caller). */
export function animationEffectStyle(kind: string, effect: string, phase: "from" | "to" | "back"): CSSProperties {
  if (phase === "back") return {};
  if (kind === "entrance") {
    if (phase === "from") {
      if (effect === "flyIn") return { opacity: 0, transform: "translateY(40px)" };
      if (effect === "zoom") return { opacity: 0, transform: "scale(0.5)" };
      return { opacity: 0 };
    }
    return {};
  }
  if (kind === "emphasis") {
    if (phase === "to") {
      if (effect === "pulse") return { transform: "scale(1.15)" };
      if (effect === "grow") return { transform: "scale(1.3)" };
      if (effect === "shrink") return { transform: "scale(0.75)" };
      if (effect === "spin") return { transform: "rotate(360deg)" };
    }
    return {};
  }
  if (phase === "to") {
    if (effect === "flyOut") return { opacity: 0, transform: "translateY(-40px)" };
    return { opacity: 0 };
  }
  return {};
}

/** Style for a slideshow object from its animation state (pending/running/finished). */
export function animationObjectStyle(
  objectId: string,
  animations: Animation[],
  running: AnimationRunState,
  done: AnimationDoneState,
): CSSProperties {
  const mine = animations.filter((animation) => animation.objectId === objectId);
  const active = mine.find((animation) => Boolean(running[animation.id]));
  if (active) {
    const state = running[active.id];
    if (state) {
      const ms = active.kind === "emphasis" ? Math.max(0, active.durationMs / 2) : Math.max(0, active.durationMs);
      return { ...animationEffectStyle(active.kind, active.effect, state.phase), transition: `opacity ${ms}ms ease, transform ${ms}ms ease` };
    }
  }
  const entrance = mine.find((animation) => animation.kind === "entrance");
  if (entrance && !done[entrance.id]) return animationEffectStyle("entrance", entrance.effect, "from");
  const exit = mine.find((animation) => animation.kind === "exit");
  if (exit && done[exit.id]) return animationEffectStyle("exit", exit.effect, "to");
  return {};
}

/** mm:ss clock for the presenter view. */
export function formatClock(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const minutes = Math.floor(total / 60);
  const seconds = total % 60;
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

/** Top-level objects whose boxes intersect a marquee rectangle (slide units). */
export function objectsInRect(objects: SlideObject[], rect: { x: number; y: number; w: number; h: number }): SlideObject[] {
  return objects.filter(
    (object) => object.x < rect.x + rect.w && object.x + object.w > rect.x && object.y < rect.y + rect.h && object.y + object.h > rect.y,
  );
}

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
  const [undoStack, setUndoStack] = useState<Deck[]>([]);
  const [redoStack, setRedoStack] = useState<Deck[]>([]);
  const canvasRef = useRef<HTMLDivElement>(null);
  const [canvasWidth, setCanvasWidth] = useState(0);
  const dragState = useRef<{ path: SelectionPath; mode: "move" | "resize" | "rotate"; startX: number; startY: number; object: SlideObject } | null>(null);
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
    if (!showActive) setPresenterView(false);
  }

  const slide = deck.slides[Math.min(slideIndex, deck.slides.length - 1)] ?? deck.slides[0];

  useEditorShortcuts(session);
  const theme = useMemo(() => THEMES.find((candidate) => candidate.id === deck.theme) ?? THEMES[0], [deck.theme]);
  const masters = useMemo(() => deck.masters ?? [], [deck.masters]);
  const selectedMaster = useMemo(() => masters.find((candidate) => candidate.id === slide.masterId) ?? masters[0] ?? null, [masters, slide.masterId]);
  const inherited = useMemo(() => inheritedObjects(deck, slide), [deck, slide]);
  useEffect(() => {
    const element = canvasRef.current;
    if (!element) return;
    const observer = new ResizeObserver(() => setCanvasWidth(element.clientWidth));
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  const scale = useMemo(() => Math.min(1.4, Math.max(0.2, (canvasWidth - 48) / deck.size.widthPt)), [canvasWidth, deck.size.widthPt]);

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
      update((current) => ({ ...current, slides: current.slides.map((candidate, index) => (index === slideIndex ? mutate(candidate) : candidate)) }), recordUndo),
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
    () => selected.map((path) => objectAtPath(slide.objects, path)).filter((object): object is SlideObject => Boolean(object)),
    [selected, slide.objects],
  );
  const primary = selectedObjects[0];
  const primaryPath = selected[0];
  const hasGroupSelection = selected.some((path) => path.length === 1 && objectAtPath(slide.objects, path)?.kind === "group");

  const selectFromPointer = (event: React.PointerEvent, path: SelectionPath) => {
    if (isInheritedId(path[0])) return;
    const target: SelectionPath = event.altKey ? path : [path[0]];
    const key = pathKey(target);
    if (event.shiftKey) {
      setSelected((current) => (current.some((item) => pathKey(item) === key) ? current.filter((item) => pathKey(item) !== key) : [...current, target]));
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
        updatePath(state.path, { x: nextX, y: nextY, children: children ? children.map((child) => translateObject(child, moveDx, moveDy)) : undefined }, false);
      } else if (state.mode === "resize") {
        const w = Math.max(24, Math.round(state.object.w + dx));
        const h = Math.max(24, Math.round(state.object.h + dy));
        const sx = state.object.w > 0 ? w / state.object.w : 1;
        const sy = state.object.h > 0 ? h / state.object.h : 1;
        updatePath(state.path, { w, h, children: children ? children.map((child) => scaleObject(child, sx, sy, state.object.x, state.object.y)) : undefined }, false);
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
      if (mode === "move" && up.pointerType !== "mouse" && Math.abs(up.clientX - startX) + Math.abs(up.clientY - startY) < 6) {
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
    selectFromPointer(event, path);
    beginDrag(event, event.altKey ? path : [path[0]], "move");
  };

  const handleObjectDoubleClick = (event: React.MouseEvent, path: SelectionPath) => {
    event.stopPropagation();
    const object = objectAtPath(slide.objects, path);
    if (object) openObjectEditor(path, object);
  };

  const handleTextChange = (path: SelectionPath, text: string) => {
    const object = objectAtPath(slide.objects, path);
    if (!object?.text) return;
    updatePath(path, { text: { ...object.text, paragraphs: [{ ...object.text.paragraphs[0], text }] } }, false);
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
    update((current) => ({ ...current, slides: [...current.slides.slice(0, slideIndex + 1), created, ...current.slides.slice(slideIndex + 1)] }));
    setSlideIndex(slideIndex + 1);
  };

  const duplicateSlide = () => {
    const { objects, idMap } = cloneObjects(slide.objects);
    const copy: Slide = JSON.parse(JSON.stringify(slide));
    copy.id = uid();
    copy.objects = objects;
    copy.animations = (slide.animations ?? []).map((animation) => ({ ...animation, id: uid(), objectId: idMap.get(animation.objectId) ?? animation.objectId }));
    update((current) => ({ ...current, slides: [...current.slides.slice(0, slideIndex + 1), copy, ...current.slides.slice(slideIndex + 1)] }));
    setSlideIndex(slideIndex + 1);
  };

  const deleteSlide = () => {
    if (deck.slides.length <= 1) return;
    update((current) => ({ ...current, slides: current.slides.filter((_, index) => index !== slideIndex) }));
    setSlideIndex(Math.max(0, slideIndex - 1));
  };

  const moveSlide = (from: number, to: number) => {
    if (to < 0 || to >= deck.slides.length) return;
    update((current) => {
      const slides = [...current.slides];
      const [moved] = slides.splice(from, 1);
      slides.splice(to, 0, moved);
      return { ...current, slides };
    });
    setSlideIndex(to);
  };

  const applyLayout = (layoutId: string) => {
    const layout = LAYOUTS.find((candidate) => candidate.id === layoutId);
    if (!layout) return;
    updateSlide((current) => ({ ...current, layout: layoutId, objects: layout.build(deck).map((object, index) => ({ ...object, z: index + 1 })) }));
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
      object = { ...textObject("", base.x, base.y, 200, 40, 16, "left"), kind, line: { x2: 200, y2: 40, beginArrow: false, endArrow: kind === "arrow", dash: "solid" }, style: { fill: null, stroke: theme.accent, strokeWidthPt: 2, opacity: 1, cornerRadiusPt: 0, shadow: false } };
    } else if (kind === "table") {
      object = {
        ...textObject("", base.x, base.y, 460, 200, 14, "left"),
        kind: "table",
        table: {
          rows: Array.from({ length: 3 }, (): import("../lib/office-types").TableRow => ({ cells: Array.from({ length: 3 }, (): import("../lib/office-types").TableCell => ({ blocks: [{ type: "paragraph" as const, props: { style: "Normal", align: "left", lineSpacing: 1.15, spaceBeforePt: 0, spaceAfterPt: 0, indentLeftPt: 0, indentRightPt: 0, firstLinePt: 0, list: null, pageBreakBefore: false }, runs: [{ text: "", bold: false, italic: false, underline: false, strike: false, color: null, highlight: null, font: null, sizePt: null, link: null, comment: null, superscript: false, subscript: false }] }], colspan: 1, rowspan: 1, background: null, align: "left", valign: "top", widthPt: null })), heightPt: null, header: false })),
          columnWidthsPt: [153, 153, 153],
          borders: true,
          borderColor: "#94A3B8",
          align: "left",
        },
      };
    } else if (kind === "chart") {
      object = { ...textObject("", base.x, base.y, 420, 260, 14, "left"), kind: "chart", chart: emptyChart() };
    } else {
      object = { ...textObject("", base.x, base.y, base.w, base.h, 16, "left"), kind, style: { fill: theme.accent, stroke: null, strokeWidthPt: 1.5, opacity: 1, cornerRadiusPt: kind === "roundRect" ? 12 : 0, shadow: false } };
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
    updateSlide((current) => ({ ...current, objects: [...current.objects, ...copies].map((object, index) => ({ ...object, z: index + 1 })) }));
    setSelected(copies.map((object) => [object.id]));
  };

  const alignSelected = (mode: "left" | "center" | "right" | "top" | "middle" | "bottom") => {
    if (selected.length === 0) return;
    updateSlide((current) => {
      let objects = current.objects;
      for (const path of selected) {
        const object = objectAtPath(objects, path);
        if (!object) continue;
        switch (mode) {
          case "left":
            objects = replaceObjectAtPath(objects, path, { x: 0 });
            break;
          case "center":
            objects = replaceObjectAtPath(objects, path, { x: Math.round((deck.size.widthPt - object.w) / 2) });
            break;
          case "right":
            objects = replaceObjectAtPath(objects, path, { x: Math.round(deck.size.widthPt - object.w) });
            break;
          case "top":
            objects = replaceObjectAtPath(objects, path, { y: 0 });
            break;
          case "middle":
            objects = replaceObjectAtPath(objects, path, { y: Math.round((deck.size.heightPt - object.h) / 2) });
            break;
          default:
            objects = replaceObjectAtPath(objects, path, { y: Math.round(deck.size.heightPt - object.h) });
            break;
        }
      }
      return { ...current, objects: refreshGroupBounds(objects) };
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
      patch.children = primary.children.map((child) => translateObject(child, axis === "x" ? delta : 0, axis === "y" ? delta : 0));
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
        animations: list.some((candidate) => candidate.id === animation.id) ? list.map((candidate) => (candidate.id === animation.id ? animation : candidate)) : [...list, animation],
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
          const backTimer = window.setTimeout(() => {
            setAnimRunning((current) => (current[animation.id] ? { ...current, [animation.id]: { effect: animation, phase: "back" } } : current));
          }, Math.round(lasting / 2));
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

  const goToSlide = useCallback(
    (index: number) => {
      clearShowTimers();
      showStartRef.current = null;
      showEndRef.current = null;
      stepIndexRef.current = 0;
      setStepIndex(0);
      setAnimRunning({});
      setAnimDone({});
      setSlideshow(index);
    },
    [clearShowTimers],
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
    if (slideshow < deck.slides.length - 1) goToSlide(slideshow + 1);
  }, [deck.slides.length, goToSlide, runStep, slideshow, stepList]);

  useEffect(() => {
    advanceRef.current = advanceShow;
  }, [advanceShow]);

  useEffect(() => {
    if (slideshow === null || stepIndex >= stepList.length) return;
    const step = stepList[stepIndex];
    if (step.waitForClick) return;
    const now = Date.now();
    const base = step.relativeTo === "previousStart" ? showStartRef.current ?? now : step.relativeTo === "previousEnd" ? showEndRef.current ?? now : now;
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
        goToSlide(Math.max(0, slideshow - 1));
        return;
      }
      if (event.key.toLowerCase() === "p") setPresenterView((value) => !value);
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [goToSlide, slideshow]);

  useEffect(() => {
    if (!showActive) return;
    const startedAt = Date.now();
    const interval = window.setInterval(() => setElapsedMs(Date.now() - startedAt), 500);
    return () => window.clearInterval(interval);
  }, [showActive]);

  useEffect(() => () => clearShowTimers(), [clearShowTimers]);

  const showObjectStyle = useCallback(
    (object: SlideObject) => animationObjectStyle(object.id, showAnimations, animRunning, animDone),
    [animDone, animRunning, showAnimations],
  );

  const slideshowSlideArea = showSlide ? (
    <div
      className="slideshow-slide"
      data-transition={showSlide.transition ?? "fade"}
      style={{
        width: presenterView ? "100%" : "90vw",
        height: presenterView ? "100%" : `${(90 * deck.size.heightPt) / deck.size.widthPt}vh`,
        background: showSlide.background ?? theme.background,
      }}
    >
      <SlidePreview deck={deck} slide={showSlide} theme={theme} width={0} full slideWidth={deck.size.widthPt} slideHeight={deck.size.heightPt} objectStyle={showObjectStyle} />
    </div>
  ) : null;

  const presenterPanel = presenterView && showSlide ? (
    <aside
      className="presenter-panel"
      style={{ width: 320, minWidth: 320, maxHeight: "100%", overflow: "auto", display: "flex", flexDirection: "column", gap: 10, padding: 12, borderRadius: 10, background: "rgba(15, 23, 42, 0.94)", color: "#E2E8F0" }}
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
    <div className="slideshow-nav" role="presentation" onClick={(event) => event.stopPropagation()} style={presenterView ? { position: "static" } : undefined}>
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
              <ToolButton icon={<Undo2 size={16} />} onClick={undo} disabled={undoStack.length === 0} title={t("common.undo")} />
              <ToolButton icon={<Redo2 size={16} />} onClick={redo} disabled={redoStack.length === 0} title={t("common.redo")} />
              <ToolButton icon={<Copy size={16} />} label={t("impress.duplicate")} onClick={duplicateSelected} disabled={selected.length === 0} />
              <ToolButton icon={<Trash2 size={16} />} label={t("common.delete")} onClick={deleteSelected} disabled={selected.length === 0} />
            </RibbonGroup>
            <RibbonGroup label={t("impress.objects")}>
              <ToolButton icon={<Type size={16} />} label={t("impress.text")} onClick={() => addObject("text")} />
              <ToolButton icon={<Square size={16} />} label={t("impress.rect")} onClick={() => addObject("rect")} />
              <ToolButton icon={<Circle size={16} />} label={t("impress.ellipse")} onClick={() => addObject("ellipse")} />
              <ToolButton icon={<Minus size={16} />} label={t("impress.line")} onClick={() => addObject("line")} />
              <ToolButton icon={<ArrowRight size={16} />} label={t("impress.arrow")} onClick={() => addObject("arrow")} />
              <ToolButton icon={<ImageIcon size={16} />} label={t("writer.image")} onClick={() => addObject("image")} />
              <ToolButton icon={<TableIcon size={16} />} label={t("writer.table")} onClick={() => addObject("table")} />
              <ToolButton icon={<LineChart size={16} />} label={t("calc.chart")} onClick={() => addObject("chart")} />
            </RibbonGroup>
            <RibbonGroup label={t("impress.arrange")}>
              <ToolButton icon={<AlignStartHorizontal size={16} />} onClick={() => alignSelected("left")} title={t("impress.alignLeft")} />
              <ToolButton icon={<AlignCenterHorizontal size={16} />} onClick={() => alignSelected("center")} title={t("impress.alignCenter")} />
              <ToolButton icon={<AlignEndHorizontal size={16} />} onClick={() => alignSelected("right")} title={t("impress.alignRight")} />
              <ToolButton icon={<Move size={16} />} onClick={() => bringForward(1)} title={t("impress.bringForward")} />
              <ToolButton icon={<Group size={16} />} label={t("impress.group")} onClick={groupSelected} disabled={selected.filter((path) => path.length === 1).length < 2} />
              <ToolButton icon={<Ungroup size={16} />} label={t("impress.ungroup")} onClick={ungroupSelected} disabled={!hasGroupSelection} />
            </RibbonGroup>
            <RibbonGroup label={t("impress.slides")}>
              <ToolButton icon={<Plus size={16} />} label={t("impress.newSlide")} onClick={addSlide} />
              <ToolButton icon={<Copy size={16} />} label={t("impress.duplicateSlide")} onClick={duplicateSlide} />
              <ToolButton icon={<Trash2 size={16} />} label={t("impress.deleteSlide")} onClick={deleteSlide} disabled={deck.slides.length <= 1} />
            </RibbonGroup>
          </>
        ) : null}

        {ribbon === "insert" ? (
          <>
            <RibbonGroup label={t("impress.slides")}>
              <ToolButton icon={<Plus size={16} />} label={t("impress.newSlide")} onClick={addSlide} />
            </RibbonGroup>
            <RibbonGroup label={t("impress.objects")}>
              <ToolButton icon={<Type size={16} />} label={t("impress.text")} onClick={() => addObject("text")} />
              <ToolButton icon={<ImageIcon size={16} />} label={t("writer.image")} onClick={() => addObject("image")} />
              <ToolButton icon={<TableIcon size={16} />} label={t("writer.table")} onClick={() => addObject("table")} />
              <ToolButton icon={<LineChart size={16} />} label={t("calc.chart")} onClick={() => addObject("chart")} />
              <ToolButton icon={<Braces size={16} />} label={t("impress.rect")} onClick={() => addObject("roundRect")} />
            </RibbonGroup>
          </>
        ) : null}

        {ribbon === "design" ? (
          <>
            <RibbonGroup label={t("impress.theme")}>
              <ToolSelect value={deck.theme} onChange={(themeId) => update((current) => ({ ...current, theme: themeId }))} options={THEMES.map((candidate) => ({ value: candidate.id, label: candidate.name }))} width={130} />
            </RibbonGroup>
            <RibbonGroup label={t("impress.layout")}>
              <ToolSelect value={slide.layout} onChange={applyLayout} options={LAYOUTS.map((layout) => ({ value: layout.id, label: layout.name }))} width={150} />
            </RibbonGroup>
            <RibbonGroup label={t("impress.master")}>
              <ToolButton icon={<LayoutTemplate size={16} />} label={t("impress.masters")} onClick={openMasterDialog} />
            </RibbonGroup>
            <RibbonGroup label={t("impress.background")}>
              <ToolColor value={slide.background ?? theme.background} onChange={(background) => updateSlide((current) => ({ ...current, background }))} title={t("impress.background")} />
              <ToolButton label={t("impress.clearBackground")} onClick={() => updateSlide((current) => ({ ...current, background: null }))} />
            </RibbonGroup>
          </>
        ) : null}

        {ribbon === "transitions" ? (
          <RibbonGroup label={t("impress.transition")}>
            <ToolSelect
              value={slide.transition ?? "none"}
              onChange={(transition) => updateSlide((current) => ({ ...current, transition: transition === "none" ? null : transition }))}
              options={[
                { value: "none", label: t("impress.transitionNone") },
                { value: "fade", label: "Fade" },
                { value: "slide", label: "Slide" },
                { value: "push", label: "Push" },
                { value: "wipe", label: "Wipe" },
              ]}
              width={130}
            />
            <ToolNumber value={slide.transitionMs} onChange={(transitionMs) => updateSlide((current) => ({ ...current, transitionMs }))} min={100} max={3000} step={100} title="ms" width={80} />
            <ToolButton label={t("impress.applyToAll")} onClick={() => update((current) => ({ ...current, slides: current.slides.map((candidate) => ({ ...candidate, transition: slide.transition, transitionMs: slide.transitionMs })) }))} />
          </RibbonGroup>
        ) : null}

        {ribbon === "animations" ? (
          <RibbonGroup label={t("impress.animations")}>
            <ToolButton icon={<Sparkles size={16} />} label={t("impress.addAnimation")} onClick={addAnimation} disabled={slide.objects.length === 0} />
          </RibbonGroup>
        ) : null}

        {ribbon === "view" ? (
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
        ) : null}

        <div className="ribbon-spacer" />
        <RibbonGroup>
          <ToolButton icon={<FolderOpen size={16} />} label={t("common.open")} onClick={() => void openIntoWorkspace()} />
          <ToolButton icon={<Save size={16} />} label={t("common.save")} onClick={() => void session.save()} disabled={session.busy} />
          <ToolButton label={t("common.saveAs")} onClick={() => void session.saveAs()} disabled={session.busy} />
          <ToolButton icon={<FileDown size={16} />} label={t("writer.exportPdf")} onClick={() => void session.exportPdf()} />
          <ToolButton icon={<Printer size={16} />} label={t("common.print")} onClick={() => window.print()} />
        </RibbonGroup>
      </Ribbon>

      <div className="impress-layout">
        <div className="slide-list">
          {deck.slides.map((candidate, index) => (
            <div
              key={candidate.id}
              className={`slide-thumb${index === slideIndex ? " is-active" : ""}`}
              role="button"
              tabIndex={0}
              aria-label={`${t("impress.slide")} ${index + 1}`}
              onClick={() => { setSlideIndex(index); setSelected([]); }}
              onKeyDown={(event) => {
                if (event.key === "Enter" || event.key === " ") {
                  event.preventDefault();
                  setSlideIndex(index);
                  setSelected([]);
                }
              }}
            >
              <span className="slide-number">{index + 1}</span>
              <SlidePreview deck={deck} slide={candidate} theme={theme} width={148} />
              <div className="slide-thumb-actions">
                <button type="button" className="icon-btn" onClick={(event) => { event.stopPropagation(); moveSlide(index, index - 1); }} title="Move up">
                  ↑
                </button>
                <button type="button" className="icon-btn" onClick={(event) => { event.stopPropagation(); moveSlide(index, index + 1); }} title="Move down">
                  ↓
                </button>
              </div>
            </div>
          ))}
          <button type="button" className="btn btn-soft slide-add" onClick={addSlide}>
            <Plus size={14} /> {t("impress.newSlide")}
          </button>
        </div>

        <div className="slide-stage" ref={canvasRef} role="presentation" onPointerDown={(event) => { if (event.target === event.currentTarget) setSelected([]); }}>
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
                style={{ left: marquee.x * scale, top: marquee.y * scale, width: marquee.w * scale, height: marquee.h * scale }}
              />
            ) : null}
            {inherited.map((object) => (
              <div
                key={object.id}
                className="slide-object is-inherited"
                style={{ left: object.x * scale, top: object.y * scale, width: object.w * scale, height: object.h * scale, transform: `rotate(${object.rotation}deg)`, zIndex: 0, pointerEvents: "none" }}
              >
                <ObjectTree object={object} path={[object.id]} depth={0} theme={theme} scale={scale} selectedKeys={INHERITED_KEYS} editingKey={null} interactive={false} />
              </div>
            ))}
            {[...slide.objects].sort((a, b) => a.z - b.z).map((object) => {
              const path: SelectionPath = [object.id];
              const isSelected = selectedKeys.has(pathKey(path));
              return (
                <div
                  key={object.id}
                  className={`slide-object${isSelected ? " is-selected" : ""}`}
                  style={{
                    left: object.x * scale,
                    top: object.y * scale,
                    width: object.w * scale,
                    height: object.h * scale,
                    transform: `rotate(${object.rotation}deg)`,
                    zIndex: object.z,
                  }}
                  onPointerDown={(event) => handleObjectPointerDown(event, path)}
                  onDoubleClick={(event) => handleObjectDoubleClick(event, path)}
                >
                  <ObjectTree
                    object={object}
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
                    onTextChange={handleTextChange}
                    onTextDone={() => setEditingText(null)}
                  />
                  {isSelected ? <SelectionHandles onHandlePointerDown={(event, mode) => beginDrag(event, path, mode)} /> : null}
                </div>
              );
            })}
          </div>
        </div>

        <div className="slide-properties">
          <h4>{t("impress.properties")}</h4>
          <div className="stack">
            <label className="field">
              <span>{t("impress.master")}</span>
              <select value={slide.masterId ?? masters[0]?.id ?? ""} onChange={(event) => selectMaster(event.target.value)}>
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
          </div>
          {primary && primaryPath ? (
            <div className="stack">
              <div className="row">
                <ToolNumber value={Math.round(primary.x)} onChange={(x) => setPrimaryPosition("x", x)} title="X" width={64} />
                <ToolNumber value={Math.round(primary.y)} onChange={(y) => setPrimaryPosition("y", y)} title="Y" width={64} />
              </div>
              <div className="row">
                <ToolNumber value={Math.round(primary.w)} onChange={(w) => setPrimarySize("w", w)} title="W" width={64} />
                <ToolNumber value={Math.round(primary.h)} onChange={(h) => setPrimarySize("h", h)} title="H" width={64} />
              </div>
              <div className="row">
                <ToolNumber value={Math.round(primary.rotation)} onChange={(rotation) => updatePath(primaryPath, { rotation })} min={-180} max={180} title={t("impress.rotation")} width={64} />
                <ToolNumber value={primary.z} onChange={(z) => updatePath(primaryPath, { z })} min={1} max={99} title="Z" width={64} />
              </div>
              {primary.kind === "rect" || primary.kind === "ellipse" || primary.kind === "roundRect" ? (
                <>
                  <label className="field">
                    <span>{t("impress.fill")}</span>
                    <input
                      type="color"
                      value={primary.style?.fill ?? "#2563eb"}
                      onChange={(event) => updatePath(primaryPath, { style: { ...(primary.style ?? defaultShapeStyle()), fill: event.target.value } })}
                    />
                  </label>
                  <label className="field">
                    <span>{t("impress.cornerRadius")}</span>
                    <input
                      type="number"
                      value={primary.style?.cornerRadiusPt ?? 0}
                      onChange={(event) => updatePath(primaryPath, { style: { ...(primary.style ?? defaultShapeStyle()), cornerRadiusPt: Number(event.target.value) } })}
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
                      updatePath(primaryPath, {
                        text: { ...primary.text!, paragraphs: [{ ...primary.text!.paragraphs[0], sizePt: Number(event.target.value) }] },
                      })
                    }
                  />
                </label>
              ) : null}
              <label className="field">
                <span>{t("writer.paragraph")}</span>
                <select value={primary.text?.paragraphs[0]?.align ?? "left"} onChange={(event) => updatePath(primaryPath, { text: primary.text ? { ...primary.text, paragraphs: [{ ...primary.text.paragraphs[0], align: event.target.value }] } : null })}>
                  <option value="left">{t("writer.alignLeft")}</option>
                  <option value="center">{t("writer.alignCenter")}</option>
                  <option value="right">{t("writer.alignRight")}</option>
                </select>
              </label>
              {primary.kind === "chart" ? <ToolButton icon={<LineChart size={14} />} label={t("impress.chartData")} onClick={() => setChartPath(primaryPath)} /> : null}
              <ToolButton label={t("impress.editText")} onClick={() => (primary.text ? setEditingText(pathKey(primaryPath)) : updatePath(primaryPath, { text: { paragraphs: [{ text: "New text", level: 0, bold: false, italic: false, underline: false, sizePt: 20, color: null, align: "left", bullet: false, runs: [] }], valign: "top", font: null, sizePt: 20, color: null, align: "left" } }))} />
            </div>
          ) : (
            <p className="muted">{t("impress.noSelection")}</p>
          )}
          <h4>{t("impress.notes")}</h4>
          <textarea className="notes-input" value={slide.notes} onChange={(event) => updateSlide((current) => ({ ...current, notes: event.target.value }))} placeholder={t("impress.notesHint")} />
          <h4>{t("impress.animations")}</h4>
          <div className="stack">
            {[...(slide.animations ?? [])]
              .sort((a, b) => a.order - b.order)
              .map((animation, index, list) => {
                const target = slide.objects.find((object) => object.id === animation.objectId);
                return (
                  <div key={animation.id} className="row" style={{ alignItems: "center", gap: 4 }}>
                    <span className="muted" style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                      {target?.name ?? t("impress.animationMissingObject")} · {animation.kind} · {animation.effect} · {animation.trigger}
                    </span>
                    <button type="button" className="icon-btn" onClick={() => reorderAnimation(animation.id, -1)} disabled={index === 0} title={t("impress.moveUp")}>
                      ↑
                    </button>
                    <button type="button" className="icon-btn" onClick={() => reorderAnimation(animation.id, 1)} disabled={index === list.length - 1} title={t("impress.moveDown")}>
                      ↓
                    </button>
                    <button type="button" className="icon-btn" onClick={() => setAnimationEditing(animation)} title={t("common.edit")}>
                      ✎
                    </button>
                    <button
                      type="button"
                      className="icon-btn"
                      onClick={() => updateSlide((current) => ({ ...current, animations: (current.animations ?? []).filter((candidate) => candidate.id !== animation.id) }))}
                      title={t("common.delete")}
                    >
                      ×
                    </button>
                  </div>
                );
              })}
            <ToolButton icon={<Sparkles size={14} />} label={t("impress.addAnimation")} onClick={addAnimation} disabled={slide.objects.length === 0} />
          </div>
        </div>
      </div>

      <div className="editor-status">
        <span>
          {slideIndex + 1} / {deck.slides.length} {t("impress.slides")}
        </span>
        <span className="spacer" />
        <span>
          {tab.path ?? t("writer.unsaved")} {tab.dirty ? "•" : ""}
        </span>
      </div>

      {slideshow !== null && showSlide ? (
        <div
          className="slideshow"
          role="presentation"
          onClick={(event) => {
            // Clicks inside the presenter control panel must not advance the
            // show to the next slide.
            if (event.target instanceof Element && event.target.closest(".presenter-panel")) return;
            advanceShow();
          }}
          style={presenterView ? { display: "flex", flexDirection: "column", alignItems: "stretch", justifyContent: "flex-start", padding: 12, gap: 8 } : undefined}
        >
          {presenterView ? (
            <div style={{ display: "flex", gap: 12, flex: 1, minHeight: 0, alignItems: "stretch", justifyContent: "center", width: "100%" }}>
              {slideshowSlideArea}
              {presenterPanel}
            </div>
          ) : (
            slideshowSlideArea
          )}
          {slideshowNav}
        </div>
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
        <AnimationDialog key={animationEditing.id} animation={animationEditing} objects={slide.objects} onClose={() => setAnimationEditing(null)} onSave={saveAnimation} />
      ) : null}
    </div>
  );

  async function replaceImage(path: SelectionPath) {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const { readFile } = await import("@tauri-apps/plugin-fs");
      const filePath = await open({ multiple: false, filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "gif", "webp", "bmp"] }] });
      if (typeof filePath !== "string") return;
      const bytes = await readFile(filePath);
      let base64 = "";
      for (let index = 0; index < bytes.length; index += 0x8000) base64 += String.fromCharCode(...bytes.subarray(index, index + 0x8000));
      const name = filePath.split(/[\\/]/).pop() ?? "image.png";
      const mime = name.endsWith(".jpg") || name.endsWith(".jpeg") ? "image/jpeg" : "image/png";
      updatePath(path, { image: { name, mime, dataBase64: btoa(base64), alt: "" }, kind: "image" });
    } catch (error) {
      reportError(error, t);
    }
  }
}

function defaultShapeStyle(): ShapeStyle {
  return { fill: "#2563eb", stroke: null, strokeWidthPt: 1.5, opacity: 1, cornerRadiusPt: 0, shadow: false };
}

function MasterDialog({
  deck,
  slide,
  update,
  onClose,
  onUseLayout,
}: {
  deck: Deck;
  slide: Slide;
  update: (mutate: (deck: Deck) => Deck, recordUndo?: boolean) => void;
  onClose: () => void;
  onUseLayout: (masterId: string, layout: SlideLayout) => void;
}) {
  const t = useT();
  const masters = deck.masters ?? [];
  const [activeId, setActiveId] = useState(slide.masterId && masters.some((master) => master.id === slide.masterId) ? slide.masterId : masters[0]?.id ?? "");
  const active = masters.find((master) => master.id === activeId) ?? masters[0];

  const addMaster = () => {
    const master = newSlideMaster(`Master ${masters.length + 1}`);
    setActiveId(master.id);
    update((current) => ({ ...current, masters: [...(current.masters ?? []), master] }));
  };
  const renameMaster = (masterId: string, name: string) =>
    update((current) => ({ ...current, masters: (current.masters ?? []).map((master) => (master.id === masterId ? { ...master, name } : master)) }), false);
  const addLayout = (masterId: string) => {
    const layout: SlideLayout = { id: uid(), name: `Layout ${(masters.find((master) => master.id === masterId)?.layouts.length ?? 0) + 1}`, kind: "blank", objects: [] };
    update((current) => ({ ...current, masters: (current.masters ?? []).map((master) => (master.id === masterId ? { ...master, layouts: [...master.layouts, layout] } : master)) }));
  };
  const renameLayout = (masterId: string, layoutId: string, name: string) =>
    update(
      (current) => ({
        ...current,
        masters: (current.masters ?? []).map((master) => (master.id === masterId ? { ...master, layouts: master.layouts.map((layout) => (layout.id === layoutId ? { ...layout, name } : layout)) } : master)),
      }),
      false,
    );
  const removeLayout = (masterId: string, layoutId: string) =>
    update((current) => ({ ...current, masters: (current.masters ?? []).map((master) => (master.id === masterId ? { ...master, layouts: master.layouts.filter((layout) => layout.id !== layoutId) } : master)) }));

  return (
    <Dialog title={t("impress.masters")} onClose={onClose} wide>
      <div className="row" style={{ alignItems: "flex-start", gap: 12 }}>
        <div className="stack" style={{ minWidth: 170 }}>
          {masters.map((master) => (
            <button key={master.id} type="button" className={`btn ${master.id === (active?.id ?? "") ? "btn-primary" : "btn-soft"}`} onClick={() => setActiveId(master.id)}>
              {master.name}
            </button>
          ))}
          <button type="button" className="btn btn-soft" onClick={addMaster}>
            + {t("impress.addMaster")}
          </button>
        </div>
        {active ? (
          <div className="stack" style={{ flex: 1 }}>
            <label className="field">
              <span>{t("impress.masterName")}</span>
              <input value={active.name} onChange={(event) => renameMaster(active.id, event.target.value)} />
            </label>
            <h4>{t("impress.layouts")}</h4>
            {active.layouts.map((layout) => (
              <div key={layout.id} className="row" style={{ gap: 4, alignItems: "center" }}>
                <input value={layout.name} onChange={(event) => renameLayout(active.id, layout.id, event.target.value)} />
                <button
                  type="button"
                  className={`btn ${slide.masterId === active.id && slide.layoutId === layout.id ? "btn-primary" : "btn-soft"}`}
                  onClick={() => onUseLayout(active.id, layout)}
                >
                  {t("impress.useLayout")}
                </button>
                <button type="button" className="icon-btn" onClick={() => removeLayout(active.id, layout.id)} title={t("common.delete")}>
                  ×
                </button>
              </div>
            ))}
            <button type="button" className="btn btn-soft" onClick={() => addLayout(active.id)}>
              + {t("impress.addLayout")}
            </button>
            <p className="muted">{t("impress.masterHint")}</p>
          </div>
        ) : null}
      </div>
    </Dialog>
  );
}

function ChartDialog({ chart, theme, onClose, onSave }: { chart: ChartData; theme: Theme; onClose: () => void; onSave: (chart: ChartData) => void }) {
  const t = useT();
  const [draft, setDraft] = useState<ChartData>(() => normalizeChartDraft(chart));
  const [tab, setTab] = useState<"data" | "chart">("data");
  // Displayed rows; caches may grow past this through paste, the derived count
  // below always covers every stored value.
  const [rowCount, setRowCount] = useState(() => chartDataRowCount(chart));
  // Text of cells that are being edited but do not parse (or were emptied).
  // The draft only ever receives parsed numbers, so invalid input is never
  // committed; blurring discards the text and restores the stored value.
  const [cellTexts, setCellTexts] = useState<Record<string, string>>({});
  // Paste anchor = the focused cell; paste starts there.
  const [pasteAnchor, setPasteAnchor] = useState<ChartGridCell>({ row: 0, column: 1 });
  const [dataTouched, setDataTouched] = useState(false);
  const gridRef = useRef<HTMLDivElement>(null);
  const patch = (change: Partial<ChartData>) => setDraft((current) => ({ ...current, ...change }));

  const categories = draft.categoriesCache ?? [];
  const seriesValues = draft.seriesValuesCache ?? draft.series.map(() => []);
  const rows = Math.max(rowCount, categories.length, ...seriesValues.map((values) => values.length), 1);
  const cellKey = (row: number, column: number) => `${row}:${column}`;

  const updateSeries = (index: number, change: Partial<ChartData["series"][number]>) => {
    setDraft((current) => ({ ...current, series: current.series.map((entry, position) => (position === index ? { ...entry, ...change } : entry)) }));
  };

  const addSeries = () => {
    setDraft((current) => {
      const values = current.seriesValuesCache ?? current.series.map(() => []);
      return {
        ...current,
        series: [...current.series, { name: `${t("impress.chartSeries")} ${current.series.length + 1}`, range: "", color: null }],
        seriesValuesCache: [...values, []],
      };
    });
  };

  const removeSeries = (index: number) => {
    setDataTouched(true);
    // Uncommitted cell text is keyed by column, so drop it instead of letting
    // it attach to a different series after the shift.
    setCellTexts({});
    setDraft((current) => {
      const values = current.seriesValuesCache ?? current.series.map(() => []);
      return {
        ...current,
        series: current.series.filter((_, position) => position !== index),
        seriesValuesCache: values.filter((_, position) => position !== index),
      };
    });
  };

  /** Shrinks or grows the visible rows; shrinking trims caches to fit. */
  const changeRowCount = (next: number) => {
    const target = Math.max(1, Math.min(1000, Math.floor(next) || 1));
    setDataTouched(true);
    setRowCount(target);
    setDraft((current) => ({
      ...current,
      categoriesCache: (current.categoriesCache ?? []).filter((_, row) => row < target),
      seriesValuesCache: current.series.map((_, index) => (current.seriesValuesCache?.[index] ?? []).filter((_, row) => row < target)),
    }));
    setCellTexts((current) => {
      const kept: Record<string, string> = {};
      for (const [key, text] of Object.entries(current)) {
        if (Number(key.split(":")[0]) < target) kept[key] = text;
      }
      return kept;
    });
  };

  /** Commits a parsed value; rows beyond the current length are padded with 0
   * because caches are dense lists (the Rust reader zero-fills sparse points). */
  const commitValue = (seriesIndex: number, row: number, value: number) => {
    setDataTouched(true);
    setDraft((current) => {
      const values = (current.seriesValuesCache ?? []).map((list) => [...list]);
      const target = values[seriesIndex] ?? [];
      while (target.length < row) target.push(0);
      target[row] = value;
      values[seriesIndex] = target;
      return { ...current, seriesValuesCache: values };
    });
  };

  /** Clearing a cell drops the trailing value or, inside a series, stores 0 so
   * the later points keep their category alignment. */
  const clearValue = (seriesIndex: number, row: number) => {
    setDataTouched(true);
    setDraft((current) => {
      const values = (current.seriesValuesCache ?? []).map((list) => [...list]);
      const target = values[seriesIndex];
      if (!target || row >= target.length) return current;
      if (row === target.length - 1) target.pop();
      else target[row] = 0;
      values[seriesIndex] = target;
      return { ...current, seriesValuesCache: values };
    });
  };

  const changeValue = (seriesIndex: number, row: number, text: string) => {
    const key = cellKey(row, seriesIndex + 1);
    const parsed = parseChartCellNumber(text);
    if (parsed === null) {
      setCellTexts((current) => ({ ...current, [key]: text }));
      return;
    }
    setCellTexts((current) => {
      if (!(key in current)) return current;
      const next = { ...current };
      delete next[key];
      return next;
    });
    commitValue(seriesIndex, row, parsed);
  };

  /** Blur commit: an emptied cell clears its value, text that never parsed is
   * dropped (invalid input is not committed). */
  const finishValue = (seriesIndex: number, row: number) => {
    const key = cellKey(row, seriesIndex + 1);
    const text = cellTexts[key];
    if (text === undefined) return;
    setCellTexts((current) => {
      const next = { ...current };
      delete next[key];
      return next;
    });
    if (parseChartCellNumber(text) === null && text.trim() === "") clearValue(seriesIndex, row);
  };

  const changeCategory = (row: number, text: string) => {
    setDataTouched(true);
    setDraft((current) => {
      const next = [...(current.categoriesCache ?? [])];
      while (next.length <= row) next.push("");
      next[row] = text;
      // Trailing empty labels do not carry data; keep the cache short.
      while (next.length > 0 && next[next.length - 1] === "") next.pop();
      return { ...current, categoriesCache: next };
    });
  };

  const handlePaste = (event: React.ClipboardEvent<HTMLDivElement>) => {
    const text = event.clipboardData?.getData("text/plain") ?? "";
    if (text.trim() === "") return;
    const block = parseChartClipboard(text);
    if (block.length === 0) return;
    event.preventDefault();
    setDataTouched(true);
    setDraft((current) => mergeChartPaste(current, pasteAnchor, block));
    setRowCount((current) => Math.max(current, pasteAnchor.row + block.length, 1));
  };

  const focusCell = (row: number, column: number) => {
    const input = gridRef.current?.querySelector<HTMLInputElement>(`[data-cell="${row}:${column}"]`);
    if (input) {
      input.focus();
      input.select();
    }
  };

  /** Spreadsheet-style navigation: up/down/Enter move between rows, left/right
   * only when the caret is already at the edge of the cell text. */
  const handleCellKeyDown = (event: React.KeyboardEvent<HTMLInputElement>, row: number, column: number) => {
    const input = event.currentTarget;
    const atStart = input.selectionStart === 0 && input.selectionEnd === 0;
    const atEnd = input.selectionStart === input.value.length && input.selectionEnd === input.value.length;
    let next: ChartGridCell | null = null;
    if (event.key === "ArrowUp" && row > 0) next = { row: row - 1, column };
    else if (event.key === "ArrowDown" && row < rows - 1) next = { row: row + 1, column };
    else if (event.key === "Enter" && row < rows - 1) next = { row: row + 1, column };
    else if (event.key === "ArrowLeft" && column > 0 && atStart) next = { row, column: column - 1 };
    else if (event.key === "ArrowRight" && column < draft.series.length && atEnd) next = { row, column: column + 1 };
    if (!next) return;
    event.preventDefault();
    focusCell(next.row, next.column);
  };

  const gridInputStyle: CSSProperties = { width: "100%", minWidth: 84, boxSizing: "border-box", border: "none", background: "transparent", padding: "6px 8px", font: "inherit", color: "inherit" };
  const gridCellStyle: CSSProperties = { padding: 0, borderBottom: "1px solid var(--border)", borderRight: "1px solid var(--border)" };
  const gridHeadStyle: CSSProperties = { position: "sticky", top: 0, zIndex: 1, background: "var(--surface)", borderBottom: "1px solid var(--border)", borderRight: "1px solid var(--border)", padding: 2, textAlign: "left", minWidth: 110 };

  return (
    <Dialog title={t("impress.chartData")} onClose={onClose} wide>
      <div className="stack">
        <div style={{ height: 190, flex: "0 0 auto", border: "1px solid var(--border)", borderRadius: 6, overflow: "hidden", background: theme.background }}>
          <ChartPreview chart={draft} theme={theme} scale={1} />
        </div>
        <div className="row" style={{ gap: 4 }}>
          <button type="button" className={`btn ${tab === "data" ? "btn-primary" : "btn-soft"}`} onClick={() => setTab("data")}>
            {t("impress.chartTabData")}
          </button>
          <button type="button" className={`btn ${tab === "chart" ? "btn-primary" : "btn-soft"}`} onClick={() => setTab("chart")}>
            {t("impress.chartTabChart")}
          </button>
        </div>

        {tab === "data" ? (
          <div className="stack">
            {chartHasCachedData(chart) ? null : <p className="muted">{t("impress.chartRangeOnlyHint")}</p>}
            <div className="row" style={{ gap: 6, alignItems: "center", flexWrap: "wrap" }}>
              <button type="button" className="btn btn-soft" onClick={() => changeRowCount(rows + 1)}>
                + {t("impress.chartAddRow")}
              </button>
              <button type="button" className="btn btn-soft" onClick={() => changeRowCount(Math.max(1, rows - 1))}>
                − {t("impress.chartRemoveRow")}
              </button>
              <label className="field" style={{ flexDirection: "row", alignItems: "center", gap: 6 }}>
                <span>{t("impress.chartRowCount")}</span>
                <input
                  type="number"
                  min={1}
                  max={1000}
                  value={rows}
                  style={{ width: 72 }}
                  onChange={(event) => {
                    const value = Number(event.target.value);
                    if (Number.isFinite(value)) changeRowCount(value);
                  }}
                />
              </label>
            </div>
            <div ref={gridRef} onPaste={handlePaste} style={{ overflowX: "auto", overflowY: "auto", maxHeight: 280, border: "1px solid var(--border)", borderRadius: 6 }}>
              <table style={{ borderCollapse: "collapse", width: "100%" }}>
                <thead>
                  <tr>
                    <th style={gridHeadStyle}>{t("impress.chartCategoryColumn")}</th>
                    {draft.series.map((entry, index) => (
                      <th key={index} style={gridHeadStyle}>
                        <div className="row" style={{ gap: 2, alignItems: "center" }}>
                          <input
                            data-series-name={index}
                            value={entry.name}
                            placeholder={t("impress.chartSeriesName")}
                            aria-label={`${t("impress.chartSeriesName")} ${index + 1}`}
                            style={gridInputStyle}
                            onChange={(event) => updateSeries(index, { name: event.target.value })}
                          />
                          <button type="button" className="icon-btn" onClick={() => removeSeries(index)} title={t("common.delete")}>
                            ×
                          </button>
                        </div>
                      </th>
                    ))}
                    <th style={{ ...gridHeadStyle, minWidth: 40 }}>
                      <button type="button" className="icon-btn" onClick={addSeries} title={t("impress.chartAddSeries")}>
                        +
                      </button>
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {Array.from({ length: rows }, (_, row) => (
                    <tr key={row}>
                      <td style={gridCellStyle}>
                        <input
                          data-cell={`${row}:0`}
                          value={categories[row] ?? ""}
                          placeholder={`${t("impress.chartCategoryColumn")} ${row + 1}`}
                          aria-label={`${t("impress.chartCategoryColumn")} ${row + 1}`}
                          style={gridInputStyle}
                          onChange={(event) => changeCategory(row, event.target.value)}
                          onFocus={() => setPasteAnchor({ row, column: 0 })}
                          onPointerDown={() => setPasteAnchor({ row, column: 0 })}
                          onKeyDown={(event) => handleCellKeyDown(event, row, 0)}
                        />
                      </td>
                      {draft.series.map((entry, seriesIndex) => {
                        const key = cellKey(row, seriesIndex + 1);
                        const stored = seriesValues[seriesIndex]?.[row];
                        return (
                          <td key={seriesIndex} style={gridCellStyle}>
                            <input
                              data-cell={key}
                              value={cellTexts[key] ?? (stored === undefined ? "" : formatChartCellValue(stored))}
                              inputMode="decimal"
                              placeholder={entry.name}
                              aria-label={`${entry.name || `${t("impress.chartSeries")} ${seriesIndex + 1}`} ${row + 1}`}
                              style={gridInputStyle}
                              onChange={(event) => changeValue(seriesIndex, row, event.target.value)}
                              onBlur={() => finishValue(seriesIndex, row)}
                              onFocus={() => setPasteAnchor({ row, column: seriesIndex + 1 })}
                              onPointerDown={() => setPasteAnchor({ row, column: seriesIndex + 1 })}
                              onKeyDown={(event) => handleCellKeyDown(event, row, seriesIndex + 1)}
                            />
                          </td>
                        );
                      })}
                      <td style={{ borderBottom: "1px solid var(--border)" }}>
                        <span className="sr-only">{t("common.actions")}</span>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <p className="muted">{t("impress.chartPasteHint")}</p>
            <p className="muted">{t("impress.chartDataHint")}</p>
            <p className="muted">{t("impress.chartDataGapNote")}</p>
          </div>
        ) : (
          <div className="stack">
            <label className="field">
              <span>{t("impress.chartKind")}</span>
              <select value={draft.kind} onChange={(event) => patch({ kind: event.target.value })}>
                {["column", "bar", "line", "pie", "area"].map((kind) => (
                  <option key={kind} value={kind}>
                    {t(`calc.chart_${kind}`)}
                  </option>
                ))}
              </select>
            </label>
            <TextField label={t("impress.chartTitle")} value={draft.title} onChange={(title) => patch({ title })} />
            <TextField label={t("impress.chartCategories")} value={draft.categories} onChange={(categories) => patch({ categories })} />
            <h4>{t("impress.chartSeries")}</h4>
            {draft.series.map((entry, index) => (
              <div key={index} className="row" style={{ gap: 4, alignItems: "center" }}>
                <input value={entry.name} placeholder={t("impress.chartSeriesName")} onChange={(event) => updateSeries(index, { name: event.target.value })} />
                <input value={entry.range} placeholder={t("impress.chartRange")} onChange={(event) => updateSeries(index, { range: event.target.value })} />
                <input type="color" value={entry.color ?? CHART_PALETTE[index % CHART_PALETTE.length]} onChange={(event) => updateSeries(index, { color: event.target.value })} />
                <button type="button" className="icon-btn" onClick={() => removeSeries(index)} title={t("common.delete")}>
                  ×
                </button>
              </div>
            ))}
            <button type="button" className="btn btn-soft" onClick={addSeries}>
              + {t("impress.chartAddSeries")}
            </button>
            <div className="row">
              <label className="check">
                <input type="checkbox" checked={draft.legend} onChange={(event) => patch({ legend: event.target.checked })} />
                {t("impress.chartLegend")}
              </label>
              <label className="check">
                <input type="checkbox" checked={draft.stacked} onChange={(event) => patch({ stacked: event.target.checked })} />
                {t("impress.chartStacked")}
              </label>
              <label className="check">
                <input type="checkbox" checked={draft.showLabels} onChange={(event) => patch({ showLabels: event.target.checked })} />
                {t("impress.chartShowLabels")}
              </label>
            </div>
            <TextField label={t("impress.chartXTitle")} value={draft.xTitle} onChange={(xTitle) => patch({ xTitle })} />
            <TextField label={t("impress.chartYTitle")} value={draft.yTitle} onChange={(yTitle) => patch({ yTitle })} />
          </div>
        )}

        <div className="row">
          <button type="button" className="btn btn-soft" onClick={onClose}>
            {t("common.cancel")}
          </button>
          <button
            type="button"
            className="btn btn-primary"
            onClick={() => onSave(dataTouched && chartHasCachedData(draft) ? syncChartDataRanges(draft) : draft)}
          >
            {t("common.save")}
          </button>
        </div>
      </div>
    </Dialog>
  );
}

function AnimationDialog({ animation, objects, onClose, onSave }: { animation: Animation; objects: SlideObject[]; onClose: () => void; onSave: (animation: Animation) => void }) {
  const t = useT();
  const [draft, setDraft] = useState<Animation>({ ...animation });
  const effects = ANIMATION_EFFECTS[draft.kind] ?? ANIMATION_EFFECTS.entrance;
  const effect = effects.includes(draft.effect) ? draft.effect : effects[0];
  const missingObject = !objects.some((object) => object.id === draft.objectId);

  return (
    <Dialog title={t("impress.animation")} onClose={onClose}>
      <div className="stack">
        <label className="field">
          <span>{t("impress.animationObject")}</span>
          <select value={draft.objectId} onChange={(event) => setDraft({ ...draft, objectId: event.target.value })}>
            {missingObject ? <option value={draft.objectId}>{t("impress.animationMissingObject")}</option> : null}
            {objects.map((object, index) => (
              <option key={object.id} value={object.id}>
                {object.name || `${object.kind} ${index + 1}`}
              </option>
            ))}
          </select>
        </label>
        <label className="field">
          <span>{t("impress.animationKind")}</span>
          <select
            value={draft.kind}
            onChange={(event) => {
              const kind = event.target.value;
              const list = ANIMATION_EFFECTS[kind] ?? [];
              setDraft({ ...draft, kind, effect: list.includes(draft.effect) ? draft.effect : list[0] ?? "fade" });
            }}
          >
            {["entrance", "emphasis", "exit"].map((kind) => (
              <option key={kind} value={kind}>
                {t(`impress.animationKind_${kind}`)}
              </option>
            ))}
          </select>
        </label>
        <label className="field">
          <span>{t("impress.animationEffect")}</span>
          <select value={effect} onChange={(event) => setDraft({ ...draft, effect: event.target.value })}>
            {effects.map((candidate) => (
              <option key={candidate} value={candidate}>
                {candidate}
              </option>
            ))}
          </select>
        </label>
        <label className="field">
          <span>{t("impress.animationTrigger")}</span>
          <select value={draft.trigger} onChange={(event) => setDraft({ ...draft, trigger: event.target.value })}>
            <option value="onClick">{t("impress.triggerOnClick")}</option>
            <option value="withPrevious">{t("impress.triggerWithPrevious")}</option>
            <option value="afterPrevious">{t("impress.triggerAfterPrevious")}</option>
          </select>
        </label>
        <div className="row">
          <ToolNumber value={draft.durationMs} onChange={(durationMs) => setDraft({ ...draft, durationMs })} min={0} max={10000} step={50} title={t("impress.animationDuration")} width={80} />
          <ToolNumber value={draft.delayMs} onChange={(delayMs) => setDraft({ ...draft, delayMs })} min={0} max={10000} step={50} title={t("impress.animationDelay")} width={80} />
        </div>
        <div className="row">
          <button type="button" className="btn btn-soft" onClick={onClose}>
            {t("common.cancel")}
          </button>
          <button type="button" className="btn btn-primary" onClick={() => onSave({ ...draft, effect })}>
            {t("common.save")}
          </button>
        </div>
      </div>
    </Dialog>
  );
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
  onObjectPointerDown?: (event: React.PointerEvent, path: SelectionPath) => void;
  onObjectDoubleClick?: (event: React.MouseEvent, path: SelectionPath) => void;
  onHandlePointerDown?: (event: React.PointerEvent, path: SelectionPath, mode: "resize" | "rotate") => void;
  onTextChange?: (path: SelectionPath, text: string) => void;
  onTextDone?: () => void;
}

function SelectionHandles({ onHandlePointerDown }: { onHandlePointerDown: (event: React.PointerEvent, mode: "resize" | "rotate") => void }) {
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

function ObjectTree(props: ObjectTreeProps) {
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
              {interactive && isSelected ? <SelectionHandles onHandlePointerDown={(event, mode) => props.onHandlePointerDown?.(event, childPath, mode)} /> : null}
            </div>
          );
        })}
      </>
    );
  }
  return (
    <SlideObjectView
      object={object}
      theme={theme}
      scale={scale}
      editing={editingKey === pathKey(path)}
      onTextChange={(text) => props.onTextChange?.(path, text)}
      onTextDone={() => props.onTextDone?.()}
    />
  );
}

function ChartPreview({ chart, theme, scale }: { chart: ChartData | null; theme: Theme; scale: number }) {
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
            <div style={{ height: "90%", aspectRatio: "1 / 1", borderRadius: "50%", background: `conic-gradient(${slices.join(", ")})`, border: `1px solid ${theme.accent}` }} />
            <div style={{ flex: 1, display: "flex", flexDirection: "column", gap: 2 * scale, overflow: "hidden" }}>
              {pieValues.map((value, row) => (
                <span key={row} style={{ display: "flex", alignItems: "center", gap: 4 * scale, whiteSpace: "nowrap", overflow: "hidden" }}>
                  <i style={{ width: 8 * scale, height: 8 * scale, background: CHART_PALETTE[row % CHART_PALETTE.length], borderRadius: 2, flex: "0 0 auto" }} />
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
        <svg width="100%" height="100%" viewBox="0 0 100 100" preserveAspectRatio="none" style={{ display: "block", width: "100%", height: "100%" }}>
          <line x1="0" y1="50" x2="100" y2="50" stroke={theme.accent} strokeWidth="1" opacity="0.5" vectorEffect="non-scaling-stroke" />
          {series.map((entry, seriesIndex) => {
            const points = values[seriesIndex].map((value, row) => `${pointX(row)},${pointY(value)}`).join(" ");
            return (
              <g key={seriesIndex}>
                {chart.kind === "area" ? <polygon points={`0,50 ${points} 100,50`} fill={colorOf(seriesIndex)} opacity="0.35" /> : null}
                <polyline points={points} fill="none" stroke={colorOf(seriesIndex)} strokeWidth="1.6" vectorEffect="non-scaling-stroke">
                  <title>{entry.name}</title>
                </polyline>
              </g>
            );
          })}
        </svg>
      );
    } else if (chart.kind === "bar") {
      plot = (
        <div style={{ display: "flex", flexDirection: "column", justifyContent: "center", gap: 3 * scale, width: "100%", height: "100%" }}>
          {Array.from({ length: rowCount }, (_, row) => (
            <div key={row} style={{ display: "flex", alignItems: "center", gap: 4 * scale }}>
              <span style={{ width: 56 * scale, flex: "0 0 auto", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", opacity: 0.8 }}>{labelOf(row)}</span>
              <div style={{ flex: 1, display: "flex", flexDirection: "column", gap: 1 }}>
                {series.map((entry, seriesIndex) => {
                  const value = values[seriesIndex]?.[row] ?? 0;
                  return (
                    <div key={seriesIndex} title={`${entry.name}: ${formatChartCellValue(value)}`} style={{ display: "flex", alignItems: "center", gap: 3 * scale }}>
                      <div style={{ width: `${(Math.max(0, value) / maxValue) * 100}%`, height: Math.max(4, 7 * scale), background: colorOf(seriesIndex), borderRadius: 2 }} />
                      {chart.showLabels ? <span style={{ fontSize: smallFont, opacity: 0.75 }}>{formatChartCellValue(value)}</span> : null}
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
        <div style={{ display: "flex", alignItems: "flex-end", justifyContent: "space-around", gap: 2 * scale, width: "100%", height: "100%" }}>
          {Array.from({ length: rowCount }, (_, row) => {
            const total = series.reduce((sum, _, seriesIndex) => sum + Math.max(0, values[seriesIndex]?.[row] ?? 0), 0);
            return (
              <div key={row} style={{ flex: 1, maxWidth: 64 * scale, height: "100%", display: "flex", flexDirection: "column", justifyContent: "flex-end", alignItems: "center", gap: 1 }}>
                {chart.showLabels && (chart.stacked || series.length === 1) ? <span style={{ fontSize: smallFont, opacity: 0.75 }}>{formatChartCellValue(chart.stacked ? total : values[0]?.[row] ?? 0)}</span> : null}
                <div style={{ width: "100%", height: `${(total / maxValue) * 100}%`, display: "flex", flexDirection: chart.stacked ? "column-reverse" : "row", alignItems: "flex-end", justifyContent: "center", gap: 1 }}>
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
    <div style={{ display: "flex", flexDirection: "column", width: "100%", height: "100%", padding: 6 * scale, gap: 3 * scale, fontSize: Math.max(8, 10 * scale), color: theme.bodyColor, overflow: "hidden", boxSizing: "border-box" }}>
      <div style={{ display: "flex", alignItems: "baseline", gap: 6 * scale, flexWrap: "wrap" }}>
        {chart.title ? <strong style={{ fontSize: Math.max(9, 12 * scale) }}>{chart.title}</strong> : null}
        <span style={{ opacity: 0.7 }}>{t(`calc.chart_${chart.kind}`)}</span>
        {chart.stacked ? <span style={{ opacity: 0.7 }}>· {t("impress.chartStacked")}</span> : null}
        {chart.showLabels ? <span style={{ opacity: 0.7 }}>· {t("impress.chartShowLabels")}</span> : null}
      </div>
      <div style={{ position: "relative", flex: 1, minHeight: 40 * scale, border: `1px dashed ${theme.accent}`, borderRadius: 4, display: "flex", alignItems: "flex-end", justifyContent: "center", gap: 4 * scale, padding: 4 * scale, overflow: "hidden" }}>
        {series.length === 0 ? (
          <span style={{ opacity: 0.6, textAlign: "center" }}>{t("impress.chartNoSeries")}</span>
        ) : plot ? (
          plot
        ) : chart.kind === "pie" ? (
          <div style={{ display: "flex", width: "100%", height: "100%" }}>
            {series.map((entry, index) => (
              <div key={index} title={`${entry.name}${entry.range ? ` · ${entry.range}` : ""}`} style={{ flex: 1, background: colorOf(index), opacity: 0.85 }} />
            ))}
          </div>
        ) : (
          series.map((entry, index) => (
            <div
              key={index}
              title={`${entry.name}${entry.range ? ` · ${entry.range}` : ""}`}
              style={{ flex: 1, maxWidth: 48 * scale, height: `${heights[index]}%`, background: colorOf(index), opacity: 0.85, borderRadius: 2 }}
            />
          ))
        )}
      </div>
      {hasValues && chart.kind !== "pie" && chart.kind !== "bar" ? (
        <div style={{ display: "flex", justifyContent: "space-around", gap: 2 * scale, opacity: 0.8, overflow: "hidden" }}>
          {Array.from({ length: rowCount }, (_, row) => (
            <span key={row} style={{ flex: 1, maxWidth: 64 * scale, textAlign: "center", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
              {labelOf(row)}
            </span>
          ))}
        </div>
      ) : null}
      <div style={{ display: "flex", justifyContent: "space-between", opacity: 0.75, gap: 6 * scale }}>
        <span>{chart.yTitle || ""}</span>
        <span style={{ textAlign: "center", flex: 1 }}>{hasValues ? t("impress.chartCachedData") : chart.categories || t("impress.chartNoCategories")}</span>
        <span>{chart.xTitle || ""}</span>
      </div>
      {chart.legend && series.length > 0 ? (
        <div style={{ display: "flex", flexWrap: "wrap", gap: 4 * scale, opacity: 0.9 }}>
          {series.map((entry, index) => (
            <span key={index} style={{ display: "inline-flex", alignItems: "center", gap: 3 * scale }}>
              <i style={{ width: 8 * scale, height: 8 * scale, background: colorOf(index), display: "inline-block", borderRadius: 2 }} />
              {entry.name || `${t("impress.chartSeries")} ${index + 1}`}
              {!hasValues && entry.range ? ` · ${entry.range}` : ""}
            </span>
          ))}
        </div>
      ) : null}
      {hasValues ? null : <div style={{ opacity: 0.55, fontSize: Math.max(7, 8 * scale) }}>{t("impress.chartSchematicNote")}</div>}
    </div>
  );
}

function SlideObjectView({ object, theme, scale, editing, onTextChange, onTextDone }: { object: SlideObject; theme: Theme; scale: number; editing: boolean; onTextChange: (text: string) => void; onTextDone: () => void }) {
  if (object.kind === "group") return null;
  if (object.kind === "image") {
    if (!object.image || object.image.dataBase64 === "") return <div className="slide-image-placeholder">{object.placeholder ?? "Double-click to add an image"}</div>;
    return <img className="slide-image" src={`data:${object.image.mime};base64,${object.image.dataBase64}`} alt={object.image.alt} draggable={false} />;
  }
  if (object.kind === "line" || object.kind === "arrow") {
    const line = object.line ?? { x2: object.w, y2: 0, beginArrow: false, endArrow: false, dash: "solid" };
    return (
      <svg width="100%" height="100%" viewBox={`0 0 ${object.w} ${object.h}`} preserveAspectRatio="none">
        <defs>
          <marker id={`arrow-${object.id}`} markerWidth="10" markerHeight="10" refX="7" refY="3" orient="auto">
            <path d="M0,0 L0,6 L8,3 z" fill={object.style?.stroke ?? theme.accent} />
          </marker>
        </defs>
        <line
          x1={0}
          y1={0}
          x2={line.x2}
          y2={line.y2}
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
          <textarea
            className="slide-text-editor"
            // eslint-disable-next-line jsx-a11y/no-autofocus -- the user just double-clicked the text object; the editor must take focus
            autoFocus
            defaultValue={object.text.paragraphs.map((paragraph) => paragraph.text).join("\n")}
            onBlur={(event) => {
              onTextChange(event.target.value);
              onTextDone();
            }}
            onKeyDown={(event) => {
              if (event.key === "Escape") onTextDone();
            }}
            style={{ fontSize: (object.text.paragraphs[0]?.sizePt ?? object.text.sizePt ?? 18) * scale, color: object.text.paragraphs[0]?.color ?? theme.bodyColor }}
          />
        ) : (
          <div className="slide-text" style={{ fontSize: (object.text.paragraphs[0]?.sizePt ?? object.text.sizePt ?? 18) * scale, color: object.text.paragraphs[0]?.color ?? theme.bodyColor, textAlign: (object.text.paragraphs[0]?.align ?? "left") as "left" | "center" | "right" }}>
            {object.text.paragraphs.map((paragraph, index) => (
              <p key={index} style={{ fontWeight: paragraph.bold ? 700 : undefined, fontStyle: paragraph.italic ? "italic" : undefined }}>
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
  const actualWidth = full ? slideWidth ?? deck.size.widthPt : width;
  const scale = actualWidth / deck.size.widthPt;
  const height = full ? slideHeight ?? deck.size.heightPt : (deck.size.heightPt * width) / deck.size.widthPt;
  const inherited = inheritedObjects(deck, slide);
  return (
    <div className="slide-preview" style={{ width: full ? "100%" : width, height: full ? "100%" : height, background: slide.background ?? theme.background, position: "relative", overflow: "hidden" }}>
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
          <ObjectTree object={object} path={[object.id]} depth={0} theme={theme} scale={full ? scale * 1.2 : scale} selectedKeys={INHERITED_KEYS} editingKey={null} interactive={false} />
        </div>
      ))}
      {[...slide.objects].sort((a, b) => a.z - b.z).map((object) => (
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
          <ObjectTree object={object} path={[object.id]} depth={0} theme={theme} scale={full ? scale * 1.2 : scale} selectedKeys={INHERITED_KEYS} editingKey={null} interactive={false} />
        </div>
      ))}
    </div>
  );
}
