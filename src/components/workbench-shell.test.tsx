import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { WorkbenchShell } from "./workbench-shell";
import { useWorkbench } from "../lib/workbench-store";
import { useOfficeTabs } from "../lib/office-store";
import type { ScreenId } from "../lib/nav";

const icons = new Proxy({}, { get: () => <span /> }) as Record<ScreenId, React.ReactNode>;

describe("WorkbenchShell (v8.2.0 desktop)", () => {
  beforeEach(() => {
    useWorkbench.getState().reset();
    useOfficeTabs.setState({ tabs: [], activeId: null });
  });

  it("renders the icon rail, live tab strip, and opens Split View on click", async () => {
    const user = userEvent.setup();
    const onNavigate = vi.fn();
    useWorkbench.getState().syncScreen("reader", ["/tmp/sample.pdf"]);

    render(
      <WorkbenchShell
        icons={icons}
        screen="reader"
        version="8.2.0"
        backgroundJobs={0}
        isDark
        readerFocus={false}
        dragging={false}
        onNavigate={onNavigate}
        onActivateTab={vi.fn()}
        onCloseTab={vi.fn()}
        onSearch={vi.fn()}
        onPalette={vi.fn()}
        onToggleTheme={vi.fn()}
        renderScreen={(id) => <div data-testid={`split-${id}`}>Secondary {id}</div>}
      >
        <div data-testid="primary-pane">Primary reader</div>
      </WorkbenchShell>,
    );

    expect(screen.getByTestId("desktop-sidebar")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /sample\.pdf/ })).toHaveAttribute("aria-selected", "true");

    await user.click(screen.getByRole("button", { name: /Split view/i }));
    expect(screen.getByTestId("split-ai")).toBeInTheDocument();

    // Promote secondary pane into main pane and close split.
    await user.click(screen.getByRole("button", { name: /Move to main pane/i }));
    expect(onNavigate).toHaveBeenCalledWith("ai", { files: [] });
    expect(screen.queryByTestId("split-ai")).not.toBeInTheDocument();
  });

  it("expands the hubs panel on demand from the slim rail", async () => {
    const user = userEvent.setup();
    render(
      <WorkbenchShell
        icons={icons}
        screen="home"
        version="8.2.0"
        backgroundJobs={0}
        isDark={false}
        readerFocus={false}
        dragging={false}
        onNavigate={vi.fn()}
        onActivateTab={vi.fn()}
        onCloseTab={vi.fn()}
        onSearch={vi.fn()}
        onPalette={vi.fn()}
        onToggleTheme={vi.fn()}
        renderScreen={() => null}
      >
        <div>Home content</div>
      </WorkbenchShell>,
    );

    const panel = document.querySelector(".workbench-hubs-panel");
    expect(panel).toHaveAttribute("hidden");
    await user.click(screen.getByRole("button", { name: /Expand navigation/i }));
    expect(panel).not.toHaveAttribute("hidden");
    expect(screen.getByRole("button", { name: /PDF Tools/i })).toBeInTheDocument();
  });
});
