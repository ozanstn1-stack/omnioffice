/**
 * Turkish AI task packs (C7): ready-made instructions with a structured output
 * shape and their own verification, so a local model's answer can be checked
 * instead of trusted. The packs are data: the screen builds the question with
 * `buildTaskPackQuestion`, sends it through the existing `ai_chat` command and
 * checks the answer with `verifyTaskPackOutput`. Nothing here calls a network -
 * the provider (Ollama by default) is the only network user and it is a local
 * server, see `isLocalProvider`.
 */

export type TaskPackId = "duzelt-dilekce" | "ozetle-sozlesme" | "kdv-ozeti" | "tebligat-sure";

export type FieldType = "string" | "number" | "array";

/** One field of the expected JSON, with the shape of array items. */
export interface FieldSpec {
  name: string;
  type: FieldType;
  /** Fields of each array item (when `type` is "array"). */
  items?: FieldSpec[];
  /** Allowed values of a string field. */
  values?: string[];
  /** Absent fields are allowed; present ones still have to match the type. */
  optional?: boolean;
}

/** A verification problem, as an i18n code plus its parameters. */
export interface VerificationIssue {
  code:
    | "json"
    | "missingField"
    | "wrongType"
    | "wrongValue"
    | "emptyField"
    | "vatMath"
    | "totalsMismatch"
    | "dateFormat"
    | "dateOrder"
    | "range"
    | "tooFew";
  field?: string;
  params?: Record<string, string | number>;
}

export interface TaskPackResult {
  ok: boolean;
  parsed: Record<string, unknown> | null;
  issues: VerificationIssue[];
}

export interface TaskPack {
  id: TaskPackId;
  titleKey: string;
  descriptionKey: string;
  /** The task instruction sent to the model (Turkish). */
  prompt: string;
  /** The JSON shape the answer must follow. */
  fields: FieldSpec[];
  /** Checks beyond the shape, e.g. the KDV arithmetic. */
  extra?: (value: Record<string, unknown>) => VerificationIssue[];
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function placeholderFor(field: FieldSpec): unknown {
  if (field.values?.length) return field.values[0];
  if (field.type === "number") return 0;
  if (field.type === "array") return [exampleFromFields(field.items ?? [])];
  return "<metin>";
}

function exampleFromFields(fields: FieldSpec[]): Record<string, unknown> {
  const example: Record<string, unknown> = {};
  for (const field of fields) example[field.name] = placeholderFor(field);
  return example;
}

function validateField(value: unknown, field: FieldSpec, path: string, issues: VerificationIssue[]): void {
  if (value === undefined || value === null) {
    if (!field.optional) issues.push({ code: "missingField", field: path });
    return;
  }
  if (field.type === "string") {
    if (typeof value !== "string") {
      issues.push({ code: "wrongType", field: path, params: { expected: "string" } });
      return;
    }
    if (value.trim() === "") {
      issues.push({ code: "emptyField", field: path });
      return;
    }
    if (field.values && !field.values.includes(value)) {
      issues.push({ code: "wrongValue", field: path, params: { expected: field.values.join(", ") } });
    }
    return;
  }
  if (field.type === "number") {
    if (typeof value !== "number" || !Number.isFinite(value)) {
      issues.push({ code: "wrongType", field: path, params: { expected: "number" } });
    }
    return;
  }
  if (!Array.isArray(value)) {
    issues.push({ code: "wrongType", field: path, params: { expected: "array" } });
    return;
  }
  value.forEach((item, index) => {
    if (!isRecord(item)) {
      issues.push({ code: "wrongType", field: `${path}[${index}]`, params: { expected: "object" } });
      return;
    }
    for (const inner of field.items ?? [])
      validateField(item[inner.name], inner, `${path}[${index}].${inner.name}`, issues);
  });
}

function validateShape(value: Record<string, unknown>, fields: FieldSpec[]): VerificationIssue[] {
  const issues: VerificationIssue[] = [];
  for (const field of fields) validateField(value[field.name], field, field.name, issues);
  return issues;
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
        items: [
          { name: "severity", type: "string", values: ["high", "medium", "low"] },
          { name: "title", type: "string" },
          { name: "suggestion", type: "string" },
        ],
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
        items: [
          { name: "heading", type: "string" },
          { name: "summary", type: "string" },
          { name: "risk", type: "string", optional: true },
        ],
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
        items: [
          { name: "rate", type: "number" },
          { name: "base", type: "number" },
          { name: "vat", type: "number" },
        ],
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
];

export function getTaskPack(id: TaskPackId): TaskPack {
  const pack = TASK_PACKS.find((candidate) => candidate.id === id);
  if (!pack) throw new Error(`unknown task pack: ${id}`);
  return pack;
}

/**
 * The question sent through `ai_chat`: the task, the JSON shape and the rules
 * a local model needs spelled out. Kept deterministic so it can be asserted.
 */
export function buildTaskPackQuestion(pack: TaskPack): string {
  const shape = JSON.stringify(exampleFromFields(pack.fields), null, 2);
  return [
    pack.prompt.trim(),
    "",
    "Yanıtı SADECE aşağıdaki JSON nesnesi olarak ver; kod bloğu, açıklama veya ek metin ekleme.",
    "Beklenen JSON biçimi:",
    shape,
    "Kurallar:",
    "- Metinde olmayan bilgi uydurma; bilinmeyen sayısal alan 0, bilinmeyen metin boş string olsun.",
    "- Sayıları nokta ondalıkla ve para birimi simgesi olmadan yaz (örn. 1234.56).",
    "- Tarihleri YYYY-AA-GG biçiminde yaz.",
  ].join("\n");
}

/** The JSON object inside a model answer (code fences and prose around it are dropped). */
export function extractJsonObject(raw: string): Record<string, unknown> | null {
  const start = raw.indexOf("{");
  const end = raw.lastIndexOf("}");
  if (start === -1 || end <= start) return null;
  try {
    const parsed: unknown = JSON.parse(raw.slice(start, end + 1));
    return isRecord(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

/** Parses and verifies a model answer against the pack's shape and checks. */
export function verifyTaskPackOutput(pack: TaskPack, raw: string): TaskPackResult {
  const parsed = extractJsonObject(raw);
  if (!parsed) return { ok: false, parsed: null, issues: [{ code: "json" }] };
  const issues = [...validateShape(parsed, pack.fields), ...(pack.extra ? pack.extra(parsed) : [])];
  return { ok: issues.length === 0, parsed, issues };
}
