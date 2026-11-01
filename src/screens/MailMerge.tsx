import { useState } from "react";
import { pickDirectoryToken, pickDocumentToken } from "../lib/file-tokens";
import { FileSpreadsheet, FileText, Play } from "lucide-react";
import { Button, Card, Field, Segmented, TextInput } from "../components/ui";
import { Screen } from "../components/layout";
import { useT } from "../lib/i18n";
import { reportError, useToasts } from "../lib/store";
import { mailMerge, type MailMergeReport } from "../lib/office-api";

/**
 * Address mail merge: a Writer template with `{{alan}}` placeholders plus a CSV
 * of recipients produces one document per row (PDF or DOCX).
 */
export function MailMerge() {
  const t = useT();
  const [template, setTemplate] = useState("");
  const [data, setData] = useState("");
  const [outputDir, setOutputDir] = useState("");
  const [format, setFormat] = useState<"pdf" | "docx">("pdf");
  const [nameField, setNameField] = useState("");
  const [report, setReport] = useState<MailMergeReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [templateLabel, setTemplateLabel] = useState("");
  const [dataLabel, setDataLabel] = useState("");
  const [dirLabel, setDirLabel] = useState("");

  const pickDocument = async (
    setter: (value: string) => void,
    label: (value: string) => void,
    extensions: string[],
  ) => {
    const picked = await pickDocumentToken(extensions).catch(() => null);
    if (picked) {
      setter(picked.token);
      label(picked.name);
    }
  };

  const pickDirectory = async (setter: (value: string) => void, label: (value: string) => void) => {
    const picked = await pickDirectoryToken().catch(() => null);
    if (picked) {
      setter(picked.token);
      label(picked.name);
    }
  };

  const run = async () => {
    if (!template || !data || !outputDir) {
      useToasts.getState().push({ kind: "info", title: t("mailMerge.needInputs") });
      return;
    }
    setBusy(true);
    setReport(null);
    try {
      const result = await mailMerge({ template, data, outputDir, format, nameField: nameField.trim() || undefined });
      setReport(result);
      useToasts.getState().push({
        kind: result.errors.length ? "info" : "success",
        title: t.plural("mailMerge.done", result.produced.length, { count: result.produced.length }),
      });
    } catch (error) {
      reportError(error, t);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Screen title={t("mailMerge.title")} subtitle={t("mailMerge.subtitle")}>
      <div className="flex flex-col gap-4">
        <Card className="p-5 flex flex-col gap-3">
          <Field label={t("mailMerge.template")} hint={t("mailMerge.templateHint")}>
            <div className="flex items-center gap-2">
              <TextInput
                value={templateLabel || template}
                onChange={(event) => {
                  setTemplateLabel("");
                  setTemplate(event.target.value);
                }}
              />
              <Button
                size="sm"
                variant="ghost"
                onClick={() => void pickDocument(setTemplate, setTemplateLabel, ["docx", "odt"])}
              >
                <FileText size={14} />
              </Button>
            </div>
          </Field>
          <Field label={t("mailMerge.data")} hint={t("mailMerge.dataHint")}>
            <div className="flex items-center gap-2">
              <TextInput
                value={dataLabel || data}
                onChange={(event) => {
                  setDataLabel("");
                  setData(event.target.value);
                }}
              />
              <Button
                size="sm"
                variant="ghost"
                onClick={() => void pickDocument(setData, setDataLabel, ["csv", "tsv", "txt"])}
              >
                <FileSpreadsheet size={14} />
              </Button>
            </div>
          </Field>
          <Field label={t("mailMerge.outputDir")}>
            <div className="flex items-center gap-2">
              <TextInput
                value={dirLabel || outputDir}
                onChange={(event) => {
                  setDirLabel("");
                  setOutputDir(event.target.value);
                }}
              />
              <Button size="sm" variant="ghost" onClick={() => void pickDirectory(setOutputDir, setDirLabel)}>
                {t("mailMerge.choose")}
              </Button>
            </div>
          </Field>
          <Field label={t("mailMerge.format")}>
            <Segmented<"pdf" | "docx">
              value={format}
              onChange={setFormat}
              options={[
                { value: "pdf", label: "PDF" },
                { value: "docx", label: "DOCX" },
              ]}
            />
          </Field>
          <Field label={t("mailMerge.nameField")} hint={t("mailMerge.nameFieldHint")}>
            <TextInput value={nameField} onChange={(event) => setNameField(event.target.value)} />
          </Field>
          <div>
            <Button size="sm" variant="primary" disabled={busy} onClick={() => void run()}>
              <Play size={14} /> {t("mailMerge.run")}
            </Button>
          </div>
        </Card>

        {report ? (
          <Card className="p-5 flex flex-col gap-2">
            <strong>{t("mailMerge.result")}</strong>
            <p className="text-sm">
              {t.plural("mailMerge.produced", report.produced.length, { count: report.produced.length })}
            </p>
            {report.errors.length ? (
              <ul className="text-sm" style={{ color: "var(--danger, #dc2626)" }}>
                {report.errors.map((error) => (
                  <li key={error.row}>
                    {t("mailMerge.row", { row: error.row })}: {error.message}
                  </li>
                ))}
              </ul>
            ) : null}
          </Card>
        ) : null}
      </div>
    </Screen>
  );
}
