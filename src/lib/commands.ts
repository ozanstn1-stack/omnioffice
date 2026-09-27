export type CommandCategory =
  | "file"
  | "edit"
  | "view"
  | "writer"
  | "calc"
  | "impress"
  | "pdf"
  | "ai"
  | "vault"
  | "plugins"
  | "settings"
  | "help";

export interface CommandContext {
  screen?: string;
  documentKind?: "writer" | "calc" | "impress";
  documentId?: string;
}

export interface CommandDefinition {
  id: string;
  titleKey: string;
  category: CommandCategory;
  keybinding?: string;
  icon?: string;
  enabled: () => boolean;
  execute: (context?: CommandContext) => void | Promise<void>;
  when?: () => boolean;
}

export interface CommandKeyEvent {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
}

interface ParsedChord {
  ctrl: boolean;
  shift: boolean;
  alt: boolean;
  key: string;
}

const NAMED_KEYS: Record<string, string> = {
  esc: "Escape",
  escape: "Escape",
  return: "Enter",
  enter: "Enter",
  space: "Space",
  spacebar: "Space",
  del: "Delete",
  delete: "Delete",
  ins: "Insert",
  insert: "Insert",
  up: "ArrowUp",
  down: "ArrowDown",
  left: "ArrowLeft",
  right: "ArrowRight",
  pageup: "PageUp",
  pagedown: "PageDown",
  home: "Home",
  end: "End",
  tab: "Tab",
  backspace: "Backspace",
  plus: "+",
  minus: "-",
  comma: ",",
  period: ".",
  slash: "/",
};

const commands = new Map<string, CommandDefinition>();

let translator: (key: string) => string = (key) => key;

export function setCommandTranslator(fn: (key: string) => string): void {
  translator = fn;
}

export function registerCommand(definition: CommandDefinition): void {
  commands.set(definition.id, definition);
}

export function unregisterCommand(id: string): boolean {
  return commands.delete(id);
}

export function listCommands(): CommandDefinition[] {
  return [...commands.values()];
}

export function getCommand(id: string): CommandDefinition | undefined {
  return commands.get(id);
}

export function commandKeybinding(id: string): string | undefined {
  return commands.get(id)?.keybinding;
}

function localizedTitle(command: CommandDefinition): string {
  try {
    const translated = translator(command.titleKey);
    if (translated) return translated;
  } catch {
  }
  return command.titleKey;
}

function isEnabled(command: CommandDefinition): boolean {
  try {
    return command.enabled();
  } catch {
    return false;
  }
}

function isVisible(command: CommandDefinition): boolean {
  if (!isEnabled(command)) return false;
  if (!command.when) return true;
  try {
    return command.when();
  } catch {
    return false;
  }
}

export function runCommand(id: string, context?: CommandContext): boolean {
  const command = commands.get(id);
  if (!command || !isEnabled(command)) return false;
  try {
    const result: unknown = command.execute(context);
    if (result && typeof result === "object" && "then" in result) {
      void Promise.resolve(result as Promise<void>).catch(() => undefined);
    }
    return true;
  } catch {
    return false;
  }
}

function fold(text: string): string {
  return text
    .toLowerCase()
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "");
}

function compareText(a: string, b: string): number {
  const left = fold(a);
  const right = fold(b);
  return left < right ? -1 : left > right ? 1 : 0;
}

function compareCommands(a: CommandDefinition, b: CommandDefinition): number {
  const byCategory = compareText(a.category, b.category);
  if (byCategory !== 0) return byCategory;
  const byTitle = compareText(localizedTitle(a), localizedTitle(b));
  if (byTitle !== 0) return byTitle;
  return compareText(a.id, b.id);
}

function scoreText(haystack: string, needle: string): number {
  if (!haystack || !needle) return -1;
  if (haystack === needle) return 1000;
  const index = haystack.indexOf(needle);
  if (index === 0) {
    return 800 - Math.min(haystack.length - needle.length, 100);
  }
  if (index > 0) {
    const before = haystack.charAt(index - 1);
    const boundary = before === " " || before === "-" || before === "_" || before === "/" || before === "." || before === ":";
    const base = boundary ? 600 : 400;
    return base - Math.min(index, 100) - Math.min(haystack.length - needle.length, 50) * 0.5;
  }
  let position = 0;
  let first = -1;
  let previous = -1;
  let gaps = 0;
  for (const char of needle) {
    const found = haystack.indexOf(char, position);
    if (found === -1) return -1;
    if (first === -1) first = found;
    if (previous !== -1) gaps += found - previous - 1;
    previous = found;
    position = found + 1;
  }
  return 250 - gaps * 4 - first * 2 - Math.min(haystack.length - needle.length, 60);
}

function scoreCommand(command: CommandDefinition, query: string, context?: CommandContext): number {
  const title = fold(localizedTitle(command));
  const titleKey = fold(command.titleKey);
  let best = scoreText(title, query);
  if (titleKey !== title) {
    best = Math.max(best, scoreText(titleKey, query) * 0.9);
  }
  best = Math.max(best, scoreText(fold(command.category), query) * 0.6);
  best = Math.max(best, scoreText(fold(command.id), query) * 0.5);
  if (best >= 0 && context?.documentKind && command.category === context.documentKind) {
    best += 120;
  }
  return best;
}

export function searchCommands(query: string, context?: CommandContext): CommandDefinition[] {
  const visible = listCommands().filter(isVisible);
  const trimmed = query.trim();
  if (!trimmed) {
    return visible.sort(compareCommands);
  }
  const needle = fold(trimmed);
  return visible
    .map((command) => ({ command, score: scoreCommand(command, needle, context) }))
    .filter((entry) => entry.score >= 0)
    .sort((a, b) => b.score - a.score || compareCommands(a.command, b.command))
    .map((entry) => entry.command);
}

function normalizeKey(key: string): string {
  const trimmed = key.trim();
  if (trimmed.length === 1) return trimmed.toUpperCase();
  const named = NAMED_KEYS[trimmed.toLowerCase()];
  if (named) return named;
  return trimmed.charAt(0).toUpperCase() + trimmed.slice(1);
}

function parseChord(binding: string): ParsedChord | null {
  const raw = binding.trim();
  if (!raw) return null;
  if (raw === "+") return { ctrl: false, shift: false, alt: false, key: "+" };
  let body = raw;
  let keyToken: string | null = null;
  if (raw.endsWith("++")) {
    keyToken = "+";
    body = raw.slice(0, -2);
  }
  const parts = body
    .split("+")
    .map((part) => part.trim())
    .filter(Boolean);
  if (keyToken === null) {
    keyToken = parts.pop() ?? null;
  }
  if (!keyToken) return null;
  let ctrl = false;
  let shift = false;
  let alt = false;
  for (const part of parts) {
    const token = part.toLowerCase();
    if (
      token === "ctrl" ||
      token === "control" ||
      token === "cmd" ||
      token === "command" ||
      token === "meta" ||
      token === "super" ||
      token === "win"
    ) {
      ctrl = true;
    } else if (token === "shift") {
      shift = true;
    } else if (token === "alt" || token === "option") {
      alt = true;
    } else {
      return null;
    }
  }
  return { ctrl, shift, alt, key: normalizeKey(keyToken) };
}

function chordsEqual(a: ParsedChord, b: ParsedChord): boolean {
  return a.ctrl === b.ctrl && a.shift === b.shift && a.alt === b.alt && a.key === b.key;
}

export function matchKeybinding(event: CommandKeyEvent): string | undefined {
  const chord: ParsedChord = {
    ctrl: Boolean(event.ctrlKey || event.metaKey),
    shift: Boolean(event.shiftKey),
    alt: Boolean(event.altKey),
    key: normalizeKey(event.key),
  };
  if (!chord.key) return undefined;
  for (const command of commands.values()) {
    if (!command.keybinding || !isEnabled(command)) continue;
    const binding = parseChord(command.keybinding);
    if (binding && chordsEqual(binding, chord)) return command.id;
  }
  return undefined;
}
