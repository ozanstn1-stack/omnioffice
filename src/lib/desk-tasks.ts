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
  /** The task can also run on a Writer text selection (even without a file path). */
  acceptsTextSelection?: boolean;
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
    acceptsTextSelection: true,
  },
  {
    id: "official-letter",
    titleKey: "desk.officialLetter",
    category: "task",
    keybinding: "Ctrl+Alt+L",
    target: "officialLetter",
    requiresFiles: false,
    acceptsTextSelection: true,
  },
  {
    id: "deadline",
    titleKey: "desk.deadline",
    category: "task",
    keybinding: "Ctrl+Alt+D",
    target: "deadline",
    requiresFiles: true,
    acceptsTextSelection: true,
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
    acceptsTextSelection: true,
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

/**
 * True when a DOM node lies inside a Writer editable block (`.writer-sheet` or
 * `[contenteditable="true"]` inside the Office workspace).
 */
export function isWriterSelectionNode(node: Node | null): boolean {
  if (!node) return false;
  const element = node instanceof HTMLElement ? node : node.parentElement;
  if (!element) return false;
  return Boolean(element.closest(".writer-sheet, [data-writer-editor], [contenteditable='true']"));
}

/**
 * Reads the active Writer selection from the DOM when present, or falls back to
 * the last remembered Writer selection captured before opening the Command
 * Palette.
 */
export function currentWriterSelection(remembered = ""): string {
  if (typeof window !== "undefined" && typeof window.getSelection === "function") {
    const selection = window.getSelection();
    if (
      selection &&
      !selection.isCollapsed &&
      (isWriterSelectionNode(selection.anchorNode) || isWriterSelectionNode(selection.focusNode))
    ) {
      const live = selection.toString().trim();
      if (live) return live;
    }
  }
  return remembered.trim();
}

/** The state a desk task needs to build its command. */
export interface DeskTaskDeps {
  getFiles: () => string[];
  getSelectionText?: () => string;
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

/**
 * Builds one palette command per desk task; `files` and any Writer text
 * selection are read at call time so both direct shortcuts and Command Palette
 * invocations carry the active selection into the target screen.
 */
export function deskTaskCommands(deps: DeskTaskDeps): DeskTaskCommand[] {
  const readSelection = () => deps.getSelectionText?.().trim() ?? "";
  return DESK_TASKS.map((task) => ({
    id: deskTaskCommandId(task),
    titleKey: task.titleKey,
    category: task.category,
    keybinding: task.keybinding,
    enabled: () =>
      !task.requiresFiles ||
      deps.getFiles().length > 0 ||
      Boolean(task.acceptsTextSelection && readSelection().length > 0),
    disabledReasonKey: "palette.taskNeedsFile",
    execute: () => {
      const selectionText = task.acceptsTextSelection ? readSelection() : "";
      deps.navigate(task.target, {
        files: deps.getFiles(),
        ...(task.pack ? { pack: task.pack } : {}),
        ...(selectionText ? { selectionText } : {}),
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
