import { describe, expect, it } from "vitest";
import { generateKvkkText, type KvkkTemplateField } from "./kvkk-templates";

describe("KVKK Compliance Templates (v5.4.0 - Issue #254)", () => {
  const fields: KvkkTemplateField = {
    companyName: "Acme Teknoloji A.Ş.",
    contactEmail: "kvkk@acme.com",
    address: "Büyükdere Cad. No: 123 Levent / İstanbul",
  };

  it("generates Employee Privacy Notice with legally accurate KVKK references", () => {
    const text = generateKvkkText("employee", fields);
    expect(text).toContain("ÇALIŞAN AYDINLATMA METNİ");
    expect(text).toContain("Acme Teknoloji A.Ş.");
    expect(text).toContain("6698 sayılı Kişisel Verilerin Korunması Kanunu");
    expect(text).toContain("kvkk@acme.com");
    expect(text).toContain("İlgili Kişi Hakları (Madde 11)");
  });

  it("generates Customer Privacy Notice", () => {
    const text = generateKvkkText("customer", fields);
    expect(text).toContain("MÜŞTERİ VE ÇEVRİMİÇİ ZİYARETÇİ AYDINLATMA METNİ");
    expect(text).toContain("Acme Teknoloji A.Ş.");
    expect(text).toContain("sözleşmenin kurulması ve ifası");
  });

  it("generates Visitor CCTV Notice", () => {
    const text = generateKvkkText("visitor", fields);
    expect(text).toContain("ZİYARETÇİ VE FİZİKİ GÜVENLİK (KAMERA) AYDINLATMA METNİ");
    expect(text).toContain("güvenlik kamerası ile görüntü kaydı");
    expect(text).toContain("30 gün boyunca güvenli şekilde saklanarak");
  });

  it("generates Explicit Consent Form", () => {
    const text = generateKvkkText("consent", fields);
    expect(text).toContain("AÇIK RIZA BEYANI");
    expect(text).toContain("[ ] AÇIK RIZA VERİYORUM");
  });
});
