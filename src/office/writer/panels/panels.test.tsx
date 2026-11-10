import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ReviewPane } from "./ReviewPane";
import { RegulationPane } from "./RegulationPane";
import { SpellPane } from "./SpellPane";

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
