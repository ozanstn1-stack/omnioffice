import { describe, expect, it } from "vitest";
import { detectEdevletVerification, EDEVLET_VERIFICATION_URL } from "./edevlet";

describe("detectEdevletVerification", () => {
  it("finds the verification block with a code and the document's own link", () => {
    const text =
      "Bu belge 5070 sayılı Elektronik İmza Kanunu kapsamında e-Devlet Kapısı üzerinden üretilmiştir. " +
      "Belge Doğrulama Kodu: 4F2A9C13 " +
      "https://www.turkiye.gov.tr/belge-dogrulama?barkod=4F2A9C13";
    const found = detectEdevletVerification(text);
    expect(found).not.toBeNull();
    expect(found?.code).toBe("4F2A9C13");
    expect(found?.url).toContain("turkiye.gov.tr/belge-dogrulama");
  });

  it("uses the official page when the document has a code but no link", () => {
    const found = detectEdevletVerification("Doğrulama Kodu: ABC12345 ile belgeyi kontrol edebilirsiniz.");
    expect(found?.code).toBe("ABC12345");
    expect(found?.url).toBe(EDEVLET_VERIFICATION_URL);
  });

  it("accepts a barkod number phrase next to a mixed code", () => {
    const found = detectEdevletVerification("Barkod Numarası: 1234567890 Doğrulama Kodu: E9K7M2X1");
    expect(found?.code).toBe("E9K7M2X1");
  });

  it("does not fire on ordinary documents", () => {
    expect(detectEdevletVerification("")).toBeNull();
    expect(detectEdevletVerification("Fatura No: GIB2026000000001 Toplam: 1.200,00 TL")).toBeNull();
    expect(detectEdevletVerification("Bu sözleşme iki nüsha olarak imzalanmıştır.")).toBeNull();
  });

  it("does not fire on the phrase alone without a code or link", () => {
    expect(detectEdevletVerification("Belge doğrulama hakkında bilgi için kuruma başvurun.")).toBeNull();
  });

  it("does not treat a plain number as a verification code", () => {
    expect(detectEdevletVerification("Barkod No: 1234567890")).toBeNull();
  });

  it("still fires when only a turkiye.gov.tr link is present next to the phrase", () => {
    const found = detectEdevletVerification("Belge doğrulama: https://www.turkiye.gov.tr/belge-dogrulama");
    expect(found?.url).toContain("turkiye.gov.tr");
  });

  it("refuses a URL whose host merely contains the name (phishing)", () => {
    const text = "Belge Doğrulama Kodu: AB12CD34 https://evil.example.com/x?u=turkiye.gov.tr";
    const found = detectEdevletVerification(text);
    expect(found?.url).toBe(EDEVLET_VERIFICATION_URL);
    expect(found?.url).not.toContain("evil.example.com");
  });

  it("refuses a plain http turkiye.gov.tr link", () => {
    const text = "Belge Doğrulama Kodu: AB12CD34 http://turkiye.gov.tr/belge-dogrulama";
    const found = detectEdevletVerification(text);
    expect(found?.url).toBe(EDEVLET_VERIFICATION_URL);
  });

  it("accepts an https subdomain of turkiye.gov.tr", () => {
    const text = "Belge Doğrulama Kodu: AB12CD34 https://dogrulama.turkiye.gov.tr/belge";
    expect(detectEdevletVerification(text)?.url).toContain("dogrulama.turkiye.gov.tr");
  });

  it("does not accept a lookalike domain that ends in turkiye.gov.tr.example", () => {
    const text = "Belge Doğrulama Kodu: AB12CD34 https://turkiye.gov.tr.example.com/x";
    expect(detectEdevletVerification(text)?.url).toBe(EDEVLET_VERIFICATION_URL);
  });
});
