import { useMemo, useState } from "react";
import { AlertTriangle, FileSearch } from "lucide-react";
import { Badge, Card, Field, Segmented, Slider, Toggle } from "../components/ui";
import { DropZone, FileList, InfoStrip, OutputBar, ResultCard } from "../components/files";
import { OptionCard, Screen, TwoColumn } from "../components/layout";
import { useT } from "../lib/i18n";
import { useTool } from "../lib/useTool";
import { ocrPdf } from "../lib/api";
import { useSettings } from "../lib/store";
import type { OcrOptions, OcrPreprocess } from "../lib/types";

export function Ocr({ initialFiles, dragging }: { initialFiles?: string[]; dragging: boolean }) {
  const t = useT();
  const settings = useSettings((s) => s.settings);
  const engine = useSettings((s) => s.engine);
  const languages = useSettings((s) => s.languages);
  const session = useTool({ suffix: "_ocr", accept: "pdf", initialPaths: initialFiles });

  const [selectedLanguages, setSelectedLanguages] = useState<string[]>(
    settings.ocrLanguages.length ? settings.ocrLanguages : ["eng"],
  );
  const [psm, setPsm] = useState(3);
  const [outputMode, setOutputMode] = useState<OcrOptions["output_mode"]>("searchable_pdf");
  const [dpi, setDpi] = useState(300);
  const [skipText, setSkipText] = useState(true);
  const [preprocess, setPreprocess] = useState<OcrPreprocess>({
    auto_rotate: true,
    deskew: true,
    contrast: true,
    denoise: false,
    binarize: false,
    grayscale: false,
  });

  // Settings-supplied languages seed the selection once they load. Derived
  // during render from the settings value so there is no setState cascade.
  const [languagesTouched, setLanguagesTouched] = useState(false);
  const [lastSettingsLanguages, setLastSettingsLanguages] = useState(settings.ocrLanguages);
  if (!languagesTouched && settings.ocrLanguages.length && settings.ocrLanguages !== lastSettingsLanguages) {
    setLastSettingsLanguages(settings.ocrLanguages);
    setSelectedLanguages(settings.ocrLanguages);
  }

  const toggleLanguage = (code: string) => {
    setLanguagesTouched(true);
    setSelectedLanguages((previous) =>
      previous.includes(code) ? previous.filter((value) => value !== code) : [...previous, code],
    );
  };

  const outputHint = useMemo(() => {
    const base = session.outputPath.replace(/\.pdf$/i, "");
    if (outputMode === "text") return `${base}.txt`;
    if (outputMode === "markdown") return `${base}.md`;
    return session.outputPath;
  }, [outputMode, session.outputPath]);

  session.registerAutoRun(() => void run());
  const run = () =>
    session.run(async (jobId, overwrite) => {
      const options: OcrOptions = {
        languages: selectedLanguages,
        psm,
        dpi,
        output_mode: outputMode,
        pages: [],
        preprocess,
        skip_text_pages: skipText,
      };
      return ocrPdf(
        session.primary?.path ?? "",
        session.outputSpec(overwrite),
        options,
        jobId,
        session.password || undefined,
      );
    });

  const engineMissing = engine !== null && !engine.tesseract;

  return (
    <Screen
      title={t("ocr.title")}
      subtitle={t("ocr.subtitle")}
      actions={engine?.tesseract_version ? <Badge tone="ok">{engine.tesseract_version}</Badge> : null}
    >
      <TwoColumn
        main={
          !session.primary ? (
            <DropZone onPaths={(paths) => void session.addPaths(paths)} dragging={dragging} accept="pdf" />
          ) : (
            <>
              {engineMissing ? (
                <Card className="p-4 flex items-start gap-3" soft>
                  <AlertTriangle size={18} style={{ color: "var(--warn)", marginTop: 2 }} />
                  <div>
                    <p className="font-semibold text-[13.5px]">{t("ocr.noEngine")}</p>
                    <p className="text-xs muted mt-1">{t("ocr.noEngineBody")}</p>
                  </div>
                </Card>
              ) : null}

              <OptionCard>
                <FileList
                  files={session.files}
                  onRemove={session.removeFile}
                  onAdd={session.pickFiles}
                  addLabel={t("common.addPdf")}
                />
              </OptionCard>
              {session.info ? (
                <Card className="p-4">
                  <InfoStrip info={session.info} error={session.infoError} />
                  {session.info.hasTextLayer ? (
                    <p className="text-xs muted mt-2">
                      {settings.language === "tr"
                        ? "Bu belgede zaten bir metin katmanı var — yalnızca taranmış sayfalar için OCR önerilir."
                        : "This document already contains a text layer — OCR is mainly useful for scanned pages."}
                    </p>
                  ) : null}
                </Card>
              ) : null}

              <OptionCard title={t("ocr.languages")}>
                <div className="flex flex-wrap gap-2">
                  {languages.length === 0 ? (
                    <p className="text-xs muted">{t("common.loading")}</p>
                  ) : (
                    languages.map((language) => (
                      <button
                        key={language.code}
                        type="button"
                        className="badge"
                        style={{
                          cursor: "pointer",
                          background: selectedLanguages.includes(language.code) ? "var(--accent)" : "var(--surface-3)",
                          color: selectedLanguages.includes(language.code) ? "var(--accent-text)" : "var(--muted)",
                          padding: "6px 12px",
                        }}
                        onClick={() => toggleLanguage(language.code)}
                      >
                        {language.name}
                      </button>
                    ))
                  )}
                </div>
                <p className="text-xs muted">
                  {t("ocr.skipText")}: {skipText ? t("info.yes") : t("info.no")}
                </p>
              </OptionCard>

              <OptionCard title={t("ocr.preprocess")}>
                <Toggle
                  checked={preprocess.auto_rotate}
                  onChange={(value) => setPreprocess({ ...preprocess, auto_rotate: value })}
                  label={t("ocr.autoRotate")}
                />
                <Toggle
                  checked={preprocess.deskew}
                  onChange={(value) => setPreprocess({ ...preprocess, deskew: value })}
                  label={t("ocr.deskew")}
                />
                <Toggle
                  checked={preprocess.contrast}
                  onChange={(value) => setPreprocess({ ...preprocess, contrast: value })}
                  label={t("ocr.contrast")}
                />
                <Toggle
                  checked={preprocess.denoise}
                  onChange={(value) => setPreprocess({ ...preprocess, denoise: value })}
                  label={t("ocr.denoise")}
                />
                <Toggle
                  checked={preprocess.binarize}
                  onChange={(value) => setPreprocess({ ...preprocess, binarize: value })}
                  label={t("ocr.binarize")}
                />
                <Toggle
                  checked={preprocess.grayscale}
                  onChange={(value) => setPreprocess({ ...preprocess, grayscale: value })}
                  label={t("ocr.grayscale")}
                />
              </OptionCard>
            </>
          )
        }
        side={
          <>
            <OutputBar
              session={session}
              runLabel={t("ocr.run")}
              onRun={() => void run()}
              disabled={!session.primary || engineMissing}
            />
            {session.primary ? (
              <>
                <OptionCard title={t("ocr.outputMode")}>
                  <Segmented<OcrOptions["output_mode"]>
                    value={outputMode}
                    onChange={setOutputMode}
                    options={[
                      { value: "searchable_pdf", label: t("ocr.searchablePdf") },
                      { value: "text", label: t("ocr.textFile") },
                      { value: "markdown", label: t("ocr.markdownFile") },
                    ]}
                  />
                  <p className="text-xs muted break-all">{outputHint}</p>
                  <label className="checkbox mt-1">
                    <input type="checkbox" checked={skipText} onChange={(event) => setSkipText(event.target.checked)} />
                    <span>{t("ocr.skipText")}</span>
                  </label>
                </OptionCard>
                <OptionCard title={t("common.advanced")}>
                  <Field label={t("ocr.psm")}>
                    <select className="select" value={psm} onChange={(event) => setPsm(Number(event.target.value))}>
                      <option value={3}>{t("ocr.psmAuto")}</option>
                      <option value={1}>Auto + OSD</option>
                      <option value={6}>{t("ocr.psmSingleBlock")}</option>
                      <option value={7}>{t("ocr.psmSingleLine")}</option>
                      <option value={11}>Sparse text</option>
                    </select>
                  </Field>
                  <Field label={t("ocr.dpi")}>
                    <Slider
                      value={dpi}
                      min={150}
                      max={450}
                      step={25}
                      onChange={setDpi}
                      format={(value) => `${value}`}
                    />
                  </Field>
                  <p className="text-xs muted flex items-center gap-1.5">
                    <FileSearch size={12} />
                    {t("progress.page")}: {t("progress.processing")}
                  </p>
                </OptionCard>
              </>
            ) : null}
            {session.result ? <ResultCard result={session.result} onReset={session.resetResult} /> : null}
          </>
        }
      />
    </Screen>
  );
}
