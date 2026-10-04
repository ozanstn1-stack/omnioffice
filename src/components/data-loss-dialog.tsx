/**
 * The Data Loss Protection gate shown before a lossy save or export.
 *
 * The backend `office_compatibility` report is the only source of truth: this
 * dialog renders the exact rows that report returned (with the matrix columns
 * derived from each row's status) and offers the lossless `.oswk` master as
 * the alternative to writing a file that drops data.
 *
 * `useDataLossPrompt` follows the same promise-based prompt pattern as
 * `useOverwritePrompt`; `DataLossDialogHost` is mounted once by the office
 * workspace, so the save flows in `useOfficeSession` can await an answer
 * without rendering UI themselves.
 */
import { create } from "zustand";
import { Badge, Button, Modal } from "./ui";
import { compatibilitySummary, countLosses, lossRowFlags, YesNo, type CompatibilityReport } from "./compatibility";
import { useT } from "../lib/i18n";

export type DataLossChoice = "continue" | "cancel" | "oswk";

export function DataLossDialog({
  target,
  report,
  onChoose,
}: {
  target: string;
  report: CompatibilityReport;
  onChoose: (choice: DataLossChoice) => void;
}) {
  const t = useT();
  const counts = countLosses(report);
  return (
    <Modal
      title={t("loss.title")}
      onClose={() => onChoose("cancel")}
      width={760}
      footer={
        <>
          <Button variant="ghost" onClick={() => onChoose("cancel")}>
            {t("loss.cancel")}
          </Button>
          <Button variant="primary" onClick={() => onChoose("oswk")}>
            {t("loss.saveOs")}
          </Button>
          <Button variant="danger" onClick={() => onChoose("continue")}>
            {t("loss.continue")}
          </Button>
        </>
      }
    >
      <p className="text-[13.5px] mb-3">{t("loss.body", { target: target.toUpperCase() })}</p>
      <div className="flex items-center gap-2 flex-wrap mb-3">
        <Badge tone={counts.lost > 0 ? "danger" : "warn"}>{compatibilitySummary(report, t)}</Badge>
      </div>
      <div className="overflow-auto">
        <table className="w-full text-xs">
          <thead>
            <tr className="text-left muted">
              <th className="py-1 pr-3 font-medium">{t("loss.feature")}</th>
              <th className="py-1 px-3 font-medium">{t("loss.supported")}</th>
              <th className="py-1 px-3 font-medium">{t("loss.imported")}</th>
              <th className="py-1 px-3 font-medium">{t("loss.exported")}</th>
              <th className="py-1 px-3 font-medium">{t("loss.transformed")}</th>
              <th className="py-1 pl-3 font-medium">{t("loss.lost")}</th>
            </tr>
          </thead>
          <tbody>
            {report.items.map((item, index) => {
              const flags = lossRowFlags(item);
              return (
                <tr
                  key={`${item.feature}-${index}`}
                  className="border-t align-top"
                  style={{ borderColor: "var(--border)" }}
                >
                  <td className="py-1.5 pr-3">
                    <p className="text-[13px]" style={{ color: "var(--text-1)" }}>
                      {item.feature}
                    </p>
                    <p className="muted">{item.message}</p>
                  </td>
                  <td className="py-1.5 px-3">
                    <YesNo value={flags.supported} />
                  </td>
                  <td className="py-1.5 px-3">
                    <YesNo value={flags.imported} />
                  </td>
                  <td className="py-1.5 px-3">
                    <YesNo value={flags.exported} />
                  </td>
                  <td className="py-1.5 px-3">
                    <YesNo value={flags.transformed} />
                  </td>
                  <td className="py-1.5 pl-3">
                    <YesNo value={flags.lost} />
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      <p className="text-xs muted mt-3">{t("loss.columnHint")}</p>
      <p className="text-xs muted mt-1">{t("loss.nativeNote")}</p>
    </Modal>
  );
}

interface DataLossPromptState {
  request: { target: string; report: CompatibilityReport; resolve: (choice: DataLossChoice) => void } | null;
  ask: (target: string, report: CompatibilityReport) => Promise<DataLossChoice>;
  answer: (choice: DataLossChoice) => void;
}

export const useDataLossPrompt = create<DataLossPromptState>((set, get) => ({
  request: null,
  ask: (target, report) =>
    new Promise<DataLossChoice>((resolve) => {
      // The save flows gate on their `busy` flag, so at most one question is
      // usually open; if a second one arrives, the older one is settled as
      // cancel rather than left hanging.
      const previous = get().request;
      if (previous) previous.resolve("cancel");
      set({ request: { target, report, resolve } });
    }),
  answer: (choice) => {
    const current = get().request;
    if (current) {
      current.resolve(choice);
      set({ request: null });
    }
  },
}));

/** Mount once (office workspace); renders the open question, if any. */
export function DataLossDialogHost() {
  const request = useDataLossPrompt((state) => state.request);
  const answer = useDataLossPrompt((state) => state.answer);
  if (!request) return null;
  return <DataLossDialog target={request.target} report={request.report} onChoose={answer} />;
}
