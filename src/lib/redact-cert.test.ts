import { describe, expect, it } from "vitest";
import {
  buildRedactionCertificate,
  maskSensitiveValue,
  renderCertificateMarkdown,
  type RedactedItemRecord,
} from "./redact-cert";

describe("Redaction Certificate Engine (v5.4.0 - Issue #253)", () => {
  it("builds a structured redaction certificate with valid metadata", () => {
    const items: RedactedItemRecord[] = [
      {
        page: 1,
        category: "tckn",
        maskedPreview: "100*****146",
        ruleRef: "KVKK Md. 5 (TCKN Koruma)",
      },
      {
        page: 2,
        category: "iban",
        maskedPreview: "TR33 0006 **** **** **** 01",
        ruleRef: "KVKK Md. 6 (Mali Veri)",
      },
    ];

    const cert = buildRedactionCertificate({
      documentName: "gizli_sozlesme.pdf",
      items,
      operatorName: "Av. Ayşe Kaya",
      organization: "Kaya Hukuk Bürosu",
      verificationStatus: "verified",
    });

    expect(cert.certificateId).toMatch(/^CERT-RED-\d{6}-\d{6}$/);
    expect(cert.documentName).toBe("gizli_sozlesme.pdf");
    expect(cert.totalRedactionsCount).toBe(2);
    expect(cert.operatorName).toBe("Av. Ayşe Kaya");
    expect(cert.organization).toBe("Kaya Hukuk Bürosu");
    expect(cert.verificationStatus).toBe("verified");
  });

  it("renders an auditable markdown report with cryptographic hashes and item table", () => {
    const items: RedactedItemRecord[] = [
      {
        page: 1,
        category: "email",
        maskedPreview: "a***@example.com",
        ruleRef: "KVKK Md. 5",
      },
    ];

    const cert = buildRedactionCertificate({
      documentName: "personel_listesi.pdf",
      items,
      originalSha256: "aaa111bbb222",
      redactedSha256: "ccc333ddd444",
    });

    expect(cert.verificationStatus).toBe("verified");
    const markdown = renderCertificateMarkdown(cert);
    expect(markdown).toContain("RESMÎ KARARTMA VE ANONİMLEŞTİRME SERTİFİKASI");
    expect(markdown).toContain("personel_listesi.pdf");
    expect(markdown).toContain("aaa111bbb222");
    expect(markdown).toContain("ccc333ddd444");
    expect(markdown).toContain("EMAIL");
    expect(markdown).toContain("a***@example.com");
    expect(markdown).toContain("6698 sayılı Kişisel Verilerin Korunması Kanunu");
  });

  it("never invents empty-file SHA-256/MD5 hashes and masks sensitive values without leaking prefixes", () => {
    const cert = buildRedactionCertificate({
      documentName: "taslak.pdf",
      items: [],
    });
    expect(cert.verificationStatus).toBe("unverified");
    expect(cert.originalSha256).not.toContain("e3b0c442");
    expect(cert.redactedSha256).not.toContain("d41d8cd9");

    const masked = maskSensitiveValue("10000000146");
    expect(masked).not.toContain("100");
    expect(masked).toContain("11 karakter maskelendi");
  });
});
