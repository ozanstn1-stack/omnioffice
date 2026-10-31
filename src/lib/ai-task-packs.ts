/**
 * Turkish AI task packs (C7): ready-made instructions with a structured output
 * shape and their own verification, so a local model's answer can be checked
 * instead of trusted. The packs are data: the screen builds the question with
 * `buildTaskPackQuestion`, sends it through the existing `ai_chat` command and
 * checks the answer with `verifyTaskPackOutput`. Nothing here calls a network -
 * the provider (Ollama by default) is the only network user and it is a local
 * server, see `isLocalProvider`.
 */

export type TaskPackId =
  | "duzelt-dilekce"
  | "ozetle-sozlesme"
  | "kdv-ozeti"
  | "tebligat-sure"
  | "efatura-analiz"
  | "kep-tebligat"
  | "mahkeme-dilekce";

export type FieldType = "string" | "number" | "array";

/** One field of the expected JSON, with the shape of array items. */
export interface FieldSpec {
  name: string;
  type: FieldType;
  /** Fields of each array item (when `type` is "array"). */
  items?: FieldSpec[];
  /** Primitive array items (when the array is a list of strings or numbers). */
  itemType?: "string" | "number";
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
  if (field.itemType) {
    value.forEach((item, index) => {
      const expected = field.itemType as "string" | "number";
      const valid =
        expected === "string"
          ? typeof item === "string" && item.trim() !== ""
          : typeof item === "number" && Number.isFinite(item);
      if (!valid) issues.push({ code: "wrongType", field: `${path}[${index}]`, params: { expected } });
    });
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

export { TASK_PACKS, getTaskPack } from "./ai-task-packs-catalog";

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
