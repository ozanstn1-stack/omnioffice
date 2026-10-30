/**
 * Person card import/export (C6/2): CSV and vCard 3.0, the two formats other
 * address books exchange. Kept pure so the parsing rules are testable without
 * the dialog; person data never leaves the device unless the user copies it.
 */
import type { PersonCardInput } from "./person-cards";
import type { TemplateField } from "./template-fields";

/** The template fields a card can fill, in the same names the templates use. */
const FIELD_MAP: Record<string, (card: PersonCardInput) => string> = {
  ad_soyad: (card) => card.name ?? "",
  adres: (card) => card.address ?? "",
  telefon: (card) => card.phone ?? "",
  eposta: (card) => card.email ?? "",
  vkn: (card) => card.taxId ?? "",
  tckn: (card) => card.taxId ?? "",
  kurum: (card) => card.name ?? "",
};

/** The template values a card can fill; fields the card has no answer for are skipped. */
export function cardTemplateValues(card: PersonCardInput, fields: TemplateField[]): Record<string, string> {
  const values: Record<string, string> = {};
  for (const field of fields) {
    const reader = FIELD_MAP[field.name];
    if (!reader) continue;
    const value = reader(card).trim();
    if (value) values[field.name] = value;
  }
  return values;
}

function csvCell(value: string): string {
  const text = value ?? "";
  return /[",\n\r]/.test(text) ? `"${text.replaceAll('"', '""')}"` : text;
}

const CSV_HEADER = ["name", "taxId", "address", "phone", "email", "note"] as const;

/** All cards as one CSV table (the header is the field names above). */
export function cardsToCsv(cards: PersonCardInput[]): string {
  const lines = [CSV_HEADER.join(",")];
  for (const card of cards) {
    lines.push(CSV_HEADER.map((key) => csvCell(String((card as Record<string, unknown>)[key] ?? ""))).join(","));
  }
  return `${lines.join("\n")}\n`;
}

/** Splits one CSV line into cells, honouring quoted cells with commas and newlines. */
function parseCsvLine(line: string): string[] {
  const cells: string[] = [];
  let current = "";
  let quoted = false;
  for (let index = 0; index < line.length; index += 1) {
    const ch = line[index];
    if (quoted) {
      if (ch === '"' && line[index + 1] === '"') {
        current += '"';
        index += 1;
      } else if (ch === '"') {
        quoted = false;
      } else {
        current += ch;
      }
    } else if (ch === '"') {
      quoted = true;
    } else if (ch === ",") {
      cells.push(current);
      current = "";
    } else {
      current += ch;
    }
  }
  cells.push(current);
  return cells;
}

/** CSV rows keyed by the header; unknown headers are ignored, missing ones stay empty. */
export function parseCardsCsv(text: string): PersonCardInput[] {
  const lines = text
    .replaceAll("\r\n", "\n")
    .split("\n")
    .filter((line) => line.trim() !== "");
  if (lines.length < 2) return [];
  const header = parseCsvLine(lines[0]).map((cell) => cell.trim().toLowerCase());
  const cards: PersonCardInput[] = [];
  for (const line of lines.slice(1)) {
    const cells = parseCsvLine(line);
    const row: Record<string, string> = {};
    header.forEach((key, index) => {
      row[key] = (cells[index] ?? "").trim();
    });
    // Accept the English header and the Turkish export aliases.
    const pick = (...keys: string[]): string => keys.map((key) => row[key]).find((value) => value) ?? "";
    const card: PersonCardInput = {
      id: "",
      name: pick("name", "ad", "ad_soyad", "ad soyad"),
      taxId: pick("taxid", "tax_id", "vkn", "tckn", "vergi_no"),
      address: pick("address", "adres"),
      phone: pick("phone", "telefon", "tel"),
      email: pick("email", "eposta", "e-posta", "e_posta"),
      note: pick("note", "not"),
      updatedAt: "",
    };
    if (card.name.trim()) cards.push(card);
  }
  return cards;
}

/** One card as a vCard 3.0 entry (the format Android and most address books read). */
export function cardToVcard(card: PersonCardInput): string {
  const lines = [
    "BEGIN:VCARD",
    "VERSION:3.0",
    `FN:${card.name ?? ""}`,
    `N:${card.name ?? ""};;;;`,
    card.phone ? `TEL;TYPE=CELL:${card.phone}` : "",
    card.email ? `EMAIL;TYPE=INTERNET:${card.email}` : "",
    card.address ? `ADR;TYPE=HOME:;;${card.address.replaceAll("\n", " ")};;;;` : "",
    card.taxId ? `X-OMNIOFFICE-TAXID:${card.taxId}` : "",
    card.note ? `NOTE:${card.note.replaceAll("\n", " ")}` : "",
    "END:VCARD",
  ].filter(Boolean);
  return `${lines.join("\n")}\n`;
}

/** Every card as one multi-entry vCard file. */
export function cardsToVcards(cards: PersonCardInput[]): string {
  return cards.map((card) => cardToVcard(card)).join("");
}

/** Reads the fields we know from a vCard text; malformed cards are skipped. */
export function parseVcards(text: string): PersonCardInput[] {
  const cards: PersonCardInput[] = [];
  for (const block of text.split(/END:VCARD/i)) {
    if (!/BEGIN:VCARD/i.test(block)) continue;
    const lines = block
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter(Boolean);
    // A property line is `NAME;PARAM=...:value` (or `NAME:value`).
    const valueOf = (property: string): string => {
      const line = lines.find((entry) => new RegExp(`^${property}[;:]`, "i").test(entry));
      if (!line) return "";
      const colon = line.indexOf(":");
      return colon === -1 ? "" : line.slice(colon + 1).trim();
    };
    const name = valueOf("FN") || valueOf("N").split(";")[0] || "";
    if (!name.trim()) continue;
    const address = valueOf("ADR").split(";").filter(Boolean).join(" ").trim();
    cards.push({
      id: "",
      name: name.trim(),
      taxId: valueOf("X-OMNIOFFICE-TAXID"),
      address,
      phone: valueOf("TEL"),
      email: valueOf("EMAIL"),
      note: valueOf("NOTE"),
      updatedAt: "",
    });
  }
  return cards;
}
/**
 * The importer recognises a vCard paste by its first line and falls back to
 * CSV, so the dialog offers one box instead of a format choice.
 */
export function parseCardsText(text: string): PersonCardInput[] {
  const trimmed = text.trimStart();
  return /^BEGIN:VCARD/i.test(trimmed) ? parseVcards(trimmed) : parseCardsCsv(trimmed);
}
