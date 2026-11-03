/**
 * The save target a development auto-run uses for the scanner.
 *
 * The E2E driver cannot answer a native save dialog, so when `PDFSAK_DEV_RUN`
 * is set the build writes next to the first page instead of asking. The branch
 * lives behind that flag only: a normal run always picks a target, and on
 * Android the mobile save path is untouched.
 */
export function devScanOutputPath(firstPath: string): string {
  const cut = Math.max(firstPath.lastIndexOf("/"), firstPath.lastIndexOf("\\"));
  const directory = cut >= 0 ? firstPath.slice(0, cut + 1) : "";
  const name = cut >= 0 ? firstPath.slice(cut + 1) : firstPath;
  const stem = name.replace(/\.[^.]+$/, "") || name;
  return `${directory}${stem}_scan.pdf`;
}
