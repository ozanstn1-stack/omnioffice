/**
 * Builds an official letter that is compliant by construction (#341 C5), laid
 * out the way the Resmî Yazışma Yönetmeliği describes it: "T.C." and the
 * institution centred at the top, the number and the date on one line, the
 * subject, the addressee in capitals, the references, a justified body with a
 * 1.25 cm first-line indent, the signature block on the right, attachments,
 * distribution and the contact block in the page footer. Times New Roman 12
 * pt, single spacing and 2.5 cm margins throughout, so `checkOfficialLetter`
 * has nothing to report on the result.
 *
 * The number's running part is counted per year, institution (DETSİS) code
 * and file plan code in local storage; a number is only used up when the
 * letter is opened or saved (`commitSequence`).
 */
import {
  defaultParaProps,
  newTextDocument,
  type Block,
  type ParaProps,
  type TabStop,
  type TextDocument,
} from "./office-types";

export interface OfficialLetterContact {
  address: string;
  phone: string;
  email: string;
  kep: string;
  web: string;
  contactPerson: string;
}

export interface OfficialLetterFields {
  /** Lines under "T.C.": the institution, then its unit. */
  institution: string[];
  electronic: boolean;
  detsis: string;
  filePlanCode: string;
  sequence: number | string;
  /** GG.AA.YYYY. */
  date: string;
  subject: string;
  addressee: string;
  /** Written under the addressee in parentheses, when set. */
  addresseePlace: string;
  /** İlgi entries, lettered a), b), ... */
  references: string[];
  /** Paragraphs separated by blank lines. */
  body: string;
  closing: string;
  signerName: string;
  signerTitle: string;
  eSigned: boolean;
  attachments: string[];
  distributionAction: string[];
  distributionInfo: string[];
  contact: OfficialLetterContact;
}

/** 2.5 cm. */
const MARGIN_PT = 70.87;
/** 1.25 cm. */
const INDENT_PT = 35.4;
/** The footer's last line ends this far above the page edge. */
const FOOTER_DISTANCE_PT = 20;
/** Where the value after "Sayı", "Konu" and "İlgi" starts. */
const LABEL_TAB: TabStop = { posPt: 42.5, align: "left" };
/** Where the date starts on the number line. */
const DATE_TAB: TabStop = { posPt: 380, align: "left" };
/** The second distribution column. */
const COLUMN_TAB: TabStop = { posPt: 226, align: "left" };
/** The signature block's left edge. */
const SIGNATURE_INDENT_PT = 255;

const TR = "tr-TR";

export function formatLetterNumber(parts: {
  electronic: boolean;
  detsis: string;
  filePlanCode: string;
  sequence: number | string;
}): string {
  const body = [parts.detsis, parts.filePlanCode, String(parts.sequence)].map((part) => part.trim()).join("-");
  return parts.electronic ? `E-${body}` : body;
}

/** "ayşe nur yılmaz" -> "Ayşe Nur YILMAZ": given names capitalised, the surname in capitals. */
export function formatSignerName(name: string): string {
  const words = name.trim().split(/\s+/).filter(Boolean);
  if (words.length < 2) return words.join(" ");
  const surname = words.pop()!.toLocaleUpperCase(TR);
  const given = words.map((word) => word.charAt(0).toLocaleUpperCase(TR) + word.slice(1).toLocaleLowerCase(TR));
  return [...given, surname].join(" ");
}

const run = (text: string, sizePt = 12, bold = false) => ({
  text,
  bold,
  italic: false,
  underline: false,
  strike: false,
  color: null,
  highlight: null,
  font: "Times New Roman",
  sizePt,
  link: null,
  comment: null,
  superscript: false,
  subscript: false,
});

const paragraph = (
  text: string,
  props: Partial<ParaProps> = {},
  options: { bold?: boolean; sizePt?: number } = {},
): Block => ({
  type: "paragraph",
  props: { ...defaultParaProps(), lineSpacing: 1, spaceAfterPt: 0, ...props },
  runs: [run(text, options.sizePt ?? 12, options.bold ?? false)],
});

const filled = (items: string[]) => items.map((item) => item.trim()).filter(Boolean);

export function buildOfficialLetter(fields: OfficialLetterFields): TextDocument {
  const doc = newTextDocument(fields.subject.trim() || "Resmî yazı");
  doc.page.marginTopPt = MARGIN_PT;
  doc.page.marginBottomPt = MARGIN_PT;
  doc.page.marginLeftPt = MARGIN_PT;
  doc.page.marginRightPt = MARGIN_PT;
  // The contact block sits low in the bottom margin, clear of the body.
  doc.page.footerDistancePt = FOOTER_DISTANCE_PT;

  const blocks: Block[] = [];
  const blank = () => blocks.push(paragraph(""));

  // Header: "T.C.", the institution in capitals, then its unit.
  blocks.push(paragraph("T.C.", { align: "center" }, { bold: true }));
  filled(fields.institution).forEach((line, index) =>
    blocks.push(paragraph(index === 0 ? line.toLocaleUpperCase(TR) : line, { align: "center" }, { bold: true })),
  );
  blank();

  // Number and date on one line, then the subject.
  const number = formatLetterNumber(fields);
  blocks.push(paragraph(`Sayı\t: ${number}\t${fields.date.trim()}`, { tabs: [LABEL_TAB, DATE_TAB] }));
  blocks.push(paragraph(`Konu\t: ${fields.subject.trim()}`, { tabs: [LABEL_TAB] }));
  blank();
  blank();

  // Addressee, centred and in capitals; its place under it in parentheses.
  blocks.push(paragraph(fields.addressee.trim().toLocaleUpperCase(TR), { align: "center" }, { bold: true }));
  if (fields.addresseePlace.trim()) {
    blocks.push(paragraph(`(${fields.addresseePlace.trim()})`, { align: "center" }));
  }
  blank();

  const references = filled(fields.references);
  references.forEach((reference, index) => {
    const letter = `${String.fromCharCode(97 + index)})`;
    const text = index === 0 ? `İlgi\t: ${letter} ${reference}` : `\t${letter} ${reference}`;
    blocks.push(paragraph(text, { tabs: [LABEL_TAB] }));
  });
  if (references.length) blank();

  // Body: justified, first-line indent; the closing sentence the same way.
  const body = fields.body
    .split(/\n\s*\n/)
    .map((part) => part.replace(/\s*\n\s*/g, " ").trim())
    .filter(Boolean);
  for (const part of [...body, ...filled([fields.closing])]) {
    blocks.push(paragraph(part, { align: "justify", firstLinePt: INDENT_PT, spaceAfterPt: 6 }));
  }
  blank();
  blank();

  // Signature block on the right.
  const signature = { align: "center", indentLeftPt: SIGNATURE_INDENT_PT };
  blocks.push(paragraph(formatSignerName(fields.signerName), signature));
  if (fields.signerTitle.trim()) blocks.push(paragraph(fields.signerTitle.trim(), signature));
  if (fields.eSigned) blocks.push(paragraph("e-imzalıdır", signature));

  const attachments = filled(fields.attachments);
  if (attachments.length === 1) {
    blank();
    blocks.push(paragraph(`Ek: ${attachments[0]}`));
  } else if (attachments.length > 1) {
    blank();
    blocks.push(paragraph("Ekler:"));
    attachments.forEach((attachment, index) => blocks.push(paragraph(`${index + 1}- ${attachment}`)));
  }

  const action = filled(fields.distributionAction);
  const info = filled(fields.distributionInfo);
  if (action.length || info.length) {
    blank();
    blocks.push(paragraph("Dağıtım:"));
    const columns = [
      action.length ? { title: "Gereği:", items: action } : null,
      info.length ? { title: "Bilgi:", items: info } : null,
    ].filter((column): column is { title: string; items: string[] } => column !== null);
    const tabs = columns.length > 1 ? [COLUMN_TAB] : [];
    blocks.push(paragraph(columns.map((column) => column.title).join("\t"), { tabs }));
    const rows = Math.max(...columns.map((column) => column.items.length));
    for (let row = 0; row < rows; row += 1) {
      blocks.push(paragraph(columns.map((column) => column.items[row] ?? "").join("\t"), { tabs }));
    }
  }
  doc.blocks = blocks;

  // Contact block in the page footer, 9 pt.
  const { contact } = fields;
  const reach = [
    contact.phone.trim() && `Telefon: ${contact.phone.trim()}`,
    contact.email.trim() && `E-posta: ${contact.email.trim()}`,
    contact.web.trim() && `İnternet: ${contact.web.trim()}`,
  ].filter(Boolean);
  const footer = [
    contact.address.trim() && `Adres: ${contact.address.trim()}`,
    reach.join("  "),
    contact.kep.trim() && `KEP Adresi: ${contact.kep.trim()}`,
    contact.contactPerson.trim() && `Bilgi için: ${contact.contactPerson.trim()}`,
  ].filter((line): line is string => Boolean(line));
  doc.footer = footer.map((line) => paragraph(line, {}, { sizePt: 9 }));
  return doc;
}

// ---------------------------------------------------------------------------
// The running number
// ---------------------------------------------------------------------------

const SEQUENCE_STORAGE_KEY = "omni.officialLetter.sequence";

export const sequenceKey = (year: number, detsis: string, filePlanCode: string) =>
  `${year}|${detsis.trim()}|${filePlanCode.trim()}`;

function readSequences(): Record<string, number> {
  try {
    const parsed = JSON.parse(localStorage.getItem(SEQUENCE_STORAGE_KEY) ?? "{}") as unknown;
    return parsed && typeof parsed === "object" ? (parsed as Record<string, number>) : {};
  } catch {
    return {};
  }
}

/** The number the next letter under `key` gets. */
export function nextSequence(key: string): number {
  const last = readSequences()[key];
  return typeof last === "number" && Number.isFinite(last) ? last + 1 : 1;
}

/** Records that `value` was used under `key`; the counter never moves back. */
export function commitSequence(key: string, value: number): void {
  try {
    const all = readSequences();
    all[key] = Math.max(all[key] ?? 0, value);
    localStorage.setItem(SEQUENCE_STORAGE_KEY, JSON.stringify(all));
  } catch {
    // Storage can be unavailable (private mode); the number is still shown.
  }
}
