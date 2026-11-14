import { Copy, Download } from "lucide-react";
import { fileBaseName } from "../../lib/format";
import type { AiBatchResult } from "../../lib/types";
import { Badge, Button, Field, TextInput } from "../../components/ui";

export interface AiMatrixPanelProps {
  label: (key: string, fallback: string) => string;
  columnsText: string;
  setColumnsText: (v: string) => void;
  batchResult: AiBatchResult | null;
}

export function parseMatrixColumns(columnsText: string): string[] {
  const cols = columnsText
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);
  return cols.length > 0 ? cols : ["Party", "Effective Date", "Liability Cap", "Termination Notice"];
}

function parseRowOutput(output: string | null | undefined, cols: string[]): Record<string, string> {
  const row: Record<string, string> = {};
  for (const col of cols) row[col] = "—";
  if (!output) return row;
  try {
    const parsed = JSON.parse(output) as Record<string, unknown>;
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
      for (const col of cols) {
        const val = parsed[col];
        row[col] = val !== undefined && val !== null && String(val).trim() !== "" ? String(val) : "—";
      }
      return row;
    }
  } catch {
    // Fallback if non-JSON output was returned
  }
  if (cols.length > 0) row[cols[0]] = output.slice(0, 160);
  return row;
}

function escapeCsvCell(value: string): string {
  const s = value.replace(/\r?\n/g, " ");
  return /[",]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
}

export function AiMatrixPanel({ label, columnsText, setColumnsText, batchResult }: AiMatrixPanelProps) {
  const cols = parseMatrixColumns(columnsText);
  const matrixResults = batchResult?.results.filter((r) => r.action === "matrix") ?? [];

  const copyMarkdown = () => {
    const header = `| ${["Document", ...cols].join(" | ")} |`;
    const sep = `| ${["---", ...cols.map(() => "---")].join(" | ")} |`;
    const rows = matrixResults.map((res) => {
      const parsed = parseRowOutput(res.output, cols);
      return `| ${[fileBaseName(res.path), ...cols.map((c) => parsed[c].replace(/\|/g, "\\|"))].join(" | ")} |`;
    });
    void navigator.clipboard?.writeText([header, sep, ...rows].join("\n"));
  };

  const exportCsv = () => {
    const header = ["Document", ...cols].map(escapeCsvCell).join(",");
    const rows = matrixResults.map((res) => {
      const parsed = parseRowOutput(res.output, cols);
      return [fileBaseName(res.path), ...cols.map((c) => parsed[c])].map(escapeCsvCell).join(",");
    });
    const blob = new Blob([[header, ...rows].join("\r\n")], { type: "text/csv;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "comparison-matrix.csv";
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <div className="flex flex-col gap-3">
      <Field
        label={label("ai.matrixColumns", "Comparison Columns")}
        hint={label("ai.matrixColumnsHint", "Comma-separated criteria to extract across every queued document")}
      >
        <TextInput
          value={columnsText}
          onChange={(e) => setColumnsText(e.target.value)}
          placeholder="Party, Effective Date, Liability Cap, Termination Notice"
        />
      </Field>

      <div className="flex flex-wrap gap-1.5">
        <Button
          size="sm"
          variant="ghost"
          onClick={() => setColumnsText("Party, Effective Date, Term, Liability Cap, Termination Notice")}
        >
          {label("ai.matrixPresetContract", "Contract Review")}
        </Button>
        <Button
          size="sm"
          variant="ghost"
          onClick={() => setColumnsText("Vendor, Invoice #, Date, Currency, Subtotal, Tax, Total")}
        >
          {label("ai.matrixPresetInvoice", "Invoice Audit")}
        </Button>
      </div>

      {matrixResults.length > 0 ? (
        <div className="flex flex-col gap-2">
          <div className="flex items-center justify-between flex-wrap gap-2">
            <span className="text-xs font-semibold">
              {label("ai.matrixTitle", "Comparison Matrix")} ({matrixResults.length})
            </span>
            <div className="flex gap-1.5">
              <Button size="sm" variant="ghost" icon={<Copy size={13} />} onClick={copyMarkdown}>
                {label("ai.matrixCopyMd", "Copy Markdown")}
              </Button>
              <Button size="sm" variant="ghost" icon={<Download size={13} />} onClick={exportCsv}>
                {label("ai.matrixExportCsv", "Export CSV")}
              </Button>
            </div>
          </div>

          <div className="card-soft overflow-x-auto max-h-[320px]">
            <table className="w-full text-xs text-left border-collapse">
              <thead>
                <tr className="border-b border-[var(--border)] bg-[var(--surface-3)]">
                  <th className="p-2 font-semibold">{label("ai.matrixDocCol", "Document")}</th>
                  {cols.map((col) => (
                    <th key={col} className="p-2 font-semibold">
                      {col}
                    </th>
                  ))}
                  <th className="p-2 font-semibold">{label("ai.matrixStatusCol", "Status")}</th>
                </tr>
              </thead>
              <tbody>
                {matrixResults.map((res, idx) => {
                  const row = parseRowOutput(res.output, cols);
                  return (
                    <tr key={idx} className="border-b border-[var(--border)] hover:bg-[var(--surface-2)]">
                      <td className="p-2 font-medium max-w-[160px] truncate" title={res.path}>
                        {fileBaseName(res.path)}
                      </td>
                      {cols.map((col) => (
                        <td key={col} className="p-2">
                          {res.success ? row[col] : "—"}
                        </td>
                      ))}
                      <td className="p-2">
                        <Badge tone={res.success ? "ok" : "danger"}>
                          {res.success ? label("ai.statusSuccess", "Success") : res.error || "Error"}
                        </Badge>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </div>
      ) : null}
    </div>
  );
}
