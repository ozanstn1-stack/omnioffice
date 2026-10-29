/**
 * Signature report summaries for the verification screen (C9).
 *
 * `pdf_verify_signatures` already answers the hard questions per signature
 * (digest match, signature validity, whether the document changed after
 * signing, chain linkage, revocation). This module turns that report into the
 * single verdict a user can act on, plus the counts the summary card shows, so
 * the screen and its tests share one set of rules instead of re-deriving them.
 */
import type { SignatureInfo, SignatureReport } from "./api";

/** Per-signature state: the revision is intact, changed, or does not verify. */
export type SignatureVerdict = "valid" | "modified" | "invalid";

/** What the document as a whole is, in one word. */
export type VerificationVerdict = "unsigned" | "valid" | "modified" | "invalid" | "unreadable";

export interface VerificationSummary {
  verdict: VerificationVerdict;
  signatures: number;
  valid: number;
  /** Signatures whose signed revision changed after signing. */
  modified: number;
  /** Signatures whose digest or signature check failed. */
  invalid: number;
  /** Valid signatures that are no longer the last revision. */
  superseded: number;
  /** Signatures that cover the whole document (a "final" signature). */
  wholeDocument: number;
  /** Signatures that carry an RFC 3161 timestamp. */
  timestamped: number;
  /** Signatures whose signer certificate or chain carries an expired certificate. */
  expired: number;
  warnings: number;
}

/**
 * "Modified after signing" wins over a plain digest/signature failure: it says
 * what actually happened to the file, which matters more than the check that
 * consequently failed.
 */
export function signatureVerdict(signature: SignatureInfo): SignatureVerdict {
  if (signature.modifiedAfterSigning) return "modified";
  if (!signature.signatureValid || !signature.digestMatches) return "invalid";
  return "valid";
}

/** True when the signature verifies and its signed revision was not changed. */
export function signatureIsIntact(signature: SignatureInfo): boolean {
  return signatureVerdict(signature) === "valid";
}

/**
 * One verdict for the document. An empty signature list is only "unsigned"
 * when the report carries no warning; a file the parser could not read must
 * not read like a file that was never signed.
 */
export function summarizeSignatureReport(report: SignatureReport): VerificationSummary {
  const signatures = report.signatures ?? [];
  const warnings = report.warnings ?? [];
  const verdicts = signatures.map(signatureVerdict);
  const modified = verdicts.filter((verdict) => verdict === "modified").length;
  const invalid = verdicts.filter((verdict) => verdict === "invalid").length;

  let verdict: VerificationVerdict;
  if (signatures.length === 0) {
    verdict = warnings.length > 0 ? "unreadable" : "unsigned";
  } else if (modified > 0) {
    verdict = "modified";
  } else if (invalid > 0) {
    verdict = "invalid";
  } else {
    verdict = "valid";
  }

  return {
    verdict,
    signatures: signatures.length,
    valid: verdicts.filter((entry) => entry === "valid").length,
    modified,
    invalid,
    superseded: signatures.filter((signature) => signature.supersededByLaterRevision).length,
    wholeDocument: signatures.filter((signature) => signature.coversWholeDocument).length,
    timestamped: signatures.filter((signature) => Boolean(signature.timestamp)).length,
    expired: signatures.filter(
      (signature) => signature.signer.expired || signature.chain.some((certificate) => certificate.expired),
    ).length,
    warnings: warnings.length,
  };
}
