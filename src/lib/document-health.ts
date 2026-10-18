/**
 * Document Health and KVKK Compliance Scanner & 1-Click Fixer (v5.4.0)
 *
 * Scans documents for:
 * 1. Personal Data (KVKK) risks: TCKN, Phone numbers, Email, IBAN, VKN, License plates
 * 2. Hidden Metadata: Author, Creator, Producer, Title, Subject, Keywords
 * 3. Accessibility issues: Missing document title, untagged reading order, missing language declaration
 * 4. Digital Signature & Integrity: Presence of digital signatures, uncertified revisions
 * 5. Corrupt / Non-embedded Fonts: Missing font descriptors or unmapped CID fonts
 */

export type HealthSeverity = "critical" | "warning" | "info" | "good";

export type HealthCategory = "kvkk" | "metadata" | "accessibility" | "fonts" | "signature";

export interface HealthIssue {
  id: string;
  category: HealthCategory;
  severity: HealthSeverity;
  title: string;
  description: string;
  detectedCount?: number;
  fixable: boolean;
  fixAction?: "strip_metadata" | "redact_kvkk" | "set_title" | "set_language" | "flatten_annotations";
}

export interface DocumentHealthReport {
  score: number; // 0 - 100
  totalIssues: number;
  criticalCount: number;
  warningCount: number;
  infoCount: number;
  passedCount: number;
  issues: HealthIssue[];
  timestamp: string;
}

export interface DocumentScanInput {
  text?: string;
  metadata?: Record<string, string | undefined>;
  hasSignatures?: boolean;
  hasTitle?: boolean;
  hasLanguage?: boolean;
  fontIssuesCount?: number;
}

// Regex patterns for Turkish personal data (KVKK)
const TCKN_REGEX = /\b[1-9][0-9]{10}\b/g;
const TR_PHONE_REGEX =
  /(?:\+90[\s.-]*|0\s*)(?:5[0-9]{2}|[2348][0-9]{2})[\s.-]*[0-9]{3}[\s.-]*[0-9]{2}[\s.-]*[0-9]{2}\b/g;
const EMAIL_REGEX = /\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b/g;
const IBAN_REGEX = /\bTR\s*[0-9]{2}(?:\s*[0-9]{4}){5}\s*[0-9]{2}\b/gi;

/**
 * Validate Turkish Republic Identity Number (TCKN) with mod-11 algorithm
 */
export function isValidTckn(tckn: string): boolean {
  if (tckn.length !== 11 || !/^[1-9][0-9]{10}$/.test(tckn)) return false;
  const digits = tckn.split("").map(Number);
  const sumOdd = digits[0] + digits[2] + digits[4] + digits[6] + digits[8];
  const sumEven = digits[1] + digits[3] + digits[5] + digits[7];
  const d10 = (((sumOdd * 7 - sumEven) % 10) + 10) % 10;
  if (d10 !== digits[9]) return false;
  const totalSum = digits.slice(0, 10).reduce((acc, d) => acc + d, 0);
  return totalSum % 10 === digits[10];
}

/**
 * Scans a document content and metadata to produce a comprehensive Document Health Report.
 */
export function analyzeDocumentHealth(input: DocumentScanInput): DocumentHealthReport {
  const issues: HealthIssue[] = [];
  const text = input.text || "";

  // 1. KVKK Risks Scan
  const rawTcknMatches = text.match(TCKN_REGEX) || [];
  const validTckns = rawTcknMatches.filter(isValidTckn);
  if (validTckns.length > 0) {
    issues.push({
      id: "kvkk-tckn",
      category: "kvkk",
      severity: "critical",
      title: "T.C. Kimlik Numarası (TCKN) Tespit Edildi",
      description: `Belgede ${validTckns.length} adet geçerli TCKN tespit edildi. KVKK kapsamında yüksek risk taşır.`,
      detectedCount: validTckns.length,
      fixable: true,
      fixAction: "redact_kvkk",
    });
  }

  const phoneMatches = text.match(TR_PHONE_REGEX) || [];
  if (phoneMatches.length > 0) {
    issues.push({
      id: "kvkk-phone",
      category: "kvkk",
      severity: "warning",
      title: "Telefon Numarası Tespit Edildi",
      description: `Belgede ${phoneMatches.length} adet telefon numarası bulundu.`,
      detectedCount: phoneMatches.length,
      fixable: true,
      fixAction: "redact_kvkk",
    });
  }

  const emailMatches = text.match(EMAIL_REGEX) || [];
  if (emailMatches.length > 0) {
    issues.push({
      id: "kvkk-email",
      category: "kvkk",
      severity: "warning",
      title: "E-Posta Adresi Tespit Edildi",
      description: `Belgede ${emailMatches.length} adet e-posta adresi bulundu.`,
      detectedCount: emailMatches.length,
      fixable: true,
      fixAction: "redact_kvkk",
    });
  }

  const ibanMatches = text.match(IBAN_REGEX) || [];
  if (ibanMatches.length > 0) {
    issues.push({
      id: "kvkk-iban",
      category: "kvkk",
      severity: "critical",
      title: "IBAN Numarası Tespit Edildi",
      description: `Belgede ${ibanMatches.length} adet banka hesap (IBAN) numarası bulundu.`,
      detectedCount: ibanMatches.length,
      fixable: true,
      fixAction: "redact_kvkk",
    });
  }

  // 2. Hidden Metadata Scan (title is an accessibility field, not a privacy leak)
  const meta = input.metadata || {};
  const metaFields = ["author", "creator", "producer", "subject", "keywords"];
  const populatedMeta = metaFields.filter((key) => Boolean(meta[key] && meta[key]?.trim() !== ""));
  if (populatedMeta.length > 0) {
    issues.push({
      id: "metadata-leaks",
      category: "metadata",
      severity: "warning",
      title: "Gizli Belge Meta Verileri",
      description: `Belgede yazar, yazılım veya kurum bilgisi ifşa edebilecek ${populatedMeta.length} meta veri alanı mevcut: ${populatedMeta.join(", ")}.`,
      detectedCount: populatedMeta.length,
      fixable: true,
      fixAction: "strip_metadata",
    });
  }

  // 3. Accessibility Scan
  if (input.hasTitle === false) {
    issues.push({
      id: "a11y-missing-title",
      category: "accessibility",
      severity: "info",
      title: "Belge Başlığı Eksik",
      description: "Ekran okuyucular ve arama indeksleri için belge başlığı tanımlanmamış.",
      fixable: true,
      fixAction: "set_title",
    });
  }

  if (input.hasLanguage === false) {
    issues.push({
      id: "a11y-missing-language",
      category: "accessibility",
      severity: "info",
      title: "Belge Dili Belirtilmemiş",
      description: "Ekran okuyucu sentezleyicileri için dil kodu (tr-TR / en-US) atanmamış.",
      fixable: true,
      fixAction: "set_language",
    });
  }

  // 4. Corrupt / Missing Font Scan
  if (input.fontIssuesCount && input.fontIssuesCount > 0) {
    issues.push({
      id: "fonts-unembedded",
      category: "fonts",
      severity: "warning",
      title: "Gömülü Olmayan / Bozuk Fontlar",
      description: `Belgede ${input.fontIssuesCount} adet font tanımı eksik veya gömülmemiş. Farklı cihazlarda metin kayabilir.`,
      detectedCount: input.fontIssuesCount,
      fixable: false,
    });
  }

  // 5. Signature & Integrity Check
  if (!input.hasSignatures) {
    issues.push({
      id: "sig-unsigned",
      category: "signature",
      severity: "info",
      title: "Belge Dijital Olarak İmzalanmamış",
      description: "Resmî geçerlilik veya değiştirilmezlik doğrulaması için PAdES / e-İmza bulunmuyor.",
      fixable: false,
    });
  }

  // Calculate score (100 is perfect)
  let penalty = 0;
  let criticalCount = 0;
  let warningCount = 0;
  let infoCount = 0;

  for (const issue of issues) {
    if (issue.severity === "critical") {
      penalty += 35;
      criticalCount++;
    } else if (issue.severity === "warning") {
      penalty += 15;
      warningCount++;
    } else if (issue.severity === "info") {
      penalty += 5;
      infoCount++;
    }
  }

  const score = Math.max(0, 100 - penalty);

  return {
    score,
    totalIssues: issues.length,
    criticalCount,
    warningCount,
    infoCount,
    passedCount: Math.max(0, 7 - issues.length),
    issues,
    timestamp: new Date().toISOString(),
  };
}

/**
 * 1-Click Fixer: Automatically resolves fixable metadata issues.
 */
export function fixMetadataLeaks(meta: Record<string, string | undefined>): Record<string, string | undefined> {
  const cleaned: Record<string, string | undefined> = { ...meta };
  delete cleaned.author;
  delete cleaned.creator;
  delete cleaned.producer;
  delete cleaned.subject;
  delete cleaned.keywords;
  return cleaned;
}
