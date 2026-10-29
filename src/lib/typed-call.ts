/**
 * Typed command calls for the modules in the entry chunk (tauri-specta, #316).
 * The generated wrappers live in their own chunk: main.tsx preloads it with
 * the language before the first render (`preloadTypedCommands`), and once it
 * is in, a call goes straight to the wrapper, as synchronously as a plain
 * `invoke`. `call` unwraps a command that returns a Result (it rejects with
 * the command's error object, as `invoke` does); `callPlain` is for the others.
 */
import type { Value } from "./generated/specta";
import type { CommandResult } from "./typed-ipc";

type TypedIpc = typeof import("./typed-ipc");
type Commands = TypedIpc["commands"];

let typedIpc: TypedIpc | null = null;

/** Loads the generated wrappers (once); later calls use them without waiting. */
export const preloadTypedCommands = (): Promise<TypedIpc> =>
  typedIpc
    ? Promise.resolve(typedIpc)
    : import("./typed-ipc").then((module) => {
        typedIpc = module;
        return module;
      });

export const call = <T>(run: (commands: Commands) => Promise<CommandResult<T, unknown>>): Promise<T> =>
  typedIpc
    ? typedIpc.unwrap(run(typedIpc.commands))
    : preloadTypedCommands().then(({ commands, unwrap }) => unwrap(run(commands)));

export const callPlain = <T>(run: (commands: Commands) => Promise<T>): Promise<T> =>
  typedIpc ? run(typedIpc.commands) : preloadTypedCommands().then(({ commands }) => run(commands));

/** Opaque JSON a command stores as-is (settings, a job's payload, a document model). */
export const asJson = (value: unknown) => value as Value;
