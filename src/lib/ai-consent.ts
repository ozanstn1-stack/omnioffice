/**
 * A remembered AI data notice (plan 4.5b). The base consent ("the extracted
 * text will be sent to {provider}") can be remembered for one provider and
 * host. The scope is the provider id plus the endpoint's host, so switching
 * provider or pointing the same provider at another host asks again. The
 * vault, embedding and page-image consents are never remembered.
 */

/** "provider|host" for the given AI settings; the host is lower-cased, without scheme or path. */
export function consentScope(provider?: string, baseUrl?: string): string {
  const raw = (baseUrl ?? "").trim();
  let host = raw.toLowerCase();
  try {
    host = new URL(raw).host.toLowerCase();
  } catch {
    // Not a URL: keep the trimmed text so an empty or odd value still compares exactly.
  }
  return `${(provider ?? "").trim()}|${host}`;
}

/** True when `saved` is the scope of these settings (and settings are known). */
export function isConsentRemembered(saved: string | undefined, provider?: string, baseUrl?: string): boolean {
  if (!saved || !provider) return false;
  return saved === consentScope(provider, baseUrl);
}

/** The host part of a saved scope, for the Settings screen ("api.deepseek.com"). */
export function consentScopeLabel(saved: string): string {
  const [provider, host] = saved.split("|");
  return host ? `${provider} · ${host}` : provider;
}
