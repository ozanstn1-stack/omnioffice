import {
  Archive,
  BookOpen,
  Calculator,
  CalendarClock,
  CalendarDays,
  Combine,
  FolderCheck,
  Eraser,
  FileCheck2,
  FileDiff,
  FileImage,
  FileSearch,
  FileSpreadsheet,
  FileText,
  GraduationCap,
  Landmark,
  LayoutTemplate,
  Lock,
  Mail,
  MailPlus,
  Minimize2,
  NotebookPen,
  Presentation,
  Receipt,
  Scale,
  ScanLine,
  ShieldCheck,
} from "lucide-react";
import { Button, Card } from "./ui";
import { useT } from "../lib/i18n";
import { PROFESSIONS, type DeskTask, type Profession, type ProfessionChoice } from "../lib/desks";
import type { Navigate, ScreenId } from "../lib/nav";

const PROFESSION_ICONS: Record<Profession, React.ReactNode> = {
  lawyer: <Scale size={18} />,
  accountant: <Calculator size={18} />,
  public: <Landmark size={18} />,
  student: <GraduationCap size={18} />,
};

const SCREEN_ICONS: Partial<Record<ScreenId, React.ReactNode>> = {
  petition: <FileText size={20} />,
  merge: <Combine size={20} />,
  redact: <Eraser size={20} />,
  eml: <Mail size={20} />,
  compare: <FileDiff size={20} />,
  scanner: <ScanLine size={20} />,
  planner: <CalendarDays size={20} />,
  deadline: <CalendarClock size={20} />,
  evidence: <FolderCheck size={20} />,
  kvkk: <ShieldCheck size={20} />,
  invoices: <Receipt size={20} />,
  spreadsheets: <FileSpreadsheet size={20} />,
  ocr: <FileSearch size={20} />,
  mailMerge: <MailPlus size={20} />,
  compress: <Minimize2 size={20} />,
  protect: <Lock size={20} />,
  documents: <FileText size={20} />,
  templates: <LayoutTemplate size={20} />,
  pdfStudio: <Archive size={20} />,
  documentHealth: <FileCheck2 size={20} />,
  imagesToPdf: <FileImage size={20} />,
  presentations: <Presentation size={20} />,
  notes: <NotebookPen size={20} />,
};

/** First-run question on Home: which desk should lead the screen. */
export function ProfessionPicker({ onChoose }: { onChoose: (choice: ProfessionChoice) => void }) {
  const t = useT();
  return (
    <section aria-labelledby="profession-title">
      <Card className="p-4 fade-in">
        <h2 id="profession-title" className="font-semibold text-[15px]">
          {t("desk.ask")}
        </h2>
        <p className="text-xs muted mt-1 mb-3">{t("desk.askHint")}</p>
        <div className="grid gap-2" style={{ gridTemplateColumns: "repeat(auto-fit, minmax(150px, 1fr))" }}>
          {PROFESSIONS.map((profession) => (
            <Button
              key={profession}
              icon={PROFESSION_ICONS[profession] as React.ReactElement}
              onClick={() => onChoose(profession)}
            >
              {t(`desk.profession.${profession}`)}
            </Button>
          ))}
        </div>
        <div className="flex justify-end mt-3">
          <Button size="sm" variant="ghost" onClick={() => onChoose("general")}>
            {t("desk.general")}
          </Button>
        </div>
      </Card>
    </section>
  );
}

/** The chosen profession's six to eight tasks, each a button to its screen. */
export function ProfessionDesk({
  profession,
  tasks,
  onNavigate,
  onChange,
}: {
  profession: Profession;
  tasks: DeskTask[];
  onNavigate: Navigate;
  onChange: () => void;
}) {
  const t = useT();
  return (
    <section aria-labelledby="desk-title">
      <div className="flex items-center gap-3 mb-3">
        <span style={{ color: "var(--accent)" }} aria-hidden>
          {PROFESSION_ICONS[profession]}
        </span>
        <h2 id="desk-title" className="text-[13px] font-bold uppercase tracking-wider muted flex-1">
          {t(`desk.title.${profession}`)}
        </h2>
        <Button size="sm" variant="ghost" onClick={onChange}>
          {t("desk.change")}
        </Button>
      </div>
      <div className="grid gap-3" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(230px, 1fr))" }}>
        {tasks.map((task) => (
          <button key={task.key} className="tool-card" onClick={() => onNavigate(task.screen)}>
            <span className="tool-icon" style={{ background: "var(--accent)", color: "var(--accent-text)" }}>
              {SCREEN_ICONS[task.screen] ?? <BookOpen size={20} />}
            </span>
            <span>
              <span className="block font-semibold text-[14.5px]">{t(`desk.task.${task.key}Title`)}</span>
              <span className="block text-xs muted mt-1 leading-relaxed">{t(`desk.task.${task.key}Body`)}</span>
            </span>
          </button>
        ))}
      </div>
    </section>
  );
}
