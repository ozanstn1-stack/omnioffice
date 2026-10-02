import { useState } from "react";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { Modal, Segmented } from "./ui";

describe("Modal focus management", () => {
  it("keeps Tab inside the dialog and restores focus to the opener on close", async () => {
    const user = userEvent.setup();
    function Harness() {
      const [open, setOpen] = useState(false);
      return (
        <>
          <button onClick={() => setOpen(true)}>open</button>
          {open ? (
            <Modal title="Test dialog" onClose={() => setOpen(false)}>
              <button>first</button>
              <button>last</button>
            </Modal>
          ) : null}
        </>
      );
    }
    render(<Harness />);
    const opener = screen.getByRole("button", { name: "open" });
    await user.click(opener);
    expect(screen.getByRole("dialog")).toBeTruthy();

    const close = screen.getByRole("button", { name: "Close" });
    const first = screen.getByRole("button", { name: "first" });
    const last = screen.getByRole("button", { name: "last" });
    await user.tab();
    expect(document.activeElement).toBe(close);
    await user.tab();
    expect(document.activeElement).toBe(first);
    await user.tab();
    expect(document.activeElement).toBe(last);
    // Tab from the last item wraps to the first focusable control (the close
    // button) instead of leaving the dialog.
    await user.tab();
    expect(document.activeElement).toBe(close);
    // Shift+Tab from the first control wraps to the last item.
    await user.tab({ shift: true });
    expect(document.activeElement).toBe(last);

    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(opener);
  });
});

describe("Segmented keyboard navigation", () => {
  it("moves the selection with the arrow keys and keeps a roving tab stop", async () => {
    const user = userEvent.setup();
    function Harness() {
      const [value, setValue] = useState<"a" | "b" | "c">("b");
      return (
        <Segmented
          value={value}
          onChange={setValue}
          options={[
            { value: "a", label: "A" },
            { value: "b", label: "B" },
            { value: "c", label: "C" },
          ]}
        />
      );
    }
    render(<Harness />);
    expect(screen.getByRole("tab", { name: "B" })).toHaveAttribute("tabindex", "0");
    expect(screen.getByRole("tab", { name: "A" })).toHaveAttribute("tabindex", "-1");

    screen.getByRole("tab", { name: "B" }).focus();
    await user.keyboard("{ArrowRight}");
    expect(screen.getByRole("tab", { name: "C" })).toHaveAttribute("aria-selected", "true");
    expect(document.activeElement).toBe(screen.getByRole("tab", { name: "C" }));

    await user.keyboard("{ArrowRight}");
    expect(screen.getByRole("tab", { name: "A" })).toHaveAttribute("aria-selected", "true");

    await user.keyboard("{ArrowLeft}");
    expect(screen.getByRole("tab", { name: "C" })).toHaveAttribute("aria-selected", "true");
  });
});
