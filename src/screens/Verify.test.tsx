import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { SignatureCertificateInfo, SignatureInfo, SignatureReport } from "../lib/api";
import { useSettings } from "../lib/store";
import { DEFAULT_SETTINGS } from "../lib/types";

let report: SignatureReport = { signatures: [], warnings: [] };
let onlineError: Error | null = null;

const invoke = vi.fn(async (command: string, _payload?: unknown) => {
  switch (command) {
    case "file_sizes":
      return [4096];
    case "file_names":
      return ["sozlesme.pdf"];
    case "pdf_verify_signatures":
      return report;
    case "pdf_verify_signatures_online":
      if (onlineError) throw onlineError;
      return report;
    default:
      return null;
  }
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...(args as [string, unknown])) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null), save: vi.fn(async () => null) }));

import { VerifyScreen } from "./Verify";

function certificate(overrides: Partial<SignatureCertificateInfo> = {}): SignatureCertificateInfo {
  return {
    subject: "CN=Örnek İmza Sahibi",
    issuer: "CN=Örnek Kök CA",
    serialHex: "1A2B",
    notBefore: "2026-01-01T00:00:00Z",
    notAfter: "2027-01-01T00:00:00Z",
    expired: false,
    isCa: false,
    sha256Fingerprint: "AA:BB:CC",
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
    notes: [],
    ...overrides,
  };
}

describe("VerifyScreen (C9)", () => {
  beforeEach(() => {
    report = { signatures: [], warnings: [] };
    onlineError = null;
    invoke.mockClear();
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS }, loaded: true });
  });

  it("verifies the handed-over PDF and reports an unsigned document", async () => {
    render(<VerifyScreen initialFiles={["/docs/sozlesme.pdf"]} />);

    expect(await screen.findByText("No digital signature")).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("pdf_verify_signatures", { path: "/docs/sozlesme.pdf" });
    // The token's remembered name, not the opaque token, is what the user sees.
    const names = await screen.findAllByText("sozlesme.pdf");
    expect(names.length).toBeGreaterThan(0);
  });

  it("shows the signer, certificate and timestamp of a valid signature", async () => {
    report = {
      signatures: [signature({ timestamp: "2026-10-29T10:00:05Z" })],
      warnings: [],
    };
    render(<VerifyScreen initialFiles={["/docs/imzali.pdf"]} />);

    expect(await screen.findByText("Signature verified")).toBeInTheDocument();
    expect(screen.getByText("CN=Örnek İmza Sahibi")).toBeInTheDocument();
    expect(screen.getByText("AA:BB:CC")).toBeInTheDocument();
    expect(screen.getByText("2026-10-29T10:00:00Z")).toBeInTheDocument();
    expect(screen.getByText(/RFC 3161 timestamp/)).toBeInTheDocument();
    expect(screen.getByText("1 of 1 signatures intact")).toBeInTheDocument();
  });

  it("calls out a document whose signed revision changed", async () => {
    report = { signatures: [signature({ modifiedAfterSigning: true })], warnings: [] };
    render(<VerifyScreen initialFiles={["/docs/degismis.pdf"]} />);

    expect(await screen.findByText("Changed after signing")).toBeInTheDocument();
    expect(screen.getByText("Changed")).toBeInTheDocument();
  });

  it("explains an unreadable document instead of calling it unsigned", async () => {
    report = { signatures: [], warnings: ["the document could not be parsed"] };
    render(<VerifyScreen initialFiles={["/docs/bozuk.pdf"]} />);

    expect(await screen.findByText("Could not read the signatures")).toBeInTheDocument();
    expect(screen.getByText("the document could not be parsed")).toBeInTheDocument();
    expect(screen.queryByText("No digital signature")).not.toBeInTheDocument();
  });

  it("keeps the online check off until the setting is on", async () => {
    report = { signatures: [signature()], warnings: [] };
    render(<VerifyScreen initialFiles={["/docs/sozlesme.pdf"]} />);

    expect(await screen.findByText("Signature verified")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Check revocation online" })).toBeDisabled();
    expect(screen.getByText("You can turn the online check on in Settings.")).toBeInTheDocument();
  });

  it("keeps the offline result when the online revocation check fails", async () => {
    report = { signatures: [signature()], warnings: [] };
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS, onlineRevocationCheck: true }, loaded: true });
    onlineError = new Error("the responder did not answer");
    const user = userEvent.setup();
    render(<VerifyScreen initialFiles={["/docs/cevrimici.pdf"]} />);

    expect(await screen.findByText("Signature verified")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Check revocation online" }));

    await waitFor(() => expect(screen.getByText(/the responder did not answer/)).toBeInTheDocument());
    expect(screen.getByText("Online check failed")).toBeInTheDocument();
    // The offline verdict and details stay on screen.
    expect(screen.getByText("Signature verified")).toBeInTheDocument();
  });
});
