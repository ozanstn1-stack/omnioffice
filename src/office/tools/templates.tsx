/** Templates: document starters shared with the office editors. */
import { useState } from "react";
import { FileSpreadsheet, FileText, Image as ImageIcon } from "lucide-react";
import { useT } from "../../lib/i18n";
import { useSettings } from "../../lib/store";
import { useOfficeTabs } from "../../lib/office-store";
import { Screen } from "../../components/layout";
import { orderTemplates, TEMPLATES, templatesFor } from ".././templates";

// ---------------------------------------------------------------------------
// Templates
// ---------------------------------------------------------------------------

export function TemplatesScreen({ onOpen }: { onOpen?: () => void } = {}) {
  const t = useT();
  const create = useOfficeTabs((state) => state.create);
  const language = useSettings((state) => state.settings.language);
  const [filter, setFilter] = useState<"all" | "writer" | "calc" | "impress">("all");
  const visible = orderTemplates(filter === "all" ? TEMPLATES : templatesFor(filter), language);
  return (
    <Screen title={t("templates.title")} subtitle={t("templates.subtitle")}>
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
          <button
            key={template.id}
            type="button"
            className="card template-card"
            onClick={() => {
              create(template.kind, template.name, template.build() as never);
              onOpen?.();
            }}
          >
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
    </Screen>
  );
}
