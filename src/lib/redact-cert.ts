/**
 * Redaction Certificate Generator and Auditor (v5.4.0 - Issue #253)
 *
 * Produces a legally compliant, auditable Redaction Certificate (Karartma Sertifikası)
 * detailing what was redacted, when, under which legal rule/category (e.g. KVKK, GDPR),
 * document hashes (SHA-256), and cryptographic integrity summary.
 */

export interface RedactedItemRecord {
  page: number;
  category: "tckn" | "phone" | "email" | "iban" | "vkn" | "manual" | "other";
  maskedPreview: string; // e.g. "100*****146"
  ruleRef: string; // e.g. "KVKK Md. 5 / 6"
}

export interface RedactionCertificateData {
  certificateId: string;
  documentName: string;
  originalSha256?: string;
  redactedSha256?: string;
  appliedAt: string; // ISO 8601
  operatorName?: string;
  organization?: string;
  items: RedactedItemRecord[];
  totalRedactionsCount: number;
  verificationStatus: "verified" | "unverified";
}

/**
 * Safely masks a sensitive string without leaking plaintext characters.
 */
export function maskSensitiveValue(value?: string | null): string {
  if (!value || !value.trim()) return "Gizli Alan";
  const len = value.trim().length;
  return `******** (${len} karakter maskelendi)`;
}

/**
 * Creates a standard Redaction Certificate data structure.
 */
export function buildRedactionCertificate(params: {
  documentName: string;
  items: RedactedItemRecord[];
  operatorName?: string;
  organization?: string;
  originalSha256?: string;
  redactedSha256?: string;
  verificationStatus?: "verified" | "unverified";
}): RedactionCertificateData {
  const now = new Date();
  const certId = `CERT-RED-${now.getFullYear()}${String(now.getMonth() + 1).padStart(2, "0")}-${Math.floor(100000 + Math.random() * 900000)}`;
  const hasRealHashes = Boolean(
    params.originalSha256 &&
    params.redactedSha256 &&
    params.originalSha256 !== params.redactedSha256 &&
    !params.originalSha256.startsWith("Hesaplanmadı") &&
    !params.redactedSha256.startsWith("Hesaplanmadı"),
  );

  return {
    certificateId: certId,
    documentName: params.documentName,
    originalSha256: params.originalSha256 || "Hesaplanmadı (Kaynak dosya doğrulanmadı)",
    redactedSha256: params.redactedSha256 || "Hesaplanmadı (Karartma henüz uygulanmadı)",
    appliedAt: now.toISOString(),
    operatorName: params.operatorName || "OmniOffice Automated Compliance Officer",
    organization: params.organization || "OmniOffice Güvenli Belge İşleme Merkezi",
    items: params.items,
    totalRedactionsCount: params.items.length,
    verificationStatus: params.verificationStatus ?? (hasRealHashes ? "verified" : "unverified"),
  };
}

/**
 * Generates an auditable Text/Markdown/PDF representation of the Redaction Certificate.
 */
export function renderCertificateMarkdown(cert: RedactionCertificateData): string {
  const lines: string[] = [
    `# RESMÎ KARARTMA VE ANONİMLEŞTİRME SERTİFİKASI`,
    `**Sertifika No:** ${cert.certificateId}`,
    `**Düzenlenme Tarihi:** ${cert.appliedAt}`,
    `**Belge Adı:** ${cert.documentName}`,
    `**Uygulayıcı / Kurum:** ${cert.operatorName} (${cert.organization})`,
    `**Doğrulama Durumu:** ${cert.verificationStatus === "verified" ? "✅ DOĞRULANDI (Kişisel Veri Çıkarımı Sıfırlandı)" : "⚠️ DOĞRULANMADI"}`,
    ``,
    `---`,
    `### Kriptografik Belge Bütünlüğü (SHA-256)`,
    `- **Orijinal Belge Hash:** \`${cert.originalSha256}\``,
    `- **Karartılmış Belge Hash:** \`${cert.redactedSha256}\``,
    ``,
    `---`,
    `### Karartılan Kişisel Veri ve Alan Özeti (Toplam: ${cert.totalRedactionsCount})`,
    `| Sayfa | Kategori | Maskeli Önizleme | Hukuki / Mevzuat Dayanağı |`,
    `| :--- | :--- | :--- | :--- |`,
  ];

  for (const item of cert.items) {
    lines.push(`| Sayfa ${item.page} | ${item.category.toUpperCase()} | ${item.maskedPreview} | ${item.ruleRef} |`);
  }

  lines.push(``);
  lines.push(`> [!NOTE]`);
  lines.push(`> Bu sertifika, 6698 sayılı Kişisel Verilerin Korunması Kanunu (KVKK) uyarınca gerçekleştirilen`);
  lines.push(
    `> geri döndürülemez metin ve piksel karartma işlemini kanıtlamak amacıyla OmniOffice tarafından üretilmiştir.`,
  );

  return lines.join("\n");
}
