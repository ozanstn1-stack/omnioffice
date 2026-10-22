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
}

/** Opens the native open dialog; an empty list means the user cancelled. */
export const pickDocumentTokens = (extensions?: string[], multiple = false) =>
  invoke<FileToken[]>("file_token_pick", { extensions, multiple });

/** Convenience for single-file tools. */
export const pickDocumentToken = async (extensions?: string[]): Promise<FileToken | null> => {
  const picked = await pickDocumentTokens(extensions);
  return picked[0] ?? null;
};

/** Opens the native save dialog; the token allows writing to the target. */
export const pickSaveToken = async (name?: string, extensions?: string[]): Promise<FileToken | null> => {
  const picked = await invoke<FileToken[]>("file_token_save", { name, extensions });
  return picked[0] ?? null;
};

/** Forgets a token when the document or save target is closed. */
export const releaseFileToken = (token: string) => invoke<boolean>("file_token_release", { token });
