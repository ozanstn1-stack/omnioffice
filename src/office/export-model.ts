import type { OfficeKind } from "../lib/office-types";

/**
 * The model handed to a non-native save. A spreadsheet gets the ranges its
 * dynamic arrays spill into, so the XLSX writer can mark them for Excel (4.2b);
 * the Calc evaluator is loaded only when a spreadsheet is saved.
 */
export async function exportModel(kind: OfficeKind, model: unknown): Promise<unknown> {
  if (kind !== "calc") return model;
  const { withSpillRanges } = await import("./calc/spill-export");
  return withSpillRanges(model as Parameters<typeof withSpillRanges>[0]);
}
