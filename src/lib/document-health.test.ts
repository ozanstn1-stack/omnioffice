import { describe, expect, it } from "vitest";
import { analyzeDocumentHealth, fixMetadataLeaks, isValidTckn, type DocumentScanInput } from "./document-health";

describe("Document Health & KVKK Engine (v5.4.0)", () => {
  it("validates TCKN algorithm accurately", () => {
    // Known valid TCKN algorithm example: 10000000146
    expect(isValidTckn("10000000146")).toBe(true);
    // Invalid checks
    expect(isValidTckn("10000000147")).toBe(false);
    expect(isValidTckn("01234567890")).toBe(false); // starts with 0
    expect(isValidTckn("12345")).toBe(false); // too short
  });

  it("detects personal data (KVKK) risks in text content", () => {
    const input: DocumentScanInput = {
      text: "Sayın yetkili, TCKN: 10000000146, tel: 0532 123 45 67, mail: info@example.com, IBAN: TR33 0006 1005 1978 6457 8412 01",
      hasSignatures: false,
      hasTitle: true,
      hasLanguage: true,
    };

    const report = analyzeDocumentHealth(input);
    expect(report.totalIssues).toBeGreaterThanOrEqual(4);
    expect(report.criticalCount).toBeGreaterThanOrEqual(2); // TCKN + IBAN

    const tcknIssue = report.issues.find((i) => i.id === "kvkk-tckn");
    expect(tcknIssue).toBeDefined();
    expect(tcknIssue?.severity).toBe("critical");
    expect(tcknIssue?.fixable).toBe(true);

    const ibanIssue = report.issues.find((i) => i.id === "kvkk-iban");
    expect(ibanIssue).toBeDefined();
    expect(ibanIssue?.severity).toBe("critical");

    const phoneIssue = report.issues.find((i) => i.id === "kvkk-phone");
    expect(phoneIssue).toBeDefined();
    expect(phoneIssue?.severity).toBe("warning");

    const emailIssue = report.issues.find((i) => i.id === "kvkk-email");
    expect(emailIssue).toBeDefined();
    expect(emailIssue?.severity).toBe("warning");

    expect(report.score).toBeLessThan(50);
  });

  it("detects hidden metadata leaks and applies 1-click fix", () => {
    const dirtyMeta = {
      title: "Resmi Yazı",
      author: "Mehmet Yılmaz",
      creator: "Microsoft Word 2019",
      producer: "macOS Quartz PDFContext",
      keywords: "gizli, bütçe",
    };

    const input: DocumentScanInput = {
      text: "Normal temiz bir metin içeriği.",
      metadata: dirtyMeta,
      hasSignatures: true,
      hasTitle: true,
      hasLanguage: true,
    };

    const report = analyzeDocumentHealth(input);
    const metaIssue = report.issues.find((i) => i.id === "metadata-leaks");
    expect(metaIssue).toBeDefined();
    expect(metaIssue?.fixable).toBe(true);

    // Apply 1-click fix
    const cleaned = fixMetadataLeaks(dirtyMeta);
    expect(cleaned.author).toBeUndefined();
    expect(cleaned.creator).toBeUndefined();
    expect(cleaned.producer).toBeUndefined();
    expect(cleaned.keywords).toBeUndefined();
    expect(cleaned.title).toBe("Resmi Yazı"); // kept
  });

  it("produces high health score for clean, signed, and accessible document", () => {
    const cleanInput: DocumentScanInput = {
      text: "Bu belge herhangi bir gizli veri veya kişisel bilgi içermez.",
      metadata: {},
      hasSignatures: true,
      hasTitle: true,
      hasLanguage: true,
      fontIssuesCount: 0,
    };

    const report = analyzeDocumentHealth(cleanInput);
    expect(report.score).toBe(100);
    expect(report.criticalCount).toBe(0);
    expect(report.warningCount).toBe(0);
    expect(report.totalIssues).toBe(0);
  });
});
