/**
 * Profession desks (issue #341, C1): the six to eight jobs a lawyer, an
 * accountant, a public servant or a student comes to the app for, each one a
 * route that already exists. Home shows the chosen desk first and keeps the
 * full catalogue behind "All tools".
 *
 * Copy lives in the dictionaries: `desk.profession.<id>` (the picker button),
 * `desk.title.<id>` (the desk heading) and `desk.task.<key>Title` /
 * `desk.task.<key>Body` for every task.
 */
import { routeEnabled, type ScreenId } from "./routes";

export const PROFESSIONS = ["lawyer", "accountant", "public", "student"] as const;

export type Profession = (typeof PROFESSIONS)[number];

/** The stored choice: a profession, "general" (no desk) or "" (not asked yet). */
export type ProfessionChoice = Profession | "general" | "";

export interface DeskTask {
  /** Copy key under `desk.task.`; unique within a desk. */
  key: string;
  screen: ScreenId;
}

export const DESKS: Record<Profession, readonly DeskTask[]> = {
  lawyer: [
    { key: "petition", screen: "petition" },
    { key: "evidenceBundle", screen: "merge" },
    { key: "kvkkRedact", screen: "redact" },
    { key: "kepMessage", screen: "eml" },
    { key: "compareDrafts", screen: "compare" },
    { key: "scanCourtPapers", screen: "scanner" },
    { key: "deadlines", screen: "planner" },
    { key: "kvkkTexts", screen: "kvkk" },
  ],
  accountant: [
    { key: "invoiceInbox", screen: "invoices" },
    { key: "periodWorkbook", screen: "spreadsheets" },
    { key: "scanReceipts", screen: "scanner" },
    { key: "searchableStatements", screen: "ocr" },
    { key: "clientLetters", screen: "mailMerge" },
    { key: "monthlyBundle", screen: "merge" },
    { key: "shrinkForMail", screen: "compress" },
    { key: "protectReports", screen: "protect" },
  ],
  public: [
    { key: "officialLetter", screen: "documents" },
    { key: "letterTemplates", screen: "templates" },
    { key: "archivePdf", screen: "pdfStudio" },
    { key: "scanIncoming", screen: "scanner" },
    { key: "searchableArchive", screen: "ocr" },
    { key: "fileBundle", screen: "merge" },
    { key: "redactPersonal", screen: "redact" },
    { key: "documentHealth", screen: "documentHealth" },
  ],
  student: [
    { key: "scanNotes", screen: "scanner" },
    { key: "photosToPdf", screen: "imagesToPdf" },
    { key: "homework", screen: "documents" },
    { key: "slides", screen: "presentations" },
    { key: "shrinkUpload", screen: "compress" },
    { key: "mergeAssignments", screen: "merge" },
    { key: "studyNotes", screen: "notes" },
    { key: "schedule", screen: "planner" },
  ],
};

export function isProfession(value: unknown): value is Profession {
  return (PROFESSIONS as readonly unknown[]).includes(value);
}

/**
 * The desk for a choice, leaving out tasks whose route is behind an off flag
 * (a desk never links to a hidden screen). "general" and "" have no desk.
 */
export function deskTasks(choice: ProfessionChoice, enabled: (id: ScreenId) => boolean = routeEnabled): DeskTask[] {
  if (!isProfession(choice)) return [];
  return DESKS[choice].filter((task) => enabled(task.screen));
}
