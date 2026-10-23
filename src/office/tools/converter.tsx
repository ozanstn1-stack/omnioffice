/** Universal converter: images/PDF/office inputs through the real engines. */
import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { appCacheDir, join } from "@tauri-apps/api/path";
import { Check, Eye, EyeOff, Plus, RefreshCw } from "lucide-react";
import { useT } from "../../lib/i18n";
import { fileBaseName } from "../../lib/format";
import { pickDirectoryToken, pickDocumentTokens } from "../../lib/file-tokens";
import { errorMessage } from "../../lib/store";
import { toAppError } from "../../lib/api";
import { isAndroid, pickAndroidFolder, pickOfficeFiles, publishOutputs, type AndroidTarget } from "../../lib/mobile";
import { flagEnabled } from "../../lib/flags";
import { uid } from "../../lib/office-types";
import * as api from "../../lib/office-api";
import { Screen } from "../../components/layout";
import { Field } from "../../components/ui";
import { compatibilityReport, gatingLossItems } from "../../components/compatibility";
import { useDataLossPrompt } from "../../components/data-loss-dialog";

// ---------------------------------------------------------------------------
// Universal converter
// ---------------------------------------------------------------------------

function PasswordField({
  value,
  onChange,
  placeholder,
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
}) {
  const [visible, setVisible] = useState(false);
  return (
    <div className="relative">
      <input
        className="input pr-10"
        type={visible ? "text" : "password"}
        value={value}
        placeholder={placeholder}
        onChange={(event) => onChange(event.target.value)}
        autoComplete="off"
        spellCheck={false}
      />
      <button
        type="button"
        className="absolute right-1 top-1/2 -translate-y-1/2 icon-btn"
        onClick={() => setVisible((previous) => !previous)}
        aria-label={visible ? "Hide" : "Show"}
        tabIndex={-1}
      >
        {visible ? <EyeOff size={15} /> : <Eye size={15} />}
      </button>
    </div>
  );
}

export function ConverterScreen() {
  const t = useT();
  const [files, setFiles] = useState<string[]>([]);
  const [target, setTarget] = useState("pdf");
  const [targets, setTargets] = useState<string[]>([
    "pdf",
    "docx",
    "xlsx",
    "pptx",
    "odt",
    "ods",
    "odp",
    "csv",
    "html",
    "oswk",
  ]);
  const [outputDir, setOutputDir] = useState("");
  const [androidDir, setAndroidDir] = useState<AndroidTarget | null>(null);
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [results, setResults] = useState<Array<{ input: string; output: string; ok: boolean; detail: string }>>([]);

  const hasPdfInput = files.some((file) => /\.pdf$/i.test(file));
  const showTablesHint = hasPdfInput && target === "docx";

  const pickFiles = async () => {
    let list: string[] = [];
    if (isAndroid()) {
      // The desktop dialog plugin cannot open the Android picker; the SAF
      // bridge copies the chosen documents into the app cache as real paths.
      list = await pickOfficeFiles(true).catch(() => []);
    } else {
      list = (
        await pickDocumentTokens(
          [
            "docx",
            "docm",
            "dotx",
            "odt",
            "rtf",
            "txt",
            "md",
            "html",
            "xlsx",
            "xlsm",
            "xls",
            "ods",
            "csv",
            "tsv",
            "pptx",
            "pptm",
            "odp",
            "oswk",
            ...(flagEnabled("experimentalUdf") ? ["udf"] : []),
            "pdf",
          ],
          true,
        ).catch(() => [])
      ).map((file) => file.token);
    }
    setFiles(list);
    if (list[0]) {
      const extension = fileBaseName(list[0]).split(".").pop() ?? "";
      const available = await api.conversionTargets(extension).catch(() => []);
      if (available.length > 0) {
        setTargets(available);
        setTarget(available[0]);
      }
    }
  };

  const pickOutputDir = async () => {
    if (isAndroid()) {
      const target = await pickAndroidFolder().catch(() => null);
      if (target) setAndroidDir(target);
      return;
    }
    const target = await pickDirectoryToken().catch(() => null);
    if (target) setOutputDir(target.token);
  };

  const run = async () => {
    if (files.length === 0) return;
    setBusy(true);
    // Data Loss Protection covers the converter as well: convert a file to any
    // non-native target only after the compatibility report has been shown.
    // The report needs the parsed model, so office inputs are opened read-only
    // for the check; PDFs and unreadable files skip it (fail-open).
    let effectiveTarget = target;
    if (target !== "oswk") {
      for (const input of files) {
        try {
          const opened = await api.openDocument(input);
          if (opened.kind !== "writer" && opened.kind !== "calc" && opened.kind !== "impress") continue;
          const report = await compatibilityReport(opened.kind, opened.model, target);
          if (gatingLossItems(target, report).length === 0) continue;
          const choice = await useDataLossPrompt.getState().ask(target, report);
          if (choice === "cancel") {
            setBusy(false);
            return;
          }
          if (choice === "oswk") {
            effectiveTarget = "oswk";
            setTarget("oswk");
          }
          break;
        } catch {
          // Not an office document we can inspect (e.g. PDF) - convert as asked.
        }
      }
    }
    const converted: typeof results = [];
    // Android cannot write next to the picked document (SAF paths), so the
    // converter stages its output in the app cache and copies each finished
    // file to the chosen folder or to Downloads afterwards.
    let stageDir = "";
    if (isAndroid()) {
      const cache = await appCacheDir().catch(() => "");
      if (cache) {
        stageDir = await join(cache, "converts", uid());
        await invoke("ensure_dir", { path: stageDir }).catch(() => undefined);
      }
    }
    for (const input of files) {
      const base = input.replace(/\.[^.\\/]+$/, "");
      const directory = isAndroid() ? stageDir : outputDir || input.replace(/[\\/][^\\/]+$/, "");
      const output = `${base}.${effectiveTarget === "html" ? "html" : effectiveTarget}`.replace(
        /^.*[\\/]/,
        `${directory}/`,
      );
      try {
        const info = await api.convertFile(input, output, password ? { password } : undefined);
        if (isAndroid()) {
          const failures = await publishOutputs([info.output], androidDir ? { dir: androidDir } : undefined);
          if (failures.length > 0) {
            converted.push({ input, output: info.output, ok: false, detail: failures[0].error });
          } else {
            converted.push({ input, output: info.output, ok: true, detail: info.warnings.join(" ") });
          }
        } else {
          converted.push({ input, output: info.output, ok: true, detail: info.warnings.join(" ") });
        }
      } catch (error) {
        const appError = toAppError(error);
        const passwordFailure = appError.code === "password_required" || appError.code === "wrong_password";
        converted.push({
          input,
          output: directory,
          ok: false,
          detail: passwordFailure ? appError.message : errorMessage(error, t),
        });
      }
    }
    setResults(converted);
    setBusy(false);
  };

  return (
    <Screen
      title={t("converter.title")}
      subtitle={t("converter.subtitle")}
      actions={
        <div className="row">
          <button type="button" className="btn btn-soft" onClick={pickFiles}>
            <Plus size={15} /> {t("converter.pickFiles")}
          </button>
          <button type="button" className="btn btn-soft" onClick={pickOutputDir}>
            {t("converter.outputFolder")}
          </button>
          <button type="button" className="btn btn-primary" onClick={run} disabled={busy || files.length === 0}>
            <RefreshCw size={15} /> {t("converter.convert")}
          </button>
        </div>
      }
    >
      <div className="card">
        <div className="row">
          <label className="field">
            <span>{t("converter.targetFormat")}</span>
            <select value={target} onChange={(event) => setTarget(event.target.value)}>
              {targets.map((candidate) => (
                <option key={candidate} value={candidate}>
                  {candidate.toUpperCase()}
                </option>
              ))}
            </select>
          </label>
          <label className="field grow">
            <span>{t("converter.outputFolder")}</span>
            {isAndroid() ? (
              <input
                readOnly
                value={
                  androidDir
                    ? t("common.androidChosenDestination", { name: androidDir.name })
                    : t("common.androidDefaultDestination")
                }
              />
            ) : (
              <input
                value={outputDir}
                placeholder={t("converter.sameFolder")}
                onChange={(event) => setOutputDir(event.target.value)}
              />
            )}
          </label>
        </div>
        {hasPdfInput ? (
          <div className="row" style={{ marginTop: 10 }}>
            <Field label={t("converter.password")} hint={t("converter.passwordHint")} className="grow">
              <PasswordField value={password} onChange={setPassword} placeholder={t("converter.password")} />
            </Field>
          </div>
        ) : null}
        {showTablesHint ? <p className="muted small">{t("converter.tablesHint")}</p> : null}
        <ul className="file-pick-list">
          {files.map((file) => (
            <li key={file}>{file}</li>
          ))}
          {files.length === 0 ? <li className="muted">{t("converter.noFiles")}</li> : null}
        </ul>
      </div>
      {results.length > 0 ? (
        <div className="card">
          <h3>{t("converter.results")}</h3>
          {results.map((result) => (
            <div key={result.input} className="row">
              <span className={result.ok ? "badge-ok" : "badge-danger"}>{result.ok ? <Check size={12} /> : "!"}</span>
              <span className="grow">{result.input}</span>
              <span className="muted">{result.detail || result.output}</span>
            </div>
          ))}
        </div>
      ) : null}
      <p className="muted">{t("converter.hint")}</p>
    </Screen>
  );
}
