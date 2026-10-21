/**
 * Capability tokens for user files (B3 migration).
 *
 * The native dialog runs on the Rust side and returns an opaque token plus a
 * display name; the webview never receives the path. Commands migrate onto
 * these tokens slice by slice, so this wrapper is the only frontend surface
 * that should learn about them.
 */
import { invoke } from "@tauri-apps/api/core";

export interface FileToken {
  token: string;
  name: string;
}

/** Opens the native open dialog; resolves to null when the user cancels. */
export const pickDocumentToken = (extensions?: string[]) =>
  invoke<FileToken | null>("file_token_pick", { extensions });

/** Opens the native save dialog; the token allows writing to the target. */
export const pickSaveToken = (name?: string, extensions?: string[]) =>
  invoke<FileToken | null>("file_token_save", { name, extensions });

/** Forgets a token when the document or save target is closed. */
export const releaseFileToken = (token: string) => invoke<boolean>("file_token_release", { token });
