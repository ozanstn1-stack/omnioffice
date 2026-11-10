import { useKeyboardOpen } from "../lib/viewport";
import { Home, LayoutGrid, Menu, Shapes, Sparkles } from "lucide-react";
import { ROUTES } from "../lib/routes";
import { useT } from "../lib/i18n";
import type { Navigate, ScreenId } from "../lib/nav";

/**
 * Phone navigation within thumb reach (C11): the four places people go most
 * and "More", which opens the full drawer. Each tab also lights up for the
 * screens that belong to it (Documents, Spreadsheets and Presentations are
 * Office). Since 7.31 the fourth tab is the assistant; the reader opens from a
 * file (Home, recents, "Open with") and stays one tap away in the drawer.
 * Since 7.32 the third tab is the Tools hub, lit for every tool screen; the
 * scanner is the hub's first PDF tile.
 */
/** Every screen the Tools hub leads to (the PDF, convert, security and productivity sections). */
const TOOL_SECTIONS = new Set(["productivity", "convert", "pdfTools", "images", "ocr", "security"]);
const TOOL_SCREENS: readonly ScreenId[] = ROUTES.filter(
  (route) => route.section !== null && TOOL_SECTIONS.has(route.section),
).map((route) => route.id);

export const BOTTOM_TABS: readonly { id: ScreenId; labelKey: string; also: readonly ScreenId[] }[] = [
  { id: "home", labelKey: "nav.home", also: [] },
  { id: "office", labelKey: "nav.office", also: ["documents", "spreadsheets", "presentations", "templates"] },
  { id: "tools", labelKey: "nav.tools", also: TOOL_SCREENS },
  { id: "ai", labelKey: "nav.assistant", also: ["aiPacks", "aiLibrary"] },
];

const ICONS: Record<string, React.ReactNode> = {
  home: <Home size={20} />,
  office: <LayoutGrid size={20} />,
  tools: <Shapes size={20} />,
  ai: <Sparkles size={20} />,
};

/**
 * The on-screen keyboard takes a large share of the visual viewport; with it
 * open the tab bar would sit on top of the keyboard and push the field away.
 * The helper and the hook live in `lib/viewport` so the screen action bar
 * uses the same measurement; the export keeps `keyboardLikelyOpen` reachable
 * for the tests that pin the threshold.
 */
export { keyboardLikelyOpen } from "../lib/viewport";

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
            <span className="bottom-nav-icon" aria-hidden>
              {ICONS[tab.id]}
            </span>
            <span className="bottom-nav-label">{t(tab.labelKey)}</span>
          </button>
        );
      })}
      <button type="button" className="bottom-nav-item" onClick={onMore}>
        <span className="bottom-nav-icon" aria-hidden>
          <Menu size={20} />
        </span>
        <span className="bottom-nav-label">{t("nav.more")}</span>
      </button>
    </nav>
  );
}
