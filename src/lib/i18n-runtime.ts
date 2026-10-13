/**
 * Language loading state, kept apart from the dictionaries so a test setup can
 * warm the inactive language without pulling in the settings store (and, with
 * it, the Tauri API that per-file `vi.mock` calls must intercept first).
 */
type Dict = Record<string, string>;

let turkish: Dict | null = null;

/** Loads the inactive dictionary the first time it is needed. */
export async function ensureLanguage(language: string): Promise<void> {
  if (language !== "tr" || turkish) return;
  const module = await import("./i18n-tr");
  turkish = module.tr;
}

/** The loaded Turkish dictionary, or null when it is not in memory yet. */
export function getTurkish(): Dict | null {
  return turkish;
}
