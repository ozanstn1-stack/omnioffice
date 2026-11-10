import type { ReactNode } from "react";
import { CommandPalette, GlobalSearch } from "./command-palette";
import { OverwriteDialog, PasswordDialog, Toasts } from "./files";
import { DataLossDialogHost } from "./data-loss-dialog";
import { FileConflictDialogHost } from "./file-conflict-dialog";
import { useT } from "../lib/i18n";
import type { RecentEntry } from "../lib/types";
import type { ScreenId } from "../lib/nav";

/**
 * Everything the app layers over the current screen, rendered once for the
 * phone shell and once for the desktop shell (they used to repeat it): toasts,
 * the background-jobs live region, the overwrite / password / data-loss /
 * file-conflict dialogs (one global host each, so the compatibility gate also
 * covers the converter outside the office workspace), the command palette
 * and global search. `presend` is the phone shell's pre-send dialog slot,
 * kept in its place between the overwrite and password dialogs.
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
      <CommandPalette open={paletteOpen} onClose={onClosePalette} context={{ screen }} onNavigate={onNavigate} />
      <GlobalSearch
        open={searchOpen}
        onClose={onCloseSearch}
        onOpenPath={onOpenPath}
        onOpenRecent={onOpenRecent}
        onNavigate={onNavigate}
      />
    </>
  );
}
