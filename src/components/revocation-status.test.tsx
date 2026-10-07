import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { RevocationStatus, formatRevocationTime } from "./revocation-status";

describe("signature revocation status", () => {
  it("says the check did not run when there is no result", () => {
    const { rerender } = render(<RevocationStatus />);
    expect(screen.getByText("Revocation not checked")).toBeInTheDocument();
    rerender(<RevocationStatus revocation={{ status: "not_checked" }} />);
    expect(screen.getByText("Revocation not checked")).toBeInTheDocument();
  });

  it("reports a good OCSP or CRL answer with its time", () => {
    const { rerender } = render(
      <RevocationStatus revocation={{ status: "good", source: "ocsp", checkedAt: "2026-10-07T12:34:56Z" }} />,
    );
    expect(screen.getByText("Not revoked (OCSP, checked 2026-10-07 12:34 UTC)")).toBeInTheDocument();
    rerender(<RevocationStatus revocation={{ status: "good", source: "crl", checkedAt: "2026-10-08T01:02:03Z" }} />);
    expect(screen.getByText("Not revoked (CRL, checked 2026-10-08 01:02 UTC)")).toBeInTheDocument();
  });

  it("shows when and why a certificate was revoked", () => {
    render(
      <RevocationStatus
        revocation={{ status: "revoked", source: "ocsp", revokedAt: "2026-03-01T08:00:00Z", reason: "keyCompromise" }}
      />,
    );
    expect(screen.getByText("Revoked on 2026-03-01 08:00 UTC")).toBeInTheDocument();
    expect(screen.getByText("Revocation reason: keyCompromise")).toBeInTheDocument();
  });

  it("explains unknown and failed checks without calling the certificate good", () => {
    const { rerender } = render(
      <RevocationStatus revocation={{ status: "unknown", detail: "the signer certificate is self-signed" }} />,
    );
    expect(screen.getByText("Revocation status unknown")).toBeInTheDocument();
    expect(screen.getByText("the signer certificate is self-signed")).toBeInTheDocument();
    rerender(<RevocationStatus revocation={{ status: "error", detail: "the server could not be reached" }} />);
    expect(screen.getByText("Revocation check failed")).toBeInTheDocument();
    expect(screen.getByText("the server could not be reached")).toBeInTheDocument();
    expect(screen.queryByText(/Not revoked/)).toBeNull();
  });

  it("leaves unrecognised time formats as they are", () => {
    expect(formatRevocationTime("yesterday")).toBe("yesterday");
  });
});
