/**
 * External file-modification conflict gate.
 *
 * A document opened in the app can be changed on disk by another program,
 * another tab or a cloud client. Saving over it would silently destroy that
 * change, so the session captures a SHA-256 fingerprint at open/save time and
 * compares it before writing. On a mismatch the user chooses:
 *
 *  - Reload external changes (discard the in-app edits, adopt the file),
 *  - Save as a new file (keep both; opens the save-with-name dialog),
 *  - Cancel (keep editing; nothing is written).
 *
 * `useFileConflictPrompt` is a promise-based prompt like `useDataLossPrompt`;
 * `FileConflictDialogHost` is mounted once by the office workspace.
 */
import { create } from "zustand";
import { Button, Modal } from "./ui";
import { useT } from "../lib/i18n";

export type ConflictChoice = "reload" | "saveAs" | "cancel";

export interface ConflictDetails {
  /** File name shown to the user. */
  name: string;
  /** SHA-256 (short) of the file when it was opened/saved. */
  openedHash: string;
  /** SHA-256 (short) of the file now on disk. */
  currentHash: string;
  /** true when the file was deleted outside the app. */
  missing: boolean;
}

export function FileConflictDialog({
  details,
  onChoose,
}: {
  details: ConflictDetails;
  onChoose: (choice: ConflictChoice) => void;
}) {
  const t = useT();
  return (
    <Modal
      title={t("conflict.title")}
      onClose={() => onChoose("cancel")}
      width={620}
      footer={
        <>
          <Button variant="ghost" onClick={() => onChoose("cancel")}>
            {t("conflict.cancel")}
          </Button>
          <Button variant="default" onClick={() => onChoose("saveAs")}>
            {t("conflict.saveAs")}
          </Button>
          {!details.missing ? (
            <Button variant="primary" onClick={() => onChoose("reload")}>
              {t("conflict.reload")}
            </Button>
          ) : null}
        </>
      }
    >
      <p className="text-[13.5px] mb-3">
        {details.missing
          ? t("conflict.bodyMissing", { name: details.name })
          : t("conflict.body", { name: details.name })}
      </p>
      {!details.missing ? (
        <div className="grid grid-cols-2 gap-3 text-xs">
          <div>
            <p className="muted mb-1">{t("conflict.openedVersion")}</p>
            <code className="break-all">{details.openedHash || "—"}</code>
          </div>
          <div>
            <p className="muted mb-1">{t("conflict.currentVersion")}</p>
            <code className="break-all">{details.currentHash || "—"}</code>
          </div>
        </div>
      ) : null}
      <p className="text-xs muted mt-3">{t("conflict.hint")}</p>
    </Modal>
  );
}

interface FileConflictPromptState {
  request: { details: ConflictDetails; resolve: (choice: ConflictChoice) => void } | null;
  ask: (details: ConflictDetails) => Promise<ConflictChoice>;
  answer: (choice: ConflictChoice) => void;
}

export const useFileConflictPrompt = create<FileConflictPromptState>((set, get) => ({
  request: null,
  ask: (details) =>
    new Promise<ConflictChoice>((resolve) => {
      const previous = get().request;
      if (previous) previous.resolve("cancel");
      set({ request: { details, resolve } });
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
export function FileConflictDialogHost() {
  const request = useFileConflictPrompt((state) => state.request);
  const answer = useFileConflictPrompt((state) => state.answer);
  if (!request) return null;
  return <FileConflictDialog details={request.details} onChoose={answer} />;
}
