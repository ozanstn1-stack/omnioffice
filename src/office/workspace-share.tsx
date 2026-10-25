/**
 * The office workspace's "Share" button (Android): sends the active tab to
 * WhatsApp, mail, Drive and the other targets of the system share sheet, as
 * the document itself or as a PDF. The current model is written to a share
 * copy first, so unsaved edits go out too (see `lib/share.ts`).
 */
import { useState } from "react";
import { Share2 } from "lucide-react";
import { isAndroid } from "../lib/mobile";
import type { OfficeTab } from "../lib/office-store";
import { shareExtensionFor, shareOfficeDocument } from "../lib/share";
import { errorMessage, useToasts } from "../lib/store";
import { useT } from "../lib/i18n";
import { Dialog } from "./office-ui";

export function ShareTabButton({ tab }: { tab: OfficeTab | null }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [sharing, setSharing] = useState(false);
  if (!isAndroid()) return null;

  const share = async (as: "document" | "pdf") => {
    if (!tab) return;
    setSharing(true);
    try {
      await shareOfficeDocument(tab, as, t("office.share"));
      setOpen(false);
    } catch (error) {
      useToasts.getState().push({ kind: "error", title: t("office.shareFailed"), detail: errorMessage(error, t) });
    } finally {
      setSharing(false);
    }
  };

  return (
    <>
      <button
        type="button"
        className="tool-btn is-icon-only"
        title={t("office.share")}
        aria-label={t("office.share")}
        disabled={!tab}
        onClick={() => setOpen(true)}
      >
        <Share2 size={15} />
      </button>
      {open && tab ? (
        <Dialog title={t("office.shareTitle", { name: tab.title })} onClose={() => setOpen(false)}>
          <p className="muted">{t("office.shareHint")}</p>
          <div className="row" style={{ justifyContent: "flex-end", marginTop: 14, flexWrap: "wrap" }}>
            <button type="button" className="btn btn-soft" disabled={sharing} onClick={() => void share("pdf")}>
              {t("office.shareAsPdf")}
            </button>
            <button type="button" className="btn btn-primary" disabled={sharing} onClick={() => void share("document")}>
              {t("office.shareAsDocument", { format: shareExtensionFor(tab.kind, tab.path).toUpperCase() })}
            </button>
          </div>
        </Dialog>
      ) : null}
    </>
  );
}
