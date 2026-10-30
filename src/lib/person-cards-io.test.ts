import { describe, expect, it } from "vitest";

import type { PersonCardInput } from "./person-cards";
import {
  cardTemplateValues,
  cardToVcard,
  cardsToCsv,
  cardsToVcards,
  parseCardsCsv,
  parseCardsText,
  parseVcards,
} from "./person-cards-io";

const ayse: PersonCardInput = {
  id: "",
  name: "Ayşe Yılmaz",
  taxId: "12345678901",
  address: "Atatürk Cad. 1, İstanbul",
  phone: "+90 555 000 00 00",
  email: "ayse@example.org",
  note: "Müşteri",
  updatedAt: "",
};

const mehmet: PersonCardInput = {
  id: "",
  name: "Mehmet, Kaya",
  taxId: "",
  address: "",
  phone: "",
  email: "mehmet@example.org",
  note: 'Not "alıntılı"',
  updatedAt: "",
};

describe("CSV", () => {
  it("round-trips cards, including commas and quotes", () => {
    const csv = cardsToCsv([ayse, mehmet]);
    const parsed = parseCardsCsv(csv);
    expect(parsed).toHaveLength(2);
    expect(parsed[0]).toMatchObject({ name: "Ayşe Yılmaz", taxId: "12345678901", email: "ayse@example.org" });
    expect(parsed[1]).toMatchObject({ name: "Mehmet, Kaya", note: 'Not "alıntılı"' });
  });

  it("skips rows without a name and accepts Turkish header aliases", () => {
    const csv = ["ad,soyad,vkn,telefon", ",,,"].join("\n");
    expect(parseCardsCsv(csv)).toEqual([]);
    const aliased = ["name,vkn,telefon", "Zeynep,1234567890,05550000000"].join("\n");
    expect(parseCardsCsv(aliased)[0]).toMatchObject({ name: "Zeynep", taxId: "1234567890", phone: "05550000000" });
  });
});

describe("vCard", () => {
  it("writes the fields an address book reads", () => {
    const vcard = cardToVcard(ayse);
    expect(vcard).toContain("BEGIN:VCARD");
    expect(vcard).toContain("FN:Ayşe Yılmaz");
    expect(vcard).toContain("EMAIL;TYPE=INTERNET:ayse@example.org");
    expect(vcard).toContain("X-OMNIOFFICE-TAXID:12345678901");
    expect(vcard).toContain("END:VCARD");
  });

  it("round-trips a multi-entry file", () => {
    const text = cardsToVcards([ayse, mehmet]);
    const parsed = parseVcards(text);
    expect(parsed).toHaveLength(2);
    expect(parsed[0]).toMatchObject({ name: "Ayşe Yılmaz", email: "ayse@example.org", taxId: "12345678901" });
    expect(parsed[1].name).toBe("Mehmet, Kaya");
  });

  it("detects the format from the first line", () => {
    expect(parseCardsText(cardsToVcards([ayse]))).toHaveLength(1);
    expect(parseCardsText(cardsToCsv([ayse]))).toHaveLength(1);
  });
});

describe("cardTemplateValues", () => {
  it("fills the template fields the card knows", () => {
    const values = cardTemplateValues(ayse, [
      { name: "ad_soyad", count: 1 },
      { name: "adres", count: 1 },
      { name: "eposta", count: 1 },
      { name: "vkn", count: 1 },
      { name: "tarih", count: 1 },
    ]);
    expect(values).toEqual({
      ad_soyad: "Ayşe Yılmaz",
      adres: "Atatürk Cad. 1, İstanbul",
      eposta: "ayse@example.org",
      vkn: "12345678901",
    });
  });

  it("does not add empty values or fields the card has no answer for", () => {
    expect(
      cardTemplateValues({ ...ayse, email: "" }, [
        { name: "eposta", count: 1 },
        { name: "konu", count: 1 },
      ]),
    ).toEqual({});
  });
});
