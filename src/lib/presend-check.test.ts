import { describe, expect, it } from "vitest";
import { canClean, presendFindings, type PresendInput, type PresendInspection } from "./presend-check";

const clean: PresendInspection = {
  annotations: [],
  embeddedFiles: [],
  attachmentCount: 0,
  hasJavascript: false,
  javascriptEntries: [],
  hasOpenAction: false,
  formFields: [],
  authorOverride: "",
  subjectOverride: "",
  creator: "",
  producer: "",
};

const input = (patch: Partial<PresendInput> = {}, inspection: Partial<PresendInspection> = {}): PresendInput => ({
  inspection: { ...clean, ...inspection },
  signatures: [],
  text: "",
  pdfa: null,
  ...patch,
});

const codes = (value: PresendInput) => presendFindings(value).map((finding) => finding.code);

describe("pre-send check", () => {
  it("finds nothing in a plain unsigned PDF", () => {
    expect(presendFindings(input({ text: "Sayın ilgili, toplantı yarın saat 10'da." }))).toEqual([]);
  });

  it("stops on a valid TCKN or IBAN and counts each kind without echoing the values", () => {
    const findings = presendFindings(
      input({ text: "TCKN 10000000146, IBAN TR33 0006 1005 1978 6457 8412 01, tel 0532 123 45 67" }),
    );
    const personal = findings.find((finding) => finding.code === "personalData");
    expect(personal?.severity).toBe("stop");
    expect(personal?.count).toBe(3);
    expect(personal?.items).toEqual(["tckn:1", "iban:1", "phone:1"]);
    expect(JSON.stringify(findings)).not.toContain("10000000146");
  });

  it("only warns for phone numbers and e-mail addresses", () => {
    const personal = presendFindings(input({ text: "info@example.com" })).find(
      (finding) => finding.code === "personalData",
    );
    expect(personal?.severity).toBe("warn");
  });

  it("reports notes, hidden annotations, attachments, scripts and metadata", () => {
    const found = codes(
      input(
        {},
        {
          annotations: [
            { page: 2, subtype: "Text", contents: "Müvekkile gösterme", hidden: false },
            { page: 3, subtype: "Square", contents: "", hidden: true },
            { page: 1, subtype: "Link", contents: "", hidden: false },
          ],
          embeddedFiles: ["taslak.docx"],
          attachmentCount: 1,
          hasJavascript: true,
          javascriptEntries: ["OpenAction"],
          authorOverride: "Av. Ayşe",
          producer: "Microsoft Word",
        },
      ),
    );
    expect(found).toEqual(["comments", "hiddenAnnotations", "attachments", "javascript", "metadata"]);
  });

  it("stops on a broken signature and warns when a signed file changed later", () => {
    const findings = presendFindings(
      input({
        signatures: [
          { fieldName: "Sig1", signatureValid: false, digestMatches: true, modifiedAfterSigning: false },
          { fieldName: "Sig2", signatureValid: true, digestMatches: true, modifiedAfterSigning: true },
        ],
      }),
    );
    expect(findings[0]).toMatchObject({ code: "signatureBroken", severity: "stop", items: ["Sig1"] });
    expect(findings.map((finding) => finding.code)).toContain("changedAfterSigning");
  });

  it("lists the worst findings first", () => {
    const found = presendFindings(
      input({ text: "TCKN 10000000146", pdfa: { valid: false } }, { producer: "Word", embeddedFiles: ["a.xml"] }),
    ).map((finding) => finding.severity);
    expect(found).toEqual(["stop", "warn", "info", "info"]);
  });

  it("offers cleaning only for an unsigned file with something the sanitizer removes", () => {
    const dirty = presendFindings(input({}, { producer: "Word" }));
    expect(canClean(dirty)).toBe(true);
    const signed = presendFindings(
      input(
        { signatures: [{ fieldName: "Sig1", signatureValid: true, digestMatches: true, modifiedAfterSigning: false }] },
        { producer: "Word" },
      ),
    );
    expect(canClean(signed)).toBe(false);
    expect(canClean(presendFindings(input({ text: "10000000146" })))).toBe(false);
  });
});
