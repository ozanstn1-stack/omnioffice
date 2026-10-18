import { describe, expect, it } from "vitest";
import { validateTCKN, createPetitionDocument } from "./petition";

describe("Petition Wizard (v5.3 - Issue #247)", () => {
  it("validates TCKN mod-11 correctly", () => {
    // Geçerli test TCKN'si örneği (10000000146 vb. mod-11 uyumlu)
    expect(validateTCKN("10000000146")).toBe(true);
    // 0 ile başlayamaz
    expect(validateTCKN("01234567890")).toBe(false);
    // 11 haneli olmalıdır
    expect(validateTCKN("123456789")).toBe(false);
    // Geçersiz sağlama toplamı
    expect(validateTCKN("11111111111")).toBe(false);
  });

  it("creates compliant TextDocument from petition data", () => {
    const doc = createPetitionDocument({
      recipientAuthority: "Çankaya Kaymakamlığına",
      subject: "İkametgah Düzeltme Talebi",
      bodyText: "Mahallemizde yapılan numarataj çalışması sonrası adres kaydım değişmiştir.",
      demands: "Adres kaydımın düzeltilmesini saygılarımla arz ederim.",
      attachments: "1. Nüfus Cüzdanı Fotokopisi\n2. Elektrik Faturası",
      fullName: "Ali Veli",
      tckn: "10000000146",
      address: "Atatürk Mah. No:1 Çankaya/Ankara",
      phone: "05551234567",
      date: "17.10.2026",
    });

    expect(doc.title).toBe("İkametgah Düzeltme Talebi");
    expect(doc.blocks.length).toBeGreaterThan(6);
    expect(doc.page.marginTopPt).toBeCloseTo(70.87, 1);
    const authorityBlock = doc.blocks[1];
    expect(authorityBlock.type === "paragraph" && authorityBlock.runs[0].text).toBe("ÇANKAYA KAYMAKAMLIĞINA");
    const signerBlock = doc.blocks[5];
    expect(signerBlock.type === "paragraph" && signerBlock.runs[0].text).toBe("ALİ VELİ");
  });
});
