/**
 * Typed command calls for the modules in the entry chunk (tauri-specta, #316).
 * The generated wrappers load on first use, so the entry chunk does not carry
 * them. `call` unwraps a command that returns a Result (it rejects with the
 * command's error object, as `invoke` does); `callPlain` is for the others.
 */
import type { CommandResult } from "./typed-ipc";

type Commands = typeof import("./generated/specta").commands;

const typedCommands = () => import("./typed-ipc");

export const call = <T>(run: (commands: Commands) => Promise<CommandResult<T, unknown>>): Promise<T> =>
  typedCommands().then(({ commands, unwrap }) => unwrap(run(commands)));

export const callPlain = <T>(run: (commands: Commands) => Promise<T>): Promise<T> =>
  typedCommands().then(({ commands }) => run(commands));
