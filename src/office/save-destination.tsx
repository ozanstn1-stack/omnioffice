/**
 * Where the first save of a document opened from another app goes (Android).
 *
 * A document shared from WhatsApp, mail or a download manager is edited in the
 * app's private copy (`cache/incoming/<uuid>/name.docx`). Its first Save used
 * to copy that file to `Downloads/OmniOffice` without saying so, and the toast
 * only read "Saved": the edited document seemed to vanish. The first save now
 * asks once - save to Downloads, choose a location, or save and share it back
 * - and later saves of the same tab go to the same place without asking.
 *
 * Same promise-based prompt pattern as `components/data-loss-dialog.tsx`; the
 * host is mounted by `ShareTabButton` (office workspace, Android only).
 */
import { create } from "zustand";
import { Download, FolderOpen, Share2 } from "lucide-react";
import { Button, Modal } from "../components/ui";
import { isAndroid, publishedUri } from "../lib/mobile";
import { useT } from "../lib/i18n";

export type SaveDestinationChoice = "downloads" | "pick" | "share" | "cancel";

/** True for the app's private copy of a document another app handed over. */
export function isIncomingCopy(path: string | null | undefined): boolean {
  return Boolean(path) && /[\\/]cache[\\/]incoming[\\/]/.test(path!);
}

/**
 * Whether saving `path` must ask where to: Android, the private copy of a
 * shared document, not yet published anywhere this session, and not a save
 * that already names its destination.
 */
export function needsSaveDestination(path: string | null | undefined, explicitTarget: boolean): boolean {
  if (explicitTarget || !isAndroid() || !isIncomingCopy(path)) return false;
  return !publishedUri(path!);
}

interface SaveDestinationState {
  request: { name: string; resolve: (choice: SaveDestinationChoice) => void } | null;
  ask: (name: string) => Promise<SaveDestinationChoice>;
  answer: (choice: SaveDestinationChoice) => void;
}

export const useSaveDestinationPrompt = create<SaveDestinationState>((set, get) => ({
  request: null,
  ask: (name) =>
    new Promise<SaveDestinationChoice>((resolve) => {
      // A second question settles the first as cancel instead of leaving it hanging.
      get().request?.resolve("cancel");
      set({ request: { name, resolve } });
    }),
  answer: (choice) => {
    const current = get().request;
    if (!current) return;
    current.resolve(choice);
    set({ request: null });
  },
}));

/** Renders the open question, if any. */
export function SaveDestinationHost() {
  const t = useT();
  const request = useSaveDestinationPrompt((state) => state.request);
  const answer = useSaveDestinationPrompt((state) => state.answer);
  if (!request) return null;
  return (
    <Modal
      title={t("office.saveSharedTitle", { name: request.name })}
      onClose={() => answer("cancel")}
      width={460}
      footer={
        <Button variant="ghost" onClick={() => answer("cancel")}>
          {t("common.cancel")}
        </Button>
      }
    >
      <p className="text-sm muted mb-3">{t("office.saveSharedBody")}</p>
      <div className="flex flex-col gap-2">
        <Button variant="primary" onClick={() => answer("downloads")}>
          <Download size={15} /> {t("office.saveSharedDownloads")}
        </Button>
        <Button onClick={() => answer("share")}>
          <Share2 size={15} /> {t("office.saveSharedShare")}
        </Button>
        <Button onClick={() => answer("pick")}>
          <FolderOpen size={15} /> {t("office.saveSharedPick")}
        </Button>
      </div>
    </Modal>
  );
}
