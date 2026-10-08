/**
 * Helpers shared by the Calc component tests. Importing this module installs
 * the pointer-event shim jsdom lacks; each test file still declares its own
 * `vi.mock` calls for Tauri (they are hoisted per file).
 */
import { fireEvent } from "@testing-library/react";
import { applyCellEdit } from "../cells";
import { CalcEditor } from "../../CalcEditor";
import { useOfficeTabs, type OfficeTab } from "../../../lib/office-store";
import { newSheet, type Sheet, type Workbook } from "../../../lib/office-types";

// jsdom has no PointerEvent: a MouseEvent subclass carries the fields the
// grid gestures read.
if (typeof window.PointerEvent === "undefined") {
  class TestPointerEvent extends MouseEvent {
    readonly pointerId: number;
    readonly pointerType: string;
    constructor(type: string, init: PointerEventInit = {}) {
      super(type, init);
      this.pointerId = init.pointerId ?? 0;
      this.pointerType = init.pointerType ?? "";
    }
  }
  window.PointerEvent = TestPointerEvent as unknown as typeof PointerEvent;
}

export function Harness({ id }: { id: string }) {
  const tab = useOfficeTabs((state) => state.tabs.find((candidate) => candidate.id === id));
  if (!tab) return null;
  return <CalcEditor tab={tab as OfficeTab & { model: Workbook }} />;
}

export function workbookOf(): Workbook {
  return useOfficeTabs.getState().tabs[0].model as Workbook;
}

export function cellAt(row: number, col: number): HTMLElement {
  const element = document.querySelector<HTMLElement>(`[data-cell="${row}:${col}"]`);
  if (!element) throw new Error(`cell ${row}:${col} is not rendered`);
  return element;
}

export function nameBox(): string {
  return document.querySelector<HTMLInputElement>(".name-box")?.value ?? "";
}

export function selectRange(range: string) {
  fireEvent.change(document.querySelector<HTMLInputElement>(".name-box")!, { target: { value: range } });
}

/** Opens a tab with the typed cells of the first sheet (`A1` ... `Z9` style addresses). */
export function seedWorkbook(
  values: Record<string, string>,
  patch: (sheet: Sheet) => Sheet = (sheet) => sheet,
): string {
  const id = useOfficeTabs.getState().create("calc", "Untitled");
  let model = useOfficeTabs.getState().tabs[0].model as Workbook;
  for (const [address, value] of Object.entries(values)) {
    const col = address.charCodeAt(0) - 65;
    const row = Number(address.slice(1)) - 1;
    model = applyCellEdit(model, 0, row, col, value);
  }
  model = { ...model, sheets: model.sheets.map((sheet, index) => (index === 0 ? patch(sheet) : sheet)) };
  setModel(id, model);
  return id;
}

export function setModel(id: string, model: Workbook) {
  useOfficeTabs.setState((state) => ({ tabs: state.tabs.map((tab) => (tab.id === id ? { ...tab, model } : tab)) }));
}

/** Adds a sheet with typed cells to the open workbook. */
export function addSheetWith(id: string, name: string, values: Record<string, string>) {
  let model = useOfficeTabs.getState().tabs.find((tab) => tab.id === id)!.model as Workbook;
  model = { ...model, sheets: [...model.sheets, newSheet(name)] };
  const sheetIndex = model.sheets.length - 1;
  for (const [address, value] of Object.entries(values)) {
    const col = address.charCodeAt(0) - 65;
    const row = Number(address.slice(1)) - 1;
    model = applyCellEdit(model, sheetIndex, row, col, value);
  }
  setModel(id, model);
}
