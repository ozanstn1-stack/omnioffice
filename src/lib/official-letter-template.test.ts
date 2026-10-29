import { beforeEach, describe, expect, it } from "vitest";
import { checkOfficialLetter } from "./official-letter";
import {
  buildOfficialLetter,
  commitSequence,
  formatLetterNumber,
  formatSignerName,
  nextSequence,
  sequenceKey,
  type OfficialLetterFields,
} from "./official-letter-template";
import type { Block, TextDocument } from "./office-types";

const fields = (): OfficialLetterFields => ({
  institution: ["İstanbul Valiliği", "İl Millî Eğitim Müdürlüğü"],
  electronic: true,
  detsis: "12345678",
  filePlanCode: "010.01",
  sequence: 42,
  date: "29.10.2026",
  subject: "Eğitim semineri",
  addressee: "Kadıköy Kaymakamlığına",
  addresseePlace: "İlçe Millî Eğitim Müdürlüğü",
  references: ["12.10.2026 tarihli ve E-87654321-010.01-11 sayılı yazınız.", "Bakanlık genelgesi."],
  body: "İlgi (a) yazınızla bildirilen eğitim semineri 5 Kasım 2026 tarihinde il merkezinde düzenlenecektir. Seminere katılacak öğretmenlerin listesinin gönderilmesini rica ederim.\n\nKatılım belgeleri seminer sonunda verilecektir.",
  closing: "Bilgilerinizi ve gereğini rica ederim.",
  signerName: "ayşe nur yılmaz",
  signerTitle: "Müdür",
  eSigned: true,
  attachments: ["Seminer programı (2 sayfa)", "Katılımcı formu"],
  distributionAction: ["Kadıköy Kaymakamlığına"],
  distributionInfo: ["Üsküdar Kaymakamlığına"],
  contact: {
    address: "Cağaloğlu Mah. Türkocağı Cad. No:1 Fatih/İstanbul",
    phone: "0212 000 00 00",
    email: "bilgi@example.gov.tr",
    kep: "istanbulmem@hs01.kep.tr",
    web: "istanbul.meb.gov.tr",
    contactPerson: "Mehmet Demir, Şef",
  },
});

const text = (block: Block) => (block.type === "paragraph" ? block.runs.map((run) => run.text).join("") : "");
const paragraphs = (doc: TextDocument) => doc.blocks.filter((block) => block.type === "paragraph");
const find = (doc: TextDocument, pattern: RegExp) => paragraphs(doc).find((block) => pattern.test(text(block)));

describe("official letter builder (#341 C5)", () => {
  it("builds a letter the regulation checker finds nothing wrong with", () => {
    const violations = checkOfficialLetter(buildOfficialLetter(fields()));
    expect(violations).toEqual([]);
  });

  it("lays the header, number, subject and addressee out as the regulation does", () => {
    const doc = buildOfficialLetter(fields());
    const [first, second, third] = paragraphs(doc);
    expect(text(first)).toBe("T.C.");
    expect(text(second)).toBe("İSTANBUL VALİLİĞİ");
    expect(text(third)).toBe("İl Millî Eğitim Müdürlüğü");
    for (const block of [first, second, third]) expect(block.type === "paragraph" && block.props.align).toBe("center");

    const number = find(doc, /^Sayı/)!;
    expect(text(number)).toBe("Sayı\t: E-12345678-010.01-42\t29.10.2026");
    expect(number.type === "paragraph" && number.props.tabs?.length).toBe(2);
    expect(text(find(doc, /^Konu/)!)).toBe("Konu\t: Eğitim semineri");

    const addressee = find(doc, /KAYMAKAMLIĞINA$/)!;
    expect(text(addressee)).toBe("KADIKÖY KAYMAKAMLIĞINA");
    expect(addressee.type === "paragraph" && addressee.props.align).toBe("center");
    expect(text(find(doc, /^\(İlçe/)!)).toBe("(İlçe Millî Eğitim Müdürlüğü)");
  });

  it("lists the references, justifies the body and closes with the signature block", () => {
    const doc = buildOfficialLetter(fields());
    expect(text(find(doc, /^İlgi/)!)).toBe("İlgi\t: a) 12.10.2026 tarihli ve E-87654321-010.01-11 sayılı yazınız.");
    expect(find(doc, /^\tb\) Bakanlık genelgesi\.$/)).toBeTruthy();

    const body = find(doc, /^İlgi \(a\) yazınızla/)!;
    expect(body.type === "paragraph" && body.props).toMatchObject({ align: "justify", firstLinePt: 35.4 });
    expect(find(doc, /^Katılım belgeleri/)).toBeTruthy();
    expect(find(doc, /^Bilgilerinizi ve gereğini rica ederim\.$/)).toBeTruthy();

    const signer = find(doc, /^Ayşe Nur YILMAZ$/)!;
    expect(signer.type === "paragraph" && signer.props.indentLeftPt).toBeGreaterThan(200);
    expect(find(doc, /^Müdür$/)).toBeTruthy();
    expect(find(doc, /^e-imzalıdır$/)).toBeTruthy();
  });

  it("adds attachments and distribution, and puts the contact block in the footer", () => {
    const doc = buildOfficialLetter(fields());
    expect(find(doc, /^Ekler:$/)).toBeTruthy();
    expect(find(doc, /^1- Seminer programı \(2 sayfa\)$/)).toBeTruthy();
    expect(find(doc, /^2- Katılımcı formu$/)).toBeTruthy();
    expect(find(doc, /^Dağıtım:$/)).toBeTruthy();
    expect(find(doc, /^Gereği:\tBilgi:$/)).toBeTruthy();
    expect(find(doc, /^Kadıköy Kaymakamlığına\tÜsküdar Kaymakamlığına$/)).toBeTruthy();

    expect(doc.page.footerDistancePt).toBe(20);
    const footer = doc.footer.map(text).join("\n");
    expect(footer).toContain("Cağaloğlu Mah.");
    expect(footer).toContain("KEP Adresi: istanbulmem@hs01.kep.tr");
    expect(footer).toContain("Bilgi için: Mehmet Demir, Şef");
  });

  it("leaves out the optional parts that are empty", () => {
    const doc = buildOfficialLetter({
      ...fields(),
      references: [],
      attachments: [" "],
      distributionAction: [],
      distributionInfo: [],
      eSigned: false,
      addresseePlace: "",
    });
    expect(find(doc, /^İlgi\t/)).toBeUndefined();
    expect(find(doc, /^Ek/)).toBeUndefined();
    expect(find(doc, /^Dağıtım/)).toBeUndefined();
    expect(find(doc, /^e-imzalıdır$/)).toBeUndefined();
    expect(find(doc, /^\(/)).toBeUndefined();
    expect(checkOfficialLetter(doc)).toEqual([]);
  });

  it("uses one attachment line without numbering", () => {
    const doc = buildOfficialLetter({ ...fields(), attachments: ["Seminer programı"] });
    expect(find(doc, /^Ek: Seminer programı$/)).toBeTruthy();
  });
});

describe("letter number and signer", () => {
  it("formats the number with the electronic prefix when the letter is electronic", () => {
    expect(formatLetterNumber({ electronic: true, detsis: " 12345678 ", filePlanCode: "010.01", sequence: 7 })).toBe(
      "E-12345678-010.01-7",
    );
    expect(formatLetterNumber({ electronic: false, detsis: "12345678", filePlanCode: "010.01", sequence: "7" })).toBe(
      "12345678-010.01-7",
    );
  });

  it("writes given names capitalised and the surname in Turkish capitals", () => {
    expect(formatSignerName("ayşe nur yılmaz")).toBe("Ayşe Nur YILMAZ");
    expect(formatSignerName("  ışıl   çağrı ")).toBe("Işıl ÇAĞRI");
    expect(formatSignerName("İBRAHİM")).toBe("İBRAHİM");
  });
});

describe("letter sequence", () => {
  beforeEach(() => localStorage.clear());

  it("counts per year, institution code and file plan code", () => {
    const key = sequenceKey(2026, "12345678", "010.01");
    expect(nextSequence(key)).toBe(1);
    commitSequence(key, 1);
    expect(nextSequence(key)).toBe(2);
    commitSequence(key, 41);
    expect(nextSequence(key)).toBe(42);
    expect(nextSequence(sequenceKey(2027, "12345678", "010.01"))).toBe(1);
    expect(nextSequence(sequenceKey(2026, "12345678", "302.01"))).toBe(1);
  });

  it("never moves the counter back and survives broken storage", () => {
    const key = sequenceKey(2026, "1", "2");
    commitSequence(key, 10);
    commitSequence(key, 3);
    expect(nextSequence(key)).toBe(11);
    localStorage.setItem("omni.officialLetter.sequence", "{not json");
    expect(nextSequence(key)).toBe(1);
  });
});
