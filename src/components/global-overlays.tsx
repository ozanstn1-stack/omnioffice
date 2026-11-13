import { lazy, Suspense, type ReactNode } from "react";
import { OverwriteDialog, PasswordDialog, Toasts } from "./files";
import { DataLossDialogHost } from "./data-loss-dialog";
import { FileConflictDialogHost } from "./file-conflict-dialog";
import { useT } from "../lib/i18n";
import type { RecentEntry } from "../lib/types";
import type { ScreenId } from "../lib/nav";

// The palette and global search load on first open (3.5): neither is drawn
// while closed, so the entry chunk does not carry them or the command list.
const CommandPalette = lazy(() => import("./command-palette").then((m) => ({ default: m.CommandPalette })));
const GlobalSearch = lazy(() => import("./command-palette").then((m) => ({ default: m.GlobalSearch })));

/**
 * Everything the app layers over the current screen, rendered once for the
 * phone shell and once for the desktop shell (they used to repeat it): toasts,
 * the background-jobs live region, the overwrite / password / data-loss /
 * file-conflict dialogs (one global host each, so the compatibility gate also
 * covers the converter outside the office workspace), the command palette
 * and global search. `presend` is the pre-send dialog slot shared by both
 * the desktop and phone shells, kept between the overwrite and password
 * dialogs.
 */
export function GlobalOverlays({
  screen,
  backgroundJobs,
  paletteOpen,
  onClosePalette,
  searchOpen,
  onCloseSearch,
  onNavigate,
  onOpenPath,
  onOpenRecent,
  presend,
}: {
  screen: ScreenId;
  backgroundJobs: number;
  paletteOpen: boolean;
  onClosePalette: () => void;
  searchOpen: boolean;
  onCloseSearch: () => void;
  onNavigate: (screen: string) => void;
  onOpenPath: (path: string) => void;
  onOpenRecent: (entry: RecentEntry) => void;
  presend?: ReactNode;
}) {
  const t = useT();
  return (
    <>
      <Toasts />
      <div className="sr-only" role="status" aria-live="polite">
        {backgroundJobs > 0 ? t("jobs.runningCount", { count: backgroundJobs }) : ""}
      </div>
      <OverwriteDialog />
      {presend}
      <PasswordDialog />
      <DataLossDialogHost />
      <FileConflictDialogHost />
      <Suspense fallback={null}>
        {paletteOpen ? (
          <CommandPalette open onClose={onClosePalette} context={{ screen }} onNavigate={onNavigate} />
        ) : null}
        {searchOpen ? (
          <GlobalSearch
            open
            onClose={onCloseSearch}
            onOpenPath={onOpenPath}
            onOpenRecent={onOpenRecent}
            onNavigate={onNavigate}
          />
        ) : null}
      </Suspense>
    </>
  );
}
