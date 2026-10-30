/** Templates: document starters shared with the office editors. */
import { lazy, Suspense, useState } from "react";
import { FileSpreadsheet, FileText, Image as ImageIcon } from "lucide-react";
import { useT } from "../../lib/i18n";
import { useSettings } from "../../lib/store";
import { useOfficeTabs, type OfficeModel } from "../../lib/office-store";
import { Button } from "../../components/ui";
import { Screen } from "../../components/layout";
import { TemplateFieldsDialog } from "../../components/template-fields-dialog";
import { fillTemplate, templateFields, type TemplateField } from "../../lib/template-fields";
import type { TextDocument } from "../../lib/office-types";
import { orderTemplates, TEMPLATES, templatesFor, type OfficeTemplate } from ".././templates";

// Kept out of the Templates chunk: the dialog carries the address-book logic
// and would push the route budget over its cap.
const PersonCardsDialog = lazy(() =>
  import("../../components/person-cards-dialog").then((module) => ({ default: module.PersonCardsDialog })),
);

// ---------------------------------------------------------------------------
// Templates
// ---------------------------------------------------------------------------

interface PendingTemplate {
  template: OfficeTemplate;
  model: OfficeModel;
  fields: TemplateField[];
}

export function TemplatesScreen({ onOpen }: { onOpen?: () => void } = {}) {
  const t = useT();
  const create = useOfficeTabs((state) => state.create);
  const language = useSettings((state) => state.settings.language);
  const [filter, setFilter] = useState<"all" | "writer" | "calc" | "impress">("all");
  const [pending, setPending] = useState<PendingTemplate | null>(null);
  const [cardsOpen, setCardsOpen] = useState(false);
  const visible = orderTemplates(filter === "all" ? TEMPLATES : templatesFor(filter), language);

  /** A Writer template with `{{alan}}` placeholders asks for their values first. */
  const openTemplate = (template: OfficeTemplate) => {
    const model = template.build() as OfficeModel;
    const fields = template.kind === "writer" ? templateFields(model as TextDocument) : [];
    if (fields.length) {
      setPending({ template, model, fields });
      return;
    }
    create(template.kind, template.name, model);
    onOpen?.();
  };

  const finish = (values: Record<string, string> | null) => {
    if (!pending) return;
    const model = values ? fillTemplate(pending.model as TextDocument, values) : pending.model;
    create(pending.template.kind, pending.template.name, model);
    setPending(null);
    onOpen?.();
  };

  return (
    <Screen
      title={t("templates.title")}
      subtitle={t("templates.subtitle")}
      actions={
        <Button size="sm" variant="ghost" onClick={() => setCardsOpen(true)}>
          {t("personCards.title")}
        </Button>
      }
    >
      <div className="row">
        {(["all", "writer", "calc", "impress"] as const).map((candidate) => (
          <button
            key={candidate}
            type="button"
            className={`chip${filter === candidate ? " is-active" : ""}`}
            onClick={() => setFilter(candidate)}
          >
            {t(`templates.${candidate}`)}
          </button>
        ))}
      </div>
      <div className="template-grid">
        {visible.map((template) => (
          <button key={template.id} type="button" className="card template-card" onClick={() => openTemplate(template)}>
            <span className={`template-icon kind-${template.kind}`}>
              {template.kind === "writer" ? (
                <FileText size={20} />
              ) : template.kind === "calc" ? (
                <FileSpreadsheet size={20} />
              ) : (
                <ImageIcon size={20} />
              )}
            </span>
            <strong>{template.name}</strong>
            <span className="muted">{template.description}</span>
            {template.language === "tr" ? (
              <span className="badge badge-accent template-lang" title={t("templates.turkish")}>
                TR
              </span>
            ) : null}
          </button>
        ))}
      </div>
      {cardsOpen ? (
        <Suspense fallback={null}>
          <PersonCardsDialog onClose={() => setCardsOpen(false)} />
        </Suspense>
      ) : null}
      {pending ? (
        <TemplateFieldsDialog
          templateName={pending.template.name}
          fields={pending.fields}
          onSubmit={(values) => finish(values)}
          onCreateEmpty={() => finish(null)}
          onClose={() => setPending(null)}
        />
      ) : null}
    </Screen>
  );
}
