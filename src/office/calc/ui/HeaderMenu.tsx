/** The context menu of a row or column header: insert, delete, hide, unhide. */
import { useEffect, useRef } from "react";

export interface HeaderMenuItem {
  id: string;
  label: string;
  onSelect: () => void;
}

export function HeaderMenu({
  label,
  x,
  y,
  items,
  onClose,
}: {
  /** Accessible name of the menu ("Row actions" / "Column actions"). */
  label: string;
  /** Pointer position in viewport pixels. */
  x: number;
  y: number;
  items: HeaderMenuItem[];
  /** Called when the menu goes away: after a choice, an outside press or Escape. */
  onClose: (reason: "select" | "outside" | "escape") => void;
}) {
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    menuRef.current?.querySelector<HTMLElement>('[role="menuitem"]')?.focus();
  }, []);

  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) onClose("outside");
    };
    window.addEventListener("pointerdown", onPointerDown, true);
    return () => window.removeEventListener("pointerdown", onPointerDown, true);
  }, [onClose]);

  const move = (direction: 1 | -1) => {
    const entries = Array.from(menuRef.current?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? []);
    if (entries.length === 0) return;
    const at = entries.indexOf(document.activeElement as HTMLElement);
    entries[(at + direction + entries.length) % entries.length].focus();
  };

  return (
    <div
      ref={menuRef}
      className="calc-header-menu"
      role="menu"
      tabIndex={-1}
      aria-label={label}
      // Kept inside the window; the menu is about 190px wide and 36px per item.
      style={{
        left: Math.max(0, Math.min(x, window.innerWidth - 200)),
        top: Math.max(0, Math.min(y, window.innerHeight - items.length * 36 - 12)),
      }}
      onKeyDown={(event) => {
        if (event.key === "ArrowDown" || event.key === "ArrowUp") {
          event.preventDefault();
          move(event.key === "ArrowDown" ? 1 : -1);
        } else if (event.key === "Escape") {
          event.preventDefault();
          event.stopPropagation();
          onClose("escape");
        } else if (event.key === "Tab") {
          event.preventDefault();
          move(event.shiftKey ? -1 : 1);
        }
      }}
    >
      {items.map((item) => (
        <button
          key={item.id}
          type="button"
          role="menuitem"
          className="calc-header-menu-item"
          onClick={() => {
            item.onSelect();
            onClose("select");
          }}
        >
          {item.label}
        </button>
      ))}
    </div>
  );
}
