/**
 * Compatibility Center: what this build can do with each format and what a
 * document would lose when saved into a target format.
 */
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ShieldCheck } from "lucide-react";
import { useT } from "../lib/i18n";
import { errorMessage } from "../lib/store";
import { Badge, Card, EmptyState } from "../components/ui";
import { CompatibilityReportView, type FormatCapabilities } from "../components/compatibility";

export function CompatibilityScreen() {
  const t = useT();
  const [extensions, setExtensions] = useState<string[]>([]);
  const [capabilities, setCapabilities] = useState<FormatCapabilities[]>([]);
  const [selected, setSelected] = useState<FormatCapabilities | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void invoke<string[]>("office_supported_extensions")
      .then((list) => {
        setExtensions(list);
        return Promise.all(list.map((extension) => invoke<FormatCapabilities>("office_capabilities", { extension })));
      })
      .then((all) => {
        setCapabilities(all);
        setSelected(all[0] ?? null);
      })
      .catch((reason) => setError(errorMessage(reason, t)));
  }, []);

  return (
    <div className="screen">
      <div className="screen-head">
        <div>
          <h1>
            <ShieldCheck size={18} /> {t("compat.title")}
          </h1>
          <p className="muted">{t("compat.subtitle")}</p>
        </div>
      </div>

      {error ? <p className="muted">{error}</p> : null}
      {!error && capabilities.length === 0 ? <EmptyState title={t("compat.loading")} /> : null}

      <div className="compat-layout">
        <Card className="compat-list">
          {capabilities.map((entry) => (
            <button
              key={entry.extension}
              type="button"
              className="palette-row"
              data-active={selected?.extension === entry.extension}
              onClick={() => setSelected(entry)}
            >
              <span className="palette-title">.{entry.extension}</span>
              <span className="spacer" />
              {entry.open ? <Badge tone="ok">{t("compat.open")}</Badge> : null}
              {entry.save ? <Badge tone="accent">{t("compat.save")}</Badge> : null}
              {entry.pdfExport ? <Badge>{t("compat.pdf")}</Badge> : null}
            </button>
          ))}
        </Card>

        <div className="stack">
          {selected ? (
            <Card>
              <div className="row">
                <strong>.{selected.extension}</strong>
                <span className="spacer" />
                <Badge tone={selected.open ? "ok" : "danger"}>{selected.open ? t("compat.supported") : t("compat.unsupported")}</Badge>
              </div>
              <p className="muted small">{t("compat.nativeHint")}</p>
              <div className="stack" style={{ marginTop: 8 }}>
                {selected.features.map((feature) => (
                  <div key={feature.feature} className="row">
                    <Badge tone={feature.level === "full" ? "ok" : feature.level === "partial" ? "warn" : "danger"}>{feature.level}</Badge>
                    <strong className="small">{feature.feature}</strong>
                    <span className="muted small">{feature.note}</span>
                  </div>
                ))}
              </div>
            </Card>
          ) : null}
          <Card>
            <strong>{t("compat.reportTitle")}</strong>
            <p className="muted small">{t("compat.reportHint")}</p>
            <CompatibilityReportView report={null} />
          </Card>
        </div>
      </div>
      <p className="muted small">
        {extensions.length} {t("compat.formats")}
      </p>
    </div>
  );
}
