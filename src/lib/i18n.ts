import { useEffect, useMemo, useState } from "react";
import { useSettings } from "./store";
import { ensureLanguage, loadedDictionary } from "./i18n-runtime";

type Dict = Record<string, string>;

export { ensureLanguage } from "./i18n-runtime";

/**
 * The dictionaries are JSON assets fetched on first use (`i18n-runtime.ts`),
 * not JS chunks: the bundle budget measures behaviour, not strings. Until a
 * dictionary lands the translator falls back to English or the raw key; the
 * app awaits English before its first render (`main.tsx`).
 */
function dictionary(language: string): Dict {
  return loadedDictionary(language) ?? loadedDictionary("en") ?? {};
}

export type Translate = (key: string, params?: Record<string, string | number>) => string;

export function makeTranslate(language: string): Translate {
  const dict = dictionary(language);
  return (key, params) => {
    let text = dict[key] ?? loadedDictionary("en")?.[key] ?? key;
    if (params) {
      for (const [name, value] of Object.entries(params)) {
        text = text.replaceAll(`{${name}}`, String(value));
      }
    }
    return text;
  };
}

export function useT(): Translate {
  const language = useSettings((state) => state.settings.language);
  // Re-render once the requested dictionary lands (English is awaited at
  // startup; Turkish and any future language load on first use).
  const [ready, setReady] = useState(() => loadedDictionary(language) !== null);
  useEffect(() => {
    if (loadedDictionary(language)) return;
    let cancelled = false;
    void ensureLanguage(language)
      .then(() => {
        if (!cancelled) setReady(true);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [language]);
  return useMemo(() => {
    // `ready` is the trigger for a fresh dictionary lookup once it lands;
    // without it the memo would keep the English fallback.
    void ready;
    return makeTranslate(language);
  }, [language, ready]);
}
