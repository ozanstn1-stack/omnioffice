import type { ReactNode } from "react";
import { Plus, X } from "lucide-react";
import { IconButton } from "./ui";
import { useT } from "../lib/i18n";
import { routeEnabled, routeFor } from "../lib/routes";
import type { ScreenId } from "../lib/nav";
import type { WorkbenchTab } from "../lib/workbench-store";

export function MobileTabGrid({
  open,
  tabs,
  activeTabId,
  icons,
  onClose,
  onActivate,
  onCloseTab,
  onNewTab,
}: {
  open: boolean;
  tabs: WorkbenchTab[];
  activeTabId: string;
  icons: Record<ScreenId, ReactNode>;
  onClose: () => void;
  onActivate: (tab: WorkbenchTab) => void;
  onCloseTab: (id: string) => void;
  onNewTab: () => void;
}) {
  const t = useT();
  if (!open) return null;

  const labelFor = (tab: WorkbenchTab) => {
    if (tab.title) return tab.title;
    return routeEnabled(tab.screen) ? t(routeFor(tab.screen).labelKey) : t("app.name");
  };

  return (
    <div
      className="sheet-overlay"
      role="presentation"
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <section className="bottom-sheet" role="dialog" aria-modal="true" aria-label={t("mobile.tabsTitle")}>
        <div className="sheet-handle" aria-hidden />
        <div className="sheet-header">
          <h2 className="font-bold text-[15px]">
            {t("mobile.tabsTitle")} ({tabs.length})
          </h2>
          <IconButton label={t("common.close")} onClick={onClose}>
            <X size={16} />
          </IconButton>
        </div>

        <div className="sheet-body">
          <div className="tab-grid-cards">
            {tabs.map((tab) => {
              const active = tab.id === activeTabId;
              return (
                <div
                  key={tab.id}
                  className={`tab-grid-card${active ? " is-active" : ""}`}
                  role="button"
                  tabIndex={0}
                  onClick={() => {
                    onActivate(tab);
                    onClose();
                  }}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" || event.key === " ") {
                      event.preventDefault();
                      onActivate(tab);
                      onClose();
                    }
                  }}
                >
                  <div className="tab-grid-card-head">
                    <span className="tab-grid-card-icon" aria-hidden>
                      {icons[tab.screen]}
                    </span>
                    {!tab.pinned ? (
                      <button
                        type="button"
                        className="tab-grid-close"
                        aria-label={t("workbench.closeTab")}
                        onClick={(event) => {
                          event.stopPropagation();
                          onCloseTab(tab.id);
                        }}
                      >
                        <X size={13} />
                      </button>
                    ) : null}
                  </div>
                  <div className="tab-grid-card-title">{labelFor(tab)}</div>
                  <div className="tab-grid-card-kind">
                    {routeEnabled(tab.screen) ? t(routeFor(tab.screen).labelKey) : ""}
                  </div>
                </div>
              );
            })}

            <button
              type="button"
              className="tab-grid-card tab-grid-card-new"
              onClick={() => {
                onClose();
                onNewTab();
              }}
            >
              <Plus size={22} aria-hidden />
              <span className="text-[12.5px] font-semibold">{t("workbench.newTab")}</span>
            </button>
          </div>
        </div>
      </section>
    </div>
  );
}
