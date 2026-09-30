import { useState } from "react";
import { Image as ImageIcon, Stamp } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { isAndroid, pickAndroidFiles } from "../lib/mobile";
import { Card, Field, Segmented, Slider, Toggle } from "../components/ui";
import { DropZone, FileList, InfoStrip, OutputBar, ResultCard } from "../components/files";
import { PageCanvas } from "../components/pages";
import { OptionCard, Screen, TwoColumn } from "../components/layout";
import { useT } from "../lib/i18n";
import { useTool } from "../lib/useTool";
import { watermarkPdf } from "../lib/api";
import { parsePageList } from "../lib/format";
import type { WatermarkOptions, WatermarkPosition } from "../lib/types";

const POSITIONS: WatermarkPosition[] = [
  "top_left",
  "top_center",
  "top_right",
  "center",
  "bottom_left",
  "bottom_center",
  "bottom_right",
];

export function Watermark({ initialFiles, dragging }: { initialFiles?: string[]; dragging: boolean }) {
  const t = useT();
  const session = useTool({ suffix: "_watermarked", accept: "pdf", initialPaths: initialFiles });
  const [options, setOptions] = useState<WatermarkOptions>({
    kind: "text",
    text: "CONFIDENTIAL",
    font_size_pt: 48,
    bold: true,
    color: "#9aa0a6",
    opacity: 0.25,
    rotation_deg: 45,
    position: "center",
    margin_pt: 24,
    tile: false,
    image_path: null,
    image_scale: 0.35,
    pages: [],
  });
  const [pagesText, setPagesText] = useState("");
  const [keepSignatures, setKeepSignatures] = useState(true);

  const patch = (values: Partial<WatermarkOptions>) => setOptions((previous) => ({ ...previous, ...values }));
  const pageCount = session.info?.pageCount ?? 1;

  const pickImage = async () => {
    if (isAndroid()) {
      const paths = await pickAndroidFiles({ multiple: false, accept: "image" }).catch(() => []);
      if (paths.length) patch({ kind: "image", image_path: paths[0] });
      return;
    }
    const picked = await open({ multiple: false, filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg"] }] });
    if (picked) patch({ kind: "image", image_path: String(picked) });
  };

  session.registerAutoRun(() => void run());
  const run = () => {
    const pages = pagesText.trim() ? (parsePageList(pagesText, pageCount) ?? []) : [];
    return session.run(async (jobId, overwrite) =>
      watermarkPdf(
        session.primary?.path ?? "",
        session.outputSpec(overwrite),
        { ...options, pages },
        jobId,
        session.password || undefined,
        keepSignatures,
      ),
    );
  };

  const positionOptions: { value: WatermarkPosition; label: string }[] = POSITIONS.map((position) => ({
    value: position,
    label: position
      .split("_")
      .map((part) => part[0].toUpperCase() + part.slice(1))
      .join(" "),
  }));

  return (
    <Screen
      title={t("nav.watermark")}
      subtitle={t("watermark.subtitle")}
      actions={<Stamp size={18} className="muted" />}
    >
      <TwoColumn
        main={
          !session.primary ? (
            <DropZone onPaths={(paths) => void session.addPaths(paths)} dragging={dragging} accept="pdf" />
          ) : (
            <>
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
                </Card>
              ) : null}
              <Card className="p-4">
                <p className="text-xs muted mb-3">
                  {t("common.preview")} · {t("common.page")} 1
                </p>
                <div className="mx-auto" style={{ maxWidth: 520 }}>
                  <PageCanvas
                    path={session.primary.path}
                    page={1}
                    password={session.password || undefined}
                    maxWidth={900}
                    overlay={
                      options.tile ? (
                        <div
                          className="absolute inset-0 flex flex-wrap items-center justify-center overflow-hidden"
                          style={{ transform: "rotate(-45deg)", opacity: options.opacity }}
                        >
                          {Array.from({ length: 12 }).map((_, index) => (
                            <span
                              key={index}
                              style={{
                                color: options.color,
                                fontWeight: options.bold ? 700 : 500,
                                fontSize: `${Math.max(10, options.font_size_pt * 0.55)}px`,
                                padding: "18px 30px",
                                whiteSpace: "nowrap",
                              }}
                            >
                              {options.kind === "text" ? options.text : t("annotate.imageTool")}
                            </span>
                          ))}
                        </div>
                      ) : (
                        <div
                          className="absolute inset-0 flex"
                          style={{
                            alignItems: options.position.startsWith("top")
                              ? "flex-start"
                              : options.position.startsWith("bottom")
                                ? "flex-end"
                                : "center",
                            justifyContent: options.position.endsWith("left")
                              ? "flex-start"
                              : options.position.endsWith("right")
                                ? "flex-end"
                                : "center",
                            padding: `${options.margin_pt * 1.2}px`,
                          }}
                        >
                          <span
                            style={{
                              color: options.color,
                              opacity: options.opacity,
                              fontWeight: options.bold ? 700 : 500,
                              fontSize: `${Math.max(10, options.font_size_pt * 0.6)}px`,
                              transform: `rotate(${-options.rotation_deg}deg)`,
                            }}
                          >
                            {options.kind === "text" ? options.text : t("annotate.imageTool")}
                          </span>
                        </div>
                      )
                    }
                  />
                </div>
              </Card>
            </>
          )
        }
        side={
          <>
            <OutputBar
              session={session}
              runLabel={t("nav.watermark")}
              onRun={() => void run()}
              disabled={!session.primary}
            />
            <OptionCard title={t("metadata.signatures")}>
              <Toggle checked={keepSignatures} onChange={setKeepSignatures} label={t("metadata.keepSignatures")} />
              <p className="muted small">{t("annotate.keepSignaturesHint")}</p>
            </OptionCard>
            <OptionCard title={t("nav.watermark")}>
              <Segmented<WatermarkOptions["kind"]>
                value={options.kind}
                onChange={(value) => patch({ kind: value })}
                options={[
                  { value: "text", label: t("annotate.textTool") },
                  { value: "image", label: t("annotate.imageTool") },
                ]}
              />
              {options.kind === "text" ? (
                <>
                  <Field label={t("annotate.textPlaceholder")}>
                    <textarea
                      className="textarea"
                      rows={2}
                      value={options.text}
                      onChange={(event) => patch({ text: event.target.value })}
                    />
                  </Field>
                  <Field label={t("annotate.fontSize")}>
                    <Slider
                      value={options.font_size_pt}
                      min={12}
                      max={160}
                      onChange={(value) => patch({ font_size_pt: value })}
                    />
                  </Field>
                  <Toggle
                    checked={options.bold}
                    onChange={(value) => patch({ bold: value })}
                    label={t("annotate.bold")}
                  />
                </>
              ) : (
                <>
                  <button className="btn btn-sm self-start" type="button" onClick={() => void pickImage()}>
                    <ImageIcon size={14} />{" "}
                    {options.image_path ? options.image_path.split(/[\\/]/).pop() : t("annotate.imageFile")}
                  </button>
                  <Field label={t("annotate.fontSize")}>
                    <Slider
                      value={Math.round(options.image_scale * 100)}
                      min={5}
                      max={100}
                      onChange={(value) => patch({ image_scale: value / 100 })}
                      format={(value) => `${value}%`}
                    />
                  </Field>
                </>
              )}
              <Field label={t("common.size")}>
                <Slider
                  value={options.opacity * 100}
                  min={5}
                  max={100}
                  onChange={(value) => patch({ opacity: value / 100 })}
                  format={(value) => `${value}%`}
                />
              </Field>
              <Field label={t("common.rotation")}>
                <Slider
                  value={options.rotation_deg}
                  min={0}
                  max={90}
                  step={5}
                  onChange={(value) => patch({ rotation_deg: value })}
                  format={(value) => `${value}°`}
                />
              </Field>
              <Field label={t("pageTools.numberingPosition")}>
                <select
                  className="select"
                  value={options.position}
                  onChange={(event) => patch({ position: event.target.value as WatermarkPosition })}
                >
                  {positionOptions.map((option) => (
                    <option key={option.value} value={option.value}>
                      {option.label}
                    </option>
                  ))}
                </select>
              </Field>
              <Field label={t("convert.margin")}>
                <Slider value={options.margin_pt} min={0} max={120} onChange={(value) => patch({ margin_pt: value })} />
              </Field>
              <Toggle checked={options.tile} onChange={(value) => patch({ tile: value })} label="Tile across page" />
              <Field label={t("pageTools.selection")} hint={t("pageTools.selectionHint")}>
                <input
                  className="input"
                  value={pagesText}
                  onChange={(event) => setPagesText(event.target.value)}
                  placeholder={t("common.all")}
                />
              </Field>
            </OptionCard>
            {session.result ? <ResultCard result={session.result} onReset={session.resetResult} /> : null}
          </>
        }
      />
    </Screen>
  );
}
