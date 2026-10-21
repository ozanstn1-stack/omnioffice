/**
 * Productivity screens barrel.
 *
 * The implementations live in `tools/` so each screen is its own lazy chunk
 * (#328); this module keeps the old import path working for tests and keeps
 * the shared paste helper.
 */
import { ArrowLeft, Search } from "lucide-react";
import { readFile } from "@tauri-apps/plugin-fs";

export { CleanerScreen } from "./tools/cleaner";
export { ConverterScreen } from "./tools/converter";
export { DataScreen, parseCsv, parseJsonTable } from "./tools/data";
export { DrawScreen } from "./tools/draw";
export { NotesScreen } from "./tools/notes";
export { PdfFormsScreen } from "./tools/pdf-forms";
export { PlannerScreen } from "./tools/planner";
export { TemplatesScreen } from "./tools/templates";

/** Raw text of a file the user already picked (paste helpers). */
export async function readTextFile(path: string): Promise<string> {
  const bytes = await readFile(path);
  return new TextDecoder().decode(bytes);
}

export { ArrowLeft, Search };
