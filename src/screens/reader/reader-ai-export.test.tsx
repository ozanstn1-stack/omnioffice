import { useRef } from "react";
import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useOfficeTabs } from "../../lib/office-store";
import { ReaderAiHeaderActions } from "./ReaderAiActions";
import { ReaderQuickActions } from "./ReaderQuickActions";
import { ReaderSelectionBar } from "./ReaderSelectionBar";
import {
  buildWriterDocumentFromAnswer,
  formatConversationMarkdown,
  formatSelectionPrompt,
  openAnswerInWriter,
  openConversationInWriter,
  openExcerptInWriter,
} from "./reader-ai-export";

describe("Reader handoff & AI export (8.3.0)", () => {
  beforeEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
  });

  it("renders Reader quick handoff actions to Annotate and PDF Studio", async () => {
    const user = userEvent.setup();
    const onNavigate = vi.fn();

    render(<ReaderQuickActions path="/docs/contract.pdf" onNavigate={onNavigate} />);

    await user.click(screen.getByRole("button", { name: /Annotate & sign/i }));
    expect(onNavigate).toHaveBeenCalledWith("annotate", { files: ["/docs/contract.pdf"] });

    await user.click(screen.getByRole("button", { name: /Open in PDF Studio/i }));
    expect(onNavigate).toHaveBeenCalledWith("pdfStudio", { files: ["/docs/contract.pdf"] });
  });

  it("builds a Writer document from an AI answer and navigates to Office", () => {
    const onNavigate = vi.fn();
    const doc = buildWriterDocumentFromAnswer("/docs/report.pdf", "Key point one [page 1]\n\nKey point two [page 4]");

    expect(doc.title).toBe("report.pdf — AI");
    expect(doc.blocks).toHaveLength(3);
    const first = doc.blocks[1];
    expect(first.type).toBe("paragraph");
    if (first.type === "paragraph") {
      expect(first.runs[0].text).toContain("[page 1]");
    }

    const tabId = openAnswerInWriter("/docs/report.pdf", "Summary paragraph", onNavigate);
    const created = useOfficeTabs.getState().tabs.find((tab) => tab.id === tabId);
    expect(created?.kind).toBe("writer");
    expect(created?.title).toBe("report.pdf — AI");
    expect(onNavigate).toHaveBeenCalledWith("office");
  });

  it("exports selected excerpts and full AI transcripts to Writer and Markdown", async () => {
    const user = userEvent.setup();
    const onNavigate = vi.fn();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });

    expect(formatSelectionPrompt("First   line\nsecond", 4, "en")).toContain("page 4");
    expect(formatSelectionPrompt("Deneme", 2, "tr")).toContain("Sayfa 2");

    const excerptTabId = openExcerptInWriter("/docs/contract.pdf", "Clause one\n\nClause two", 7, onNavigate);
    const excerptTab = useOfficeTabs.getState().tabs.find((tab) => tab.id === excerptTabId);
    expect(excerptTab?.title).toBe("contract.pdf — p.7");
    expect(onNavigate).toHaveBeenCalledWith("office");

    const entries = [
      { role: "user" as const, content: "What is the termination period?" },
      { role: "assistant" as const, content: "Thirty days notice [page 3]." },
      { role: "assistant" as const, content: "   " },
    ];
    const markdown = formatConversationMarkdown("/docs/contract.pdf", entries);
    expect(markdown).toContain("# contract.pdf");
    expect(markdown).toContain("## Question");
    expect(markdown).toContain("Thirty days notice [page 3].");

    const transcriptTabId = openConversationInWriter("/docs/contract.pdf", entries, onNavigate);
    expect(useOfficeTabs.getState().tabs.find((tab) => tab.id === transcriptTabId)?.title).toBe(
      "contract.pdf — AI Report",
    );

    render(
      <ReaderAiHeaderActions
        path="/docs/contract.pdf"
        entries={entries}
        running={false}
        onClear={vi.fn()}
        onNavigate={onNavigate}
      />,
    );

    await user.click(screen.getByRole("button", { name: /Copy chat/i }));
    expect(writeText).toHaveBeenCalledWith(markdown);

    await user.click(screen.getByRole("button", { name: /Export chat to Writer/i }));
    expect(onNavigate).toHaveBeenCalledWith("office");
  });

  it("shows floating ReaderSelectionBar on text selection and triggers copy, AI draft, Annotate, and Writer", async () => {
    const user = userEvent.setup();
    const onAskAi = vi.fn();
    const onNavigate = vi.fn();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });

    function Harness() {
      const containerRef = useRef<HTMLDivElement>(null);
      return (
        <div>
          <div ref={containerRef} data-testid="reader-surface">
            <p data-testid="selectable-text">Contract termination notice period is thirty days</p>
          </div>
          <ReaderSelectionBar
            containerRef={containerRef}
            path="/docs/contract.pdf"
            page={3}
            onAskAi={onAskAi}
            onNavigate={onNavigate}
          />
        </div>
      );
    }

    render(<Harness />);
    const selectParagraph = () => {
      const target = screen.getByTestId("selectable-text");
      const range = document.createRange();
      range.selectNodeContents(target);
      const selection = window.getSelection();
      selection?.removeAllRanges();
      selection?.addRange(range);
      fireEvent.mouseUp(document);
    };

    selectParagraph();
    expect(screen.getByRole("toolbar", { name: /Selected text actions/i })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /Ask AI/i }));
    expect(onAskAi).toHaveBeenCalledWith(expect.stringContaining("thirty days"));
    expect(screen.queryByRole("toolbar")).not.toBeInTheDocument();

    selectParagraph();
    await user.click(screen.getByRole("button", { name: /^Copy$/i }));
    expect(writeText).toHaveBeenCalledWith("Contract termination notice period is thirty days");

    selectParagraph();
    await user.click(screen.getByRole("button", { name: /Annotate & sign/i }));
    expect(onNavigate).toHaveBeenCalledWith("annotate", { files: ["/docs/contract.pdf"] });

    selectParagraph();
    await user.click(screen.getByRole("button", { name: /Send to Writer/i }));
    expect(onNavigate).toHaveBeenCalledWith("office");
    expect(useOfficeTabs.getState().tabs[0]?.title).toBe("contract.pdf — p.3");
  });
});
