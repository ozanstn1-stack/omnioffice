import { useState } from "react";
import { Plus, Save, Trash2 } from "lucide-react";
import { Badge, Button, Card, Field, TextInput, Toggle } from "../../components/ui";
import { pickSaveToken } from "../../lib/file-tokens";
import { fileBaseName } from "../../lib/format";
import { pdfCreateFormFields, type NewFormFieldSpec } from "../../lib/api";
import { useT } from "../../lib/i18n";
import { reportError } from "../../lib/store";

export function FormBuilderPanel({
  input,
  password,
  shownPage,
  onFieldsCreated,
}: {
  input: string;
  password?: string;
  shownPage: number;
  onFieldsCreated: (savedPath: string) => void;
}) {
  const t = useT();
  const [name, setName] = useState("field_1");
  const [fieldType, setFieldType] = useState<"text" | "checkbox" | "choice" | "signature">("text");
  const [required, setRequired] = useState(false);
  const [defaultValue, setDefaultValue] = useState("");
  const [optionsText, setOptionsText] = useState("Option A, Option B");
  const [x, setX] = useState(72);
  const [y, setY] = useState(680);
  const [width, setWidth] = useState(200);
  const [height, setHeight] = useState(24);
  const [queued, setQueued] = useState<NewFormFieldSpec[]>([]);
  const [saving, setSaving] = useState(false);
  const [savedCount, setSavedCount] = useState<number | null>(null);

  const addField = () => {
    const trimmed = name.trim();
    if (!trimmed) return;
    const options =
      fieldType === "choice"
        ? optionsText
            .split(",")
            .map((item) => item.trim())
            .filter(Boolean)
        : [];
    const next: NewFormFieldSpec = {
      name: trimmed,
      fieldType,
      page: shownPage,
      rect: [x, y, x + Math.max(12, width), y + Math.max(12, height)],
      required,
      defaultValue: defaultValue.trim() || null,
      options,
    };
    setQueued((prev) => [...prev, next]);
    setName(`field_${queued.length + 2}`);
    setY((prev) => Math.max(48, prev - (height + 16)));
    setSavedCount(null);
  };

  const saveFields = async () => {
    if (!input || !queued.length) return;
    const base = fileBaseName(input).replace(/\.pdf$/i, "") || "document";
    const target = await pickSaveToken(`${base}-form.pdf`, ["pdf"]).catch(() => null);
    if (!target) return;
    setSaving(true);
    setSavedCount(null);
    try {
      const count = await pdfCreateFormFields({
        input,
        output: { path: target.token, overwrite: "replace" },
        password: password || null,
        fields: queued,
      });
      setSavedCount(count);
      setQueued([]);
      onFieldsCreated(target.token);
    } catch (caught) {
      reportError(caught, t);
    } finally {
      setSaving(false);
    }
  };

  return (
    <Card>
      <div className="row" style={{ justifyContent: "space-between", marginBottom: 8 }}>
        <strong>{t("studio.formBuilderTitle")}</strong>
        {savedCount !== null ? <Badge tone="ok">{t("studio.formBuilderSaved", { count: savedCount })}</Badge> : null}
      </div>
      <p className="muted small" style={{ marginBottom: 10 }}>
        {t("studio.formBuilderHint", { page: shownPage })}
      </p>
      <div className="stack" style={{ gap: 8 }}>
        <div className="row" style={{ gap: 8, flexWrap: "wrap" }}>
          <Field label={t("studio.formFieldName")}>
            <TextInput className="input-sm" value={name} onChange={(e) => setName(e.target.value)} />
          </Field>
          <Field label={t("studio.formFieldType")}>
            <select
              className="select input-sm"
              value={fieldType}
              onChange={(e) => {
                const next = e.target.value as "text" | "checkbox" | "choice" | "signature";
                setFieldType(next);
                if (next === "checkbox") {
                  setWidth(18);
                  setHeight(18);
                } else if (next === "signature") {
                  setWidth(200);
                  setHeight(48);
                } else {
                  setWidth(200);
                  setHeight(24);
                }
              }}
            >
              <option value="text">{t("studio.fieldTypeText")}</option>
              <option value="checkbox">{t("studio.fieldTypeCheckbox")}</option>
              <option value="choice">{t("studio.fieldTypeChoice")}</option>
              <option value="signature">{t("studio.fieldTypeSignature")}</option>
            </select>
          </Field>
        </div>
        <div className="row" style={{ gap: 8, flexWrap: "wrap" }}>
          <Field label={t("studio.coordX0")}>
            <TextInput className="input-sm" type="number" value={x} onChange={(e) => setX(Number(e.target.value) || 0)} />
          </Field>
          <Field label={t("studio.coordY0")}>
            <TextInput className="input-sm" type="number" value={y} onChange={(e) => setY(Number(e.target.value) || 0)} />
          </Field>
          <Field label={t("studio.coordX1")}>
            <TextInput className="input-sm" type="number" value={width} onChange={(e) => setWidth(Number(e.target.value) || 24)} />
          </Field>
          <Field label={t("studio.coordY1")}>
            <TextInput className="input-sm" type="number" value={height} onChange={(e) => setHeight(Number(e.target.value) || 18)} />
          </Field>
        </div>
        {fieldType === "choice" ? (
          <Field label={t("studio.formFieldOptions")}>
            <TextInput className="input-sm" value={optionsText} onChange={(e) => setOptionsText(e.target.value)} />
          </Field>
        ) : (
          <Field label={t("studio.formFieldDefault")}>
            <TextInput className="input-sm" value={defaultValue} onChange={(e) => setDefaultValue(e.target.value)} />
          </Field>
        )}
        <Toggle checked={required} onChange={setRequired} label={t("studio.required")} />
        <div className="row" style={{ gap: 8 }}>
          <Button size="sm" variant="default" icon={<Plus size={13} />} onClick={addField} disabled={!name.trim()}>
            {t("studio.formBuilderAdd")}
          </Button>
          <Button
            size="sm"
            variant="primary"
            icon={<Save size={13} />}
            disabled={!input || !queued.length || saving}
            onClick={() => void saveFields()}
          >
            {t("studio.formBuilderSave", { count: queued.length })}
          </Button>
        </div>
        {queued.length ? (
          <div className="stack" style={{ gap: 4, marginTop: 4 }}>
            {queued.map((item, idx) => (
              <div key={`${item.name}-${idx}`} className="row" style={{ justifyContent: "space-between" }}>
                <span className="small">
                  <strong>{item.name}</strong> ({item.fieldType}) · p.{item.page} [{item.rect.map((n) => Math.round(n)).join(", ")}]
                </span>
                <Button
                  size="sm"
                  variant="ghost"
                  icon={<Trash2 size={12} />}
                  onClick={() => setQueued((prev) => prev.filter((_, i) => i !== idx))}
                >
                  {t("common.remove")}
                </Button>
              </div>
            ))}
          </div>
        ) : null}
      </div>
    </Card>
  );
}
