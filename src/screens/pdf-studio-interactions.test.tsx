import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const pdfInfo = {
  path: "C:/docs/a.pdf",
  fileName: "a.pdf",
  fileSizeBytes: 2048,
  pageCount: 2,
  pdfVersion: "1.7",
  encrypted: false,
  hasTextLayer: true,
  metadata: {
    title: "",
    author: "",
    subject: "",
    keywords: "",
    creator: "",
    producer: "",
    creation_date: "",
    mod_date: "",
  },
  pageGeometries: [
    { page: 1, width_pt: 595.28, height_pt: 841.89, display_width_pt: 595.28, display_height_pt: 841.89, rotation: 0 },
    { page: 2, width_pt: 595.28, height_pt: 841.89, display_width_pt: 595.28, display_height_pt: 841.89, rotation: 0 },
  ],
  imageCount: 1,
  title: "",
  author: "",
  producer: "",
};

const sampleSignature = {
  fieldName: "Sig1",
  subFilter: "adbe.pkcs7.detached",
  coversWholeDocument: true,
  supersededByLaterRevision: false,
  modifiedAfterSigning: false,
  digestMatches: true,
  signatureValid: true,
  chain: [],
  chainLinked: true,
  selfSignedChain: true,
  signer: {
    subject: "CN=Ayşe Signer",
    issuer: "CN=Test CA",
    serialHex: "01",
    notBefore: "2026-01-01T00:00:00Z",
    notAfter: "2027-01-01T00:00:00Z",
    expired: false,
    isCa: false,
    sha256Fingerprint: "AA:BB",
  },
  signingTime: null,
  timestamp: null,
  algorithm: "RSA-SHA256",
  trust: "unknown",
  revocation: { status: "not_checked" },
  notes: [],
};

const makeField = (overrides: Record<string, unknown>) => ({
  flags: 0,
  required: false,
  readOnly: false,
  value: "",
  values: [],
  defaultValue: "",
  tooltip: null,
  maxLength: null,
  multiline: false,
  password: false,
  comb: false,
  combo: false,
  editable: false,
  multiSelect: false,
  options: [],
  page: 1,
  rect: [40, 600, 240, 624],
  tabOrder: null,
  widgetCount: 1,
  hasScript: false,
  ...overrides,
});

const makeObject = (overrides: Record<string, unknown>) => ({
  page: 1,
  id: null,
  matrix: null,
  resourceName: null,
  fieldName: null,
  contents: "",
  flags: 0,
  hidden: false,
  tabOrder: null,
  ...overrides,
});

const makeTextRun = (overrides: Record<string, unknown>) => ({
  page: 1,
  fontSizePt: 12,
  x: 72,
  y: 720,
  widthPt: 80,
  renderMode: 0,
  editable: true,
  note: null,
  ...overrides,
});

let failOnlineRevocation = false;
let formFieldsFixture: Record<string, unknown>[] = [];
let pageObjectsFixture: Record<string, unknown>[] = [];
let validationIssuesFixture: Record<string, unknown>[] = [];
let textRunsFixture: Record<string, unknown>[] = [];

const invoke = vi.fn(async (command: string, _payload?: unknown) => {
  switch (command) {
    case "pdf_info":
      return pdfInfo;
    case "page_preview":
    case "page_thumbnail":
      return { dataUrl: "data:image/png;base64,iVBORw0KGgo=", width: 595, height: 842 };
    case "file_sizes":
      return [2048];
    case "suggest_output":
      return { token: "C:/docs/out.pdf", path: "C:/docs/out.pdf" };
    case "file_token_pick":
      return [{ token: "tok-pfx", name: "signer.p12" }];
    case "file_token_save":
      return [{ token: "tok-out", name: "signed.pdf", path: "C:/docs/signed.pdf" }];
    case "pdf_list_signing_certificates":
      return [];
    case "pdf_verify_signatures":
      return { signatures: [sampleSignature], warnings: [] };
    case "pdf_verify_signatures_online":
      if (failOnlineRevocation) throw new Error("network down");
      return { signatures: [sampleSignature], warnings: [] };
    case "pdf_archive_validation_data":
      return { signatures: 1, certificates: 2, vriKeys: ["ABC"], warnings: [] };
    case "pdf_sign":
      return { output: "C:/docs/signed.pdf", signature: sampleSignature };
    case "pdf_list_form_fields":
      return formFieldsFixture;
    case "pdf_list_objects":
      return pageObjectsFixture;
    case "pdf_validate_form":
      return validationIssuesFixture;
    case "pdf_fill_form":
      return { filled: 2, skipped: [], warnings: [] };
    case "flatten_pdf":
      return { annotationsFlattened: 0, fieldsFlattened: 2, pagesTouched: 1, warnings: [] };
    case "pdf_edit_objects":
      return { edited: 1, deleted: 1, warnings: [] };
    case "pdf_list_text_runs":
      return textRunsFixture;
    case "pdf_edit_text_runs":
      return { edited: 1, warnings: [] };
    default:
      return null;
  }
});

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...(args as [string])) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/api/path", () => ({
  documentDir: vi.fn(async () => "C:/docs"),
  appDataDir: vi.fn(async () => "C:/appdata"),
  join: vi.fn(async (...parts: string[]) => parts.join("/")),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(async () => null),
  save: vi.fn(async () => "C:/docs/out.pdf"),
}));

if (typeof window.PointerEvent === "undefined") {
  window.PointerEvent = MouseEvent as unknown as typeof PointerEvent;
}
if (!HTMLElement.prototype.setPointerCapture) {
  HTMLElement.prototype.setPointerCapture = () => undefined;
}

import { PdfStudio } from "./PdfStudio";
import { useSettings, useToasts } from "../lib/store";
import { DEFAULT_SETTINGS } from "../lib/types";

async function renderStudio() {
  const user = userEvent.setup();
  render(<PdfStudio dragging={false} initialFiles={["C:/docs/a.pdf"]} />);
  await screen.findByText("2.00 KB");
  return user;
}

describe("PdfStudio interactions", () => {
  beforeEach(() => {
    invoke.mockClear();
    failOnlineRevocation = false;
    formFieldsFixture = [];
    pageObjectsFixture = [];
    validationIssuesFixture = [];
    textRunsFixture = [];
    useToasts.setState({ toasts: [] });
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS, language: "en", onlineRevocationCheck: false } });
  });

  it("verifies signatures offline, falls back on online failure, and archives DSS", async () => {
    const user = await renderStudio();

    await user.click(screen.getByRole("button", { name: "Signatures" }));
    await user.click(screen.getByRole("button", { name: "Verify signatures" }));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("pdf_verify_signatures", { path: "C:/docs/a.pdf" }));
    expect(await screen.findByText(/CN=Ayşe Signer/)).toBeInTheDocument();

    failOnlineRevocation = true;
    act(() => {
      useSettings.setState({ settings: { ...DEFAULT_SETTINGS, language: "en", onlineRevocationCheck: true } });
    });
    await user.click(screen.getByRole("button", { name: "Verify signatures" }));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("pdf_verify_signatures_online", { path: "C:/docs/a.pdf" }));
    expect(useToasts.getState().toasts.map((toast) => toast.title)).toContain(
      "The online revocation check could not run; showing the offline result.",
    );

    await user.click(screen.getByRole("button", { name: "Archive validation data" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("pdf_archive_validation_data", { input: "C:/docs/a.pdf", output: null }),
    );
    expect(await screen.findByText(/certificates stored: 2/)).toBeInTheDocument();
  });

  it("picks a PFX certificate, signs via pdf_sign, and clears the password from state", async () => {
    const user = await renderStudio();

    await user.click(screen.getByRole("button", { name: "Signatures" }));
    await user.click(screen.getByRole("button", { name: "Choose PFX/P12…" }));
    expect(await screen.findByText("signer.p12")).toBeInTheDocument();

    const passwordInput = document.querySelector<HTMLInputElement>('input[type="password"]')!;
    await user.type(passwordInput, "s3cr3t!");
    await user.click(screen.getByRole("button", { name: "Sign and save as…" }));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "pdf_sign",
        expect.objectContaining({
          input: "C:/docs/a.pdf",
          output: "tok-out",
          pfxPath: "tok-pfx",
          pfxPassword: "s3cr3t!",
        }),
      ),
    );
    expect(passwordInput.value).toBe("");
    expect(useToasts.getState().toasts.map((toast) => toast.title)).toContain("Document signed");
  });

  it("fills checkbox and multi-select form fields, validates errors, and flattens on fill", async () => {
    formFieldsFixture = [
      makeField({ name: "fullName", fieldType: "text", required: true }),
      makeField({ name: "agree", fieldType: "checkbox", value: "Off", defaultValue: "Off", rect: [40, 560, 56, 576] }),
      makeField({
        name: "roles",
        fieldType: "choice",
        multiSelect: true,
        options: [
          { value: "admin", label: "Admin" },
          { value: "editor", label: "Editor" },
        ],
        rect: [40, 480, 180, 540],
      }),
    ];
    validationIssuesFixture = [{ field: "fullName", code: "required", severity: "error", message: "missing" }];

    const user = await renderStudio();

    await user.click(screen.getByRole("button", { name: "Forms & objects" }));
    await user.click(await screen.findByRole("checkbox", { name: "Yes / Off" }));

    const multiSelect = document.querySelector<HTMLSelectElement>("select[multiple]")!;
    await user.selectOptions(multiSelect, ["admin", "editor"]);

    await user.click(screen.getByRole("button", { name: "Validate" }));
    expect(await screen.findByText(/Required field is empty — missing/)).toBeInTheDocument();

    await user.click(screen.getByRole("checkbox", { name: "Flatten after fill" }));
    await user.click(screen.getByRole("button", { name: "Fill and save as…" }));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("flatten_pdf", expect.anything()));
    expect(invoke).toHaveBeenCalledWith("pdf_fill_form", {
      request: {
        input: "C:/docs/a.pdf",
        output: { path: "tok-out", overwrite: "replace" },
        values: [
          { name: "fullName", value: "", values: [] },
          { name: "agree", value: "true", values: [] },
          { name: "roles", value: "", values: ["admin", "editor"] },
        ],
        password: null,
      },
    });
  });

  it("rotates and deletes page objects with singular and plural pending counts", async () => {
    pageObjectsFixture = [
      makeObject({
        index: 0,
        kind: "image",
        subtype: "Image",
        rect: [40, 500, 160, 620],
        matrix: [120, 0, 0, 120, 40, 500],
        resourceName: "Im0",
      }),
      makeObject({
        index: 1,
        kind: "annotation",
        subtype: "Text",
        id: "5 0",
        rect: [200, 500, 260, 540],
        contents: "Sticky",
      }),
    ];

    const user = await renderStudio();

    await user.click(screen.getByRole("button", { name: "Forms & objects" }));
    await user.click(await screen.findByRole("button", { name: "Image Im0" }));
    await user.click(screen.getByRole("button", { name: "Rotate -90°" }));
    expect(screen.getByText("1 pending edit")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /Annotation · Text/ }));
    await user.click(screen.getByRole("button", { name: "Rotate +90°" }));
    expect(screen.getByText("2 pending edits")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Delete object" }));
    await user.click(screen.getByRole("button", { name: "Apply edits and save as…" }));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("pdf_edit_objects", expect.anything()));
    expect(invoke).toHaveBeenCalledWith("pdf_edit_objects", {
      request: {
        input: "C:/docs/a.pdf",
        output: { path: "tok-out", overwrite: "replace" },
        edits: [
          { page: 1, index: 0, action: "rotate", degrees: -90 },
          { page: 1, index: 1, action: "delete" },
        ],
        password: null,
      },
    });
  });

  it("blocks edits on read-only text runs and applies editable run replacements", async () => {
    textRunsFixture = [
      makeTextRun({ index: 0, text: "Draft title", font: "F1", fontSizePt: 14 }),
      makeTextRun({
        index: 1,
        text: "Locked glyph",
        font: "F2",
        fontSizePt: 11,
        y: 700,
        widthPt: 90,
        editable: false,
        note: "Identity-H font",
      }),
    ];

    const user = await renderStudio();

    await user.click(screen.getByRole("button", { name: "Text" }));
    await user.click(await screen.findByRole("button", { name: /Locked glyph/ }));

    const lockedInput = screen.getByDisplayValue("Locked glyph");
    await user.clear(lockedInput);
    await user.type(lockedInput, "Edited glyph");
    expect(screen.getByRole("button", { name: "Apply text" })).toBeDisabled();

    await user.click(screen.getByRole("button", { name: /Draft title/ }));
    const editableInput = screen.getByDisplayValue("Draft title");
    await user.clear(editableInput);
    await user.type(editableInput, "Final title");

    const applyButton = screen.getByRole("button", { name: "Apply text" });
    expect(applyButton).toBeEnabled();
    await user.click(applyButton);

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("pdf_edit_text_runs", expect.anything()));
    expect(invoke).toHaveBeenCalledWith("pdf_edit_text_runs", {
      request: {
        input: "C:/docs/a.pdf",
        output: { path: "tok-out", overwrite: "replace" },
        edits: [{ page: 1, index: 0, text: "Final title" }],
        password: null,
      },
    });
  });
});
