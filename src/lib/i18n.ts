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
  //
  // The state records *which* language is loaded, not a plain "ready" flag: a
  // component mounted while the settings still said English (App, the drawer)
  // had ready=true already, so the later setReady(true) for Turkish changed
  // nothing, no re-render happened and the translator built during the fetch
  // kept the English fallback - the mixed-language UI seen on a phone.
  const [loadedLanguage, setLoadedLanguage] = useState(() => (loadedDictionary(language) ? language : null));
  useEffect(() => {
    // Already loaded: the memo below rebuilds the translator because
    // `language` changed, so there is nothing to wait for.
    if (loadedDictionary(language)) return;
    let cancelled = false;
    void ensureLanguage(language)
      .then(() => {
        if (!cancelled) setLoadedLanguage(language);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [language]);
  return useMemo(() => {
    // `loadedLanguage` is the trigger for a fresh dictionary lookup once the
    // requested one lands; without it the memo would keep the English fallback.
    void loadedLanguage;
    return makeTranslate(language);
  }, [language, loadedLanguage]);
}
