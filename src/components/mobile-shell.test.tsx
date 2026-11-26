import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { MobileShell } from "./mobile-shell";
import { useWorkbench } from "../lib/workbench-store";
import type { ScreenId } from "../lib/nav";

const icons = new Proxy({}, { get: () => <span /> }) as Record<ScreenId, React.ReactNode>;

describe("MobileShell (v8.2.0 Android bottom-first)", () => {
  beforeEach(() => {
    useWorkbench.getState().reset();
  });

  it("renders bottom thumb omnibar on Home and opens the visual tab grid", async () => {
    const user = userEvent.setup();
    const onSearch = vi.fn();
    const onActivateTab = vi.fn();
    useWorkbench.getState().syncScreen("merge", []);

    render(
      <MobileShell
        icons={icons}
        screen="home"
        activeLabel="Home"
        isDark={false}
        readerImmersive={false}
        sheetOpen={false}
        onOpenSheet={vi.fn()}
        onCloseSheet={vi.fn()}
        onNavigate={vi.fn()}
        onActivateTab={onActivateTab}
        onCloseTab={vi.fn()}
        onSearch={onSearch}
        onToggleTheme={vi.fn()}
      >
        <div>Home body</div>
      </MobileShell>,
    );

    await user.click(screen.getByRole("button", { name: /Search tools, files or ask AI/i }));
    expect(onSearch).toHaveBeenCalledTimes(1);

    await user.click(screen.getByRole("button", { name: /Open tabs \(2\)/i }));
    expect(screen.getByRole("dialog", { name: /Open tabs/i })).toBeInTheDocument();
  });

  it("shows a back button on sub-screens and quick-create items inside the pull-up sheet", async () => {
    const user = userEvent.setup();
    const onNavigate = vi.fn();
    const onCloseSheet = vi.fn();

    render(
      <MobileShell
        icons={icons}
        screen="merge"
        activeLabel="Merge"
        isDark
        readerImmersive={false}
        sheetOpen
        onOpenSheet={vi.fn()}
        onCloseSheet={onCloseSheet}
        onNavigate={onNavigate}
        onActivateTab={vi.fn()}
        onCloseTab={vi.fn()}
        onSearch={vi.fn()}
        onToggleTheme={vi.fn()}
      >
        <div>Merge body</div>
      </MobileShell>,
    );

    expect(screen.getByRole("button", { name: /^Back$/i })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /^Scan$/i }));
    expect(onNavigate).toHaveBeenCalledWith("scanner");
    expect(onCloseSheet).toHaveBeenCalled();
  });

  it("supports controlled tabsOpen state and sets mobile dock dataset attribute on Home", async () => {
    const user = userEvent.setup();
    const onOpenTabs = vi.fn();
    const onCloseTabs = vi.fn();

    const { rerender, unmount } = render(
      <MobileShell
        icons={icons}
        screen="home"
        activeLabel="Home"
        isDark={false}
        readerImmersive={false}
        sheetOpen={false}
        onOpenSheet={vi.fn()}
        onCloseSheet={vi.fn()}
        tabsOpen={false}
        onOpenTabs={onOpenTabs}
        onCloseTabs={onCloseTabs}
        onNavigate={vi.fn()}
        onActivateTab={vi.fn()}
        onCloseTab={vi.fn()}
        onSearch={vi.fn()}
        onToggleTheme={vi.fn()}
      >
        <div>Home</div>
      </MobileShell>,
    );

    expect(document.documentElement.dataset.mobileDock).toBe("open");
    await user.click(screen.getByRole("button", { name: /Open tabs \(1\)/i }));
    expect(onOpenTabs).toHaveBeenCalledTimes(1);

    rerender(
      <MobileShell
        icons={icons}
        screen="home"
        activeLabel="Home"
        isDark={false}
        readerImmersive={false}
        sheetOpen={false}
        onOpenSheet={vi.fn()}
        onCloseSheet={vi.fn()}
        tabsOpen
        onOpenTabs={onOpenTabs}
        onCloseTabs={onCloseTabs}
        onNavigate={vi.fn()}
        onActivateTab={vi.fn()}
        onCloseTab={vi.fn()}
        onSearch={vi.fn()}
        onToggleTheme={vi.fn()}
      >
        <div>Home</div>
      </MobileShell>,
    );
    expect(screen.getByRole("dialog", { name: /Open tabs/i })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /^Close$/i }));
    expect(onCloseTabs).toHaveBeenCalledTimes(1);

    unmount();
    expect(document.documentElement.dataset.mobileDock).toBeUndefined();
  });
});
