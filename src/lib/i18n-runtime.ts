/**
 * Dictionary loading, kept apart from the translator so a test setup can prime
 * the dictionaries without a network round trip (and without pulling in the
 * settings store before each file's `vi.mock` calls).
 *
 * The dictionaries are JSON assets imported with `?url`: Vite emits them as
 * static files, so the JS bundle no longer carries the strings (they used to
 * cost ~57 KB gzip of the budget) and a session fetches only the language it
 * reads. The CSP allows same-origin fetches (`connect-src 'self'`).
 */
import enUrl from "./i18n/en.json?url";
import trUrl from "./i18n/tr.json?url";

type Dict = Record<string, string>;

// The cache lives on `globalThis` so a test that calls `vi.resetModules()` and
// re-imports a component still sees the dictionaries primed by the test setup
// (a plain module-level object would be recreated empty by that reset).
const store = ((globalThis as Record<string, unknown>).__i18nStore ??= {
  dictionaries: {} as Record<string, Dict>,
  pending: new Map<string, Promise<void>>(),
}) as {
  dictionaries: Record<string, Dict>;
  pending: Map<string, Promise<void>>;
};

const urls: Record<string, string> = { en: enUrl, tr: trUrl };

/** The asset URL for a language (`en` is the fallback). */
export function dictionaryUrl(language: string): string {
  return urls[language] ?? urls.en;
}

/** Fetches and caches a dictionary. Idempotent and safe to call concurrently. */
export async function ensureLanguage(language: string): Promise<void> {
  if (store.dictionaries[language]) return;
  const inflight = store.pending.get(language);
  if (inflight) return inflight;
  const load = (async () => {
    const response = await fetch(dictionaryUrl(language));
    if (!response.ok) {
      throw new Error(`i18n: ${language} (HTTP ${response.status})`);
    }
    store.dictionaries[language] = (await response.json()) as Dict;
  })().finally(() => store.pending.delete(language));
  store.pending.set(language, load);
  return load;
}

/** The loaded dictionary, or null while it is still being fetched. */
export function loadedDictionary(language: string): Dict | null {
  return store.dictionaries[language] ?? null;
}

/** Test-only: primes a dictionary without a fetch (see `src/test-setup.ts`). */
export function primeDictionary(language: string, dict: Dict): void {
  store.dictionaries[language] = dict;
}
