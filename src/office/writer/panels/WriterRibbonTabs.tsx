/**
 * Home and Insert tabs for the Writer ribbon. Separated from `WriterRibbon` to
 * keep each ribbon module compact and single-responsibility.
 */
import {
  AlignCenter,
  AlignJustify,
  AlignLeft,
  AlignRight,
  Bold,
  Eraser,
  FileDiff,
  FileText,
  Highlighter,
  Image as ImageIcon,
  Indent,
  Italic,
  Languages,
  Link2,
  List,
  ListOrdered,
  ListOrdered as TocIcon,
  Maximize2,
  MessageSquare,
  Minimize2,
  Minus,
  Outdent,
  Paintbrush,
  Redo2,
  RefreshCw,
  Search,
  SeparatorHorizontal,
  Sparkles,
  SpellCheck,
  Strikethrough,
  Table as TableIcon,
  Underline,
  Undo2,
  Wand2,
} from "lucide-react";
import { useT } from "../../../lib/i18n";
import type { ParaProps, Run, TextDocument } from "../../../lib/office-types";
import { RibbonGroup, ToolButton, ToolColor, ToolSelect } from "../../office-ui";
import { StyleGallery } from "../StyleGallery";
import type { WriterAiTask } from "../WriterAiDialog";

export const HIGHLIGHT_COLORS = [
  "#FEF08A",
  "#BBF7D0",
  "#BFDBFE",
  "#FBCFE8",
  "#FED7AA",
  "#E9D5FF",
  "#FECACA",
  "#A7F3D0",
];

export const AI_RIBBON: Array<{ task: WriterAiTask; icon: typeof Wand2 }> = [
  { task: "rewrite", icon: Wand2 },
  { task: "shorten", icon: Minimize2 },
  { task: "expand", icon: Maximize2 },
  { task: "fix", icon: SpellCheck },
  { task: "translate", icon: Languages },
  { task: "tone", icon: Sparkles },
];

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

const FONT_SIZES = [8, 9, 10, 11, 12, 14, 16, 18, 20, 24, 28, 32, 40, 48].map((size) => ({
  value: String(size),
  label: `${size}`,
}));

const LINE_SPACINGS = [1, 1.15, 1.5, 2].map((value) => ({ value: String(value), label: `${value}` }));

export function WriterHomeTab({ state, actions }: { state: WriterRibbonState; actions: WriterRibbonActions }) {
  const t = useT();
  const { document, selection, activeStyle, activeRun, styleOptions, historyDepth, busy, aiConfigured } = state;

  return (
    <>
      <RibbonGroup label={t("writer.clipboard")}>
        <ToolButton
          icon={<Undo2 size={16} />}
          label={t("common.undo")}
          onClick={actions.undo}
          disabled={busy || historyDepth.undo === 0}
        />
        <ToolButton
          icon={<Redo2 size={16} />}
          label={t("common.redo")}
          onClick={actions.redo}
          disabled={busy || historyDepth.redo === 0}
        />
      </RibbonGroup>
      <RibbonGroup label={t("writer.font")}>
        <ToolSelect
          value={activeStyle}
          onChange={actions.setParagraphStyle}
          options={styleOptions}
          title={t("writer.style")}
          width={132}
        />
        <ToolSelect
          value={String(activeRun?.sizePt ?? document.styles.find((style) => style.id === activeStyle)?.sizePt ?? 11)}
          onChange={(value) => actions.applyRunChange({ sizePt: Number(value) })}
          options={FONT_SIZES}
          title={t("writer.fontSize")}
          width={64}
        />
        <ToolButton
          icon={<Bold size={16} />}
          onClick={() => actions.toggleInline("bold")}
          active={Boolean(activeRun?.bold)}
          title={t("writer.bold")}
        />
        <ToolButton
          icon={<Italic size={16} />}
          onClick={() => actions.toggleInline("italic")}
          active={Boolean(activeRun?.italic)}
          title={t("writer.italic")}
        />
        <ToolButton
          icon={<Underline size={16} />}
          onClick={() => actions.toggleInline("underline")}
          active={Boolean(activeRun?.underline)}
          title={t("writer.underline")}
        />
        <ToolButton
          icon={<Strikethrough size={16} />}
          onClick={() => actions.toggleInline("strike")}
          active={Boolean(activeRun?.strike)}
          title={t("writer.strike")}
        />
        <ToolColor
          value={activeRun?.color ?? "#1f2328"}
          onChange={(color) => actions.applyRunColor(color, false)}
          title={t("writer.textColor")}
        />
        <ToolButton
          icon={<Highlighter size={16} />}
          onClick={() => actions.applyRunColor(activeRun?.highlight ?? HIGHLIGHT_COLORS[0], true)}
          active={Boolean(activeRun?.highlight)}
          title={t("writer.highlight")}
        />
        <ToolButton icon={<Eraser size={16} />} onClick={actions.clearFormatting} title={t("writer.clearFormatting")} />
        <ToolButton
          icon={<Paintbrush size={16} />}
          onClick={actions.toggleFormatPainter}
          active={state.formatPainterActive}
          title={t("writer.formatPainter")}
        />
      </RibbonGroup>
      <RibbonGroup label={t("writer.paragraph")}>
        <ToolButton
          icon={<AlignLeft size={16} />}
          onClick={() => actions.setAlign("left")}
          active={selection?.paragraph.align === "left"}
          title={t("writer.alignLeft")}
        />
        <ToolButton
          icon={<AlignCenter size={16} />}
          onClick={() => actions.setAlign("center")}
          active={selection?.paragraph.align === "center"}
          title={t("writer.alignCenter")}
        />
        <ToolButton
          icon={<AlignRight size={16} />}
          onClick={() => actions.setAlign("right")}
          active={selection?.paragraph.align === "right"}
          title={t("writer.alignRight")}
        />
        <ToolButton
          icon={<AlignJustify size={16} />}
          onClick={() => actions.setAlign("justify")}
          active={selection?.paragraph.align === "justify"}
          title={t("writer.alignJustify")}
        />
        <ToolButton
          icon={<List size={16} />}
          onClick={() => actions.toggleList("bullet")}
          active={selection?.paragraph.list?.kind === "bullet"}
          title={t("writer.bullets")}
        />
        <ToolButton
          icon={<ListOrdered size={16} />}
          onClick={() => actions.toggleList("number")}
          active={selection?.paragraph.list?.kind === "number"}
          title={t("writer.numbering")}
        />
        <ToolButton
          icon={<Indent size={16} />}
          onClick={() => (selection?.paragraph.list ? actions.changeListLevel(1) : actions.setIndent(24))}
          title={t("writer.increaseIndent")}
        />
        <ToolButton
          icon={<Outdent size={16} />}
          onClick={() => (selection?.paragraph.list ? actions.changeListLevel(-1) : actions.setIndent(-24))}
          title={t("writer.decreaseIndent")}
        />
        <ToolSelect
          value={String(selection?.paragraph.lineSpacing ?? 1.15)}
          onChange={(value) => actions.setLineSpacing(Number(value))}
          options={LINE_SPACINGS}
          title={t("writer.lineSpacing")}
          width={70}
        />
      </RibbonGroup>
      <RibbonGroup label={t("writer.styles")}>
        <StyleGallery document={document} active={activeStyle} onApply={actions.setParagraphStyle} />
      </RibbonGroup>
      <RibbonGroup label={t("writer.find")}>
        <ToolButton icon={<Search size={16} />} label={t("common.find")} onClick={actions.openFind} />
      </RibbonGroup>
      <RibbonGroup label={t("ai.edit.group")}>
        {AI_RIBBON.map(({ task, icon: Icon }) => (
          <ToolButton
            key={task}
            icon={<Icon size={16} />}
            keepFocus
            disabled={!aiConfigured || busy}
            title={
              aiConfigured ? t(`ai.edit.btn.${task}`) : `${t(`ai.edit.btn.${task}`)} - ${t("ai.edit.notConfigured")}`
            }
            onClick={() => actions.startAi(task)}
          />
        ))}
      </RibbonGroup>
    </>
  );
}

export function WriterInsertTab({
  editingHeader,
  actions,
}: {
  editingHeader: "header" | "footer" | null;
  actions: WriterRibbonActions;
}) {
  const t = useT();
  return (
    <>
      <RibbonGroup label={t("writer.insert")}>
        <ToolButton icon={<ImageIcon size={16} />} label={t("writer.image")} onClick={actions.insertImageBlock} />
        <ToolButton icon={<TableIcon size={16} />} label={t("writer.table")} onClick={actions.openTableDialog} />
        <ToolButton icon={<Link2 size={16} />} label={t("writer.link")} onClick={actions.insertLink} />
      </RibbonGroup>
      <RibbonGroup label={t("writer.pages")}>
        <ToolButton
          icon={<SeparatorHorizontal size={16} />}
          label={t("writer.pageBreak")}
          onClick={actions.insertPageBreak}
        />
        <ToolButton icon={<Minus size={16} />} label={t("writer.horizontalRule")} onClick={actions.insertRule} />
        <ToolButton
          label={t("writer.sectionBreak")}
          onClick={() => actions.insertSectionBreak("newPage")}
          title={t("writer.sectionBreak")}
        />
        <ToolButton
          label={t("writer.sectionContinuous")}
          onClick={() => actions.insertSectionBreak("continuous")}
          title={t("writer.sectionContinuous")}
        />
      </RibbonGroup>
      <RibbonGroup label={t("writer.notes")}>
        <ToolButton label={t("writer.insertFootnote")} onClick={() => actions.insertFootnote(false)} />
        <ToolButton label={t("writer.insertEndnote")} onClick={() => actions.insertFootnote(true)} />
      </RibbonGroup>
      <RibbonGroup label={t("writer.fields")}>
        <ToolButton label={t("writer.fieldPage")} onClick={() => actions.insertField("page")} />
        <ToolButton label={t("writer.fieldPages")} onClick={() => actions.insertField("pages")} />
        <ToolButton label={t("writer.fieldDate")} onClick={() => actions.insertField("date")} />
        <ToolButton label={t("writer.crossReference")} onClick={actions.openCrossReference} />
      </RibbonGroup>
      <RibbonGroup label={t("writer.tableOfContents")}>
        <ToolButton icon={<TocIcon size={16} />} label={t("writer.insertToc")} onClick={actions.insertToc} />
        <ToolButton icon={<RefreshCw size={16} />} label={t("writer.updateToc")} onClick={actions.updateToc} />
      </RibbonGroup>
      <RibbonGroup label={t("writer.review")}>
        <ToolButton icon={<FileDiff size={16} />} label={t("writer.compare")} onClick={actions.compareWithFile} />
      </RibbonGroup>
      <RibbonGroup label={t("writer.headerFooter")}>
        <ToolButton
          icon={<FileText size={16} />}
          label={t("writer.header")}
          onClick={() => actions.toggleHeaderFooter("header")}
          active={editingHeader === "header"}
        />
        <ToolButton
          icon={<FileText size={16} />}
          label={t("writer.footer")}
          onClick={() => actions.toggleHeaderFooter("footer")}
          active={editingHeader === "footer"}
        />
        <ToolButton label={t("writer.pageNumbers")} onClick={actions.insertPageNumbers} />
      </RibbonGroup>
      <RibbonGroup label={t("writer.comments")}>
        <ToolButton icon={<MessageSquare size={16} />} label={t("writer.addComment")} onClick={actions.addComment} />
      </RibbonGroup>
    </>
  );
}
