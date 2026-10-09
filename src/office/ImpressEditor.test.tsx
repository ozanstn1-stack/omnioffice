import { fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// The editor module pulls in the session hook, which touches Tauri at import
// time; stubs keep the pure helpers testable in jsdom.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null), save: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-fs", () => ({ readFile: vi.fn(async () => new Uint8Array()) }));

import {
  animationObjectStyle,
  animationTimeline,
  formatClock,
  groupSelection,
  ImpressEditor,
  inheritedObjects,
  mergeChartPaste,
  objectsInRect,
  parseChartCellNumber,
  parseChartClipboard,
  refreshGroupBounds,
  scaleObject,
  syncChartDataRanges,
  translateObject,
  ungroupSelection,
} from "./ImpressEditor";
import { useOfficeTabs, type OfficeTab } from "../lib/office-store";
import { useSettings } from "../lib/store";
import {
  defaultRun,
  newAnimation,
  newDeck,
  newSlide,
  newSlideMaster,
  newSlideObject,
  newTextFrame,
  type ChartData,
  type Deck,
  type SlideObject,
} from "../lib/office-types";
import { setCaretOffset } from "./writer/caret";

// jsdom has no PointerEvent, so testing-library would fall back to a plain
// Event and drop button/clientX/pointerId. MouseEvent carries those fields and
// is the standard stand-in for pointer event tests.
if (typeof window.PointerEvent === "undefined") {
  window.PointerEvent = MouseEvent as unknown as typeof PointerEvent;
}

function Harness({ id }: { id: string }) {
  const tab = useOfficeTabs((state) => state.tabs.find((candidate) => candidate.id === id));
  if (!tab) return null;
  return <ImpressEditor tab={tab as OfficeTab & { model: Deck }} />;
}

function rect(id: string, x: number, y: number, w: number, h: number, z: number): SlideObject {
  return { ...newSlideObject("rect", x, y, w, h), id, z };
}

describe("group and ungroup transforms", () => {
  it("groups a selection into absolute children under a bounding box", () => {
    const a = rect("a", 100, 100, 100, 50, 1);
    const b = rect("b", 220, 140, 80, 80, 2);
    const grouped = groupSelection([a, b], ["a", "b"], "g1");

    expect(grouped).toHaveLength(1);
    const group = grouped[0];
    expect(group.kind).toBe("group");
    expect({ x: group.x, y: group.y, w: group.w, h: group.h }).toEqual({ x: 100, y: 100, w: 200, h: 120 });
    expect(group.children?.map((child) => child.id)).toEqual(["a", "b"]);
    // Child coordinates stay absolute, not relative to the group box.
    expect(group.children?.map((child) => ({ x: child.x, y: child.y }))).toEqual([
      { x: 100, y: 100 },
      { x: 220, y: 140 },
    ]);
  });

  it("returns group children to the slide on ungroup", () => {
    const grouped = groupSelection([rect("a", 100, 100, 100, 50, 1), rect("b", 220, 140, 80, 80, 2)], ["a", "b"], "g1");
    const result = ungroupSelection(grouped, ["g1"]);

    expect(result.map((object) => object.id)).toEqual(["a", "b"]);
    expect(result.map((object) => object.z)).toEqual([1, 2]);
    expect(result.every((object) => object.kind !== "group")).toBe(true);
  });

  it("translates every nested child by the same delta", () => {
    const group = groupSelection(
      [rect("a", 100, 100, 100, 50, 1), rect("b", 220, 140, 80, 80, 2)],
      ["a", "b"],
      "g1",
    )[0];
    const moved = translateObject(group, 10, -5);

    expect({ x: moved.x, y: moved.y }).toEqual({ x: 110, y: 95 });
    expect(moved.children?.map((child) => ({ x: child.x, y: child.y }))).toEqual([
      { x: 110, y: 95 },
      { x: 230, y: 135 },
    ]);
  });

  it("scales children about the group origin when the group resizes", () => {
    const group = groupSelection(
      [rect("a", 100, 100, 100, 50, 1), rect("b", 220, 140, 80, 80, 2)],
      ["a", "b"],
      "g1",
    )[0];
    const scaled = scaleObject(group, 2, 2, group.x, group.y);

    expect({ w: scaled.w, h: scaled.h }).toEqual({ w: 400, h: 240 });
    expect(scaled.children?.[1]).toMatchObject({ x: 340, y: 180, w: 160, h: 160 });
  });

  it("recomputes the group box after a child moved", () => {
    const group = groupSelection(
      [rect("a", 100, 100, 100, 50, 1), rect("b", 220, 140, 80, 80, 2)],
      ["a", "b"],
      "g1",
    )[0];
    const nudged = refreshGroupBounds([
      { ...group, children: [{ ...group.children![0], x: 0, y: 0 }, group.children![1]] },
    ]);

    expect({ x: nudged[0].x, y: nudged[0].y, w: nudged[0].w, h: nudged[0].h }).toEqual({ x: 0, y: 0, w: 300, h: 220 });
  });

  it("nests groups recursively and ungroups one level at a time", () => {
    const inner = groupSelection([rect("a", 0, 0, 50, 50, 1), rect("b", 60, 0, 50, 50, 2)], ["a", "b"], "g1");
    const outer = groupSelection([inner[0], rect("c", 0, 80, 40, 40, 2)], ["g1", "c"], "g2");

    expect(outer).toHaveLength(1);
    expect(outer[0].children?.map((child) => child.id)).toEqual(["g1", "c"]);
    expect(outer[0].children?.[0].kind).toBe("group");

    const oneLevel = ungroupSelection(outer, ["g2"]);
    expect(oneLevel.map((object) => object.id)).toEqual(["g1", "c"]);
    expect(oneLevel[0].kind).toBe("group");

    const twoLevels = ungroupSelection(oneLevel, ["g1"]);
    expect(twoLevels.map((object) => object.id)).toEqual(["a", "b", "c"]);
  });
});

describe("animation ordering", () => {
  it("merges withPrevious into the running step and anchors afterPrevious to its end", () => {
    const onClick = { ...newAnimation("o1", "entrance", "fade", "onClick", 1), id: "a1", durationMs: 400, delayMs: 0 };
    const withPrevious = {
      ...newAnimation("o2", "entrance", "zoom", "withPrevious", 2),
      id: "a2",
      durationMs: 900,
      delayMs: 100,
    };
    const afterPrevious = {
      ...newAnimation("o3", "exit", "fadeOut", "afterPrevious", 3),
      id: "a3",
      durationMs: 300,
      delayMs: 50,
    };

    // Deliberately unsorted input: the timeline orders by `order`.
    const steps = animationTimeline([afterPrevious, withPrevious, onClick]);

    expect(steps).toHaveLength(2);
    expect(steps[0].animations.map((animation) => animation.id)).toEqual(["a1", "a2"]);
    expect(steps[0].waitForClick).toBe(true);
    expect(steps[0].relativeTo).toBe("start");
    expect(steps[0].durationMs).toBe(1000);
    expect(steps[1].animations.map((animation) => animation.id)).toEqual(["a3"]);
    expect(steps[1].waitForClick).toBe(false);
    expect(steps[1].relativeTo).toBe("previousEnd");
    expect(steps[1].durationMs).toBe(350);
  });

  it("keeps auto steps free of click gating", () => {
    const auto = { ...newAnimation("o1", "entrance", "fade", "afterPrevious", 1), id: "b1" };
    const clicked = { ...newAnimation("o2", "entrance", "fade", "onClick", 2), id: "b2" };
    const steps = animationTimeline([auto, clicked]);

    expect(steps.map((step) => step.waitForClick)).toEqual([false, true]);
  });

  it("paints pending entrances hidden and finished exits hidden", () => {
    const entrance = { ...newAnimation("o1", "entrance", "flyIn", "onClick", 1), id: "e1" };
    const exit = { ...newAnimation("o1", "exit", "fadeOut", "onClick", 1), id: "x1" };

    expect(animationObjectStyle("o1", [entrance], {}, {})).toMatchObject({ opacity: 0, transform: "translateY(40px)" });
    expect(animationObjectStyle("o1", [entrance], {}, { e1: true })).toEqual({});
    expect(animationObjectStyle("o1", [exit], {}, { x1: true })).toEqual({ opacity: 0 });

    const running = { e1: { effect: entrance, phase: "to" as const } };
    const style = animationObjectStyle("o1", [entrance], running, {});
    expect(style.opacity).toBeUndefined();
    expect(String(style.transition)).toContain("500ms");
  });
});

describe("master and layout inheritance", () => {
  it("returns master objects then layout objects with prefixed ids", () => {
    const deck = newDeck();
    const master = newSlideMaster("Master");
    master.objects = [{ ...rect("m1", 0, 0, 100, 50, 1), placeholder: "title" }];
    const layout = master.layouts[1];
    layout.objects = [{ ...rect("l1", 10, 10, 100, 50, 1), placeholder: "content" }];
    deck.masters = [master];
    const slide = { ...newSlide("titleContent"), masterId: master.id, layoutId: layout.id, objects: [] };

    expect(inheritedObjects(deck, slide).map((object) => object.id)).toEqual(["master:m1", `layout:l1`]);
  });

  it("suppresses a placeholder role the slide already fills", () => {
    const deck = newDeck();
    const master = newSlideMaster("Master");
    master.objects = [{ ...rect("m1", 0, 0, 100, 50, 1), placeholder: "title" }];
    const layout = master.layouts[1];
    layout.objects = [{ ...rect("l1", 10, 10, 100, 50, 1), placeholder: "content" }];
    deck.masters = [master];
    const slide = {
      ...newSlide("titleContent"),
      masterId: master.id,
      layoutId: layout.id,
      objects: [{ ...rect("s1", 5, 5, 10, 10, 1), placeholder: "content" }],
    };

    expect(inheritedObjects(deck, slide).map((object) => object.id)).toEqual(["master:m1"]);
  });
});

describe("presenter clock", () => {
  it("formats elapsed time as mm:ss", () => {
    expect(formatClock(0)).toBe("00:00");
    expect(formatClock(9_000)).toBe("00:09");
    expect(formatClock(65_000)).toBe("01:05");
    expect(formatClock(3_600_000)).toBe("60:00");
  });
});

describe("marquee selection", () => {
  it("keeps only the objects the rectangle touches", () => {
    const a = rect("a", 0, 0, 100, 100, 1);
    const b = rect("b", 200, 200, 100, 100, 2);
    expect(objectsInRect([a, b], { x: 50, y: 50, w: 100, h: 100 }).map((object) => object.id)).toEqual(["a"]);
    // Overlapping boxes intersect; boxes that only touch an edge do not.
    expect(objectsInRect([a, b], { x: 150, y: 150, w: 100, h: 100 }).map((object) => object.id)).toEqual(["b"]);
    expect(objectsInRect([a, b], { x: 100, y: 100, w: 100, h: 100 }).map((object) => object.id)).toEqual([]);
  });
});

describe("pointer gestures on the slide canvas", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
  });

  it("moves an object with a single pointer drag", () => {
    const deck = newDeck("Touch");
    deck.slides[0].objects = [
      { ...newSlideObject("rect", 100, 100, 200, 100), id: "r1", z: 1 },
      { ...newSlideObject("rect", 500, 300, 100, 100), id: "r2", z: 2 },
    ];
    const id = useOfficeTabs.getState().create("impress", "Touch", deck);
    render(<Harness id={id} />);

    const objects = document.querySelectorAll<HTMLElement>(".slide-object:not(.is-inherited)");
    expect(objects.length).toBe(2);
    fireEvent.pointerDown(objects[0], { pointerId: 1, pointerType: "mouse", button: 0, clientX: 100, clientY: 100 });
    fireEvent.pointerMove(window, { pointerId: 1, pointerType: "mouse", clientX: 180, clientY: 130 });
    fireEvent.pointerUp(window, { pointerId: 1, pointerType: "mouse" });

    const model = useOfficeTabs.getState().tabs[0].model as Deck;
    // jsdom has no layout: the canvas falls back to scale 0.2, so 80 screen px
    // are 400 slide points.
    expect(model.slides[0].objects[0].x).toBe(500);
    expect(model.slides[0].objects[0].y).toBe(250);
  });

  it("selects with a marquee and clears on a plain canvas tap", () => {
    const deck = newDeck("Touch");
    deck.slides[0].objects = [
      { ...newSlideObject("rect", 100, 100, 200, 100), id: "r1", z: 1 },
      { ...newSlideObject("rect", 500, 300, 100, 100), id: "r2", z: 2 },
    ];
    const id = useOfficeTabs.getState().create("impress", "Touch", deck);
    render(<Harness id={id} />);

    const canvas = document.querySelector<HTMLElement>(".slide-canvas")!;
    fireEvent.pointerDown(canvas, { pointerId: 3, pointerType: "mouse", button: 0, clientX: 0, clientY: 0 });
    fireEvent.pointerMove(window, { pointerId: 3, pointerType: "mouse", clientX: 60, clientY: 60 });
    expect(document.querySelector(".slide-marquee")).not.toBeNull();
    fireEvent.pointerUp(window, { pointerId: 3, pointerType: "mouse", clientX: 60, clientY: 60 });

    const selected = document.querySelectorAll(".slide-object.is-selected");
    expect(selected.length).toBe(1);
    expect(document.querySelector(".slide-marquee")).toBeNull();
  });

  it("enters text editing when a text object is double-tapped on touch", () => {
    const deck = newDeck("Touch");
    deck.slides[0].objects = [
      { ...newSlideObject("rect", 100, 100, 200, 100), id: "r1", z: 1, text: newTextFrame("Hello", 20) },
    ];
    const id = useOfficeTabs.getState().create("impress", "Touch", deck);
    render(<Harness id={id} />);

    const object = document.querySelector<HTMLElement>(".slide-object:not(.is-inherited)")!;
    const tap = (pointerId: number) => {
      fireEvent.pointerDown(object, { pointerId, pointerType: "touch", button: 0, clientX: 40, clientY: 40 });
      fireEvent.pointerUp(window, { pointerId, pointerType: "touch", clientX: 40, clientY: 40 });
    };

    tap(1);
    expect(document.querySelector(".slide-text-editor")).toBeNull();
    tap(2);
    expect(document.querySelector(".slide-text-editor")).not.toBeNull();
  });

  it("keeps paragraph structure and preserves runs when an imported frame is edited", () => {
    const deck = newDeck("Import");
    const frame = newTextFrame("", 20);
    const [base] = frame.paragraphs;
    frame.paragraphs = [
      { ...base, text: "Intro", runs: [{ ...defaultRun("Intro"), bold: true }] },
      { ...base, text: "Detail", level: 1, bullet: true, runs: [defaultRun("Detail")] },
    ];
    deck.slides[0].objects = [{ ...newSlideObject("rect", 100, 100, 200, 100), id: "r1", z: 1, text: frame }];
    const id = useOfficeTabs.getState().create("impress", "Import", deck);
    render(<Harness id={id} />);

    const object = document.querySelector<HTMLElement>(".slide-object:not(.is-inherited)")!;
    for (const pointerId of [1, 2]) {
      fireEvent.pointerDown(object, { pointerId, pointerType: "touch", button: 0, clientX: 40, clientY: 40 });
      fireEvent.pointerUp(window, { pointerId, pointerType: "touch", clientX: 40, clientY: 40 });
    }
    const editor = document.querySelector<HTMLElement>(".slide-text-editor")!;
    const surfaces = editor.querySelectorAll<HTMLElement>(".slide-text-paragraph");
    expect(surfaces).toHaveLength(2);
    // Model a DOM edit in the second paragraph and commit it by blurring.
    surfaces[1].textContent = "Detail changed";
    fireEvent.input(surfaces[1]);
    fireEvent.blur(surfaces[1]);

    const paragraphs = (useOfficeTabs.getState().tabs[0].model as Deck).slides[0].objects[0].text!.paragraphs;
    expect(paragraphs.map((paragraph) => paragraph.text)).toEqual(["Intro", "Detail changed"]);
    // The untouched paragraph keeps its bold run.
    expect(paragraphs[0].runs).toHaveLength(1);
    expect(paragraphs[0].runs[0]).toMatchObject({ text: "Intro", bold: true });
    // The edited paragraph keeps its paragraph formatting and its run mapping.
    expect(paragraphs[1]).toMatchObject({ level: 1, bullet: true });
    expect(paragraphs[1].runs.map((run) => run.text).join("")).toBe("Detail changed");
  });

  it("splits a paragraph on Enter and merges it back on Backspace as undo steps", () => {
    const deck = newDeck("Structure");
    deck.slides[0].objects = [
      { ...newSlideObject("rect", 100, 100, 300, 100), id: "r1", z: 1, text: newTextFrame("Intro", 20) },
    ];
    const id = useOfficeTabs.getState().create("impress", "Structure", deck);
    render(<Harness id={id} />);

    fireEvent.doubleClick(document.querySelector(".slide-object:not(.is-inherited)")!);
    const paragraphTexts = () =>
      (useOfficeTabs.getState().tabs[0].model as Deck).slides[0].objects[0].text!.paragraphs.map((p) => p.text);

    const first = () => document.querySelectorAll<HTMLElement>(".slide-text-paragraph")[0];
    setCaretOffset(first(), 3);
    fireEvent.keyDown(first(), { key: "Enter" });
    expect(paragraphTexts()).toEqual(["Int", "ro"]);

    // The structural edit was one undo step: Ctrl+Z restores the original.
    fireEvent.keyDown(window, { key: "z", ctrlKey: true });
    expect(paragraphTexts()).toEqual(["Intro"]);

    // Backspace at offset 0 merges the second paragraph into the first.
    setCaretOffset(first(), 3);
    fireEvent.keyDown(first(), { key: "Enter" });
    const second = () => document.querySelectorAll<HTMLElement>(".slide-text-paragraph")[1];
    setCaretOffset(second(), 0);
    fireEvent.keyDown(second(), { key: "Backspace" });
    expect(paragraphTexts()).toEqual(["Intro"]);
  });

  it("applies toolbar formatting to the edited paragraph", () => {
    const deck = newDeck("Format");
    deck.slides[0].objects = [
      { ...newSlideObject("rect", 100, 100, 300, 100), id: "r1", z: 1, text: newTextFrame("Hello", 20) },
    ];
    const id = useOfficeTabs.getState().create("impress", "Format", deck);
    render(<Harness id={id} />);

    fireEvent.doubleClick(document.querySelector(".slide-object:not(.is-inherited)")!);
    fireEvent.click(screen.getByTitle("Bold"));
    fireEvent.click(screen.getByTitle("Italic"));

    const paragraph = (useOfficeTabs.getState().tabs[0].model as Deck).slides[0].objects[0].text!.paragraphs[0];
    expect(paragraph.bold).toBe(true);
    expect(paragraph.italic).toBe(true);
  });
});

describe("chart data helpers", () => {
  const base: ChartData = {
    kind: "column",
    title: "Revenue",
    categories: "Data!$A$2:$A$4",
    series: [
      { name: "A", range: "Data!$B$2:$B$4", color: null },
      { name: "B", range: "", color: null },
    ],
    legend: true,
    xTitle: "",
    yTitle: "",
    stacked: false,
    showLabels: false,
    categoriesCache: ["Q1", "Q2", "Q3"],
    seriesValuesCache: [[1, 2], []],
  };

  it("parses tab-, comma- and newline-separated clipboard text", () => {
    expect(parseChartClipboard("Region\tSales\nNorth\t10\nSouth,20\n")).toEqual([
      ["Region", "Sales"],
      ["North", "10"],
      ["South", "20"],
    ]);
    // Blank interior lines stay so pasted columns keep their row alignment.
    expect(parseChartClipboard("1\n\n3")).toEqual([["1"], [""], ["3"]]);
  });

  it("parses locale-independent decimals and rejects everything else", () => {
    expect(parseChartCellNumber("1.5")).toBe(1.5);
    expect(parseChartCellNumber("-2e2")).toBe(-200);
    expect(parseChartCellNumber(".5")).toBe(0.5);
    expect(parseChartCellNumber("+3")).toBe(3);
    // Comma is a clipboard separator, not a decimal mark.
    expect(parseChartCellNumber("1,5")).toBeNull();
    expect(parseChartCellNumber("")).toBeNull();
    expect(parseChartCellNumber("abc")).toBeNull();
    expect(parseChartCellNumber("Infinity")).toBeNull();
  });

  it("merges a pasted block into the caches and skips invalid numbers", () => {
    const merged = mergeChartPaste(base, { row: 1, column: 1 }, [["7"], ["-3"], ["oops"]]);
    expect(merged.categoriesCache).toEqual(["Q1", "Q2", "Q3"]);
    expect(merged.seriesValuesCache).toEqual([[1, 7, -3], []]);
  });

  it("pastes labels into the category column", () => {
    const merged = mergeChartPaste(base, { row: 1, column: 0 }, [["R2"], ["R3"]]);
    expect(merged.categoriesCache).toEqual(["Q1", "R2", "R3"]);
  });

  it("zero-fills a gap when a pasted value skips rows, matching the reader", () => {
    const merged = mergeChartPaste(base, { row: 3, column: 1 }, [["5"]]);
    expect(merged.seriesValuesCache![0]).toEqual([1, 2, 0, 5]);
  });

  it("derives workbook ranges from caches and leaves empty series alone", () => {
    const synced = syncChartDataRanges({
      ...base,
      categoriesCache: ["Q1", "Q2"],
      seriesValuesCache: [[], [4, 5, 6]],
    });
    expect(synced.categories).toBe("Sheet1!$A$2:$A$3");
    expect(synced.series[0].range).toBe("Data!$B$2:$B$4");
    expect(synced.series[1].range).toBe("Sheet1!$C$2:$C$4");
  });
});

describe("Impress chart data editor", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
  });

  function chartDeck(overrides: Partial<ChartData> = {}): Deck {
    const deck = newDeck("Charts");
    const object = newSlideObject("chart", 100, 100, 400, 260);
    object.chart = {
      kind: "column",
      title: "Revenue",
      categories: "Data!$A$2:$A$4",
      series: [{ name: "Sales", range: "Data!$B$2:$B$4", color: null }],
      legend: true,
      xTitle: "",
      yTitle: "",
      stacked: false,
      showLabels: false,
      categoriesCache: ["Q1", "Q2", "Q3"],
      seriesValuesCache: [[10, 20, 30]],
      ...overrides,
    };
    deck.slides[0].objects = [object];
    return deck;
  }

  /** Creates a tab with the deck, opens the chart dialog by double-click. */
  function openChartDialog(deck: Deck): { id: string; dialog: HTMLElement } {
    const id = useOfficeTabs.getState().create("impress", "Charts", deck);
    render(<Harness id={id} />);
    fireEvent.doubleClick(document.querySelector(".slide-object:not(.is-inherited)")!);
    return { id, dialog: document.querySelector('[role="dialog"]') as HTMLElement };
  }

  function cell(dialog: HTMLElement, row: number, column: number): HTMLInputElement {
    return dialog.querySelector<HTMLInputElement>(`[data-cell="${row}:${column}"]`)!;
  }

  function savedChart(id: string): ChartData {
    const model = useOfficeTabs.getState().tabs.find((tab) => tab.id === id)!.model as Deck;
    return model.slides[0].objects[0].chart!;
  }

  it("prefills the data grid from the imported chart caches", () => {
    const { dialog } = openChartDialog(chartDeck());
    expect(dialog).not.toBeNull();
    expect(cell(dialog, 0, 0).value).toBe("Q1");
    expect(cell(dialog, 2, 0).value).toBe("Q3");
    expect(cell(dialog, 1, 1).value).toBe("20");
    expect(dialog.querySelector<HTMLInputElement>('[data-series-name="0"]')!.value).toBe("Sales");
  });

  it("opens a range-only chart with an empty grid and the range hint", () => {
    const { dialog } = openChartDialog(chartDeck({ categoriesCache: [], seriesValuesCache: [] }));
    expect(cell(dialog, 0, 0).value).toBe("");
    expect(cell(dialog, 0, 1).value).toBe("");
    expect(within(dialog).getByText(/only carries cell ranges/)).toBeTruthy();
  });

  it("commits typed values and saves caches plus workbook-backed ranges", () => {
    const { id, dialog } = openChartDialog(chartDeck());
    fireEvent.change(cell(dialog, 1, 1), { target: { value: "25.5" } });
    fireEvent.change(cell(dialog, 2, 0), { target: { value: "Q3b" } });
    fireEvent.click(within(dialog).getByText("Save"));

    const chart = savedChart(id);
    expect(chart.categoriesCache).toEqual(["Q1", "Q2", "Q3b"]);
    expect(chart.seriesValuesCache).toEqual([[10, 25.5, 30]]);
    // Data was edited, so the ranges now point at the embedded workbook the
    // Rust exporter builds from these caches.
    expect(chart.categories).toBe("Sheet1!$A$2:$A$4");
    expect(chart.series[0].range).toBe("Sheet1!$B$2:$B$4");
  });

  it("does not commit invalid input and keeps imported ranges untouched", () => {
    const { id, dialog } = openChartDialog(chartDeck());
    const target = cell(dialog, 0, 1);
    fireEvent.change(target, { target: { value: "1,5" } });
    expect(target.value).toBe("1,5");
    fireEvent.blur(target);
    expect(target.value).toBe("10");
    fireEvent.click(within(dialog).getByText("Save"));

    const chart = savedChart(id);
    expect(chart.seriesValuesCache).toEqual([[10, 20, 30]]);
    expect(chart.categories).toBe("Data!$A$2:$A$4");
    expect(chart.series[0].range).toBe("Data!$B$2:$B$4");
  });

  it("pastes a tab and newline separated block at the focused cell", () => {
    const { id, dialog } = openChartDialog(chartDeck());
    const target = cell(dialog, 1, 1);
    fireEvent.pointerDown(target, { pointerId: 1, button: 0 });
    fireEvent.paste(target, { clipboardData: { getData: () => "41\n42\n" } });
    fireEvent.click(within(dialog).getByText("Save"));

    const chart = savedChart(id);
    expect(chart.seriesValuesCache).toEqual([[10, 41, 42]]);
  });

  it("adds a series column with an empty value list aligned with the others", () => {
    const { id, dialog } = openChartDialog(chartDeck());
    fireEvent.click(dialog.querySelector<HTMLButtonElement>('button[title="Add series"]')!);
    fireEvent.change(cell(dialog, 0, 2), { target: { value: "9" } });
    fireEvent.click(within(dialog).getByText("Save"));

    const chart = savedChart(id);
    expect(chart.series).toHaveLength(2);
    expect(chart.seriesValuesCache).toEqual([[10, 20, 30], [9]]);
  });
});

describe("transition names", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
  });
  afterEach(() => {
    useSettings.setState((state) => ({ settings: { ...state.settings, language: "en" } }));
  });

  /** The labels of the slide transition dropdown, in the given app language. */
  function transitionLabels(language: "en" | "tr"): string[] {
    useSettings.setState((state) => ({ settings: { ...state.settings, language } }));
    const id = useOfficeTabs.getState().create("impress", "Transitions", newDeck("Transitions"));
    render(<Harness id={id} />);
    fireEvent.click(screen.getByRole("button", { name: language === "en" ? "Transitions" : "Geçişler" }));
    const select = document.querySelector<HTMLSelectElement>("select.tool-select")!;
    return [...select.options].map((option) => option.textContent ?? "");
  }

  it("lists the transitions in English", () => {
    expect(transitionLabels("en")).toEqual(["None", "Fade", "Slide", "Push", "Wipe"]);
  });

  it("lists the transitions in Turkish instead of English words", () => {
    expect(transitionLabels("tr")).toEqual(["Yok", "Solma", "Kaydırma", "İtme", "Silme"]);
  });
});

describe("keyboard shortcuts", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
  });

  function seed(): string {
    const deck = newDeck("Keys");
    deck.slides[0].objects = [rect("r1", 100, 100, 200, 100, 1), rect("r2", 500, 300, 100, 100, 2)];
    const id = useOfficeTabs.getState().create("impress", "Keys", deck);
    render(<Harness id={id} />);
    return id;
  }

  function modelObjects(id: string): SlideObject[] {
    return (useOfficeTabs.getState().tabs.find((tab) => tab.id === id)!.model as Deck).slides[0].objects;
  }

  function selectObject(index: number, pointerId = 1): void {
    const object = document.querySelectorAll<HTMLElement>(".slide-object:not(.is-inherited)")[index];
    fireEvent.pointerDown(object, { pointerId, pointerType: "mouse", button: 0, clientX: 10, clientY: 10 });
    fireEvent.pointerUp(window, { pointerId, pointerType: "mouse" });
  }

  it("deletes the selection with Delete and restores it with Ctrl+Z / Ctrl+Y", () => {
    const id = seed();
    selectObject(0);
    fireEvent.keyDown(window, { key: "Delete" });
    expect(modelObjects(id)).toHaveLength(1);
    fireEvent.keyDown(window, { key: "z", ctrlKey: true });
    expect(modelObjects(id)).toHaveLength(2);
    fireEvent.keyDown(window, { key: "y", ctrlKey: true });
    expect(modelObjects(id)).toHaveLength(1);
  });

  it("duplicates with Ctrl+D and pastes a copied object with Ctrl+V", () => {
    const id = seed();
    selectObject(0);
    fireEvent.keyDown(window, { key: "d", ctrlKey: true });
    expect(modelObjects(id)).toHaveLength(3);
    // The duplicate is offset by 16 points.
    expect(modelObjects(id)[2]).toMatchObject({ x: 116, y: 116 });

    selectObject(0, 2);
    fireEvent.keyDown(window, { key: "c", ctrlKey: true });
    fireEvent.keyDown(window, { key: "v", ctrlKey: true });
    expect(modelObjects(id)).toHaveLength(4);
    expect(modelObjects(id)[3]).toMatchObject({ x: 116, y: 116 });
  });

  it("cuts with Ctrl+X", () => {
    const id = seed();
    selectObject(0);
    fireEvent.keyDown(window, { key: "x", ctrlKey: true });
    expect(modelObjects(id)).toHaveLength(1);
  });

  it("scrolls the slide stage with the arrow keys when nothing is edited", () => {
    seed();
    const stage = document.querySelector<HTMLElement>(".slide-stage")!;
    fireEvent.keyDown(window, { key: "ArrowRight" });
    expect(stage.scrollLeft).toBe(60);
    fireEvent.keyDown(window, { key: "ArrowDown" });
    expect(stage.scrollTop).toBe(60);
    fireEvent.keyDown(window, { key: "ArrowLeft" });
    expect(stage.scrollLeft).toBe(0);
    fireEvent.keyDown(window, { key: "ArrowUp" });
    expect(stage.scrollTop).toBe(0);
  });

  it("does not fire shortcuts while a text field has focus", () => {
    const id = seed();
    selectObject(0);
    fireEvent.keyDown(document.querySelector(".notes-input")!, { key: "Delete" });
    expect(modelObjects(id)).toHaveLength(2);
  });
});

describe("align and distribute", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
  });

  function threeObjects(): string {
    const deck = newDeck("Arrange");
    deck.slides[0].objects = [
      rect("a", 100, 50, 50, 30, 1),
      rect("b", 200, 90, 100, 30, 2),
      rect("c", 400, 130, 50, 30, 3),
    ];
    const id = useOfficeTabs.getState().create("impress", "Arrange", deck);
    render(<Harness id={id} />);
    return id;
  }

  function selectAll(): void {
    document.querySelectorAll<HTMLElement>(".slide-object:not(.is-inherited)").forEach((object, index) => {
      fireEvent.pointerDown(object, {
        pointerId: index + 1,
        pointerType: "mouse",
        button: 0,
        shiftKey: true,
        clientX: 10,
        clientY: 10,
      });
      fireEvent.pointerUp(window, { pointerId: index + 1, pointerType: "mouse" });
    });
  }

  function modelObjects(id: string): SlideObject[] {
    return (useOfficeTabs.getState().tabs.find((tab) => tab.id === id)!.model as Deck).slides[0].objects;
  }

  it("aligns a multi-object selection to its bounding box", () => {
    const id = threeObjects();
    selectAll();
    fireEvent.click(screen.getByTitle("Align left"));
    expect(modelObjects(id).map((object) => object.x)).toEqual([100, 100, 100]);

    fireEvent.click(screen.getByTitle("Align top"));
    expect(modelObjects(id).map((object) => object.y)).toEqual([50, 50, 50]);
  });

  it("keeps slide-edge alignment for a single object", () => {
    const id = threeObjects();
    fireEvent.pointerDown(document.querySelectorAll<HTMLElement>(".slide-object:not(.is-inherited)")[0], {
      pointerId: 1,
      pointerType: "mouse",
      button: 0,
      clientX: 10,
      clientY: 10,
    });
    fireEvent.pointerUp(window, { pointerId: 1, pointerType: "mouse" });
    fireEvent.click(screen.getByTitle("Align left"));
    expect(modelObjects(id)[0].x).toBe(0);
  });

  it("distributes three objects with equal gaps", () => {
    const id = threeObjects();
    selectAll();
    fireEvent.click(screen.getByTitle("Distribute horizontally"));
    // Span 100..450 with widths 50+100+50 leaves a 75 gap between edges.
    expect(modelObjects(id).map((object) => object.x)).toEqual([100, 225, 400]);
  });
});

describe("hidden slides", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
  });

  it("marks hidden slides and skips them when the show advances", () => {
    const deck = newDeck("Hidden");
    const slides = [newSlide(), { ...newSlide(), hidden: true }, newSlide()];
    deck.slides = slides;
    const id = useOfficeTabs.getState().create("impress", "Hidden", deck);
    render(<Harness id={id} />);

    expect(document.querySelectorAll(".slide-hidden-badge")).toHaveLength(1);

    fireEvent.click(screen.getByRole("button", { name: "View" }));
    fireEvent.click(screen.getByRole("button", { name: "Start show" }));
    expect(document.querySelector(".slideshow-nav span")!.textContent).toContain("1 / 3");
    fireEvent.click(document.querySelector(".slideshow")!);
    // Slide 2 is hidden, so the show lands on slide 3.
    expect(document.querySelector(".slideshow-nav span")!.textContent).toContain("3 / 3");
  });

  it("toggles slide.hidden from the properties panel", () => {
    const deck = newDeck("Hide toggle");
    useOfficeTabs.getState().create("impress", "Hide toggle", deck);
    render(<Harness id={useOfficeTabs.getState().tabs[0].id} />);

    fireEvent.click(screen.getByLabelText("Hidden"));
    expect((useOfficeTabs.getState().tabs[0].model as Deck).slides[0].hidden).toBe(true);
  });
});

describe("header and footer", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
  });

  it("renders slide numbers for every slide when enabled and removes them when off", () => {
    const deck = newDeck("Footers");
    deck.slides = [newSlide(), newSlide()];
    deck.footer = {
      enabled: true,
      text: "Office",
      showText: true,
      showSlideNumber: true,
      showDate: false,
      dateText: "",
    };
    const id = useOfficeTabs.getState().create("impress", "Footers", deck);
    render(<Harness id={id} />);

    const numbers = [...document.querySelectorAll(".slide-footer-right")].map((element) => element.textContent);
    // One footer per slide thumbnail plus the editor canvas.
    expect(numbers.slice(0, 2)).toEqual(["1", "2"]);
    expect(numbers).toHaveLength(3);

    // Turn the numbers off through the Design ribbon dialog.
    fireEvent.click(screen.getByRole("button", { name: "Design" }));
    fireEvent.click(screen.getByRole("button", { name: "Header & footer" }));
    const dialog = document.querySelector<HTMLElement>('[role="dialog"]')!;
    fireEvent.click(within(dialog).getByLabelText("Slide number"));
    fireEvent.click(within(dialog).getByLabelText("Close"));

    const after = [...document.querySelectorAll(".slide-footer-right")].map((element) => element.textContent);
    expect(after.every((value) => value === "")).toBe(true);
  });
});

describe("slide sorter", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
  });

  it("switches to a grid and reorders slides with a pointer drag", () => {
    const deck = newDeck("Sorter");
    const slides = [newSlide(), newSlide(), newSlide()];
    slides.forEach((slide, index) => {
      slide.objects = [{ ...newSlideObject("rect", 10, 10, 50, 50), id: `s${index}`, z: 1 }];
    });
    deck.slides = slides;
    const id = useOfficeTabs.getState().create("impress", "Sorter", deck);
    render(<Harness id={id} />);

    fireEvent.click(screen.getByRole("button", { name: "View" }));
    fireEvent.click(screen.getByRole("button", { name: "Slide sorter" }));
    const items = document.querySelectorAll<HTMLElement>(".sorter-item");
    expect(items).toHaveLength(3);

    fireEvent.pointerDown(items[0], { pointerId: 1, pointerType: "mouse", button: 0, clientX: 10, clientY: 10 });
    fireEvent.pointerEnter(items[2]);
    fireEvent.pointerUp(window, { pointerId: 1, pointerType: "mouse" });

    const model = useOfficeTabs.getState().tabs.find((tab) => tab.id === id)!.model as Deck;
    expect(model.slides.map((slide) => slide.id)).toEqual([slides[1].id, slides[2].id, slides[0].id]);
  });
});

describe("connectors", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
  });

  it("creates a connector between two shapes and follows a moved shape", () => {
    const deck = newDeck("Connect");
    deck.slides[0].objects = [rect("a", 100, 100, 200, 100, 1), rect("b", 500, 100, 200, 100, 2)];
    const id = useOfficeTabs.getState().create("impress", "Connect", deck);
    render(<Harness id={id} />);

    fireEvent.click(screen.getByRole("button", { name: "Connector" }));
    const objects = () => document.querySelectorAll<HTMLElement>(".slide-object:not(.is-inherited)");
    fireEvent.pointerDown(objects()[0], { pointerId: 1, pointerType: "mouse", button: 0, clientX: 10, clientY: 10 });
    fireEvent.pointerUp(window, { pointerId: 1, pointerType: "mouse" });
    fireEvent.pointerDown(objects()[1], { pointerId: 2, pointerType: "mouse", button: 0, clientX: 10, clientY: 10 });
    fireEvent.pointerUp(window, { pointerId: 2, pointerType: "mouse" });

    const modelObjects = () =>
      (useOfficeTabs.getState().tabs.find((tab) => tab.id === id)!.model as Deck).slides[0].objects;
    const line = modelObjects().find((object) => object.kind === "line")!;
    expect(line.line).toMatchObject({ beginObject: "a", endObject: "b" });

    // Drag the first shape; the connector is recomputed with it. Both ends
    // anchor on the facing edges, so the box starts at the moved shape's edge.
    fireEvent.pointerDown(objects()[0], { pointerId: 3, pointerType: "mouse", button: 0, clientX: 0, clientY: 0 });
    fireEvent.pointerMove(window, { pointerId: 3, pointerType: "mouse", clientX: 10, clientY: 0 });
    fireEvent.pointerUp(window, { pointerId: 3, pointerType: "mouse" });
    const moved = modelObjects().find((object) => object.kind === "line")!;
    expect(moved).toMatchObject({ x: 350, y: 150, w: 150, h: 1 });
  });
});

describe("image crop", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
  });

  it("applies a crop from the properties panel and previews it", () => {
    const deck = newDeck("Crop");
    const image = { ...newSlideObject("image", 100, 100, 200, 100), id: "img1", z: 1 };
    image.image = { name: "a.png", mime: "image/png", dataBase64: "aGk=", alt: "" };
    deck.slides[0].objects = [image];
    useOfficeTabs.getState().create("impress", "Crop", deck);
    render(<Harness id={useOfficeTabs.getState().tabs[0].id} />);

    fireEvent.pointerDown(document.querySelector(".slide-object:not(.is-inherited)")!, {
      pointerId: 1,
      pointerType: "mouse",
      button: 0,
      clientX: 10,
      clientY: 10,
    });
    fireEvent.pointerUp(window, { pointerId: 1, pointerType: "mouse" });
    fireEvent.click(screen.getByRole("button", { name: "Crop" }));

    const handle = document.querySelector<HTMLElement>('[data-crop-handle="left"]')!;
    expect(handle).not.toBeNull();
    fireEvent.pointerDown(handle, { pointerId: 2, pointerType: "mouse", button: 0, clientX: 0, clientY: 0 });
    fireEvent.pointerMove(window, { pointerId: 2, pointerType: "mouse", clientX: 10, clientY: 0 });
    fireEvent.pointerUp(window, { pointerId: 2, pointerType: "mouse" });

    const crop = (useOfficeTabs.getState().tabs[0].model as Deck).slides[0].objects[0].image!.crop!;
    // 10 screen px at the 0.2 jsdom scale over a 200-point image.
    expect(crop.left).toBeCloseTo(0.25, 5);
    expect(crop.left + crop.right).toBeLessThan(1);

    const preview = document.querySelector<HTMLImageElement>(".slide-image-crop img")!;
    expect(preview.style.width).toContain("133.3");
  });
});

describe("slideshow ink tools", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
  });

  it("draws a pen stroke, keeps the click from advancing and erases the stroke", () => {
    const deck = newDeck("Ink");
    deck.slides = [newSlide(), newSlide()];
    useOfficeTabs.getState().create("impress", "Ink", deck);
    render(<Harness id={useOfficeTabs.getState().tabs[0].id} />);

    fireEvent.click(screen.getByRole("button", { name: "View" }));
    fireEvent.click(screen.getByRole("button", { name: "Start show" }));
    fireEvent.click(screen.getByTitle("Pen"));
    const area = document.querySelector<HTMLElement>(".slideshow-slide")!;
    fireEvent.pointerDown(area, { pointerId: 1, pointerType: "mouse", button: 0, clientX: 10, clientY: 10 });
    fireEvent.pointerMove(area, { pointerId: 1, pointerType: "mouse", clientX: 60, clientY: 40 });
    fireEvent.pointerUp(area, { pointerId: 1, pointerType: "mouse", clientX: 60, clientY: 40 });
    expect(document.querySelectorAll(".slideshow-ink path")).toHaveLength(1);

    // With a tool active a click must not advance the show.
    fireEvent.click(document.querySelector(".slideshow")!);
    expect(document.querySelector(".slideshow-nav span")!.textContent).toContain("1 / 2");

    fireEvent.click(screen.getByTitle("Laser pointer"));
    fireEvent.pointerMove(area, { pointerId: 2, pointerType: "mouse", clientX: 30, clientY: 30 });
    expect(document.querySelector(".slideshow-laser")).not.toBeNull();

    // The eraser removes the stroke under the pointer.
    fireEvent.click(screen.getByTitle("Eraser"));
    fireEvent.pointerDown(area, { pointerId: 3, pointerType: "mouse", button: 0, clientX: 60, clientY: 40 });
    expect(document.querySelectorAll(".slideshow-ink path")).toHaveLength(0);
  });
});
