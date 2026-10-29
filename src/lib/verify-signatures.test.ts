import { describe, expect, it } from "vitest";

import type { SignatureCertificateInfo, SignatureInfo, SignatureReport } from "./api";
import {
  signatureIsIntact,
  signatureVerdict,
  summarizeSignatureReport,
  type VerificationVerdict,
} from "./verify-signatures";

function certificate(overrides: Partial<SignatureCertificateInfo> = {}): SignatureCertificateInfo {
  return {
    subject: "CN=Örnek İmza Sahibi",
    issuer: "CN=Örnek Kök CA",
    serialHex: "01",
    notBefore: "2026-01-01T00:00:00Z",
    notAfter: "2027-01-01T00:00:00Z",
    expired: false,
    isCa: false,
    sha256Fingerprint: "AA:BB",
    ...overrides,
  };
}

function signature(overrides: Partial<SignatureInfo> = {}): SignatureInfo {
  return {
    fieldName: "Signature1",
    subFilter: "adbe.pkcs7.detached",
    coversWholeDocument: true,
    supersededByLaterRevision: false,
    modifiedAfterSigning: false,
    digestMatches: true,
    signatureValid: true,
    chain: [certificate()],
    chainLinked: true,
    selfSignedChain: false,
    signer: certificate(),
    signingTime: "2026-10-29T10:00:00Z",
    timestamp: null,
    algorithm: "sha256WithRSAEncryption",
    trust: "unknown",
    revocation: undefined,
    notes: [],
    ...overrides,
  };
}

function report(overrides: Partial<SignatureReport> = {}): SignatureReport {
  return { signatures: [signature()], warnings: [], ...overrides };
}

function verdictOf(value: SignatureReport): VerificationVerdict {
  return summarizeSignatureReport(value).verdict;
}

describe("signatureVerdict", () => {
  it("calls an intact signature valid", () => {
    expect(signatureVerdict(signature())).toBe("valid");
    expect(signatureIsIntact(signature())).toBe(true);
  });

  it("reports a changed signed revision as modified", () => {
    expect(signatureVerdict(signature({ modifiedAfterSigning: true }))).toBe("modified");
    expect(signatureIsIntact(signature({ modifiedAfterSigning: true }))).toBe(false);
  });

  it("reports a failed digest or signature check as invalid", () => {
    expect(signatureVerdict(signature({ digestMatches: false }))).toBe("invalid");
    expect(signatureVerdict(signature({ signatureValid: false }))).toBe("invalid");
  });
});

describe("summarizeSignatureReport", () => {
  it("treats a file with no signatures and no warnings as unsigned", () => {
    const summary = summarizeSignatureReport(report({ signatures: [] }));
    expect(summary.verdict).toBe("unsigned");
    expect(summary.signatures).toBe(0);
  });

  it("treats an unreadable file as unreadable, not unsigned", () => {
    expect(verdictOf(report({ signatures: [], warnings: ["the document could not be parsed"] }))).toBe("unreadable");
  });

  it("keeps a single modified signature from reading as valid", () => {
    expect(verdictOf(report({ signatures: [signature(), signature({ modifiedAfterSigning: true })] }))).toBe(
      "modified",
    );
  });

  it("prefers modified over invalid when both are present", () => {
    const summary = summarizeSignatureReport(
      report({
        signatures: [signature({ digestMatches: false }), signature({ modifiedAfterSigning: true })],
      }),
    );
    expect(summary.verdict).toBe("modified");
    expect(summary.invalid).toBe(1);
    expect(summary.modified).toBe(1);
  });

  it("reports an invalid signature when nothing was modified", () => {
    expect(verdictOf(report({ signatures: [signature({ signatureValid: false })] }))).toBe("invalid");
  });

  it("counts valid signatures, superseded revisions, coverage and timestamps", () => {
    const summary = summarizeSignatureReport(
      report({
        signatures: [
          signature(),
          signature({
            fieldName: "Signature2",
            supersededByLaterRevision: true,
            coversWholeDocument: false,
            timestamp: "2026-10-29T10:00:01Z",
          }),
          signature({ fieldName: "Signature3", signer: certificate({ expired: true }) }),
        ],
        warnings: ["an unsigned note"],
      }),
    );
    expect(summary.verdict).toBe("valid");
    expect(summary.signatures).toBe(3);
    expect(summary.valid).toBe(3);
    expect(summary.superseded).toBe(1);
    expect(summary.wholeDocument).toBe(2);
    expect(summary.timestamped).toBe(1);
    expect(summary.expired).toBe(1);
    expect(summary.warnings).toBe(1);
  });

  it("counts an expired certificate deeper in the chain", () => {
    const summary = summarizeSignatureReport(
      report({ signatures: [signature({ chain: [certificate(), certificate({ isCa: true, expired: true })] })] }),
    );
    expect(summary.expired).toBe(1);
    expect(summary.verdict).toBe("valid");
  });
});
