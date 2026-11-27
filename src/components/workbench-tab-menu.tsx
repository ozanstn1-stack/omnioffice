import { useEffect, useRef } from "react";
import { Columns2, ListX, Pin, PinOff, X } from "lucide-react";
import { useT } from "../lib/i18n";
import { HOME_TAB_ID, type WorkbenchTab } from "../lib/workbench-store";

export interface WorkbenchTabMenuProps {
  tab: WorkbenchTab;
  x: number;
  y: number;
  canOpenSplit: boolean;
  hasClosableRight: boolean;
  hasClosableOthers: boolean;
  onCloseMenu: () => void;
  onTogglePin: (id: string) => void;
  onOpenInSplit: (tab: WorkbenchTab) => void;
  onCloseToRight: (id: string) => void;
  onCloseOthers: (id: string) => void;
  onCloseTab: (id: string) => void;
}

/** Accessible context menu for managing a single Workbench tab. */
export function WorkbenchTabMenu({
  tab,
  x,
  y,
  canOpenSplit,
  hasClosableRight,
  hasClosableOthers,
  onCloseMenu,
  onTogglePin,
  onOpenInSplit,
  onCloseToRight,
  onCloseOthers,
  onCloseTab,
}: WorkbenchTabMenuProps) {
  const t = useT();
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      if (menuRef.current && !menuRef.current.contains(event.target as Node)) onCloseMenu();
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onCloseMenu();
    };
    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [onCloseMenu]);

  const maxLeft = typeof window !== "undefined" ? Math.max(8, window.innerWidth - 216) : x;
  const maxTop = typeof window !== "undefined" ? Math.max(8, window.innerHeight - 220) : y;
  const left = Math.max(8, Math.min(x, maxLeft));
  const top = Math.max(8, Math.min(y, maxTop));

  const run = (fn: () => void) => {
    fn();
    onCloseMenu();
  };

  return (
    <div
      ref={menuRef}
      role="menu"
      aria-label={t("workbench.tabMenu")}
      className="workbench-tab-menu fade-in"
      style={{ left, top }}
    >
      {tab.id !== HOME_TAB_ID ? (
        <button
          type="button"
          role="menuitem"
          className="workbench-tab-menu-item"
          onClick={() => run(() => onTogglePin(tab.id))}
        >
          {tab.pinned ? <PinOff size={13} aria-hidden /> : <Pin size={13} aria-hidden />}
          <span>{tab.pinned ? t("workbench.unpinTab") : t("workbench.pinTab")}</span>
        </button>
      ) : null}

      {canOpenSplit ? (
        <button
          type="button"
          role="menuitem"
          className="workbench-tab-menu-item"
          onClick={() => run(() => onOpenInSplit(tab))}
        >
          <Columns2 size={13} aria-hidden />
          <span>{t("workbench.openInSplit")}</span>
        </button>
      ) : null}

      {hasClosableRight ? (
        <button
          type="button"
          role="menuitem"
          className="workbench-tab-menu-item"
          onClick={() => run(() => onCloseToRight(tab.id))}
        >
          <ListX size={13} aria-hidden />
          <span>{t("workbench.closeToRight")}</span>
        </button>
      ) : null}

      {hasClosableOthers ? (
        <button
          type="button"
          role="menuitem"
          className="workbench-tab-menu-item"
          onClick={() => run(() => onCloseOthers(tab.id))}
        >
          <ListX size={13} aria-hidden />
          <span>{t("workbench.closeOthers")}</span>
        </button>
      ) : null}

      {!tab.pinned ? (
        <button
          type="button"
          role="menuitem"
          className="workbench-tab-menu-item"
          onClick={() => run(() => onCloseTab(tab.id))}
        >
          <X size={13} aria-hidden />
          <span>{t("workbench.closeTab")}</span>
        </button>
      ) : null}
    </div>
  );
}
