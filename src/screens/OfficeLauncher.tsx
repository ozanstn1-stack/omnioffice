import { useEffect } from "react";
import { FilePlus2, FolderOpen, LayoutTemplate } from "lucide-react";
import { Screen } from "../components/layout";
import { RecentTable } from "../components/recent-table";
import { useT } from "../lib/i18n";
import { useRecent, useSettings, useToasts } from "../lib/store";
import { flagEnabled } from "../lib/flags";
import { openOfficePath, useOfficeTabs } from "../lib/office-store";
import { openIntoWorkspace } from "../office/useOfficeSession";
import { shareRecent } from "../lib/presend-gate";

export type OfficeKind = "writer" | "calc" | "impress";

const OFFICE_EXTENSIONS: Record<OfficeKind, string[]> = {
  // UYAP `.udf` is experimental and only listed with the flag on.
  writer: ["docx", "odt", "rtf", "txt", "md", ...(flagEnabled("experimentalUdf") ? ["udf"] : [])],
  calc: ["xlsx", "xls", "ods", "csv", "tsv"],
  impress: ["pptx", "odp"],
};

const TITLE: Record<OfficeKind, string> = {
  writer: "nav.documents",
  calc: "nav.spreadsheets",
  impress: "nav.presentations",
};

const NEW_LABEL: Record<OfficeKind, string> = {
  writer: "home.newDocument",
  calc: "home.newSpreadsheet",
  impress: "home.newPresentation",
};

/**
 * Documents / Spreadsheets / Presentations (7.32.1): blank, from a template
 * or open, then this kind's recent files in Home's table. Moved out of
 * App.tsx; it used to be a plain list of file names.
 */
export function OfficeLauncher({
  kind,
  onOpen,
  onTemplates,
}: {
  kind: OfficeKind;
  onOpen: () => void;
  onTemplates: () => void;
}) {
  const t = useT();
  const recent = useRecent((state) => state.entries);
  const refreshRecent = useRecent((state) => state.refresh);
  const language = useSettings((state) => state.settings.language);

  useEffect(() => {
    void refreshRecent();
  }, [refreshRecent]);

  const openPath = async (path: string) => {
    // The shared opener also handles legacy imports, the conflict fingerprint
    // and the recent list, which a direct openDocument call skipped.
    const result = await openOfficePath(path);
    if (result.ok) {
      onOpen();
    } else {
      useToasts.getState().push({ kind: "error", title: t("errors.title"), detail: result.error });
    }
  };

  const related = recent.filter((entry) =>
    OFFICE_EXTENSIONS[kind].includes((entry.fileName.split(".").pop() ?? "").toLowerCase()),
  );

  return (
    <Screen
      title={t(TITLE[kind])}
      subtitle={t("office.noTabsHint")}
      actions={
        <>
          <button type="button" className="btn btn-ghost" onClick={onTemplates}>
            <LayoutTemplate size={16} /> {t("home.fromTemplate")}
          </button>
          <button
            type="button"
            className="btn"
            onClick={async () => {
              const opened = await openIntoWorkspace();
              if (opened) onOpen();
            }}
          >
            <FolderOpen size={16} /> {t("common.open")}
          </button>
          <button
            type="button"
            className="btn btn-primary"
            onClick={() => {
              useOfficeTabs.getState().create(kind);
              onOpen();
            }}
          >
            <FilePlus2 size={16} /> {t(NEW_LABEL[kind])}
          </button>
        </>
      }
    >
      <section aria-labelledby="launcher-recent">
        <h2 id="launcher-recent" className="section-title">
          {t("home.recent")}
        </h2>
        <RecentTable
          entries={related}
          language={language}
          filters={false}
          limit={20}
          onOpen={(entry) => void openPath(entry.token)}
          onShare={(entry) => void shareRecent(entry.token, entry.fileName).catch(() => undefined)}
        />
      </section>
    </Screen>
  );
}
