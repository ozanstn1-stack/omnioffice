/**
 * Desk-task commands for the command palette (D13).
 *
 * The palette mirrors the route manifest for navigation; the desk tasks go one
 * step further and take the current file selection with them: "redact this
 * PDF", "extract the VAT summary", "build an evidence bundle". A task is data
 * — its title key, shortcut, target screen and whether it needs a file — so a
 * new desk task is one entry and it automatically appears in the palette with
 * a disabled reason when nothing is selected.
 *
 * Registration is a pure function of the dependencies, which keeps it testable
 * without the app shell and keeps the plugin command registry untouched.
 */
import type { CommandCategory } from "./commands";
import type { Navigate, ScreenId } from "./nav";
import type { TaskPackId } from "./ai-task-packs";

export interface DeskTask {
  /** Stable id; the palette command is `task.<id>`. */
  id: string;
  titleKey: string;
  category: CommandCategory;
  keybinding: string;
  target: ScreenId;
  /** The task needs files from the current selection. */
  requiresFiles: boolean;
  /** AI task pack the target screen should preselect, when it is an AI task. */
  pack?: TaskPackId;
}

export const DESK_TASKS: readonly DeskTask[] = [
  {
    id: "redact",
    titleKey: "desk.redact",
    category: "task",
    keybinding: "Ctrl+Alt+R",
    target: "redact",
    requiresFiles: true,
  },
  {
    id: "vat",
    titleKey: "desk.vat",
    category: "ai",
    keybinding: "Ctrl+Alt+V",
    target: "aiPacks",
    requiresFiles: true,
    pack: "kdv-ozeti",
  },
  {
    id: "evidence",
    titleKey: "desk.evidence",
    category: "task",
    keybinding: "Ctrl+Alt+E",
    target: "evidence",
    requiresFiles: true,
  },
  {
    id: "petition",
    titleKey: "desk.petition",
    category: "task",
    keybinding: "Ctrl+Alt+P",
    target: "petition",
    requiresFiles: false,
  },
  {
    id: "official-letter",
    titleKey: "desk.officialLetter",
    category: "task",
    keybinding: "Ctrl+Alt+L",
    target: "officialLetter",
    requiresFiles: false,
  },
  {
    id: "deadline",
    titleKey: "desk.deadline",
    category: "task",
    keybinding: "Ctrl+Alt+D",
    target: "deadline",
    requiresFiles: true,
  },
  {
    id: "kvkk",
    titleKey: "desk.kvkk",
    category: "task",
    keybinding: "Ctrl+Alt+K",
    target: "kvkk",
    requiresFiles: true,
  },
  {
    id: "health",
    titleKey: "desk.health",
    category: "task",
    keybinding: "Ctrl+Alt+H",
    target: "documentHealth",
    requiresFiles: true,
  },
  {
    id: "cleaner",
    titleKey: "desk.cleaner",
    category: "task",
    keybinding: "Ctrl+Alt+C",
    target: "cleaner",
    requiresFiles: true,
  },
  {
    id: "data",
    titleKey: "desk.data",
    category: "task",
    keybinding: "Ctrl+Alt+Y",
    target: "data",
    requiresFiles: false,
  },
] as const;

export const deskTaskCommandId = (task: DeskTask) => `task.${task.id}`;

/** The state a desk task needs to build its command. */
export interface DeskTaskDeps {
  getFiles: () => string[];
  navigate: Navigate;
  register: (command: DeskTaskCommand) => void;
  unregister: (id: string) => void;
}

export interface DeskTaskCommand {
  id: string;
  titleKey: string;
  category: CommandCategory;
  keybinding: string;
  enabled: () => boolean;
  disabledReasonKey: string;
  execute: () => void;
}

/** Builds one palette command per desk task; `files` is read at call time. */
export function deskTaskCommands(deps: DeskTaskDeps): DeskTaskCommand[] {
  return DESK_TASKS.map((task) => ({
    id: deskTaskCommandId(task),
    titleKey: task.titleKey,
    category: task.category,
    keybinding: task.keybinding,
    enabled: () => !task.requiresFiles || deps.getFiles().length > 0,
    disabledReasonKey: "palette.taskNeedsFile",
    execute: () => {
      deps.navigate(task.target, {
        files: deps.getFiles(),
        ...(task.pack ? { pack: task.pack } : {}),
      });
    },
  }));
}

/** Registers (or re-registers) every desk task and returns the ids it owns. */
export function registerDeskTaskCommands(deps: DeskTaskDeps): string[] {
  const commands = deskTaskCommands(deps);
  for (const command of commands) deps.register(command);
  return commands.map((command) => command.id);
}

/** Removes every desk task command, leaving plugin/view commands untouched. */
export function unregisterDeskTaskCommands(ids: readonly string[], unregister: (id: string) => void): void {
  for (const id of ids) unregister(id);
}
