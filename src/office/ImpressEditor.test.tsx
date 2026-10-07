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

  it("keeps paragraph structure and drops stale runs when an imported frame is edited", () => {
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
    const editor = document.querySelector<HTMLTextAreaElement>(".slide-text-editor")!;
    fireEvent.change(editor, { target: { value: "Intro\nDetail changed" } });
    fireEvent.blur(editor);

    const paragraphs = (useOfficeTabs.getState().tabs[0].model as Deck).slides[0].objects[0].text!.paragraphs;
    expect(paragraphs.map((paragraph) => paragraph.text)).toEqual(["Intro", "Detail changed"]);
    expect(paragraphs[0].runs.map((run) => run.text)).toEqual(["Intro"]);
    expect(paragraphs[1]).toMatchObject({ level: 1, bullet: true, runs: [] });
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
