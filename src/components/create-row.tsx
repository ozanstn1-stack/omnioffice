import { FileSpreadsheet, FileText, LayoutTemplate, Presentation } from "lucide-react";
import { useT } from "../lib/i18n";
import { useOfficeTabs } from "../lib/office-store";
import type { Navigate } from "../lib/nav";

/** The greeting on Home, by the local hour (7.31 redesign). */
export function greetingKey(hour: number): string {
  if (hour >= 5 && hour < 12) return "home.greetMorning";
  if (hour >= 12 && hour < 18) return "home.greetDay";
  return "home.greetEvening";
}

/** "Tuesday, 10 November" in the interface language. */
export function todayLabel(language: string, now: Date = new Date()): string {
  try {
    return new Intl.DateTimeFormat(language, { weekday: "long", day: "numeric", month: "long" }).format(now);
  } catch {
    return now.toDateString();
  }
}

const BLANKS = [
  {
    kind: "writer",
    labelKey: "home.newDocument",
    hint: "DOCX · ODT",
    tile: "doc-tile-writer",
    icon: <FileText size={20} />,
  },
  {
    kind: "calc",
    labelKey: "home.newSpreadsheet",
    hint: "XLSX · ODS · CSV",
    tile: "doc-tile-calc",
    icon: <FileSpreadsheet size={20} />,
  },
  {
    kind: "impress",
    labelKey: "home.newPresentation",
    hint: "PPTX · ODP",
    tile: "doc-tile-impress",
    icon: <Presentation size={20} />,
  },
] as const;

/**
 * One click from Home to a blank document, spreadsheet or presentation (it
 * used to take Office → the tab bar's small icons), plus the template
 * gallery. Each kind keeps its colour on every surface: blue documents, green
 * spreadsheets, orange presentations.
 */
export function CreateRow({ onNavigate }: { onNavigate: Navigate }) {
  const t = useT();
  return (
    <section aria-labelledby="home-create">
      <h2 id="home-create" className="section-title">
        {t("home.create")}
      </h2>
      <div className="create-row">
        {BLANKS.map((blank) => (
          <button
            key={blank.kind}
            type="button"
            className="create-card"
            onClick={() => {
              useOfficeTabs.getState().create(blank.kind);
              onNavigate("office");
            }}
          >
            <span className={`doc-tile doc-tile-lg ${blank.tile}`} aria-hidden>
              {blank.icon}
            </span>
            <span className="min-w-0">
              <span className="create-card-title">{t(blank.labelKey)}</span>
              <span className="create-card-hint">{blank.hint}</span>
            </span>
          </button>
        ))}
        <button type="button" className="create-card create-card-dashed" onClick={() => onNavigate("templates")}>
          <span className="doc-tile doc-tile-lg" aria-hidden>
            <LayoutTemplate size={20} />
          </span>
          <span className="min-w-0">
            <span className="create-card-title">{t("home.fromTemplate")}</span>
            <span className="create-card-hint">{t("home.templateHint")}</span>
          </span>
        </button>
      </div>
    </section>
  );
}
