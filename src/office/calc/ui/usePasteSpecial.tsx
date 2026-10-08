/**
 * The Paste special flow: work out what is on the clipboard (the rich copy
 * kept in the editor when the system clipboard still holds its text, else the
 * plain text from elsewhere), ask what to paste, and paste it as one undo step.
 */
import { useState, type ReactNode, type RefObject } from "react";
import { useT } from "../../../lib/i18n";
import type { Sheet, Workbook } from "../../../lib/office-types";
import { useToasts } from "../../../lib/store";
import type { CellPosition } from "../grid-types";
import { isSheetProtected } from "../protection";
import { clipboardFromText, pasteSpecial, type ClipboardData, type PasteOptions } from "../paste-special";
import { PasteSpecialDialog } from "./PasteSpecialDialog";

/** The editor's own copy: the cells and the text it put on the system clipboard. */
export interface InternalClipboard {
  data: ClipboardData;
  text: string;
}

export function usePasteSpecial(host: {
  sheet: Sheet;
  sheetIndex: number;
  /** The top-left cell of the selection, where the paste lands. */
  target: CellPosition;
  internal: RefObject<InternalClipboard | null>;
  commit: (mutate: (workbook: Workbook) => Workbook) => void;
  /** Selects the pasted block. */
  select: (anchor: CellPosition, focus: CellPosition) => void;
}): { open: () => Promise<void>; dialog: ReactNode } {
  const t = useT();
  const { sheet, sheetIndex, target, internal, commit, select } = host;
  const [pending, setPending] = useState<{ data: ClipboardData; external: boolean } | null>(null);

  const open = async () => {
    const push = useToasts.getState().push;
    if (isSheetProtected(sheet)) {
      push({ kind: "info", title: t("calc.sheetProtected") });
      return;
    }
    const own = internal.current;
    let text = "";
    try {
      text = (await navigator.clipboard.readText()) ?? "";
    } catch {
      // The clipboard cannot be read (blocked, or no API): the editor's own copy is all there is.
    }
    // Operating systems change line endings on the way through the clipboard.
    const same = (a: string, b: string) => a.replace(/\r\n/g, "\n").trimEnd() === b.replace(/\r\n/g, "\n").trimEnd();
    if (own && (text === "" || same(text, own.text))) {
      setPending({ data: own.data, external: false });
      return;
    }
    const data = clipboardFromText(text);
    if (data.values.length === 0) {
      push({ kind: "info", title: t("calc.pasteNothing") });
      return;
    }
    setPending({ data, external: true });
  };

  const apply = (options: PasteOptions) => {
    if (!pending) return;
    const { data } = pending;
    setPending(null);
    commit((workbook) => pasteSpecial(workbook, sheetIndex, data, target, options));
    const rows = options.transpose ? (data.values[0]?.length ?? 1) : data.values.length;
    const cols = options.transpose ? data.values.length : (data.values[0]?.length ?? 1);
    select(target, { row: target.row + Math.max(1, rows) - 1, col: target.col + Math.max(1, cols) - 1 });
  };

  return {
    open,
    dialog: pending ? (
      <PasteSpecialDialog external={pending.external} onClose={() => setPending(null)} onApply={apply} />
    ) : null,
  };
}
