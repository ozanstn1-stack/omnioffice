import { useLayoutEffect, useState } from "react";

export interface ListDropdownOptions {
  /** `useId()` of the editor; option ids are `${id}-${index}`. */
  id: string;
  /** The choices of the active cell's list validation (empty: no list). */
  items: string[];
  /** Changes when the list must close: another cell, sheet, or an edit starting. */
  resetKey: string;
  /** The active cell's shown value, read when the list opens. */
  currentText: () => string;
  /** Writes a chosen value into the active cell. */
  onChoose: (value: string) => void;
  /** Gets the keyboard back when the list closes. */
  gridRef: React.RefObject<HTMLDivElement | null>;
  /** The popup element: it takes the keyboard while the list is open. */
  popupRef: React.RefObject<HTMLDivElement | null>;
}

/**
 * The choice list of a list-validated Calc cell (#415, slice 3): its open
 * state and highlighted index, the keyboard (arrows, Home/End, Enter, Escape,
 * Tab, Alt+Up), closing when the cell changes, and keeping the keyboard and
 * the highlighted choice in view while it is open.
 */
export function useListDropdown({
  id,
  items,
  resetKey,
  currentText,
  onChoose,
  gridRef,
  popupRef,
}: ListDropdownOptions) {
  const [listDropdown, setListDropdown] = useState<{ index: number } | null>(null);

  // The choice list belongs to one cell: moving the selection, switching
  // sheets or starting an edit closes it.
  const [lastResetKey, setLastResetKey] = useState(resetKey);
  if (lastResetKey !== resetKey) {
    setLastResetKey(resetKey);
    setListDropdown(null);
  }
  const open = listDropdown !== null && items.length > 0;
  const index = Math.min(listDropdown?.index ?? 0, Math.max(0, items.length - 1));

  const show = () => {
    // Start on the cell's current value when it is one of the choices.
    const current = currentText().trim().toLowerCase();
    const found = items.findIndex((item) => item.toLowerCase() === current);
    setListDropdown({ index: Math.max(0, found) });
  };

  /** Closes the choice list and gives the keyboard back to the grid. */
  const close = () => {
    setListDropdown(null);
    gridRef.current?.focus({ preventScroll: true });
  };

  /** Writes a chosen list value into the active cell, like typing it. */
  const choose = (value: string) => {
    close();
    onChoose(value);
  };

  const handleKey = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (!listDropdown) return;
    const last = items.length - 1;
    const current = Math.min(listDropdown.index, last);
    const keys: Record<string, () => void> = {
      ArrowDown: () => setListDropdown({ index: Math.min(last, current + 1) }),
      ArrowUp: () => (event.altKey ? close() : setListDropdown({ index: Math.max(0, current - 1) })),
      Home: () => setListDropdown({ index: 0 }),
      End: () => setListDropdown({ index: last }),
      Enter: () => choose(items[current]),
      Escape: close,
      Tab: close,
    };
    const action = keys[event.key];
    if (!action) return;
    // Handled keys stay out of the grid, which would otherwise move the
    // selection or start typing into the cell.
    event.preventDefault();
    event.stopPropagation();
    action();
  };

  // Opening the list moves the keyboard into it; the highlighted choice stays
  // in view while the arrow keys move it.
  useLayoutEffect(() => {
    if (open) popupRef.current?.focus({ preventScroll: true });
  }, [open, popupRef]);
  useLayoutEffect(() => {
    if (!open) return;
    popupRef.current?.querySelector<HTMLElement>('[aria-selected="true"]')?.scrollIntoView?.({ block: "nearest" });
  }, [open, index, popupRef]);

  return {
    id,
    open,
    index,
    show,
    close,
    choose,
    handleKey,
    /** Highlights a choice (the pointer over an option). */
    highlight: (next: number) => setListDropdown({ index: next }),
    /** Closes without moving focus (focus already left the list). */
    dismiss: () => setListDropdown(null),
  };
}
