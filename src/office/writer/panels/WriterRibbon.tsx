/**
 * The Writer ribbon: tabs and tool groups. It owns no state beyond what it is
 * given; every button calls back into the editor.
 */
import {
  Columns2,
  FileDown,
  FolderOpen,
  LayoutList,
  ListTree,
  MessageSquare,
  Printer,
  Save,
  Scale,
  Search,
  SpellCheck,
  Stamp,
} from "lucide-react";
import { useT } from "../../../lib/i18n";
import { Ribbon, RibbonGroup, ToolButton, ToolNumber, ToolSelect } from "../../office-ui";
import { openIntoWorkspace } from "../../useOfficeSession";
import { fitWidthZoom } from "../zoom";
import { WriterHomeTab, WriterInsertTab, type WriterRibbonActions, type WriterRibbonState } from "./WriterRibbonTabs";

export type { WriterRibbonActions, WriterRibbonState };

export interface WriterRibbonProps {
  active: string;
  onSelect: (id: string) => void;
  state: WriterRibbonState;
  actions: WriterRibbonActions;
}

const PAGE_SIZE_OPTIONS = [
  { value: "a4", label: "A4" },
  { value: "a5", label: "A5" },
  { value: "letter", label: "Letter" },
  { value: "legal", label: "Legal" },
  { value: "a3", label: "A3" },
];

function WriterLayoutTab({ state, actions }: { state: WriterRibbonState; actions: WriterRibbonActions }) {
  const t = useT();
  const { document, selection } = state;
  return (
    <>
      <RibbonGroup label={t("writer.pageSetup")}>
        <ToolSelect
          value={document.page.size}
          onChange={actions.setPageSize}
          options={PAGE_SIZE_OPTIONS}
          title={t("writer.pageSize")}
          width={90}
        />
        <ToolSelect
          value={document.page.orientation}
          onChange={actions.setOrientation}
          options={[
            { value: "portrait", label: t("writer.portrait") },
            { value: "landscape", label: t("writer.landscape") },
          ]}
          title={t("writer.orientation")}
          width={110}
        />
        <ToolSelect
          value=""
          onChange={(value) => {
            if (value) actions.applyMarginPreset(value as "normal" | "narrow" | "wide");
          }}
          options={[
            { value: "", label: t("writer.margins") },
            { value: "normal", label: t("writer.marginNormal") },
            { value: "narrow", label: t("writer.marginNarrow") },
            { value: "wide", label: t("writer.marginWide") },
          ]}
          title={t("writer.margins")}
          width={110}
        />
        <ToolButton
          icon={<Columns2 size={16} />}
          onClick={() => actions.setColumns(document.page.columns > 1 ? 1 : 2)}
          active={document.page.columns > 1}
          title={t("writer.columns")}
        />
        <ToolButton label={t("writer.sections")} onClick={actions.openSections} title={t("writer.sections")} />
      </RibbonGroup>
      <RibbonGroup label={t("writer.watermark")}>
        <ToolButton icon={<Stamp size={16} />} label={t("writer.watermark")} onClick={actions.openWatermark} />
      </RibbonGroup>
      <RibbonGroup label={t("writer.spacing")}>
        <ToolNumber
          value={selection?.paragraph.spaceBeforePt ?? 0}
          onChange={(value) => actions.setSpace(value, selection?.paragraph.spaceAfterPt ?? 0)}
          min={0}
          max={144}
          title={t("writer.spaceBefore")}
        />
        <ToolNumber
          value={selection?.paragraph.spaceAfterPt ?? 0}
          onChange={(value) => actions.setSpace(selection?.paragraph.spaceBeforePt ?? 0, value)}
          min={0}
          max={144}
          title={t("writer.spaceAfter")}
        />
      </RibbonGroup>
      <RibbonGroup label={t("writer.margins")}>
        <ToolNumber
          value={document.page.marginTopPt}
          onChange={(value) => actions.setMargin("marginTopPt", value)}
          min={0}
          max={288}
          title={t("writer.marginTop")}
        />
        <ToolNumber
          value={document.page.marginBottomPt}
          onChange={(value) => actions.setMargin("marginBottomPt", value)}
          min={0}
          max={288}
          title={t("writer.marginBottom")}
        />
        <ToolNumber
          value={document.page.marginLeftPt}
          onChange={(value) => actions.setMargin("marginLeftPt", value)}
          min={0}
          max={288}
          title={t("writer.marginLeft")}
        />
        <ToolNumber
          value={document.page.marginRightPt}
          onChange={(value) => actions.setMargin("marginRightPt", value)}
          min={0}
          max={288}
          title={t("writer.marginRight")}
        />
      </RibbonGroup>
    </>
  );
}

function WriterReviewTab({ state, actions }: { state: WriterRibbonState; actions: WriterRibbonActions }) {
  const t = useT();
  const { document, reviewOpen, commentsOpen, spellOpen, regulationOpen } = state;
  return (
    <>
      <RibbonGroup label={t("writer.trackChanges")}>
        <ToolButton
          label={t("writer.suggesting")}
          onClick={actions.toggleTrackChanges}
          active={document.trackChanges === true}
          title={t("writer.suggestingHint")}
        />
        <ToolButton
          label={t("writer.showRevisions")}
          onClick={actions.toggleShowRevisions}
          active={document.showRevisions !== false}
        />
        <ToolButton label={t("writer.reviewPane")} onClick={actions.toggleReviewPane} active={reviewOpen} />
      </RibbonGroup>
      <RibbonGroup label={t("writer.revisions")}>
        <ToolButton label={t("writer.previousChange")} onClick={() => actions.jumpToRevision(false)} />
        <ToolButton label={t("writer.nextChange")} onClick={() => actions.jumpToRevision(true)} />
        <ToolButton label={t("writer.acceptAll")} onClick={actions.acceptAllRevisions} />
        <ToolButton label={t("writer.rejectAll")} onClick={actions.rejectAllRevisions} />
      </RibbonGroup>
      <RibbonGroup label={t("writer.comments")}>
        <ToolButton icon={<MessageSquare size={16} />} label={t("writer.addComment")} onClick={actions.addComment} />
        <ToolButton
          icon={<MessageSquare size={16} />}
          label={t("writer.comments")}
          onClick={actions.toggleComments}
          active={commentsOpen}
        />
      </RibbonGroup>
      <RibbonGroup label={t("writer.find")}>
        <ToolButton icon={<Search size={16} />} label={t("writer.findReplace")} onClick={actions.openFind} />
      </RibbonGroup>
      <RibbonGroup label={t("writer.proofing")}>
        <ToolButton
          icon={<SpellCheck size={16} />}
          label={t("writer.spellCheck")}
          onClick={actions.runSpellCheck}
          active={spellOpen}
        />
        <ToolButton
          icon={<Scale size={16} />}
          label={t("writer.officialLetterCheck")}
          onClick={actions.runRegulationCheck}
          active={regulationOpen}
        />
      </RibbonGroup>
    </>
  );
}

function WriterViewTab({ state, actions }: { state: WriterRibbonState; actions: WriterRibbonActions }) {
  const t = useT();
  const { document, view, navOpen, zoom } = state;
  return (
    <>
      <RibbonGroup label={t("writer.view")}>
        <ToolButton
          icon={<LayoutList size={16} />}
          label={t("writer.paginated")}
          onClick={() => actions.setView("paginated")}
          active={view === "paginated"}
        />
        <ToolButton
          label={t("writer.continuous")}
          onClick={() => actions.setView("continuous")}
          active={view === "continuous"}
        />
        <ToolButton
          icon={<ListTree size={16} />}
          label={t("writer.navigation")}
          onClick={actions.toggleNav}
          active={navOpen}
        />
      </RibbonGroup>
      <RibbonGroup label={t("writer.zoom")}>
        <ToolButton
          label={t("reader.fitWidth")}
          onClick={() => actions.setZoom(fitWidthZoom(document.page.widthPt, window.innerWidth))}
        />
        {[0.75, 1, 1.25, 1.5].map((level) => (
          <ToolButton
            key={level}
            label={`${level * 100}%`}
            onClick={() => actions.setZoom(level)}
            active={zoom === level}
          />
        ))}
      </RibbonGroup>
      <RibbonGroup label={t("writer.print")}>
        <ToolButton icon={<Printer size={16} />} label={t("common.print")} onClick={actions.print} />
        <ToolButton icon={<FileDown size={16} />} label={t("writer.exportPdf")} onClick={actions.exportPdf} />
      </RibbonGroup>
    </>
  );
}

export function WriterRibbon({ active, onSelect, state, actions }: WriterRibbonProps) {
  const t = useT();
  return (
    <Ribbon
      tabs={[
        { id: "home", label: t("writer.tabHome") },
        { id: "insert", label: t("writer.tabInsert") },
        { id: "layout", label: t("writer.tabLayout") },
        { id: "review", label: t("writer.tabReview") },
        { id: "view", label: t("writer.tabView") },
      ]}
      active={active}
      onSelect={onSelect}
    >
      {active === "home" ? <WriterHomeTab state={state} actions={actions} /> : null}
      {active === "insert" ? <WriterInsertTab editingHeader={state.editingHeader} actions={actions} /> : null}
      {active === "layout" ? <WriterLayoutTab state={state} actions={actions} /> : null}
      {active === "review" ? <WriterReviewTab state={state} actions={actions} /> : null}
      {active === "view" ? <WriterViewTab state={state} actions={actions} /> : null}

      <div className="ribbon-spacer" />
      <RibbonGroup>
        <ToolButton icon={<FolderOpen size={16} />} label={t("common.open")} onClick={() => void openIntoWorkspace()} />
        <ToolButton icon={<Save size={16} />} label={t("common.save")} onClick={actions.save} disabled={state.busy} />
        <ToolButton label={t("common.saveAs")} onClick={actions.saveAs} disabled={state.busy} />
        <ToolButton icon={<FileDown size={16} />} label={t("writer.exportPdf")} onClick={actions.exportPdf} />
      </RibbonGroup>
    </Ribbon>
  );
}
