/**
 * e-Devlet "barkodlu belge" verification detection.
 *
 * Documents produced through the e-Devlet Kapısı carry a barcode/verification
 * block with a verification code ("Doğrulama Kodu" / "Barkod No"). This module
 * recognises that block in a PDF's text layer and returns the official
 * verification page, so the Reader can offer a button. **Nothing is sent
 * automatically**: the button only opens the page in the browser, and the code
 * is shown so the user can type it there themselves.
 *
 * Detection is deliberately strict to avoid a false-positive button: it needs
 * a verification phrase *and* either a turkiye.gov.tr link or a code that mixes
 * letters and digits.
 */

/** The official e-Devlet document verification page. */
export const EDEVLET_VERIFICATION_URL = "https://www.turkiye.gov.tr/belge-dogrulama";

export interface EdevletVerification {
  /** The verification/barcode code, when the document carries one. */
  code: string;
  /** The page to open: the document's own turkiye.gov.tr link, or the portal. */
  url: string;
}

/** Phrases that only appear in an e-Devlet verification block. */
const PHRASES = [
  "belge doğrulama",
  "belge dogrulama",
  "doğrulama kodu",
  "dogrulama kodu",
  "barkodlu belge",
  "barkod numarası",
  "barkod numarasi",
  "evrak doğrulama",
  "evrak dogrulama",
];

/** A code that mixes letters and digits (a bare number is too weak on its own). */
const CODE = /(?=[A-Z0-9-]{6,24}\b)(?=[A-Z0-9-]*[A-Z])(?=[A-Z0-9-]*[0-9])[A-Z0-9-]{6,24}/g;

/** Any absolute web URL in the document, so each candidate can be parsed. */
const WEB_URL = /https?:\/\/[^\s"<>()]+/gi;

/**
 * The document's own turkiye.gov.tr link, if any. The **host** must be exactly
 * `turkiye.gov.tr` or a subdomain of it, and the scheme must be https: a URL
 * whose path merely contains the name (`https://evil.example/x?u=turkiye.gov.tr`)
 * is not the official portal, and plain http is refused.
 */
function officialTurkiyeUrl(text: string): string | null {
  for (const candidate of text.match(WEB_URL) ?? []) {
    let parsed: URL;
    try {
      parsed = new URL(candidate);
    } catch {
      continue;
    }
    if (parsed.protocol !== "https:") continue;
    const host = parsed.hostname.toLowerCase();
    if (host === "turkiye.gov.tr" || host.endsWith(".turkiye.gov.tr")) {
      return parsed.toString();
    }
  }
  return null;
}

function nearby(text: string, index: number, before: number, after: number): string {
  return text.slice(Math.max(0, index - before), Math.min(text.length, index + after));
}

/**
 * Returns the verification details when `text` contains an e-Devlet
 * verification block, otherwise `null`.
 */
export function detectEdevletVerification(text: string): EdevletVerification | null {
  if (!text) return null;
  const normalized = text.replace(/\s+/g, " ");
  const lower = normalized.toLocaleLowerCase("tr");

  let phraseIndex = -1;
  for (const phrase of PHRASES) {
    const index = lower.indexOf(phrase);
    if (index >= 0 && (phraseIndex === -1 || index < phraseIndex)) {
      phraseIndex = index;
    }
  }
  if (phraseIndex === -1) return null;

  const url = officialTurkiyeUrl(normalized);
  const window = nearby(normalized, phraseIndex, 80, 220).toUpperCase();
  const codeMatch = window.match(CODE);
  const code = codeMatch ? codeMatch[0] : "";

  // Require a real signal: either the document's own official link or a mixed
  // code next to the phrase.
  if (!url && !code) return null;

  return { code, url: url ?? EDEVLET_VERIFICATION_URL };
}
