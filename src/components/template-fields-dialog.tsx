/**
 * The form a Writer template with `{{alan}}` placeholders shows before it is
 * created: one labelled input per field, today's date proposed for date
 * fields, and a way to create the document with the placeholders intact.
 */
import { useEffect, useState } from "react";
import { Button, Field, Modal, Select, TextInput } from "./ui";
import { useT } from "../lib/i18n";
import { useSettings } from "../lib/store";
import { listPersonCards, type PersonCard } from "../lib/person-cards";
import { cardTemplateValues } from "../lib/person-cards-io";
import type { TemplateField } from "../lib/template-fields";

/** Date-like field names get today's date as the initial value. */
const DATE_FIELD = /tarih|date/i;

function todayFor(language: string): string {
  return new Date().toLocaleDateString(language === "tr" ? "tr-TR" : "en-GB");
}

function initialValues(fields: TemplateField[], language: string): Record<string, string> {
  const values: Record<string, string> = {};
  for (const field of fields) {
    values[field.name] = DATE_FIELD.test(field.name) ? todayFor(language) : "";
  }
  return values;
}

export function TemplateFieldsDialog({
  templateName,
  fields,
  onSubmit,
  onCreateEmpty,
  onClose,
}: {
  templateName?: string;
  fields: TemplateField[];
  onSubmit: (values: Record<string, string>) => void;
  onCreateEmpty: () => void;
  onClose: () => void;
}) {
  const t = useT();
  const language = useSettings((state) => state.settings.language);
  const [values, setValues] = useState<Record<string, string>>(() => initialValues(fields, language));
  const [cards, setCards] = useState<PersonCard[]>([]);

  // Person cards are optional: with none saved the form shows no picker, and a
  // failed read must not block the template itself.
  useEffect(() => {
    let alive = true;
    listPersonCards()
      .then((list) => {
        if (alive) setCards(list.cards);
      })
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, []);

  const applyCard = (id: string) => {
    const card = cards.find((candidate) => candidate.id === id);
    if (!card) return;
    setValues((previous) => ({ ...previous, ...cardTemplateValues(card, fields) }));
  };

  const label = (name: string): string => {
    const key = `templates.field.${name}`;
    const translated = t(key);
    return translated === key ? name : translated;
  };

  return (
    <Modal
      title={templateName ? t("templates.formTitleNamed", { name: templateName }) : t("templates.formTitle")}
      onClose={onClose}
      width={640}
      footer={
        <div className="row" style={{ justifyContent: "flex-end", flexWrap: "wrap" }}>
          <Button size="sm" variant="ghost" onClick={onClose}>
            {t("templates.formCancel")}
          </Button>
          <Button size="sm" onClick={onCreateEmpty}>
            {t("templates.formCreateEmpty")}
          </Button>
          <Button size="sm" variant="primary" onClick={() => onSubmit(values)}>
            {t("templates.formFill")}
          </Button>
        </div>
      }
    >
      <div className="stack">
        <p className="muted small">{t("templates.formHint")}</p>
        {cards.length ? (
          <Field label={t("templates.formFromCard")}>
            <Select
              value=""
              onChange={(value) => applyCard(value)}
              options={[
                { value: "", label: t("templates.formFromCardPlaceholder") },
                ...cards.map((card) => ({ value: card.id, label: card.name })),
              ]}
            />
          </Field>
        ) : null}
        {fields.map((field) => (
          <Field key={field.name} label={label(field.name)} hint={`{{${field.name}}}`}>
            <TextInput
              value={values[field.name] ?? ""}
              aria-label={label(field.name)}
              placeholder={label(field.name)}
              onChange={(event) => setValues((previous) => ({ ...previous, [field.name]: event.target.value }))}
            />
          </Field>
        ))}
      </div>
    </Modal>
  );
}
