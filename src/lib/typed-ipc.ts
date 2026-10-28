/**
 * Typed command calls (tauri-specta, #316). The generated wrappers in
 * `generated/specta.ts` resolve to `{ status: "ok", data }` or
 * `{ status: "error", error }`; `unwrap` turns that back into the promise a
 * plain `invoke` gives (it rejects with the command's error object), so the
 * screens keep their `reportError` handling while the arguments and results
 * are checked against the Rust types.
 */
export { commands } from "./generated/specta";

export type CommandResult<T, E> = { status: "ok"; data: T } | { status: "error"; error: E };

export async function unwrap<T, E>(call: Promise<CommandResult<T, E>>): Promise<T> {
  const result = await call;
  if (result.status === "error") throw result.error;
  return result.data;
}
