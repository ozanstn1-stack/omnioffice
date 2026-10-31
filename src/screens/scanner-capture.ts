/**
 * Stages the system camera's shots into the app cache (D4 batch capture).
 *
 * The WebView file chooser hands the camera app's captures over as `File`
 * objects; Rust needs paths, so each capture is copied into `cacheDir/scans/`
 * and the path is returned. One shot is one photo — the detector later decides
 * whether it holds one page or a two-page spread.
 */
import { appCacheDir, join } from "@tauri-apps/api/path";
import { writeFile } from "@tauri-apps/plugin-fs";
import { uid } from "../lib/format";
import { call } from "../lib/typed-call";

/** Copies the camera's files into the cache and returns their paths. */
export async function stageCameraFiles(files: FileList): Promise<string[]> {
  const cache = await appCacheDir();
  const directory = await join(cache, "scans", uid("scan"));
  await call((c) => c.ensureDir(directory));
  const staged: string[] = [];
  for (const file of Array.from(files)) {
    const cleanName = (file.name || `page-${staged.length + 1}.jpg`).replace(/[\\/:*?"<>|]/g, "_");
    const dest = await join(directory, cleanName);
    const bytes = new Uint8Array(await file.arrayBuffer());
    await writeFile(dest, bytes);
    staged.push(dest);
  }
  return staged;
}
