/**
 * The folder a development run seeds the vault with.
 *
 * The E2E driver cannot answer the native folder picker, so a `PDFSAK_DEV_RUN`
 * launch hands a file over through the existing dev context and the vault scans
 * that file's folder instead. It runs only behind the auto-run flag and never on
 * Android: a normal launch must not start indexing a folder on its own.
 */
export function devVaultFolders(paths: string[]): string[] {
  const folders: string[] = [];
  for (const path of paths) {
    const cut = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
    if (cut <= 0) continue;
    const folder = path.slice(0, cut);
    if (!folders.some((seen) => seen.toLowerCase() === folder.toLowerCase())) folders.push(folder);
  }
  return folders;
}
