/**
 * The Turkish AI task packs catalogue (C7, D5).
 *
 * A pack is data: an instruction, the JSON shape the answer must follow and
 * the checks beyond that shape. The validation engine lives in
 * `ai-task-packs.ts`; this file only declares the packs (and the arithmetic /
 * date rules that are specific to one pack), so adding a pack stays a data
 * change and the file that the screen imports stays under the line budget.
 *
 * Nothing here calls a network: the provider is the only network user and the
 * local presets are a localhost server.
 */
import type { FieldSpec, TaskPack, VerificationIssue } from "./ai-task-packs";

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function dateValue(value: unknown): number | null {
  if (typeof value !== "string" || !/^\d{4}-\d{2}-\d{2}$/.test(value)) return null;
  const parsed = Date.parse(`${value}T00:00:00Z`);
  return Number.isNaN(parsed) ? null : parsed;
}

function numbersIn(value: Record<string, unknown>): Record<string, number> {
  const numbers: Record<string, number> = {};
  for (const [key, entry] of Object.entries(value)) {
    if (typeof entry === "number" && Number.isFinite(entry)) numbers[key] = entry;
  }
  return numbers;
}

/** KDV: each row's vat ≈ base * rate / 100 and the totals match the rows. */
function verifyVat(value: Record<string, unknown>): VerificationIssue[] {
  const issues: VerificationIssue[] = [];
  const rows = Array.isArray(value.rows) ? value.rows : [];
  let base = 0;
  let vat = 0;
  rows.forEach((row, index) => {
    if (!isRecord(row)) return;
    const rate = typeof row.rate === "number" ? row.rate : 0;
    const rowBase = typeof row.base === "number" ? row.base : 0;
    const rowVat = typeof row.vat === "number" ? row.vat : 0;
    if (rate < 0 || rate > 100)
      issues.push({ code: "range", field: `rows[${index}].rate`, params: { min: 0, max: 100 } });
    base += rowBase;
    vat += rowVat;
    const expected = (rowBase * rate) / 100;
    const tolerance = Math.max(1, Math.abs(rowBase) * 0.005);
    if (Math.abs(expected - rowVat) > tolerance) {
      issues.push({
        code: "vatMath",
        field: `rows[${index}].vat`,
        params: { expected: expected.toFixed(2), actual: rowVat.toFixed(2) },
      });
    }
  });
  const numbers = numbersIn(value);
  if (typeof numbers.totalBase === "number" && Math.abs(numbers.totalBase - base) > 1) {
    issues.push({
      code: "totalsMismatch",
      field: "totalBase",
      params: { expected: base.toFixed(2), actual: numbers.totalBase.toFixed(2) },
    });
  }
  if (typeof numbers.totalVat === "number" && Math.abs(numbers.totalVat - vat) > 1) {
    issues.push({
      code: "totalsMismatch",
      field: "totalVat",
      params: { expected: vat.toFixed(2), actual: numbers.totalVat.toFixed(2) },
    });
  }
  return issues;
}

/** Notification deadline: real dates, the last day not before the service day, a sane period. */
function verifyDeadline(value: Record<string, unknown>): VerificationIssue[] {
  const issues: VerificationIssue[] = [];
  const service = dateValue(value.serviceDate);
  const last = dateValue(value.lastDay);
  if (service === null) issues.push({ code: "dateFormat", field: "serviceDate" });
  if (last === null) issues.push({ code: "dateFormat", field: "lastDay" });
  if (service !== null && last !== null && last < service) {
    issues.push({ code: "dateOrder", field: "lastDay" });
  }
  if (typeof value.periodDays === "number" && (value.periodDays < 1 || value.periodDays > 3650)) {
    issues.push({ code: "range", field: "periodDays", params: { min: 1, max: 3650 } });
  }
  return issues;
}

/** E-invoice: line arithmetic, the VAT per line and the three totals. */
function verifyInvoice(value: Record<string, unknown>): VerificationIssue[] {
  const issues: VerificationIssue[] = [];
  const rows = Array.isArray(value.rows) ? value.rows : [];
  let baseSum = 0;
  let vatSum = 0;
  rows.forEach((row, index) => {
    if (!isRecord(row)) return;
    const quantity = typeof row.quantity === "number" ? row.quantity : 0;
    const unitPrice = typeof row.unitPrice === "number" ? row.unitPrice : 0;
    const vatRate = typeof row.vatRate === "number" ? row.vatRate : 0;
    const lineBase = typeof row.lineBase === "number" ? row.lineBase : 0;
    const lineVat = typeof row.lineVat === "number" ? row.lineVat : 0;
    if (vatRate < 0 || vatRate > 100) {
      issues.push({ code: "range", field: `rows[${index}].vatRate`, params: { min: 0, max: 100 } });
    }
    const expectedBase = quantity * unitPrice;
    if (Math.abs(expectedBase - lineBase) > Math.max(0.05, Math.abs(expectedBase) * 0.001)) {
      issues.push({
        code: "totalsMismatch",
        field: `rows[${index}].lineBase`,
        params: { expected: expectedBase.toFixed(2), actual: lineBase.toFixed(2) },
      });
    }
    const expectedVat = (lineBase * vatRate) / 100;
    if (Math.abs(expectedVat - lineVat) > Math.max(0.05, Math.abs(expectedVat) * 0.005)) {
      issues.push({
        code: "vatMath",
        field: `rows[${index}].lineVat`,
        params: { expected: expectedVat.toFixed(2), actual: lineVat.toFixed(2) },
      });
    }
    baseSum += lineBase;
    vatSum += lineVat;
  });
  const numbers = numbersIn(value);
  if (typeof numbers.totalBase === "number" && Math.abs(numbers.totalBase - baseSum) > 0.5) {
    issues.push({
      code: "totalsMismatch",
      field: "totalBase",
      params: { expected: baseSum.toFixed(2), actual: numbers.totalBase.toFixed(2) },
    });
  }
  if (typeof numbers.totalVat === "number" && Math.abs(numbers.totalVat - vatSum) > 0.5) {
    issues.push({
      code: "totalsMismatch",
      field: "totalVat",
      params: { expected: vatSum.toFixed(2), actual: numbers.totalVat.toFixed(2) },
    });
  }
  if (typeof numbers.total === "number" && Math.abs(numbers.total - (baseSum + vatSum)) > 0.5) {
    issues.push({
      code: "totalsMismatch",
      field: "total",
      params: { expected: (baseSum + vatSum).toFixed(2), actual: numbers.total.toFixed(2) },
    });
  }
  return issues;
}

/** KEP notice: the deadline rules plus a non-empty subject/summary pair. */
function verifyKep(value: Record<string, unknown>): VerificationIssue[] {
  return verifyDeadline(value);
}

/** Court petition: a known court type, at least one request and one evidence item. */
function verifyCourtPetition(value: Record<string, unknown>): VerificationIssue[] {
  const issues: VerificationIssue[] = [];
  const requests = Array.isArray(value.requests) ? value.requests : [];
  const evidence = Array.isArray(value.evidence) ? value.evidence : [];
  if (requests.length === 0) issues.push({ code: "tooFew", field: "requests", params: { min: 1 } });
  if (evidence.length === 0) issues.push({ code: "tooFew", field: "evidence", params: { min: 1 } });
  return issues;
}

const itemFields = (fields: FieldSpec[]) => fields;

export const TASK_PACKS: TaskPack[] = [
  {
    id: "duzelt-dilekce",
    titleKey: "aiPacks.pack.dilekce.title",
    descriptionKey: "aiPacks.pack.dilekce.description",
    prompt:
      "Sen Türk hukuk diline hâkim bir editörsün. Verilen dilekçeyi incele: dil bilgisi, imla, noktalama, " +
      "resmî hitap ve tutarlılık sorunlarını bul; her sorun için kısa bir başlık, önem derecesi ve somut bir " +
      "düzeltme önerisi yaz. Ardından dilekçenin düzeltilmiş tam metnini ver. Metinde olmayan bilgi uydurma.",
    fields: [
      { name: "summary", type: "string" },
      {
        name: "issues",
        type: "array",
        items: itemFields([
          { name: "severity", type: "string", values: ["high", "medium", "low"] },
          { name: "title", type: "string" },
          { name: "suggestion", type: "string" },
        ]),
      },
      { name: "corrected", type: "string" },
    ],
  },
  {
    id: "ozetle-sozlesme",
    titleKey: "aiPacks.pack.sozlesme.title",
    descriptionKey: "aiPacks.pack.sozlesme.description",
    prompt:
      "Sen sözleşme inceleyen bir uzmansın. Verilen sözleşme metnindeki maddeleri başlık, tek cümlelik özet " +
      "ve varsa taraf için risk olarak çıkar; sonunda sözleşmenin kısa bir özetini yaz. Maddede yazmayan bir " +
      "hüküm ekleme, yorumu risk alanında açıkça belirt.",
    fields: [
      { name: "summary", type: "string" },
      {
        name: "clauses",
        type: "array",
        items: itemFields([
          { name: "heading", type: "string" },
          { name: "summary", type: "string" },
          { name: "risk", type: "string", optional: true },
        ]),
      },
    ],
    extra: (value) => {
      const clauses = Array.isArray(value.clauses) ? value.clauses : [];
      return clauses.length === 0 ? [{ code: "tooFew", field: "clauses", params: { min: 1 } }] : [];
    },
  },
  {
    id: "kdv-ozeti",
    titleKey: "aiPacks.pack.kdv.title",
    descriptionKey: "aiPacks.pack.kdv.description",
    prompt:
      "Sen muhasebe uzmanısın. Verilen fatura/KDV dökümünden oran bazında matrah ve KDV satırlarını çıkar " +
      "(satırlar: rate, base, vat), sonra toplam matrah (totalBase) ve toplam KDV (totalVat) yaz. Sayıları " +
      "nokta ondalıkla ve para birimi simgesi olmadan ver. Hesaplamadığın bir sayıyı uydurma.",
    fields: [
      {
        name: "rows",
        type: "array",
        items: itemFields([
          { name: "rate", type: "number" },
          { name: "base", type: "number" },
          { name: "vat", type: "number" },
        ]),
      },
      { name: "totalBase", type: "number" },
      { name: "totalVat", type: "number" },
    ],
    extra: verifyVat,
  },
  {
    id: "tebligat-sure",
    titleKey: "aiPacks.pack.tebligat.title",
    descriptionKey: "aiPacks.pack.tebligat.description",
    prompt:
      "Sen tebligat sürelerini hesaplayan bir hukuk asistanısın. Verilen tebligat metninden tebliğ tarihini " +
      "(serviceDate), süreyi gün olarak (periodDays) ve son günü (lastDay) çıkar; hesabın adımlarını kısaca " +
      "reasoning alanında açıkla. Tarihleri YYYY-AA-GG biçiminde yaz. Metinde olmayan bir tarih uydurma.",
    fields: [
      { name: "serviceDate", type: "string" },
      { name: "periodDays", type: "number" },
      { name: "lastDay", type: "string" },
      { name: "reasoning", type: "string" },
    ],
    extra: verifyDeadline,
  },
  {
    id: "efatura-analiz",
    titleKey: "aiPacks.pack.efatura.title",
    descriptionKey: "aiPacks.pack.efatura.description",
    prompt:
      "Sen e-fatura denetleyen bir muhasebe uzmanısın. Faturadan satıcı ve alıcı adını, fatura tarihini ve " +
      "kalemleri çıkar (her kalem: açıklama, miktar, birim fiyat, KDV oranı, satır matrahı, satır KDV'si). " +
      "Sonra toplam matrah, toplam KDV ve genel toplamı yaz. Sayıları nokta ondalıkla ve para birimi simgesi " +
      "olmadan ver; kalemlerde yazmayan bir tutarı uydurma.",
    fields: [
      { name: "supplier", type: "string" },
      { name: "customer", type: "string" },
      { name: "invoiceDate", type: "string" },
      {
        name: "rows",
        type: "array",
        items: itemFields([
          { name: "description", type: "string" },
          { name: "quantity", type: "number" },
          { name: "unitPrice", type: "number" },
          { name: "vatRate", type: "number" },
          { name: "lineBase", type: "number" },
          { name: "lineVat", type: "number" },
        ]),
      },
      { name: "totalBase", type: "number" },
      { name: "totalVat", type: "number" },
      { name: "total", type: "number" },
    ],
    extra: verifyInvoice,
  },
  {
    id: "kep-tebligat",
    titleKey: "aiPacks.pack.kep.title",
    descriptionKey: "aiPacks.pack.kep.description",
    prompt:
      "Sen KEP ve tebligat metinlerini okuyan bir hukuk asistanısın. Göndereni, konuyu ve tebliğ tarihini " +
      "(serviceDate) çıkar; tebligatın hukuki süresini gün olarak (periodDays) ve son gününü (lastDay) hesapla. " +
      "Metni başlık ve özet olarak bölümlere ayır (sections). Tarihleri YYYY-AA-GG biçiminde yaz; metinde " +
      "olmayan bir tarih veya süre uydurma, hesaplamayı reasoning alanında göster.",
    fields: [
      { name: "sender", type: "string" },
      { name: "subject", type: "string" },
      { name: "serviceDate", type: "string" },
      { name: "periodDays", type: "number" },
      { name: "lastDay", type: "string" },
      {
        name: "sections",
        type: "array",
        items: itemFields([
          { name: "heading", type: "string" },
          { name: "summary", type: "string" },
        ]),
      },
      { name: "reasoning", type: "string" },
    ],
    extra: (value) => {
      const issues = verifyKep(value);
      const sections = Array.isArray(value.sections) ? value.sections : [];
      if (sections.length === 0) issues.push({ code: "tooFew", field: "sections", params: { min: 1 } });
      return issues;
    },
  },
  {
    id: "mahkeme-dilekce",
    titleKey: "aiPacks.pack.mahkeme.title",
    descriptionKey: "aiPacks.pack.mahkeme.description",
    prompt:
      "Sen dilekçe sınıflandıran bir hukuk asistanısın. Verilen dilekçeden mahkeme türünü seç " +
      "(courtType: asliye-hukuk, asliye-ceza, sulh-hukuk, sulh-ceza, idare, aile, is, icra, anayasa, " +
      "bilinmiyor), talep konusunu tek cümleyle yaz, talepleri (requests) ve delilleri (evidence) liste halinde " +
      "çıkar, kısa bir özet ekle. Metinde olmayan bir talep veya delil uydurma.",
    fields: [
      {
        name: "courtType",
        type: "string",
        values: [
          "asliye-hukuk",
          "asliye-ceza",
          "sulh-hukuk",
          "sulh-ceza",
          "idare",
          "aile",
          "is",
          "icra",
          "anayasa",
          "bilinmiyor",
        ],
      },
      { name: "subject", type: "string" },
      { name: "requests", type: "array", itemType: "string" },
      { name: "evidence", type: "array", itemType: "string" },
      { name: "summary", type: "string" },
    ],
    extra: verifyCourtPetition,
  },
];

export function getTaskPack(id: TaskPack["id"]): TaskPack {
  const pack = TASK_PACKS.find((candidate) => candidate.id === id);
  if (!pack) throw new Error(`unknown task pack: ${id}`);
  return pack;
}
