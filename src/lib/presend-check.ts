/**
 * Pre-send check (issue #341, C10): what a PDF carries that its sender may
 * not mean to send, read from the real file just before it is shared.
 *
 * The inputs are the existing commands' results (`inspect_document`,
 * `pdf_verify_signatures`, `page_text`, `pdfa_validate`); this module only
 * turns them into findings, worst first. Copy lives under `presend.<code>`.
 */
import { countPersonalData } from "./document-health";
import type { SignatureInfo } from "./api";
import type { DocumentInspection } from "./types";

export type PresendCode =
  | "signatureBroken"
  | "changedAfterSigning"
  | "personalData"
  | "comments"
  | "hiddenAnnotations"
  | "attachments"
  | "javascript"
  | "metadata"
  | "editableForm"
  | "signed"
  | "notPdfa";

/** "stop": do not send as is; "warn": look first; "info": worth knowing. */
export type PresendSeverity = "stop" | "warn" | "info";

export interface PresendFinding {
  code: PresendCode;
  severity: PresendSeverity;
  count: number;
  /** Short, non-sensitive details: field names, attachment names, data kinds. */
  items: string[];
}

export type PresendInspection = Pick<
  DocumentInspection,
  | "annotations"
  | "embeddedFiles"
  | "attachmentCount"
  | "hasJavascript"
  | "javascriptEntries"
  | "hasOpenAction"
  | "formFields"
  | "authorOverride"
  | "subjectOverride"
  | "creator"
  | "producer"
>;

export type PresendSignature = Pick<
  SignatureInfo,
  "fieldName" | "signatureValid" | "digestMatches" | "modifiedAfterSigning"
>;

export interface PresendInput {
  inspection: PresendInspection;
  signatures: PresendSignature[];
  /** Text of the pages read for personal data (the first pages of a long file). */
  text: string;
  /** PDF/A validation result; null when it was not run or failed to run. */
  pdfa: { valid: boolean } | null;
}

/** Annotation kinds that are page furniture rather than someone's note. */
const NON_NOTE_SUBTYPES = new Set(["Link", "Widget", "Popup", "PrinterMark", "TrapNet", "Watermark"]);

const ORDER: Record<PresendSeverity, number> = { stop: 0, warn: 1, info: 2 };

export function presendFindings(input: PresendInput): PresendFinding[] {
  const findings: PresendFinding[] = [];
  const add = (code: PresendCode, severity: PresendSeverity, count: number, items: string[] = []) => {
    if (count > 0) findings.push({ code, severity, count, items });
  };
  const { inspection, signatures } = input;

  const broken = signatures.filter((signature) => !signature.signatureValid || !signature.digestMatches);
  add(
    "signatureBroken",
    "stop",
    broken.length,
    broken.map((signature) => signature.fieldName),
  );
  const changed = signatures.filter(
    (signature) => signature.signatureValid && signature.digestMatches && signature.modifiedAfterSigning,
  );
  add(
    "changedAfterSigning",
    "warn",
    changed.length,
    changed.map((signature) => signature.fieldName),
  );

  const personal = countPersonalData(input.text);
  const kinds = (Object.entries(personal) as [string, number][]).filter(([, count]) => count > 0);
  const personalTotal = kinds.reduce((sum, [, count]) => sum + count, 0);
  add(
    "personalData",
    personal.tckn > 0 || personal.iban > 0 ? "stop" : "warn",
    personalTotal,
    kinds.map(([kind, count]) => `${kind}:${count}`),
  );

  const notes = inspection.annotations.filter(
    (annotation) => !annotation.hidden && !NON_NOTE_SUBTYPES.has(annotation.subtype) && annotation.contents.trim(),
  );
  add("comments", "warn", notes.length, unique(notes.map((annotation) => `p${annotation.page}`)));
  const hidden = inspection.annotations.filter((annotation) => annotation.hidden && annotation.subtype !== "Popup");
  add("hiddenAnnotations", "warn", hidden.length, unique(hidden.map((annotation) => `p${annotation.page}`)));

  const attachments = Math.max(inspection.embeddedFiles.length, inspection.attachmentCount);
  add("attachments", "warn", attachments, inspection.embeddedFiles);

  const scripts = inspection.hasJavascript ? Math.max(1, inspection.javascriptEntries.length) : 0;
  add("javascript", "warn", scripts);

  const metadata = [
    ["author", inspection.authorOverride],
    ["subject", inspection.subjectOverride],
    ["creator", inspection.creator],
    ["producer", inspection.producer],
  ].filter(([, value]) => Boolean(value?.trim()));
  add(
    "metadata",
    "info",
    metadata.length,
    metadata.map(([name]) => name as string),
  );

  const editable = inspection.formFields.filter((field) => !field.readOnly && field.kind !== "Sig");
  add(
    "editableForm",
    "info",
    editable.length,
    editable.slice(0, 5).map((field) => field.name),
  );

  const valid = signatures.length - broken.length;
  add("signed", "info", valid);

  if (input.pdfa && !input.pdfa.valid) add("notPdfa", "info", 1);

  return findings.sort((a, b) => ORDER[a.severity] - ORDER[b.severity]);
}

/**
 * Codes the sanitizer removes (metadata, attachments, scripts). Cleaning is
 * offered only for an unsigned file: rewriting a signed PDF breaks its
 * signature.
 */
const CLEANABLE: ReadonlySet<PresendCode> = new Set(["metadata", "attachments", "javascript"]);

export function canClean(findings: readonly PresendFinding[]): boolean {
  if (findings.some((finding) => finding.code === "signed" || finding.code === "signatureBroken")) return false;
  return findings.some((finding) => CLEANABLE.has(finding.code));
}

/** The sanitizer options used for "Clean and send": links and form actions stay. */
export const SEND_SANITIZE_OPTIONS = {
  removeJavascript: true,
  removeEmbeddedFiles: true,
  removeActions: false,
  removeMetadata: true,
  removeOpenAction: true,
  removeFormActions: false,
  removeAnnotationsUnsafe: true,
  removeLinks: false,
} as const;

function unique(values: string[]): string[] {
  return [...new Set(values)];
}
