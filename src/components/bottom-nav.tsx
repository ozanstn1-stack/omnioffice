import { useEffect, useState } from "react";
import { BookOpen, Home, LayoutGrid, Menu, ScanLine } from "lucide-react";
import { useT } from "../lib/i18n";
import type { Navigate, ScreenId } from "../lib/nav";

/**
 * Phone navigation within thumb reach (C11): the four places people go most
 * and "More", which opens the full drawer. Each tab also lights up for the
 * screens that belong to it (Documents, Spreadsheets and Presentations are
 * Office).
 */
export const BOTTOM_TABS: readonly { id: ScreenId; labelKey: string; also: readonly ScreenId[] }[] = [
  { id: "home", labelKey: "nav.home", also: [] },
  { id: "office", labelKey: "nav.office", also: ["documents", "spreadsheets", "presentations", "templates"] },
  { id: "reader", labelKey: "nav.reader", also: [] },
  { id: "scanner", labelKey: "nav.scanner", also: ["imagesToPdf"] },
];

const ICONS: Record<string, React.ReactNode> = {
  home: <Home size={20} />,
  office: <LayoutGrid size={20} />,
  reader: <BookOpen size={20} />,
  scanner: <ScanLine size={20} />,
};

/**
 * The on-screen keyboard takes a large share of the visual viewport; with it
 * open the tab bar would sit on top of the keyboard and push the field away.
 */
export function keyboardLikelyOpen(viewportHeight: number, windowHeight: number): boolean {
  return windowHeight > 0 && viewportHeight < windowHeight * 0.75;
}

function useKeyboardOpen(): boolean {
  const [open, setOpen] = useState(false);
  useEffect(() => {
    const viewport = typeof window !== "undefined" ? window.visualViewport : null;
    if (!viewport) return;
    const update = () => setOpen(keyboardLikelyOpen(viewport.height, window.innerHeight));
    viewport.addEventListener("resize", update);
    return () => viewport.removeEventListener("resize", update);
  }, []);
  return open;
}

export function BottomNav({
  screen,
  onNavigate,
  onMore,
}: {
  screen: ScreenId;
  onNavigate: Navigate;
  onMore: () => void;
}) {
  const t = useT();
  const keyboardOpen = useKeyboardOpen();
  if (keyboardOpen) return null;
  return (
    <nav className="bottom-nav" aria-label={t("nav.bottom")}>
      {BOTTOM_TABS.map((tab) => {
        const active = tab.id === screen || tab.also.includes(screen);
        return (
          <button
            key={tab.id}
            type="button"
            className="bottom-nav-item"
            aria-current={active ? "page" : undefined}
            onClick={() => onNavigate(tab.id)}
          >
            <span aria-hidden>{ICONS[tab.id]}</span>
            <span className="bottom-nav-label">{t(tab.labelKey)}</span>
          </button>
        );
      })}
      <button type="button" className="bottom-nav-item" onClick={onMore}>
        <span aria-hidden>
          <Menu size={20} />
        </span>
        <span className="bottom-nav-label">{t("nav.more")}</span>
      </button>
    </nav>
  );
}
