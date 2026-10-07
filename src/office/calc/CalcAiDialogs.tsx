/**
 * Calc AI dialogs: "Summarize column" (Copy / Insert below) and "Suggest
 * formula" (inserted into the active cell only on Accept).
 */
import { useState } from "react";
import { Copy } from "lucide-react";
import { useT } from "../../lib/i18n";
import { useSettings, useToasts } from "../../lib/store";
import { AiAssistDialog, aiLanguageName, copyText, type AiStatus } from "../ai/editor-ai";

export function SummarizeColumnDialog({
  docId,
  status,
  column,
  lines,
  onInsert,
  onClose,
}: {
  docId: string;
  status: AiStatus;
  /** Column letter, for the texts. */
  column: string;
  /** The values that are sent, one per line. */
  lines: string[];
  onInsert: (summary: string) => void;
  onClose: () => void;
}) {
  const t = useT();
  const uiLanguage = useSettings((state) => state.settings.language);
  const text = lines.join("\n");
  return (
    <AiAssistDialog
      title={t("ai.edit.summarizeColumnTitle")}
      docId={docId}
      status={status}
      category="column"
      sends={t("ai.edit.sendsColumn", { count: lines.length, column, chars: text.length })}
      autoRun
      streamPreview
      buildRequest={() => ({
        task: "summarize_column",
        text,
        options: { language: aiLanguageName(uiLanguage) },
      })}
      renderResult={(reply) => (
        <div className="ai-dlg-pane">
          <pre className="ai-dlg-text" data-testid="ai-suggestion">
            {reply.text}
          </pre>
        </div>
      )}
      extraActions={(reply) => (
        <button
          type="button"
          className="btn"
          onClick={() =>
            void copyText(reply.text).then((ok) =>
              useToasts
                .getState()
                .push(
                  ok
                    ? { kind: "success", title: t("ai.edit.copied") }
                    : { kind: "error", title: t("ai.edit.copyFailed") },
                ),
            )
          }
        >
          <Copy size={13} aria-hidden /> {t("ai.edit.copy")}
        </button>
      )}
      acceptLabel={t("ai.edit.insertBelow")}
      onAccept={(reply) => onInsert(reply.text)}
      onClose={onClose}
    />
  );
}

export function SuggestFormulaDialog({
  docId,
  status,
  cell,
  headers,
  selection,
  onAccept,
  onClose,
}: {
  docId: string;
  status: AiStatus;
  /** Address the formula goes into on Accept. */
  cell: string;
  /** "A: Name; B: Amount" */
  headers: string;
  /** Selection address as shown in the name box. */
  selection: string;
  onAccept: (formula: string) => void;
  onClose: () => void;
}) {
  const t = useT();
  const [request, setRequest] = useState("");
  return (
    <AiAssistDialog
      title={t("ai.edit.suggestFormulaTitle")}
      docId={docId}
      status={status}
      category="headers"
      sends={t("ai.edit.sendsFormula")}
      form={
        <label>
          <span>{t("ai.edit.formulaRequest")}</span>
          <textarea
            value={request}
            onChange={(event) => setRequest(event.target.value)}
            placeholder={t("ai.edit.formulaPlaceholder")}
            maxLength={2000}
          />
        </label>
      }
      canRun={request.trim().length > 0}
      buildRequest={() => ({
        task: "suggest_formula",
        text: request.trim(),
        options: {
          context: `Headers (row 1): ${headers || "none"}\nSelected cells: ${selection}\nActive cell: ${cell}`,
        },
      })}
      renderResult={(reply) => (
        <div className="ai-dlg-pane">
          <div className="ai-dlg-label">{t("ai.edit.formulaFor", { cell })}</div>
          <pre className="ai-dlg-text" data-testid="ai-suggestion">
            {reply.text}
          </pre>
        </div>
      )}
      acceptLabel={t("ai.edit.formulaAccept", { cell })}
      onAccept={(reply) => onAccept(reply.text)}
      onClose={onClose}
    />
  );
}
