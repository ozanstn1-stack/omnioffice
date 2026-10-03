import { afterEach, describe, expect, it, vi } from "vitest";
import {
  commandKeybinding,
  getCommand,
  listCommands,
  matchKeybinding,
  registerCommand,
  runCommand,
  searchCommands,
  setCommandTranslator,
  unregisterCommand,
  type CommandDefinition,
} from "./commands";

const tracked: string[] = [];

function define(
  definition: Partial<CommandDefinition> & Pick<CommandDefinition, "id" | "titleKey">,
): CommandDefinition {
  const command: CommandDefinition = {
    id: definition.id,
    titleKey: definition.titleKey,
    category: definition.category ?? "view",
    keybinding: definition.keybinding,
    icon: definition.icon,
    enabled: definition.enabled ?? (() => true),
    execute: definition.execute ?? (() => undefined),
    when: definition.when,
  };
  registerCommand(command);
  tracked.push(command.id);
  return command;
}

afterEach(() => {
  for (const id of tracked.splice(0)) unregisterCommand(id);
  setCommandTranslator((key) => key);
});

describe("command registry", () => {
  it("registers, lists, retrieves and removes commands", () => {
    define({ id: "file.new", titleKey: "commands.new" });
    expect(getCommand("file.new")?.titleKey).toBe("commands.new");
    expect(listCommands().map((command) => command.id)).toContain("file.new");
    expect(unregisterCommand("file.new")).toBe(true);
    expect(getCommand("file.new")).toBeUndefined();
    expect(unregisterCommand("file.new")).toBe(false);
  });

  it("replaces a command registered under the same id", () => {
    define({ id: "file.new", titleKey: "commands.new" });
    define({ id: "file.new", titleKey: "commands.newer" });
    expect(getCommand("file.new")?.titleKey).toBe("commands.newer");
    expect(listCommands().filter((command) => command.id === "file.new")).toHaveLength(1);
  });

  it("runs an enabled command with the supplied context", () => {
    const execute = vi.fn();
    define({ id: "file.new", titleKey: "commands.new", execute });
    const context = { screen: "documents", documentKind: "writer" as const, documentId: "doc-1" };
    expect(runCommand("file.new", context)).toBe(true);
    expect(execute).toHaveBeenCalledWith(context);
  });

  it("is a no-op for missing and disabled commands", () => {
    const execute = vi.fn();
    define({ id: "file.hidden", titleKey: "commands.hidden", enabled: () => false, execute });
    expect(runCommand("nope")).toBe(false);
    expect(runCommand("file.hidden")).toBe(false);
    expect(execute).not.toHaveBeenCalled();
  });

  it("swallows synchronous and asynchronous failures", async () => {
    const sync = vi.fn(() => {
      throw new Error("boom");
    });
    const rejects = vi.fn(() => Promise.reject(new Error("boom")));
    define({ id: "file.sync", titleKey: "commands.sync", execute: sync });
    define({ id: "file.async", titleKey: "commands.async", execute: rejects });
    expect(runCommand("file.sync")).toBe(false);
    expect(runCommand("file.async")).toBe(true);
    await Promise.resolve();
  });
});

describe("keybinding matching", () => {
  it("matches modifiers regardless of order and treats meta as ctrl", () => {
    define({ id: "view.palette", titleKey: "commands.palette", keybinding: "Ctrl+Shift+P" });
    define({ id: "file.save", titleKey: "commands.save", keybinding: "Shift+Ctrl+S" });
    define({ id: "view.next", titleKey: "commands.next", keybinding: "Alt+ArrowDown" });
    define({ id: "edit.plus", titleKey: "commands.plus", keybinding: "Ctrl++" });

    expect(
      matchKeybinding({ key: "P", ctrlKey: true, metaKey: false, shiftKey: false, altKey: false }),
    ).toBeUndefined();
    expect(matchKeybinding({ key: "P", ctrlKey: true, metaKey: false, shiftKey: true, altKey: false })).toBe(
      "view.palette",
    );
    expect(matchKeybinding({ key: "p", ctrlKey: false, metaKey: true, shiftKey: true, altKey: false })).toBe(
      "view.palette",
    );
    expect(matchKeybinding({ key: "s", ctrlKey: true, metaKey: false, shiftKey: true, altKey: false })).toBe(
      "file.save",
    );
    expect(matchKeybinding({ key: "ArrowDown", ctrlKey: false, metaKey: false, shiftKey: false, altKey: true })).toBe(
      "view.next",
    );
    expect(
      matchKeybinding({ key: "ArrowDown", ctrlKey: true, metaKey: false, shiftKey: false, altKey: true }),
    ).toBeUndefined();
    expect(matchKeybinding({ key: "+", ctrlKey: true, metaKey: false, shiftKey: false, altKey: false })).toBe(
      "edit.plus",
    );
  });

  it("ignores disabled commands and reports keybindings", () => {
    define({ id: "view.off", titleKey: "commands.off", keybinding: "Ctrl+U", enabled: () => false });
    define({ id: "view.on", titleKey: "commands.on", keybinding: "Ctrl+K" });
    expect(
      matchKeybinding({ key: "u", ctrlKey: true, metaKey: false, shiftKey: false, altKey: false }),
    ).toBeUndefined();
    expect(matchKeybinding({ key: "k", ctrlKey: true, metaKey: false, shiftKey: false, altKey: false })).toBe(
      "view.on",
    );
    expect(commandKeybinding("view.on")).toBe("Ctrl+K");
    expect(commandKeybinding("missing")).toBeUndefined();
  });
});

describe("search", () => {
  function defineOrganizeCommands(): void {
    setCommandTranslator(
      (key) =>
        ({
          "cmd.org": "Org",
          "cmd.organize": "Organize",
          "cmd.reorganize": "Reorganize",
          "cmd.fuzzy": "Open Registry Guide",
        })[key] ?? key,
    );
    define({ id: "exact", titleKey: "cmd.org" });
    define({ id: "prefix", titleKey: "cmd.organize" });
    define({ id: "contains", titleKey: "cmd.reorganize" });
    define({ id: "fuzzy", titleKey: "cmd.fuzzy" });
  }

  it("ranks an exact title first and a fuzzy subsequence last", () => {
    defineOrganizeCommands();
    expect(searchCommands("org").map((command) => command.id)).toEqual(["exact", "prefix", "contains", "fuzzy"]);
  });

  it("searches the localized title and hides disabled commands", () => {
    setCommandTranslator((key) => (key === "cmd.duzenle" ? "Düzenle" : key));
    define({ id: "localized", titleKey: "cmd.duzenle" });
    define({ id: "disabled", titleKey: "cmd.duzenle.disabled", enabled: () => false });
    expect(searchCommands("duzen").map((command) => command.id)).toContain("localized");
    expect(searchCommands("düzen").map((command) => command.id)).toContain("localized");
    expect(searchCommands("").map((command) => command.id)).not.toContain("disabled");
  });

  it("returns enabled commands sorted by category and title for an empty query", () => {
    define({ id: "view.b", titleKey: "t.b", category: "view" });
    define({ id: "view.a", titleKey: "t.a", category: "view" });
    define({ id: "file.c", titleKey: "t.c", category: "file" });
    define({ id: "hidden", titleKey: "t.hidden", category: "file", when: () => false });
    expect(searchCommands("").map((command) => command.id)).toEqual(["file.c", "view.a", "view.b"]);
    expect(searchCommands("hidden")).toEqual([]);
  });

  it("boosts commands for the active document kind", () => {
    setCommandTranslator((key) => (key === "cmd.same" ? "Same" : key));
    define({ id: "writer", titleKey: "cmd.same", category: "writer" });
    define({ id: "pdf", titleKey: "cmd.same", category: "pdf" });
    expect(searchCommands("same").map((command) => command.id)).toEqual(["pdf", "writer"]);
    expect(searchCommands("same", { documentKind: "writer" }).map((command) => command.id)).toEqual(["writer", "pdf"]);
  });
});
