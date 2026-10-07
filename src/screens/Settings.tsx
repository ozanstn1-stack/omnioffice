import { useEffect, useState } from "react";
import {
  AlertTriangle,
  CheckCircle2,
  FileSearch,
  FileText,
  HardDrive,
  KeyRound,
  ShieldCheck,
  Sparkles,
} from "lucide-react";
import { Badge, Button, Card, Kbd } from "../components/ui";
import { Screen } from "../components/layout";
import { useT } from "../lib/i18n";
import { reportError, useSettings, useToasts } from "../lib/store";
import { useUpdate } from "../lib/update";
import { AiSettings } from "../components/ai-settings";
import { appInfo, diagnosticsReport, updateOpen } from "../lib/api";
import { saveFileBytes } from "../lib/mobile";

function VersionLine() {
  const t = useT();
  const [version, setVersion] = useState("");
  useEffect(() => {
    void appInfo()
      .then((info) => setVersion(`v${info.appVersion} · core ${info.coreVersion} · ${info.platform}`))
      .catch(() => setVersion("v1.2.0"));
  }, []);
  return (
    <p className="text-xs muted">
      {t("settings.version")}: {version || "…"}
    </p>
  );
}

/** Weekly update check toggle plus a manual "check now". */
function UpdateControls() {
  const t = useT();
  const enabled = useSettings((s) => s.settings.updateCheck);
  const updateSettings = useSettings((s) => s.update);
  const info = useUpdate((s) => s.info);
  const checking = useUpdate((s) => s.checking);
  const error = useUpdate((s) => s.error);
  const check = useUpdate((s) => s.check);
  const [ran, setRan] = useState(false);

  const result =
    !ran || checking
      ? ""
      : error
        ? t("update.failed", { error })
        : info?.newer
          ? t("update.available", { version: info.latest })
          : info
            ? t("update.upToDate", { version: info.current })
            : "";

  return (
    <div className="flex flex-col gap-2">
      <label className="checkbox">
        <input
          type="checkbox"
          checked={enabled}
          onChange={(event) => void updateSettings({ updateCheck: event.target.checked })}
        />
        <span>{t("update.weekly")}</span>
      </label>
      <div className="flex items-center gap-2 flex-wrap">
        <Button
          size="sm"
          variant="ghost"
          disabled={checking}
          onClick={() => {
            setRan(true);
            void check(true);
          }}
        >
          {checking ? t("update.checking") : t("update.checkNow")}
        </Button>
        {ran && info?.newer && !checking ? (
          <Button
            size="sm"
            variant="primary"
            onClick={() =>
              void updateOpen(info.downloadUrl ?? info.releaseUrl).catch((err: unknown) => reportError(err, t))
            }
          >
            {info.downloadUrl ? t("update.download") : t("update.openRelease")}
          </Button>
        ) : null}
        <span className="text-xs muted" role="status">
          {result}
        </span>
      </div>
      <p className="text-xs muted">{t("update.hint")}</p>
    </div>
  );
}

/** Writes a redacted plain-text report the user can attach to an issue. */
function DiagnosticsExport() {
  const t = useT();
  const [busy, setBusy] = useState(false);
  const exportReport = async () => {
    setBusy(true);
    try {
      const report = await diagnosticsReport();
      const stamp = new Date().toISOString().slice(0, 10);
      const saved = await saveFileBytes(new TextEncoder().encode(report), `omnioffice-diagnostics-${stamp}.txt`, {
        name: "Text",
        extensions: ["txt"],
      });
      if (saved) useToasts.getState().push({ kind: "success", title: t("settings.diagnosticsSaved"), detail: saved });
    } catch (error) {
      reportError(error, t);
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="flex flex-col gap-2">
      <div>
        <Button size="sm" variant="ghost" disabled={busy} onClick={() => void exportReport()}>
          {t("settings.diagnosticsExport")}
        </Button>
      </div>
      <p className="text-xs muted">{t("settings.diagnosticsHint")}</p>
    </div>
  );
}

export function Settings() {
  const t = useT();
  const settings = useSettings((s) => s.settings);
  const engine = useSettings((s) => s.engine);
  const languages = useSettings((s) => s.languages);
  const update = useSettings((s) => s.update);

  const toggleLanguage = (code: string) => {
    const next = settings.ocrLanguages.includes(code)
      ? settings.ocrLanguages.filter((value) => value !== code)
      : [...settings.ocrLanguages, code];
    void update({ ocrLanguages: next.length ? next : ["eng"] });
  };

  const shortcuts: [string, string][] = [
    ["Ctrl + Shift + P", t("palette.open")],
    ["Ctrl + Shift + F", t("palette.search")],
    ["Ctrl + O", t("common.openFile")],
    ["Ctrl + ,", t("nav.settings")],
    ["Ctrl + S", t("common.save")],
    ["Ctrl + Z", t("common.undo")],
    ["Ctrl + Y", t("common.redo")],
  ];

  return (
    <Screen title={t("settings.title")} subtitle={t("settings.subtitle")}>
      <div className="grid gap-4" style={{ gridTemplateColumns: "repeat(auto-fit, minmax(min(340px, 100%), 1fr))" }}>
        <Card className="p-5 flex flex-col gap-4">
          <h3 className="font-semibold flex items-center gap-2">
            <Sparkles size={16} style={{ color: "var(--accent)" }} /> {t("settings.appearance")}
          </h3>
          <div>
            <label className="label">{t("settings.theme")}</label>
            <div className="seg">
              {(["dark", "light", "system", "midnight", "paper"] as const).map((theme) => (
                <button key={theme} data-active={settings.theme === theme} onClick={() => void update({ theme })}>
                  {t(`settings.${theme}`)}
                </button>
              ))}
            </div>
          </div>
          <div>
            <label className="label">{t("settings.language")}</label>
            <div className="seg">
              <button data-active={settings.language === "en"} onClick={() => void update({ language: "en" })}>
                English
              </button>
              <button data-active={settings.language === "tr"} onClick={() => void update({ language: "tr" })}>
                Türkçe
              </button>
            </div>
          </div>
        </Card>

        <Card className="p-5 flex flex-col gap-4">
          <h3 className="font-semibold flex items-center gap-2">
            <HardDrive size={16} style={{ color: "var(--accent)" }} /> {t("settings.defaults")}
          </h3>
          <div>
            <label className="label">{t("settings.defaultOutputDir")}</label>
            <input
              className="input"
              value={settings.defaultOutputDir}
              placeholder={t("settings.sameAsInput")}
              onChange={(event) => void update({ defaultOutputDir: event.target.value })}
            />
          </div>
          <div>
            <label className="label">{t("settings.defaultCompression")}</label>
            <div className="seg">
              {(["low", "medium", "high"] as const).map((preset) => (
                <button
                  key={preset}
                  data-active={settings.defaultCompression === preset}
                  onClick={() => void update({ defaultCompression: preset })}
                >
                  {t(`compress.${preset}`)}
                </button>
              ))}
            </div>
          </div>
          <div>
            <label className="label">{t("settings.defaultImageDpi")}</label>
            <div className="seg">
              {[72, 150, 300].map((dpi) => (
                <button
                  key={dpi}
                  data-active={settings.defaultImageDpi === dpi}
                  onClick={() => void update({ defaultImageDpi: dpi })}
                >
                  {dpi}
                </button>
              ))}
            </div>
          </div>
          <div>
            <label className="label">{t("settings.defaultExportFormat")}</label>
            <div className="seg">
              {(["jpg", "png"] as const).map((format) => (
                <button
                  key={format}
                  data-active={settings.defaultExportFormat === format}
                  onClick={() => void update({ defaultExportFormat: format })}
                >
                  {format.toUpperCase()}
                </button>
              ))}
            </div>
          </div>
          <div>
            <label className="label">{t("settings.autosave")}</label>
            <div className="seg">
              {[0, 15, 30, 60, 300].map((seconds) => (
                <button
                  key={seconds}
                  data-active={(settings.autosaveSeconds ?? 30) === seconds}
                  onClick={() => void update({ autosaveSeconds: seconds })}
                >
                  {seconds === 0 ? t("settings.autosaveOff") : seconds < 60 ? `${seconds} s` : `${seconds / 60} min`}
                </button>
              ))}
            </div>
            <p className="text-xs muted">{t("settings.autosaveHint")}</p>
          </div>
        </Card>

        <Card className="p-5 flex flex-col gap-4">
          <h3 className="font-semibold flex items-center gap-2">
            <FileText size={16} style={{ color: "var(--accent)" }} /> {t("settings.officeDefaults")}
          </h3>
          <div>
            <label className="label">{t("settings.defaultWriterFormat")}</label>
            <div className="seg">
              {(["docx", "odt"] as const).map((format) => (
                <button
                  key={format}
                  data-active={settings.defaultWriterFormat === format}
                  onClick={() => void update({ defaultWriterFormat: format })}
                >
                  {format.toUpperCase()}
                </button>
              ))}
            </div>
          </div>
          <div>
            <label className="label">{t("settings.defaultCalcFormat")}</label>
            <div className="seg">
              {(["xlsx", "ods"] as const).map((format) => (
                <button
                  key={format}
                  data-active={settings.defaultCalcFormat === format}
                  onClick={() => void update({ defaultCalcFormat: format })}
                >
                  {format.toUpperCase()}
                </button>
              ))}
            </div>
          </div>
          <div>
            <label className="label">{t("settings.defaultImpressFormat")}</label>
            <div className="seg">
              {(["pptx", "odp"] as const).map((format) => (
                <button
                  key={format}
                  data-active={settings.defaultImpressFormat === format}
                  onClick={() => void update({ defaultImpressFormat: format })}
                >
                  {format.toUpperCase()}
                </button>
              ))}
            </div>
          </div>
          <label className="checkbox">
            <input
              type="checkbox"
              checked={settings.versionHistory}
              onChange={(event) => void update({ versionHistory: event.target.checked })}
            />
            <span>{t("settings.versionHistory")}</span>
          </label>
          <label className="checkbox">
            <input
              type="checkbox"
              checked={settings.showImportWarnings}
              onChange={(event) => void update({ showImportWarnings: event.target.checked })}
            />
            <span>{t("settings.showImportWarnings")}</span>
          </label>
        </Card>

        <Card className="p-5 flex flex-col gap-4">
          <h3 className="font-semibold flex items-center gap-2">
            <FileSearch size={16} style={{ color: "var(--accent)" }} /> {t("settings.ocrLanguage")}
          </h3>
          <div className="flex flex-wrap gap-2">
            {languages.map((language) => (
              <button
                key={language.code}
                type="button"
                className="badge"
                style={{
                  cursor: "pointer",
                  padding: "6px 12px",
                  background: settings.ocrLanguages.includes(language.code) ? "var(--accent)" : "var(--surface-3)",
                  color: settings.ocrLanguages.includes(language.code) ? "var(--accent-text)" : "var(--muted)",
                }}
                onClick={() => toggleLanguage(language.code)}
              >
                {language.name}
              </button>
            ))}
          </div>
          <label className="checkbox">
            <input
              type="checkbox"
              checked={settings.showRecentFiles}
              onChange={(event) => void update({ showRecentFiles: event.target.checked })}
            />
            <span>{t("settings.showRecentFiles")}</span>
          </label>
          <label className="checkbox">
            <input
              type="checkbox"
              checked={!settings.onboardingDone}
              onChange={(event) => void update({ onboardingDone: !event.target.checked })}
            />
            <span>{t("settings.showWelcome")}</span>
          </label>
          <label className="checkbox">
            <input
              type="checkbox"
              checked={settings.autoCleanupTemp}
              onChange={(event) => void update({ autoCleanupTemp: event.target.checked })}
            />
            <span>{t("settings.autoCleanupTemp")}</span>
          </label>
        </Card>

        <Card className="p-5 flex flex-col gap-4">
          <h3 className="font-semibold flex items-center gap-2">
            <ShieldCheck size={16} style={{ color: "var(--ok)" }} /> {t("settings.engines")}
          </h3>
          <ul className="flex flex-col gap-2 text-[13px]">
            <li className="flex items-center justify-between">
              <span>{t("settings.enginePdfium")}</span>
              <Badge tone={engine?.pdfium ? "ok" : "danger"}>{engine?.pdfium ? t("info.yes") : t("info.no")}</Badge>
            </li>
            <li className="flex items-center justify-between">
              <span>{t("settings.engineQpdf")}</span>
              <Badge tone={engine?.qpdf ? "ok" : "danger"}>{engine?.qpdf ? t("info.yes") : t("info.no")}</Badge>
            </li>
            <li className="flex items-center justify-between">
              <span>{t("settings.engineTesseract")}</span>
              <Badge tone={engine?.tesseract ? "ok" : "danger"}>
                {engine?.tesseract ? (engine.tesseract_version ?? t("info.yes")) : t("info.no")}
              </Badge>
            </li>
          </ul>
          <p className="text-xs muted">{t("settings.ocrInstalled", { count: engine?.ocr_languages.length ?? 0 })}</p>
        </Card>

        <Card className="p-5 flex flex-col gap-4">
          <h3 className="font-semibold flex items-center gap-2">
            <KeyRound size={16} style={{ color: "var(--accent)" }} /> {t("settings.shortcuts")}
          </h3>
          <ul className="flex flex-col gap-2 text-[13px]">
            {shortcuts.map(([keys, label]) => (
              <li key={keys} className="flex items-center justify-between">
                <span className="muted">{label}</span>
                <span className="flex gap-1">
                  {keys.split(" + ").map((key) => (
                    <Kbd key={`${keys}-${key}`}>{key}</Kbd>
                  ))}
                </span>
              </li>
            ))}
          </ul>
          <p className="text-xs muted">{t("settings.shortcutsHint")}</p>
        </Card>

        <AiSettings />

        <Card className="p-5 flex flex-col gap-3">
          <h3 className="font-semibold flex items-center gap-2">
            {engine?.tesseract ? (
              <CheckCircle2 size={16} style={{ color: "var(--ok)" }} />
            ) : (
              <AlertTriangle size={16} style={{ color: "var(--warn)" }} />
            )}
            {t("settings.privacyTitle")}
          </h3>
          <p className="text-[13px] muted leading-relaxed">{t("settings.privacyBody")}</p>
          <VersionLine />
          <UpdateControls />
          <DiagnosticsExport />
        </Card>
      </div>
    </Screen>
  );
}
