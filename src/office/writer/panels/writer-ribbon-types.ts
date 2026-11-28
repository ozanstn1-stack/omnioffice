/**
 * Ribbon state and action contracts for the Writer editor.
 */
import type { ParaProps, Run, TextDocument } from "../../../lib/office-types";
import type { WriterAiTask } from "../WriterAiDialog";

export interface WriterRibbonState {
  document: TextDocument;
  selection: { paragraph: ParaProps; run: Run } | null;
  activeStyle: string;
  activeRun: Run | undefined;
  styleOptions: Array<{ value: string; label: string }>;
  historyDepth: { undo: number; redo: number };
  busy: boolean;
  aiConfigured: boolean;
  formatPainterActive: boolean;
  editingHeader: "header" | "footer" | null;
  reviewOpen: boolean;
  commentsOpen: boolean;
  spellOpen: boolean;
  regulationOpen: boolean;
  view: "paginated" | "continuous";
  navOpen: boolean;
  zoom: number;
}

export interface WriterRibbonActions {
  undo: () => void;
  redo: () => void;
  setParagraphStyle: (styleId: string) => void;
  applyRunChange: (patch: Partial<Run>) => void;
  toggleInline: (field: "bold" | "italic" | "underline" | "strike" | "superscript" | "subscript") => void;
  applyRunColor: (color: string, highlight: boolean) => void;
  clearFormatting: () => void;
  toggleFormatPainter: () => void;
  setAlign: (align: string) => void;
  toggleList: (kind: "bullet" | "number") => void;
  changeListLevel: (delta: number) => void;
  setIndent: (delta: number) => void;
  setLineSpacing: (spacing: number) => void;
  openFind: () => void;
  startAi: (task: WriterAiTask) => void;
  insertImageBlock: () => void;
  insertChartBlock?: () => void;
  openTableDialog: () => void;
  insertLink: () => void;
  insertPageBreak: () => void;
  insertRule: () => void;
  insertSectionBreak: (start: string) => void;
  insertFootnote: (endnote: boolean) => void;
  insertField: (kind: "page" | "pages" | "date") => void;
  openCrossReference: () => void;
  insertToc: () => void;
  updateToc: () => void;
  compareWithFile: () => void;
  toggleHeaderFooter: (target: "header" | "footer") => void;
  insertPageNumbers: () => void;
  addComment: () => void;
  setPageSize: (size: string) => void;
  setOrientation: (orientation: string) => void;
  applyMarginPreset: (preset: "normal" | "narrow" | "wide") => void;
  setColumns: (columns: number) => void;
  openSections: () => void;
  openWatermark: () => void;
  setSpace: (beforePt: number, afterPt: number) => void;
  setMargin: (key: "marginTopPt" | "marginRightPt" | "marginBottomPt" | "marginLeftPt", value: number) => void;
  toggleTrackChanges: () => void;
  toggleShowRevisions: () => void;
  toggleReviewPane: () => void;
  jumpToRevision: (next: boolean) => void;
  acceptAllRevisions: () => void;
  rejectAllRevisions: () => void;
  toggleComments: () => void;
  runSpellCheck: () => void;
  runRegulationCheck: () => void;
  setView: (view: "paginated" | "continuous") => void;
  toggleNav: () => void;
  setZoom: (zoom: number) => void;
  print: () => void;
  exportPdf: () => void;
  save: () => void;
  saveAs: () => void;
}
