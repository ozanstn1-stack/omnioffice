/**
 * Capability tokens for user files (B3 migration).
 *
 * The native dialog runs on the Rust side and returns opaque tokens plus
 * display names; the webview never receives the paths. Commands resolve the
 * tokens in `paths::input_file`/`output_file`.
 */
import { invoke } from "@tauri-apps/api/core";

export interface FileToken {
  token: string;
  name: string;
  /** Save targets only: the location the user just picked (for display). */
  path?: string;
}

/** Opens the native open dialog; an empty list means the user cancelled. */
export const pickDocumentTokens = (extensions?: string[], multiple = false) =>
  invoke<FileToken[]>("file_token_pick", { extensions, multiple });

/** Convenience for single-file tools. */
export const pickDocumentToken = async (extensions?: string[]): Promise<FileToken | null> => {
  const picked = await pickDocumentTokens(extensions);
  return picked[0] ?? null;
};

/** Opens the native folder dialog; the token allows writing into the folder. */
export const pickDirectoryToken = () => invoke<FileToken | null>("file_token_pick_directory");

/** Opens the native save dialog; the token allows writing to the target. */
export const pickSaveToken = async (name?: string, extensions?: string[]): Promise<FileToken | null> => {
  const picked = await invoke<FileToken[]>("file_token_save", { name, extensions });
  return picked[0] ?? null;
};

/** Forgets a token when the document or save target is closed. */
export const releaseFileToken = (token: string) => invoke<boolean>("file_token_release", { token });

/** Asks Rust for a suggested save target: a write token and the display path. */
export const suggestOutput = (input: string, suffix: string, directory?: string) =>
  invoke<{ token: string; path: string }>("suggest_output", { input, suffix, directory });

/** The display path of a suggested target, or `fallback` if Rust refused. */
export const suggestedOutputPath = async (input: string, suffix: string, fallback: string) =>
  (await suggestOutput(input, suffix).catch(() => ({ path: fallback }))).path;

/** Display names for values that may be tokens or paths (never a path for tokens). */
export const displayNames = (values: string[]) => invoke<(string | null)[]>("file_names", { values });
