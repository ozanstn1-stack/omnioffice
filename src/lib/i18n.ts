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

export type Translate = {
  (key: string, params?: Record<string, string | number>): string;
  /**
   * Count-aware string: picks `key.one` or `key.other` through
   * `Intl.PluralRules`, so "{count} file(s)" becomes "1 file" / "3 files" in
   * English and "1 dosya" / "3 dosya" in Turkish, which has no plural marker
   * after a numeral. A key without variants falls back to `key` itself.
   */
  plural(key: string, count: number, params?: Record<string, string | number>): string;
};

function fill(template: string, params?: Record<string, string | number>): string {
  if (!params) return template;
  let text = template;
  for (const [name, value] of Object.entries(params)) {
    text = text.replaceAll(`{${name}}`, String(value));
  }
  return text;
}

export function pluralVariant(language: string, count: number): string {
  // Turkish selects "one" for 1 and "other" otherwise, like English; the values
  // in tr.json are the same for both because the noun never takes -ler/-lar
  // after a numeral.
  return new Intl.PluralRules(language).select(count);
}

export function makeTranslate(language: string): Translate {
  const dict = dictionary(language);
  const lookup = (key: string) => dict[key] ?? loadedDictionary("en")?.[key];
  const translate = ((key: string, params?: Record<string, string | number>) => {
    return fill(lookup(key) ?? key, params);
  }) as Translate;
  translate.plural = (key, count, params) => {
    const variant = pluralVariant(language, count);
    const template = lookup(`${key}.${variant}`) ?? lookup(`${key}.other`) ?? lookup(key);
    return fill(template ?? key, { count, ...params });
  };
  return translate;
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
