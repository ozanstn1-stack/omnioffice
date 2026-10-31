/**
 * The "add a source manifest" dialog of the provenance card (C8, D7).
 *
 * Kept separate from the card so the card stays about reading and presenting
 * the chain: the dialog owns the picker, the note and the save action.
 */
import { ShieldAlert, ShieldCheck } from "lucide-react";
import { Button, Modal, TextArea } from "./ui";
import { FileList } from "./files";
import type { FileToken } from "../lib/file-tokens";
import { useT } from "../lib/i18n";

export function ProvenanceAddDialog({
  open,
  docx,
  sources,
  note,
  saving,
  onClose,
  onPickSources,
  onRemoveSource,
  onNote,
  onSave,
}: {
  open: boolean;
  docx: boolean;
  sources: FileToken[];
  note: string;
  saving: boolean;
  onClose: () => void;
  onPickSources: () => void;
  onRemoveSource: (index: number) => void;
  onNote: (note: string) => void;
  onSave: () => void;
}) {
  const t = useT();
  if (!open) return null;
  return (
    <Modal
      title={t("provenance.add")}
      onClose={onClose}
      width={720}
      footer={
        <div className="row" style={{ justifyContent: "flex-end", flexWrap: "wrap" }}>
          <Button size="sm" variant="ghost" onClick={onClose}>
            {t("provenance.close")}
          </Button>
          <Button size="sm" variant="primary" disabled={saving} onClick={onSave}>
            {saving ? t("provenance.saving") : t("provenance.save")}
          </Button>
        </div>
      }
    >
      <div className="stack">
        <p className="muted small">{t("provenance.subtitle")}</p>
        {docx ? <p className="muted small">{t("provenance.docxSupported")}</p> : null}
        <FileList
          files={sources.map((source) => ({ path: source.token, name: source.name, sizeBytes: 0 }))}
          onRemove={onRemoveSource}
          onAdd={onPickSources}
          addLabel={t("provenance.pickSources")}
        />
        <label className="text-xs muted" htmlFor="provenance-note">
          {t("provenance.noteLabel")}
        </label>
        <TextArea
          id="provenance-note"
          rows={3}
          value={note}
          placeholder={t("provenance.notePlaceholder")}
          onChange={(event) => onNote(event.target.value)}
        />
        <p className="muted small">
          <ShieldCheck size={13} /> {t("provenance.replaces")}
        </p>
        <p className="muted small row">
          <ShieldAlert size={13} /> {t("provenance.signedNote")}
        </p>
      </div>
    </Modal>
  );
}
