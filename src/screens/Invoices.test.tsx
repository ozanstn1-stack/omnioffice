import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const invoice = {
  kind: "writer",
  title: "GIB2026000000001",
  model: { id: "doc", title: "GIB2026000000001", blocks: [] },
  invoice: {
    number: "GIB2026000000001",
    ettn: "7f4e1c2a-0000-4000-8000-000000000001",
    issueDate: "2026-03-14",
    issueTime: "11:42:00",
    typeCode: "SATIS",
    profile: "TICARIFATURA",
    currency: "TRY",
    supplier: {
      name: "Örnek Danışmanlık A.Ş.",
      taxId: "1234567890",
      taxIdScheme: "VKN",
      taxOffice: "Kadıköy",
      address: "",
      city: "İstanbul",
    },
    customer: {
      name: "Alıcı Ltd. Şti.",
      taxId: "9876543210",
      taxIdScheme: "VKN",
      taxOffice: "",
      address: "",
      city: "",
    },
    lines: [
      {
        id: "1",
        name: "Danışmanlık hizmeti",
        quantity: 10,
        unit: "C62",
        unitPrice: 150,
        lineTotal: 1500,
        vatRate: 20,
        vatAmount: 300,
        withholdingRate: 0,
        withholdingAmount: 0,
      },
    ],
    taxSubtotals: [{ rate: 20, taxable: 1500, tax: 300, name: "KDV", exemptionReason: "" }],
    withholdingSubtotals: [],
    lineExtensionAmount: 1500,
    taxExclusiveAmount: 1500,
    taxInclusiveAmount: 1800,
    allowanceTotal: 0,
    taxAmount: 300,
    withholdingAmount: 0,
    payableAmount: 1800,
    notes: [],
  },
  warnings: [],
  issues: [],
};

const invoke = vi.fn(async (command: string, payload?: { paths?: string[]; path?: string }) => {
  switch (command) {
    case "office_vat_rates":
      return [{ rate: 20, label: "Genel oran", since: "2023-07-10", until: "" }];
    case "office_open_invoice":
      return invoice;
    case "office_read_invoice_qr":
      return {
        payload: {
          vknTckn: "1234567890",
          avknTckn: "9876543210",
          senaryo: "TICARIFATURA",
          tip: "SATIS",
          tarih: "2026-03-15",
          no: "GIB2026000000001",
          ettn: "7f4e1c2a-0000-4000-8000-000000000001",
          paraBirimi: "TRY",
          malHizmetToplam: "1500.00",
          vergiDahil: "1800.00",
          odenecek: "1800.00",
          kdvMatrah: { "20": "1500.00" },
          hesaplananKdv: { "20": "300.00" },
        },
        raw: "{}",
        page: 1,
        taxIdValid: true,
        warnings: [],
      };
    case "office_invoice_summary":
      return {
        workbook: { id: "wb", title: "KDV Özeti", sheets: [] },
        warnings: [],
        count: payload?.paths?.length ?? 0,
      };
    case "office_save_document":
    case "office_export_pdf":
      return { path: String(payload?.path ?? ""), warnings: [] };
    default:
      return null;
  }
});

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...(args as [string])) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(async () => null),
  save: vi.fn(async () => "C:/out/KDV-Ozeti.xlsx"),
}));

import { Invoices } from "./Invoices";

describe("invoice screen", () => {
  beforeEach(() => {
    invoke.mockClear();
    localStorage.clear();
  });

  it("shows the parsed invoice fields and its lines", async () => {
    render(<Invoices initialFiles={["C:/invoices/fatura.xml"]} />);
    expect(await screen.findByText("GIB2026000000001")).toBeInTheDocument();
    expect(screen.getByText("Örnek Danışmanlık A.Ş.")).toBeInTheDocument();
    expect(screen.getByText("Alıcı Ltd. Şti.")).toBeInTheDocument();
    expect(screen.getByText("Danışmanlık hizmeti")).toBeInTheDocument();
    // Payable amount is shown with Turkish grouping.
    expect(screen.getByText(/1\.800,00 TRY/)).toBeInTheDocument();
    // A clean invoice reports no issues.
    expect(screen.getByText(/totals, VAT and payable amount match/i)).toBeInTheDocument();
    // The dated rate table is fetched and shown.
    expect(await screen.findByText("Genel oran")).toBeInTheDocument();
  });

  it("reads a PDF's karekod and shows the mismatch with the UBL invoice in red", async () => {
    const user = userEvent.setup();
    render(<Invoices initialFiles={["C:/invoices/fatura.xml", "C:/invoices/fatura.pdf"]} />);
    await screen.findByText("GIB2026000000001");
    await user.click(screen.getByRole("button", { name: /Read karekod/i }));
    // The karekod says 2026-03-15 while the invoice says 2026-03-14.
    expect(await screen.findByText(/The karekod disagrees/i)).toBeInTheDocument();
    const mismatch = screen.getByText(/tarih:/i);
    expect(mismatch).toHaveStyle({ color: "var(--danger, #dc2626)" });
  });

  it("builds the VAT summary workbook and saves it as XLSX", async () => {
    const user = userEvent.setup();
    render(<Invoices initialFiles={["C:/invoices/fatura.xml"]} />);
    await screen.findByText("GIB2026000000001");
    await user.click(screen.getByRole("button", { name: /VAT summary/i }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "office_invoice_summary",
        expect.objectContaining({ paths: ["C:/invoices/fatura.xml"] }),
      ),
    );
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "office_save_document",
        expect.objectContaining({ kind: "calc", path: "C:/out/KDV-Ozeti.xlsx" }),
      ),
    );
  });

  it("hides the vendor accounting exports by default", async () => {
    // Luca/Logo/Zirve exports are experimental: no real import template has
    // verified them (RELEASE_READINESS.md), so no vendor name may appear.
    render(<Invoices initialFiles={["C:/invoices/fatura.xml"]} />);
    await screen.findByText("GIB2026000000001");
    expect(screen.queryByRole("button", { name: /Luca/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Logo/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Zirve/i })).not.toBeInTheDocument();
  });

  it("offers Luca, Logo and Zirve once accountingVendorExport is on", async () => {
    localStorage.setItem("flag:accountingVendorExport", "1");
    render(<Invoices initialFiles={["C:/invoices/fatura.xml"]} />);
    await screen.findByText("GIB2026000000001");
    expect(screen.getByRole("button", { name: /Luca/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Logo/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Zirve/i })).toBeInTheDocument();
  });

  it("opens KDV1 declaration summary modal upon clicking KDV Beyani", async () => {
    const user = userEvent.setup();
    render(<Invoices initialFiles={["C:/invoices/fatura.xml"]} />);
    await screen.findByText("GIB2026000000001");
    const kdvBtn = screen.getByRole("button", { name: /KDV Beyanı/i });
    expect(kdvBtn).toBeInTheDocument();
    await user.click(kdvBtn);
    expect(await screen.findByText("KDV1 Beyannamesi Yardımcı Özeti")).toBeInTheDocument();
  });

  it("detects duplicate invoices and removes duplicates when clicking deduplicate", async () => {
    const user = userEvent.setup();
    // Pass two files that both return the same mocked invoice
    render(<Invoices initialFiles={["C:/invoices/fatura1.xml", "C:/invoices/fatura2.xml"]} />);
    const headers = await screen.findAllByText("GIB2026000000001");
    expect(headers.length).toBeGreaterThan(0);
    expect(await screen.findByText(/1 adet mükerrer fatura tespit edildi/i)).toBeInTheDocument();
    const cleanBtn = screen.getByRole("button", { name: /Mükerrerleri Ayıkla/i });
    expect(cleanBtn).toBeInTheDocument();
    await user.click(cleanBtn);
    expect(screen.queryByText(/mükerrer fatura tespit edildi/i)).not.toBeInTheDocument();
  });
});
