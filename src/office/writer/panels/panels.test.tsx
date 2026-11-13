import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { newTextDocument } from "../../../lib/office-types";
import { ReviewPane } from "./ReviewPane";
import { RegulationPane } from "./RegulationPane";
import { SpellPane } from "./SpellPane";
import { WriterRibbon, type WriterRibbonActions, type WriterRibbonState } from "./WriterRibbon";

describe("Writer side panes (3.3a)", () => {
  it("selects a revision by click or key and resolves it without selecting it", async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    const onResolve = vi.fn();
    render(
      <ReviewPane
        revisions={[{ id: "r1", kind: "insert", author: "Ada", date: "", text: "hello", blockIndex: 0 }]}
        active={null}
        onSelect={onSelect}
        onResolve={onResolve}
        onClose={vi.fn()}
      />,
    );
    const card = screen.getByRole("button", { pressed: false });
    fireEvent.keyDown(card, { key: "Enter" });
    expect(onSelect).toHaveBeenCalledWith("r1");
    await user.click(screen.getByRole("button", { name: "Reject" }));
    expect(onResolve).toHaveBeenCalledWith("r1", false);
    expect(onSelect).toHaveBeenCalledTimes(1);
  });

  it("jumps only to violations that have a paragraph and fixes one or all", async () => {
    const user = userEvent.setup();
    const onJump = vi.fn();
    const onFix = vi.fn();
    const onFixAll = vi.fn();
    const base = { ruleId: "REG-01", regulationArticle: "Md. 1", severity: "error" as const, message: "m" };
    render(
      <RegulationPane
        violations={[
          { ...base, id: "a", ruleTitle: "Margins", canAutoFix: true },
          { ...base, id: "b", ruleTitle: "Date", canAutoFix: false, blockIndex: 3 },
        ]}
        onRecheck={vi.fn()}
        onJump={onJump}
        onFix={onFix}
        onFixAll={onFixAll}
        onClose={vi.fn()}
      />,
    );
    fireEvent.keyDown(screen.getByText("Margins").closest(".comment-card")!, { key: " " });
    expect(onJump).not.toHaveBeenCalled();
    fireEvent.keyDown(screen.getByText("Date").closest(".comment-card")!, { key: " " });
    expect(onJump).toHaveBeenCalledWith(3);
    await user.click(screen.getByRole("button", { name: "Auto-fix" }));
    expect(onFix).toHaveBeenCalledWith("a");
    await user.click(screen.getByRole("button", { name: "Auto-fix All Formatting" }));
    expect(onFixAll).toHaveBeenCalledTimes(1);
  });

  it("applies a spelling suggestion and re-checks on request", async () => {
    const user = userEvent.setup();
    const onApply = vi.fn();
    const onRecheck = vi.fn();
    const onJump = vi.fn();
    const error = { blockIndex: 1, word: "teh", start: 0, end: 3, suggestions: ["the"] };
    render(
      <SpellPane
        errors={[error]}
        loading={false}
        onRecheck={onRecheck}
        onJump={onJump}
        onApply={onApply}
        onClose={vi.fn()}
      />,
    );
    await user.click(screen.getByRole("button", { name: "the" }));
    expect(onApply).toHaveBeenCalledWith(error, "the");
    expect(onJump).not.toHaveBeenCalled();
    await user.click(screen.getByTitle("Recheck"));
    expect(onRecheck).toHaveBeenCalledTimes(1);
  });
});

function makeRibbonActions(): WriterRibbonActions {
  return {
    undo: vi.fn(),
    redo: vi.fn(),
    setParagraphStyle: vi.fn(),
    applyRunChange: vi.fn(),
    toggleInline: vi.fn(),
    applyRunColor: vi.fn(),
    clearFormatting: vi.fn(),
    toggleFormatPainter: vi.fn(),
    setAlign: vi.fn(),
    toggleList: vi.fn(),
    changeListLevel: vi.fn(),
    setIndent: vi.fn(),
    setLineSpacing: vi.fn(),
    openFind: vi.fn(),
    startAi: vi.fn(),
    insertImageBlock: vi.fn(),
    openTableDialog: vi.fn(),
    insertLink: vi.fn(),
    insertPageBreak: vi.fn(),
    insertRule: vi.fn(),
    insertSectionBreak: vi.fn(),
    insertFootnote: vi.fn(),
    insertField: vi.fn(),
    openCrossReference: vi.fn(),
    insertToc: vi.fn(),
    updateToc: vi.fn(),
    compareWithFile: vi.fn(),
    toggleHeaderFooter: vi.fn(),
    insertPageNumbers: vi.fn(),
    addComment: vi.fn(),
    setPageSize: vi.fn(),
    setOrientation: vi.fn(),
    applyMarginPreset: vi.fn(),
    setColumns: vi.fn(),
    openSections: vi.fn(),
    openWatermark: vi.fn(),
    setSpace: vi.fn(),
    setMargin: vi.fn(),
    toggleTrackChanges: vi.fn(),
    toggleShowRevisions: vi.fn(),
    toggleReviewPane: vi.fn(),
    jumpToRevision: vi.fn(),
    acceptAllRevisions: vi.fn(),
    rejectAllRevisions: vi.fn(),
    toggleComments: vi.fn(),
    runSpellCheck: vi.fn(),
    runRegulationCheck: vi.fn(),
    setView: vi.fn(),
    toggleNav: vi.fn(),
    setZoom: vi.fn(),
    print: vi.fn(),
    exportPdf: vi.fn(),
    save: vi.fn(),
    saveAs: vi.fn(),
  };
}

function makeRibbonState(patch: Partial<WriterRibbonState> = {}): WriterRibbonState {
  const document = newTextDocument();
  return {
    document,
    selection: null,
    activeStyle: "Normal",
    activeRun: undefined,
    styleOptions: document.styles.map((style) => ({ value: style.id, label: style.name })),
    historyDepth: { undo: 1, redo: 0 },
    busy: false,
    aiConfigured: false,
    formatPainterActive: false,
    editingHeader: null,
    reviewOpen: false,
    commentsOpen: false,
    spellOpen: false,
    regulationOpen: false,
    view: "paginated",
    navOpen: false,
    zoom: 1,
    ...patch,
  };
}

describe("WriterRibbon", () => {
  it("switches tabs, enforces disabled state, and dispatches editor callbacks", async () => {
    const user = userEvent.setup();
    const actions = makeRibbonActions();
    const onSelect = vi.fn();

    const { rerender } = render(
      <WriterRibbon active="home" onSelect={onSelect} state={makeRibbonState()} actions={actions} />,
    );

    expect(screen.getByRole("button", { name: "Undo" })).not.toBeDisabled();
    expect(screen.getByRole("button", { name: "Redo" })).toBeDisabled();
    await user.click(screen.getByTitle("Bold"));
    expect(actions.toggleInline).toHaveBeenCalledWith("bold");

    await user.click(screen.getByRole("button", { name: "Insert" }));
    expect(onSelect).toHaveBeenCalledWith("insert");

    rerender(<WriterRibbon active="insert" onSelect={onSelect} state={makeRibbonState()} actions={actions} />);
    await user.click(screen.getByRole("button", { name: "Page break" }));
    expect(actions.insertPageBreak).toHaveBeenCalledTimes(1);

    rerender(<WriterRibbon active="layout" onSelect={onSelect} state={makeRibbonState()} actions={actions} />);
    await user.click(screen.getByTitle("Columns"));
    expect(actions.setColumns).toHaveBeenCalledWith(2);

    rerender(<WriterRibbon active="review" onSelect={onSelect} state={makeRibbonState()} actions={actions} />);
    await user.click(screen.getByRole("button", { name: "Accept all" }));
    expect(actions.acceptAllRevisions).toHaveBeenCalledTimes(1);

    rerender(<WriterRibbon active="view" onSelect={onSelect} state={makeRibbonState()} actions={actions} />);
    await user.click(screen.getByRole("button", { name: "125%" }));
    expect(actions.setZoom).toHaveBeenCalledWith(1.25);

    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(actions.save).toHaveBeenCalledTimes(1);
  });
});
