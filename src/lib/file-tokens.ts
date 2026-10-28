/**
 * Capability tokens for user files (B3 migration).
 *
 * The native dialog runs on the Rust side and returns opaque tokens plus
 * display names; the webview never receives the paths. Commands resolve the
 * tokens in `paths::input_file`/`output_file`.
 */
import { call, callPlain } from "./typed-call";
import { rememberTokenName } from "./file-token-names";

export interface FileToken {
  token: string;
  name: string;
  /** Save targets only: the location the user just picked (for display). */
  path?: string;
}

/** Opens the native open dialog; an empty list means the user cancelled. */
export const pickDocumentTokens = async (extensions?: string[], multiple = false) => {
  const refs = await call((c) => c.fileTokenPick(extensions ?? null, multiple));
  for (const ref of refs) rememberTokenName(ref.token, ref.name);
  return refs;
};

/** Convenience for single-file tools. */
export const pickDocumentToken = async (extensions?: string[]): Promise<FileToken | null> => {
  const picked = await pickDocumentTokens(extensions);
  return picked[0] ?? null;
};

/** Opens the native folder dialog; the token allows writing into the folder. */
export const pickDirectoryToken = async (): Promise<FileToken | null> => {
  const picked = await call((c) => c.fileTokenPickDirectory());
  if (picked) rememberTokenName(picked.token, picked.name);
  return picked;
};

/** Opens the native save dialog; the token allows writing to the target. */
export const pickSaveToken = async (name?: string, extensions?: string[]): Promise<FileToken | null> => {
  const picked = await call((c) => c.fileTokenSave(name ?? null, extensions ?? null));
  const first = picked[0] ?? null;
  if (first) rememberTokenName(first.token, first.name);
  return first;
};

/** Forgets a token when the document or save target is closed. */
export const releaseFileToken = (token: string) => callPlain((c) => c.fileTokenRelease(token));

/** Asks Rust for a suggested save target: a write token and the display path. */
export const suggestOutput = (input: string, suffix: string, directory?: string) =>
  callPlain((c) => c.suggestOutput(input, suffix, directory ?? null));

/** The display path of a suggested target, or `fallback` if Rust refused. */
export const suggestedOutputPath = async (input: string, suffix: string, fallback: string) =>
  (await suggestOutput(input, suffix).catch(() => ({ path: fallback }))).path;

/** Display names for values that may be tokens or paths (never a path for tokens). */
export const displayNames = (values: string[]) => callPlain((c) => c.fileNames(values));
