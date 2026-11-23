import type { ReactNode } from "react";
import { FileSpreadsheet, FileText, Lock, Presentation, ScanLine, X } from "lucide-react";
import { buildNavHubs, NavTree } from "./sidebar";
import { Badge, IconButton } from "./ui";
import { useT } from "../lib/i18n";
import { useOfficeTabs } from "../lib/office-store";
import type { ScreenId } from "../lib/nav";

export function MobileToolsSheet({
  open,
  icons,
  screen,
  onClose,
  onPick,
}: {
  open: boolean;
  icons: Record<ScreenId, ReactNode>;
  screen: ScreenId;
  onClose: () => void;
  onPick: (id: ScreenId) => void;
}) {
  const t = useT();
  if (!open) return null;

  const createOffice = (kind: "writer" | "calc" | "impress") => {
    useOfficeTabs.getState().create(kind);
    onPick("office");
  };

  return (
    <div
      className="sheet-overlay"
      role="presentation"
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <aside className="bottom-sheet" role="dialog" aria-modal="true" aria-label={t("mobile.toolsDrawer")}>
        <div className="sheet-handle" aria-hidden />
        <div className="sheet-header">
          <h2 className="font-bold text-[15px]">{t("mobile.toolsDrawer")}</h2>
          <IconButton label={t("common.close")} onClick={onClose}>
            <X size={16} />
          </IconButton>
        </div>

        <div className="sheet-body">
          <div className="sheet-quick-grid" aria-label={t("mobile.quickCreate")}>
            <button type="button" className="sheet-quick-card" onClick={() => createOffice("writer")}>
              <span className="doc-tile doc-tile-writer" aria-hidden>
                <FileText size={16} />
              </span>
              <span className="truncate">{t("home.newDocument")}</span>
            </button>
            <button type="button" className="sheet-quick-card" onClick={() => createOffice("calc")}>
              <span className="doc-tile doc-tile-calc" aria-hidden>
                <FileSpreadsheet size={16} />
              </span>
              <span className="truncate">{t("home.newSpreadsheet")}</span>
            </button>
            <button type="button" className="sheet-quick-card" onClick={() => createOffice("impress")}>
              <span className="doc-tile doc-tile-impress" aria-hidden>
                <Presentation size={16} />
              </span>
              <span className="truncate">{t("home.newPresentation")}</span>
            </button>
            <button type="button" className="sheet-quick-card" onClick={() => onPick("scanner")}>
              <span className="doc-tile doc-tile-pdf" aria-hidden>
                <ScanLine size={16} />
              </span>
              <span className="truncate">{t("mobile.scanNow")}</span>
            </button>
          </div>

          <nav className="sheet-nav" aria-label={t("workbench.allHubs")}>
            <NavTree hubs={buildNavHubs(t, icons)} screen={screen} onPick={onPick} showLabels />
          </nav>
        </div>

        <div className="sheet-footer">
          <Badge tone="ok">
            <Lock size={10} /> {t("nav.privacy")}
          </Badge>
        </div>
      </aside>
    </div>
  );
}
