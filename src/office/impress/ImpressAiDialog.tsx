/**
 * "Slides from outline": the user types an outline, the model returns a
 * validated slide list, and Accept appends title + bullet slides.
 */
import { useState } from "react";
import { useT } from "../../lib/i18n";
import type { AiEditResult, AiOutlineSlide } from "../../lib/types";
import { AI_EDIT_MAX_CHARS, AiAssistDialog, type AiStatus } from "../ai/editor-ai";

export function ImpressAiDialog({
  docId,
  status,
  onAccept,
  onClose,
}: {
  docId: string;
  status: AiStatus;
  onAccept: (slides: AiOutlineSlide[]) => void;
  onClose: () => void;
}) {
  const t = useT();
  const [outline, setOutline] = useState("");
  return (
    <AiAssistDialog
      title={t("ai.edit.outlineToSlidesTitle")}
      docId={docId}
      status={status}
      category="outline"
      sends={t("ai.edit.sendsOutline")}
      form={
        <label>
          <span>{t("ai.edit.outlineLabel")}</span>
          <textarea
            value={outline}
            onChange={(event) => setOutline(event.target.value)}
            placeholder={t("ai.edit.outlinePlaceholder")}
            maxLength={AI_EDIT_MAX_CHARS}
          />
        </label>
      }
      canRun={outline.trim().length > 0}
      buildRequest={() => ({ task: "outline_to_slides", text: outline.trim() })}
      renderResult={(reply: AiEditResult) => (
        <div className="ai-dlg-pane">
          <div className="ai-dlg-label">{t("ai.edit.slidesPreview", { count: reply.slides?.length ?? 0 })}</div>
          <ol className="ai-dlg-list" data-testid="ai-slides">
            {(reply.slides ?? []).map((slide, index) => (
              <li key={index}>
                <strong>{slide.title}</strong>
                {slide.bullets.length > 0 ? (
                  <ul>
                    {slide.bullets.map((bullet, position) => (
                      <li key={position}>{bullet}</li>
                    ))}
                  </ul>
                ) : null}
              </li>
            ))}
          </ol>
        </div>
      )}
      acceptLabel={(reply) => t("ai.edit.slidesAccept", { count: reply.slides?.length ?? 0 })}
      onAccept={(reply) => {
        if (reply.slides && reply.slides.length > 0) onAccept(reply.slides);
      }}
      onClose={onClose}
    />
  );
}
