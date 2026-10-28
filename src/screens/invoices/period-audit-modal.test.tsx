import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { UblInvoice, UblParty } from "../../lib/office-api";
import type { Workbook } from "../../lib/office-types";

const invoke = vi.fn(async (_command: string, _payload?: unknown) => ({ path: "/out/audit.xlsx" }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invoke(...(args as [string, unknown])),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("../../lib/file-tokens", () => ({
  pickSaveToken: async (name: string) => ({ token: "save-token", name, path: `/out/${name}` }),
}));

import { PeriodAuditModal } from "./period-audit-modal";
import { periodAuditWorkbook } from "../../lib/period-audit-workbook";
import { auditPeriod } from "../../lib/period-audit";
import { useSettings } from "../../lib/store";
import { DEFAULT_SETTINGS } from "../../lib/types";

const OWN = "1234567890";
const party = (taxId: string): UblParty => ({
  name: `P${taxId}`,
  taxId,
  taxIdScheme: "",
  taxOffice: "",
  address: "",
  city: "",
});

function invoice(
  number: string,
  supplier: string,
  customer: string,
  taxable: number,
  vat: number,
  date = "2026-09-10",
): UblInvoice {
  return {
    number,
    ettn: `ettn-${number}`,
    issueDate: date,
    issueTime: "",
    typeCode: "SATIS",
    profile: "TICARIFATURA",
    currency: "TRY",
    supplier: party(supplier),
    customer: party(customer),
    lines: [],
    taxSubtotals: [{ rate: 20, taxable, tax: vat, name: "KDV", exemptionReason: "" }],
    withholdingSubtotals: [],
    lineExtensionAmount: taxable,
    taxExclusiveAmount: taxable,
    taxInclusiveAmount: taxable + vat,
    allowanceTotal: 0,
    taxAmount: vat,
    withholdingAmount: 0,
    payableAmount: taxable + vat,
    notes: [],
  };
}

const invoices = [
  invoice("ABC2026000000001", OWN, "10000000146", 1000, 200),
  invoice("ABC2026000000003", OWN, "10000000146", 3000, 600, "2026-09-12"),
  invoice("ZZZ2026000000007", "10000000146", OWN, 500, 100),
  invoice("ABC2026000000009", OWN, "10000000146", 50, 10, "2026-08-30"),
];

describe("period close audit", () => {
  beforeEach(() => {
    invoke.mockClear();
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS, language: "en" }, loaded: true });
  });

  it("opens on the busiest month, lists the findings and compares the declaration", async () => {
    const user = userEvent.setup();
    render(<PeriodAuditModal invoices={invoices} ownTaxId={OWN} onClose={vi.fn()} />);
    expect(screen.getByLabelText(/^Period$/)).toHaveValue("2026-09");
    expect(screen.getByText(/Gap in the sales numbering/)).toBeInTheDocument();
    expect(screen.getByText(/ABC2026000000002/)).toBeInTheDocument();
    expect(screen.getByText(/Dated outside the period/)).toBeInTheDocument();

    await user.type(screen.getByLabelText(/Declared deductible VAT/), "90");
    expect(screen.getByText(/does not match the declaration/)).toBeInTheDocument();
    expect(screen.getByRole("cell", { name: "10,00" })).toBeInTheDocument();
  });

  it("exports the audit as a workbook", async () => {
    const user = userEvent.setup();
    render(<PeriodAuditModal invoices={invoices} ownTaxId={OWN} onClose={vi.fn()} />);
    await user.click(screen.getByRole("button", { name: /Export to Excel/ }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("office_save_document", expect.anything()));
    const payload = invoke.mock.calls.find(([command]) => command === "office_save_document")?.[1] as {
      kind: string;
      path: string;
      model: Workbook;
    };
    expect(payload.kind).toBe("calc");
    expect(payload.path).toBe("save-token");
    expect(payload.model.sheets.map((sheet) => sheet.name)).toEqual(["Summary", "Findings", "Invoices"]);
  });
});

describe("audit workbook", () => {
  it("writes the period's invoices with their side and the findings", () => {
    const t = (key: string) => key;
    const report = auditPeriod({ invoices, period: "2026-09", ownTaxId: OWN, declared: { salesVat: 800 } });
    const workbook = periodAuditWorkbook(report, invoices, OWN, t);
    const [summary, findings, rows] = workbook.sheets;
    expect(summary.cells.B2.value).toEqual({ kind: "text", value: "2026-09" });
    expect(summary.cells.B8.value).toEqual({ kind: "number", value: 800 });
    expect(findings.cells.B2.value).toEqual({ kind: "text", value: expect.stringMatching(/^periodAudit\.code\./) });
    // Header plus the three September invoices; the August one is left out.
    expect(Object.keys(rows.cells).filter((address) => /^A\d+$/.test(address))).toHaveLength(4);
    expect(rows.cells.C4.value).toEqual({ kind: "text", value: "periodAudit.side.purchase" });
    expect(rows.cells.H2.value).toEqual({ kind: "number", value: 1000 });
  });
});
