/** PDF forms: list fields and fill them in place. */
import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { appCacheDir, join } from "@tauri-apps/api/path";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { pickSaveToken } from "../../lib/file-tokens";
import { Plus, Save, Trash2 } from "lucide-react";
import { useT } from "../../lib/i18n";
import { reportError, useToasts } from "../../lib/store";
import {
  isAndroid,
  pickAndroidFiles,
  pickAndroidSaveTarget,
  publishOutputs,
  type AndroidTarget,
} from "../../lib/mobile";
import { fileBaseName } from "../../lib/format";
import { uid } from "../../lib/office-types";
import * as api from "../../lib/office-api";
import { Screen } from "../../components/layout";

// ---------------------------------------------------------------------------
// PDF forms
// ---------------------------------------------------------------------------

export function PdfFormsScreen() {
  const t = useT();
  const [path, setPath] = useState("");
  const [fields, setFields] = useState<api.PdfFormField[]>([]);
  const [busy, setBusy] = useState(false);

  const pick = async () => {
    try {
      // Android: the SAF picker copies the PDF into the app cache.
      const selection = isAndroid()
        ? (await pickAndroidFiles({ multiple: false, accept: "pdf" }))[0]
        : await openDialog({ multiple: false, filters: [{ name: "PDF", extensions: ["pdf"] }] });
      if (typeof selection === "string") {
        setPath(selection);
        setFields(await api.pdfListForm(selection).catch(() => []));
      }
    } catch (error) {
      reportError(error, t);
    }
  };

  const addField = () => {
    setFields([
      ...fields,
      {
        kind: "text",
        name: `field_${fields.length + 1}`,
        page: 1,
        x: 72,
        y: 120 + fields.length * 36,
        w: 200,
        h: 22,
        value: "",
        options: [],
        fontSize: 11,
        required: false,
      },
    ]);
  };

  const save = async () => {
    if (!path) return;
    const defaultPath = path.replace(/\.pdf$/i, "-form.pdf");
    let output: string | null;
    let androidTarget: AndroidTarget | null = null;
    if (isAndroid()) {
      androidTarget = await pickAndroidSaveTarget(fileBaseName(defaultPath), "application/pdf").catch(() => null);
      if (!androidTarget) return;
      const stageDir = await join(await appCacheDir(), "forms", `form-${uid()}`);
      await invoke("ensure_dir", { path: stageDir });
      output = await join(stageDir, fileBaseName(defaultPath));
    } else {
      const pickedTarget = await pickSaveToken(fileBaseName(defaultPath), ["pdf"]).catch(() => null);
      if (!pickedTarget) return;
      output = pickedTarget.token;
    }
    if (!output) return;
    setBusy(true);
    try {
      // The save dialog already confirmed replacing an existing file, so the
      // backend must not refuse it again ("error" made every overwrite fail).
      const written = await api.pdfAddForm({
        input: path,
        output,
        fields,
        overwrite: "replace",
      });
      if (androidTarget) {
        const failures = await publishOutputs([written || output], { file: androidTarget });
        if (failures.length) throw new Error(failures[0].error);
      }
      useToasts.getState().push({
        kind: "success",
        title: t("forms.saved"),
        detail: androidTarget ? androidTarget.name : output,
      });
    } catch (error) {
      reportError(error, t);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Screen
      title={t("forms.title")}
      subtitle={t("forms.subtitle")}
      actions={
        <div className="row">
          <button type="button" className="btn btn-soft" onClick={pick}>
            {t("forms.pickPdf")}
          </button>
          <button type="button" className="btn btn-soft" onClick={addField}>
            <Plus size={15} /> {t("forms.addField")}
          </button>
          <button
            type="button"
            className="btn btn-primary"
            onClick={save}
            disabled={!path || busy || fields.length === 0}
          >
            <Save size={15} /> {t("forms.save")}
          </button>
        </div>
      }
    >
      <div className="card">
        <p className="mono">{path || t("forms.noPdf")}</p>
        <table className="table">
          <thead>
            <tr>
              <th>{t("forms.type")}</th>
              <th>{t("forms.name")}</th>
              <th>{t("forms.page")}</th>
              <th>X</th>
              <th>Y</th>
              <th>W</th>
              <th>H</th>
              <th>{t("forms.value")}</th>
              <th>{t("forms.options")}</th>
              <th>
                <span className="sr-only">{t("common.actions")}</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {fields.map((field, index) => (
              <tr key={index}>
                <td>
                  <select
                    aria-label={t("forms.type")}
                    value={field.kind}
                    onChange={(event) =>
                      setFields(
                        fields.map((candidate, position) =>
                          position === index ? { ...candidate, kind: event.target.value } : candidate,
                        ),
                      )
                    }
                  >
                    <option value="text">text</option>
                    <option value="checkbox">checkbox</option>
                    <option value="radio">radio</option>
                    <option value="dropdown">dropdown</option>
                  </select>
                </td>
                <td>
                  <input
                    aria-label={t("forms.name")}
                    value={field.name}
                    onChange={(event) =>
                      setFields(
                        fields.map((candidate, position) =>
                          position === index ? { ...candidate, name: event.target.value } : candidate,
                        ),
                      )
                    }
                  />
                </td>
                {(["page", "x", "y", "w", "h"] as const).map((key) => (
                  <td key={key}>
                    <input
                      type="number"
                      aria-label={t(`forms.${key}`)}
                      value={field[key]}
                      onChange={(event) =>
                        setFields(
                          fields.map((candidate, position) =>
                            position === index ? { ...candidate, [key]: Number(event.target.value) } : candidate,
                          ),
                        )
                      }
                    />
                  </td>
                ))}
                <td>
                  <input
                    aria-label={t("forms.value")}
                    value={field.value}
                    onChange={(event) =>
                      setFields(
                        fields.map((candidate, position) =>
                          position === index ? { ...candidate, value: event.target.value } : candidate,
                        ),
                      )
                    }
                  />
                </td>
                <td>
                  <input
                    aria-label={t("forms.options")}
                    value={field.options.join(",")}
                    placeholder="A,B,C"
                    onChange={(event) =>
                      setFields(
                        fields.map((candidate, position) =>
                          position === index
                            ? {
                                ...candidate,
                                options: event.target.value
                                  .split(",")
                                  .map((option) => option.trim())
                                  .filter(Boolean),
                              }
                            : candidate,
                        ),
                      )
                    }
                  />
                </td>
                <td>
                  <button
                    type="button"
                    className="icon-btn"
                    onClick={() => setFields(fields.filter((_, position) => position !== index))}
                    aria-label={t("forms.removeField")}
                  >
                    <Trash2 size={13} />
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        <p className="muted">{t("forms.hint")}</p>
      </div>
    </Screen>
  );
}
