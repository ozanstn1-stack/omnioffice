import { describe, expect, it, vi } from "vitest";
import enJson from "./i18n/en.json";
import trJson from "./i18n/tr.json";

const en = enJson as Record<string, string>;
const tr = trJson as Record<string, string>;
import {
  DESK_TASKS,
  deskTaskCommandId,
  deskTaskCommands,
  registerDeskTaskCommands,
  unregisterDeskTaskCommands,
} from "./desk-tasks";
import type { CommandDefinition } from "./commands";
import type { Navigate } from "./nav";

describe("desk task catalogue (D13)", () => {
  it("keeps ids and shortcuts unique and every title translated", () => {
    expect(new Set(DESK_TASKS.map((task) => task.id)).size).toBe(DESK_TASKS.length);
    expect(new Set(DESK_TASKS.map((task) => task.keybinding)).size).toBe(DESK_TASKS.length);
    for (const task of DESK_TASKS) {
      expect(en[task.titleKey], task.titleKey).toBeTruthy();
      expect(tr[task.titleKey], task.titleKey).toBeTruthy();
      expect(task.keybinding).toMatch(/^Ctrl\+Alt\+[A-Z]$/);
    }
  });

  it("covers the profession desk screens", () => {
    const targets = DESK_TASKS.map((task) => task.target);
    for (const screen of [
      "redact",
      "aiPacks",
      "evidence",
      "petition",
      "officialLetter",
      "deadline",
      "kvkk",
      "documentHealth",
      "cleaner",
      "data",
    ] as const) {
      expect(targets, screen).toContain(screen);
    }
    // The VAT task selects its pack, so the palette does not land on the wrong one.
    expect(DESK_TASKS.find((task) => task.id === "vat")?.pack).toBe("kdv-ozeti");
  });

  it("disables a file task without a selection and explains why", () => {
    let files: string[] = [];
    const navigate = vi.fn<Navigate>();
    const commands = deskTaskCommands({
      getFiles: () => files,
      navigate,
      register: vi.fn(),
      unregister: vi.fn(),
    });
    const redact = commands.find((command) => command.id === "task.redact");
    expect(redact?.enabled()).toBe(false);
    expect(redact?.disabledReasonKey).toBe("palette.taskNeedsFile");
    files = ["/docs/belge.pdf"];
    expect(redact?.enabled()).toBe(true);
    redact?.execute();
    expect(navigate).toHaveBeenCalledWith("redact", { files: ["/docs/belge.pdf"] });
    // A task that works without a file stays enabled.
    expect(commands.find((command) => command.id === "task.petition")?.enabled()).toBe(true);
  });

  it("takes its pack and files to the AI screen", () => {
    const navigate = vi.fn<Navigate>();
    const commands = deskTaskCommands({
      getFiles: () => ["/docs/fatura.pdf"],
      navigate,
      register: vi.fn(),
      unregister: vi.fn(),
    });
    commands.find((command) => command.id === "task.vat")?.execute();
    expect(navigate).toHaveBeenCalledWith("aiPacks", { files: ["/docs/fatura.pdf"], pack: "kdv-ozeti" });
  });
});

describe("desk task registration", () => {
  it("registers one command per task and removes only its own ids", () => {
    const registered: CommandDefinition[] = [];
    const unregister = vi.fn();
    const ids = registerDeskTaskCommands({
      getFiles: () => [],
      navigate: vi.fn(),
      register: (command) => registered.push(command as CommandDefinition),
      unregister,
    });
    expect(ids).toEqual(DESK_TASKS.map(deskTaskCommandId));
    expect(registered).toHaveLength(DESK_TASKS.length);
    expect(registered.every((command) => command.category === "task" || command.category === "ai")).toBe(true);
    unregisterDeskTaskCommands(ids, unregister);
    expect(unregister).toHaveBeenCalledTimes(ids.length);
  });
});
