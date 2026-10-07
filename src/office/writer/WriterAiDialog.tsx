/**
 * Preview dialog for the Writer AI actions (rewrite, shorten, expand, fix,
 * translate, change tone) on the selection or the current paragraph.
 */
import { useState } from "react";
import { useT } from "../../lib/i18n";
import { useSettings } from "../../lib/store";
import type { AiEditResult, AiEditTask } from "../../lib/types";
import { AI_LANGUAGES, AI_TONES, AiAssistDialog, aiLanguageName, type AiStatus, type AiTone } from "../ai/editor-ai";

export type WriterAiTask = Extract<AiEditTask, "rewrite" | "shorten" | "expand" | "fix" | "translate" | "tone">;

export function WriterAiDialog({
  task,
  docId,
  status,
  original,
  wholeParagraph,
  onAccept,
  onClose,
}: {
  task: WriterAiTask;
  docId: string;
  status: AiStatus;
  original: string;
  /** True when no text was selected and the current paragraph is used. */
  wholeParagraph: boolean;
  onAccept: (result: AiEditResult) => void;
  onClose: () => void;
}) {
  const t = useT();
  const uiLanguage = useSettings((state) => state.settings.language);
  const [language, setLanguage] = useState<string>(uiLanguage);
  const [tone, setTone] = useState<AiTone>("formal");

  const form =
    task === "translate" ? (
      <label>
        <span>{t("ai.edit.targetLanguage")}</span>
        <select value={language} onChange={(event) => setLanguage(event.target.value)}>
          {AI_LANGUAGES.map((option) => (
            <option key={option.code} value={option.code}>
              {option.label}
            </option>
          ))}
        </select>
      </label>
    ) : task === "tone" ? (
      <label>
        <span>{t("ai.edit.tone")}</span>
        <select value={tone} onChange={(event) => setTone(event.target.value as AiTone)}>
          {AI_TONES.map((value) => (
            <option key={value} value={value}>
              {t(`ai.edit.tone.${value}`)}
            </option>
          ))}
        </select>
      </label>
    ) : undefined;

  const chars = original.length;
  return (
    <AiAssistDialog
      title={t(`ai.edit.${task}Title`)}
      docId={docId}
      status={status}
      sends={t(wholeParagraph ? "ai.edit.sendsParagraph" : "ai.edit.sendsSelection", { chars })}
      form={form}
      autoRun={form === undefined}
      streamPreview
      original={original}
      buildRequest={() => ({
        task,
        text: original,
        options: task === "translate" ? { language: aiLanguageName(language) } : task === "tone" ? { tone } : undefined,
      })}
      acceptLabel={t("ai.edit.accept")}
      onAccept={onAccept}
      onClose={onClose}
    />
  );
}
