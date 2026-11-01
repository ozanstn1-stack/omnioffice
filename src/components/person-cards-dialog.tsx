/**
 * Person cards (C6/2): the local, encrypted address book behind the template
 * form and the mail merge. Everything stays on the device; export copies text
 * to the clipboard only when the user asks, and the import box warns about
 * personal data before it saves anything.
 */
import { useCallback, useEffect, useState } from "react";
import { ArrowLeft, Copy, Pencil, Plus, Trash2, Upload } from "lucide-react";
import { Badge, Button, Field, Modal, Spinner, TextArea, TextInput } from "./ui";
import { useT } from "../lib/i18n";
import {
  deletePersonCard,
  listPersonCards,
  savePersonCard,
  type PersonCard,
  type PersonCardInput,
} from "../lib/person-cards";
import { cardsToCsv, cardsToVcards, parseCardsText } from "../lib/person-cards-io";
import { reportError, useToasts } from "../lib/store";

const EMPTY: PersonCardInput = {
  id: "",
  name: "",
  taxId: "",
  address: "",
  phone: "",
  email: "",
  note: "",
  updatedAt: "",
};

function CardRow({ card, onEdit, onDelete }: { card: PersonCard; onEdit: () => void; onDelete: () => void }) {
  const t = useT();
  return (
    <div
      className="flex items-center justify-between gap-2 py-2 border-b last:border-0"
      style={{ borderColor: "var(--border)" }}
    >
      <div className="min-w-0">
        <strong className="text-sm">{card.name}</strong>
        <p className="muted small truncate">
          {[card.email, card.phone, card.taxId].filter(Boolean).join(" · ") || t("personCards.noDetails")}
        </p>
      </div>
      <div className="row">
        <Button
          size="sm"
          variant="ghost"
          icon={<Pencil size={13} />}
          aria-label={t("personCards.edit")}
          onClick={onEdit}
        >
          {t("personCards.edit")}
        </Button>
        <Button
          size="sm"
          variant="ghost"
          icon={<Trash2 size={13} />}
          aria-label={t("personCards.delete")}
          onClick={onDelete}
        >
          {t("personCards.delete")}
        </Button>
      </div>
    </div>
  );
}

export function PersonCardsDialog({ onClose }: { onClose: () => void }) {
  const t = useT();
  const push = useToasts((state) => state.push);
  const [cards, setCards] = useState<PersonCard[]>([]);
  const [warnings, setWarnings] = useState<string[]>([]);
  const [encrypted, setEncrypted] = useState(true);
  const [loading, setLoading] = useState(true);
  const [draft, setDraft] = useState<PersonCardInput | null>(null);
  const [importText, setImportText] = useState("");
  const [importing, setImporting] = useState(false);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const list = await listPersonCards();
      setCards(list.cards);
      setWarnings(list.warnings);
      setEncrypted(list.encrypted);
    } catch (error) {
      reportError(error, t);
    } finally {
      setLoading(false);
    }
  }, [t]);

  useEffect(() => {
    // The loader owns the list state; the dialog starts with a fresh read.
    // eslint-disable-next-line react-hooks/set-state-in-effect -- the loader owns the list state
    void load();
  }, [load]);

  const save = async () => {
    if (!draft) return;
    setBusy(true);
    try {
      await savePersonCard(draft);
      push({ kind: "success", title: t("personCards.saved"), detail: draft.name });
      setDraft(null);
      await load();
    } catch (error) {
      reportError(error, t);
    } finally {
      setBusy(false);
    }
  };

  const remove = async (card: PersonCard) => {
    try {
      await deletePersonCard(card.id);
      await load();
    } catch (error) {
      reportError(error, t);
    }
  };

  const copy = async (text: string, title: string) => {
    try {
      await navigator.clipboard.writeText(text);
      push({ kind: "success", title });
    } catch (error) {
      reportError(error, t);
    }
  };

  const runImport = async () => {
    const parsed = parseCardsText(importText);
    if (!parsed.length) {
      push({ kind: "info", title: t("personCards.importEmpty") });
      return;
    }
    setImporting(true);
    try {
      for (const card of parsed) await savePersonCard(card);
      push({ kind: "success", title: t.plural("personCards.imported", parsed.length, { count: parsed.length }) });
      setImportText("");
      await load();
    } catch (error) {
      reportError(error, t);
    } finally {
      setImporting(false);
    }
  };

  const title = draft ? (draft.id ? t("personCards.editTitle") : t("personCards.addTitle")) : t("personCards.title");

  return (
    <Modal
      title={title}
      onClose={onClose}
      width={760}
      footer={
        <div className="row" style={{ justifyContent: "flex-end", flexWrap: "wrap" }}>
          <Button size="sm" variant="ghost" onClick={onClose}>
            {t("personCards.close")}
          </Button>
          {!draft ? (
            <Button size="sm" variant="primary" icon={<Plus size={14} />} onClick={() => setDraft({ ...EMPTY })}>
              {t("personCards.add")}
            </Button>
          ) : (
            <Button size="sm" variant="primary" disabled={busy} onClick={() => void save()}>
              {t("personCards.save")}
            </Button>
          )}
        </div>
      }
    >
      {draft ? (
        <div className="stack">
          <Button size="sm" variant="ghost" icon={<ArrowLeft size={13} />} onClick={() => setDraft(null)}>
            {t("personCards.back")}
          </Button>
          <Field label={t("personCards.name")}>
            <TextInput
              value={draft.name}
              aria-label={t("personCards.name")}
              onChange={(event) => setDraft({ ...draft, name: event.target.value })}
            />
          </Field>
          <div className="row" style={{ alignItems: "flex-start" }}>
            <Field label={t("personCards.taxId")}>
              <TextInput
                value={draft.taxId ?? ""}
                aria-label={t("personCards.taxId")}
                onChange={(event) => setDraft({ ...draft, taxId: event.target.value })}
              />
            </Field>
            <Field label={t("personCards.phone")}>
              <TextInput
                value={draft.phone ?? ""}
                aria-label={t("personCards.phone")}
                onChange={(event) => setDraft({ ...draft, phone: event.target.value })}
              />
            </Field>
          </div>
          <Field label={t("personCards.email")}>
            <TextInput
              value={draft.email ?? ""}
              aria-label={t("personCards.email")}
              onChange={(event) => setDraft({ ...draft, email: event.target.value })}
            />
          </Field>
          <Field label={t("personCards.address")}>
            <TextArea
              rows={2}
              value={draft.address ?? ""}
              aria-label={t("personCards.address")}
              onChange={(event) => setDraft({ ...draft, address: event.target.value })}
            />
          </Field>
          <Field label={t("personCards.note")}>
            <TextInput
              value={draft.note ?? ""}
              aria-label={t("personCards.note")}
              onChange={(event) => setDraft({ ...draft, note: event.target.value })}
            />
          </Field>
        </div>
      ) : (
        <div className="stack">
          <p className="muted small">{t("personCards.intro")}</p>
          {!encrypted ? <Badge tone="warn">{t("personCards.plainWarning")}</Badge> : null}
          {warnings.length ? (
            <p className="muted small">
              {t.plural("personCards.warnings", warnings.length, { count: warnings.length })}: {warnings.join(" · ")}
            </p>
          ) : null}
          {loading ? (
            <div className="row">
              <Spinner size={14} /> <span className="muted small">{t("personCards.loading")}</span>
            </div>
          ) : cards.length ? (
            <div>
              {cards.map((card) => (
                <CardRow
                  key={card.id}
                  card={card}
                  onEdit={() => setDraft({ ...card })}
                  onDelete={() => void remove(card)}
                />
              ))}
            </div>
          ) : (
            <p className="muted small">{t("personCards.empty")}</p>
          )}
          <div className="row" style={{ flexWrap: "wrap" }}>
            <Button
              size="sm"
              variant="ghost"
              icon={<Copy size={13} />}
              disabled={!cards.length}
              onClick={() => void copy(cardsToCsv(cards), t("personCards.csvCopied"))}
            >
              {t("personCards.exportCsv")}
            </Button>
            <Button
              size="sm"
              variant="ghost"
              icon={<Copy size={13} />}
              disabled={!cards.length}
              onClick={() => void copy(cardsToVcards(cards), t("personCards.vcardsCopied"))}
            >
              {t("personCards.exportVcards")}
            </Button>
          </div>
          <Field label={t("personCards.importTitle")} hint={t("personCards.kvkk")}>
            <TextArea
              rows={4}
              value={importText}
              aria-label={t("personCards.importTitle")}
              placeholder={t("personCards.importPlaceholder")}
              onChange={(event) => setImportText(event.target.value)}
            />
          </Field>
          <div className="row">
            <Button
              size="sm"
              icon={<Upload size={13} />}
              disabled={importing || !importText.trim()}
              onClick={() => void runImport()}
            >
              {t("personCards.import")}
            </Button>
          </div>
        </div>
      )}
    </Modal>
  );
}
