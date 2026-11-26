import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useOfficeTabs } from "../../lib/office-store";
import { ReaderQuickActions } from "./ReaderQuickActions";
import { buildWriterDocumentFromAnswer, openAnswerInWriter } from "./reader-ai-export";

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
});
