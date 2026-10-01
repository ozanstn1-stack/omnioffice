import { useRef, useState } from "react";
import { ClipboardList, FileImage, Play, Trash2, XCircle } from "lucide-react";
import { Badge, Button, Card, Field, Select, Slider, Toggle } from "../components/ui";
import { DropZone, FileList, ResultCard } from "../components/files";
import { OptionCard, Screen, TwoColumn } from "../components/layout";
import { useT } from "../lib/i18n";
import { useTool } from "../lib/useTool";
import { cancelJob, compressPdf, ocrPdf, pdfToImages, protectPdf, rotatePages, watermarkPdf } from "../lib/api";
import { fileBaseName, formatBytes, joinPath } from "../lib/format";
import { errorMessage, useSettings } from "../lib/store";

type Operation = "compress" | "watermark" | "ocr" | "protect" | "rotate" | "images";

type Status = "pending" | "processing" | "completed" | "failed" | "cancelled";

interface QueueItem {
  path: string;
  name: string;
  status: Status;
  output?: string;
  detail?: string;
}

export function Batch({ initialFiles, dragging }: { initialFiles?: string[]; dragging: boolean }) {
  const t = useT();
  const session = useTool({
    suffix: "_batch",
    accept: "pdf",
    multiple: true,
    loadInfo: false,
    multiOutput: true,
    initialPaths: initialFiles,
  });
  const [operation, setOperation] = useState<Operation>("compress");
  const [queue, setQueue] = useState<QueueItem[]>([]);
  const [running, setRunning] = useState(false);
  const [skipExisting, setSkipExisting] = useState(true);
  const [result, setResult] = useState<{ path: string; pageCount?: number; message?: string } | null>(null);

  // operation options
  const [compressionLevel, setCompressionLevel] = useState<"low" | "medium" | "high">("medium");
  const [watermarkText, setWatermarkText] = useState("CONFIDENTIAL");
  const [password, setPassword] = useState("");
  const [rotateDegrees, setRotateDegrees] = useState<90 | 180 | 270>(90);
  const [ocrLanguage, setOcrLanguage] = useState("eng");
  const [imageFormat, setImageFormat] = useState<"jpeg" | "png">("jpeg");
  const [imageDpi, setImageDpi] = useState(150);
  const batchLanguages = useSettings((state) => state.languages);
  const currentJobRef = useRef<string | null>(null);
  // Monotonic per-run job counter (Date.now is impure and may collide).
  const jobCounter = useRef(0);

  const outputDir = session.outputDir || session.outputPath.replace(/[^\\/]*$/, "");

  const start = async () => {
    if (!session.files.length) return;
    setRunning(true);
    const items: QueueItem[] = session.files.map((file) => ({ path: file.path, name: file.name, status: "pending" }));
    setQueue(items);
    setResult(null);

    for (const [index, item] of items.entries()) {
      setQueue((previous) => previous.map((entry, i) => (i === index ? { ...entry, status: "processing" } : entry)));
      const jobId = `batch-${index}-${jobCounter.current++}`;
      currentJobRef.current = jobId;
      try {
        const stem = fileBaseName(item.path).replace(/\.[^.]+$/, "");
        const output = await runOne(jobId, item.path, stem);
        setQueue((previous) =>
          previous.map((entry, i) =>
            i === index ? { ...entry, status: "completed", output: output ?? undefined } : entry,
          ),
        );
      } catch (error) {
        const message = errorMessage(error, t);
        const cancelled = (error as { code?: string })?.code === "cancelled";
        setQueue((previous) =>
          previous.map((entry, i) =>
            i === index ? { ...entry, status: cancelled ? "cancelled" : "failed", detail: message } : entry,
          ),
        );
        if (cancelled) break;
      }
    }
    currentJobRef.current = null;
    setRunning(false);
  };

  const runOne = async (jobId: string, path: string, stem: string): Promise<string | null> => {
    const targetFor = (suffix: string, extension = "pdf") => joinPath(outputDir, `${stem}${suffix}.${extension}`);
    const target = targetFor("_batch");
    const spec = { path: target, overwrite: (skipExisting ? "unique_name" : "replace") as "unique_name" | "replace" };

    switch (operation) {
      case "compress": {
        const res = await compressPdf(
          path,
          spec,
          {
            strategy: "raster",
            preset: compressionLevel,
            dpi: 150,
            jpeg_quality: 60,
            grayscale: false,
            remove_metadata: true,
          },
          jobId,
        );
        return res.path;
      }
      case "watermark": {
        const res = await watermarkPdf(
          path,
          spec,
          {
            kind: "text",
            text: watermarkText,
            font_size_pt: 42,
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
          },
          jobId,
        );
        return res.path;
      }
      case "ocr": {
        const res = await ocrPdf(
          path,
          spec,
          {
            languages: [ocrLanguage],
            psm: 3,
            dpi: 250,
            output_mode: "searchable_pdf",
            pages: [],
            preprocess: { auto_rotate: false, deskew: false, contrast: true, denoise: false, binarize: false, grayscale: false },
            skip_text_pages: true,
          },
          jobId,
        );
        return res.path;
      }
      case "protect": {
        if (!password) throw { code: "invalid_input", message: t("security.passwordRequired") };
        const res = await protectPdf(
          path,
          spec,
          {
            userPassword: password,
            ownerPassword: password,
            allowPrinting: true,
            allowCopying: true,
            allowEditing: false,
            allowCommenting: true,
          } as never,
          jobId,
        );
        return res.path;
      }
      case "rotate": {
        const res = await rotatePages({
          input: path,
          pages: [],
          degrees: rotateDegrees,
          output: spec,
          jobId,
        });
        return res.path;
      }
      case "images": {
        const res = await pdfToImages({
          input: path,
          outputDir: joinPath(outputDir, `${stem}_images`),
          format: imageFormat,
          dpi: imageDpi,
          jpegQuality: 88,
          grayscale: false,
          namePrefix: "page",
          pages: [],
          overwrite: "unique_name",
          jobId,
        });
        return res.files[0]?.path ?? null;
      }
      default:
        return null;
    }
  };

  const completed = queue.filter((item) => item.status === "completed").length;

  return (
    <Screen
      title={t("batch.title")}
      subtitle={t("batch.subtitle")}
      actions={
        queue.length ? (
          <Badge tone={running ? "accent" : "default"}>{t("batch.completedOf", { done: completed, total: queue.length })}</Badge>
        ) : null
      }
    >
      <TwoColumn
        main={
          <>
            {!session.files.length ? (
              <DropZone onPaths={(paths) => void session.addPaths(paths)} dragging={dragging} multiple accept="pdf" />
            ) : (
              <OptionCard title={`${session.files.length} ${t("common.pages")}`}>
                <FileList
                  files={session.files}
                  reorder
                  onRemove={session.removeFile}
                  onMove={session.moveFile}
                  onAdd={session.pickFiles}
                  addLabel={t("batch.addFiles")}
                />
              </OptionCard>
            )}

            {queue.length ? (
              <Card className="p-4">
                <p className="font-semibold text-[13.5px] mb-3 flex items-center gap-2">
                  <ClipboardList size={15} /> {t("batch.queue")}
                </p>
                <div className="flex flex-col gap-1.5 max-h-[360px] overflow-y-auto">
                  {queue.map((item, index) => (
                    <div key={`${item.path}-${index}`} className="card-soft flex items-center gap-3 px-3 py-2 text-[13px]">
                      <span className="w-24 shrink-0">
                        {item.status === "completed" ? (
                          <Badge tone="ok">{t("common.done")}</Badge>
                        ) : item.status === "processing" ? (
                          <Badge tone="accent">{t("common.processing")}</Badge>
                        ) : item.status === "failed" ? (
                          <Badge tone="danger">!</Badge>
                        ) : item.status === "cancelled" ? (
                          <Badge tone="warn">{t("common.cancel")}</Badge>
                        ) : (
                          <Badge>{t("common.pending")}</Badge>
                        )}
                      </span>
                      <span className="truncate flex-1" title={item.path}>
                        {item.name}
                      </span>
                      {item.detail ? <span className="text-xs muted truncate max-w-[220px]">{item.detail}</span> : null}
                      {item.output ? <span className="text-xs muted truncate max-w-[200px]">{fileBaseName(item.output)}</span> : null}
                    </div>
                  ))}
                </div>
              </Card>
            ) : null}
          </>
        }
        side={
          <>
            <Card className="p-4 flex flex-col gap-3">
              <Field label={t("batch.operation")}>
                <Select<Operation>
                  value={operation}
                  onChange={setOperation}
                  options={[
                    { value: "compress", label: t("nav.compress") },
                    { value: "watermark", label: t("nav.watermark") },
                    { value: "ocr", label: t("nav.ocr") },
                    { value: "protect", label: t("nav.protect") },
                    { value: "rotate", label: t("common.rotate") },
                    { value: "images", label: t("nav.pdfToImages") },
                  ]}
                />
              </Field>

              {operation === "compress" ? (
                <div className="seg self-start">
                  {(["low", "medium", "high"] as const).map((level) => (
                    <button key={level} data-active={compressionLevel === level} onClick={() => setCompressionLevel(level)}>
                      {t(`compress.${level}`)}
                    </button>
                  ))}
                </div>
              ) : null}

              {operation === "watermark" ? (
                <Field label={t("annotate.textPlaceholder")}>
                  <input className="input" value={watermarkText} onChange={(event) => setWatermarkText(event.target.value)} />
                </Field>
              ) : null}

              {operation === "ocr" ? (
                <Field label={t("ocr.languages")}>
                  <select className="select" value={ocrLanguage} onChange={(event) => setOcrLanguage(event.target.value)}>
                    {(batchLanguages.length ? batchLanguages : [{ code: "eng", name: "English" }]).map((language) => (
                      <option key={language.code} value={language.code}>
                        {language.name}
                      </option>
                    ))}
                  </select>
                </Field>
              ) : null}

              {operation === "protect" ? (
                <Field label={t("security.userPassword")}>
                  <input className="input" type="password" value={password} onChange={(event) => setPassword(event.target.value)} />
                </Field>
              ) : null}

              {operation === "rotate" ? (
                <Field label={t("pageTools.degrees")}>
                  <div className="flex gap-2">
                    {[90, 180, 270].map((value) => (
                      <Button key={value} size="sm" variant={rotateDegrees === value ? "primary" : "default"} onClick={() => setRotateDegrees(value as 90 | 180 | 270)}>
                        {value}°
                      </Button>
                    ))}
                  </div>
                </Field>
              ) : null}

              {operation === "images" ? (
                <>
                  <Field label={t("convert.format")}>
                    <div className="seg">
                      <button data-active={imageFormat === "jpeg"} onClick={() => setImageFormat("jpeg")}>
                        JPG
                      </button>
                      <button data-active={imageFormat === "png"} onClick={() => setImageFormat("png")}>
                        PNG
                      </button>
                    </div>
                  </Field>
                  <Field label={t("convert.resolution")}>
                    <Slider value={imageDpi} min={72} max={300} step={2} onChange={setImageDpi} format={(value) => `${value}`} />
                  </Field>
                </>
              ) : null}

              <Toggle checked={skipExisting} onChange={setSkipExisting} label={t("batch.skipExisting")} />

              <div className="flex gap-2">
                <Button
                  variant="primary"
                  icon={<Play size={15} />}
                  disabled={!session.files.length || running}
                  onClick={() => void start()}
                >
                  {t("batch.start")}
                </Button>
                <Button
                  variant="ghost"
                  icon={<Trash2 size={15} />}
                  disabled={running}
                  onClick={() => {
                    setQueue([]);
                    session.clearFiles();
                  }}
                >
                  {t("batch.clear")}
                </Button>
              </div>
              {running ? (
                <Button variant="danger" size="sm" icon={<XCircle size={14} />} onClick={() => { if (currentJobRef.current) void cancelJob(currentJobRef.current).catch(() => undefined); }}>
                  {t("progress.cancel")}
                </Button>
              ) : null}
            </Card>
            <Card className="p-4 flex flex-col gap-2 text-xs muted">
              <div className="flex items-center gap-2">
                <FileImage size={13} /> {t("common.outputFolder")}: {outputDir || "—"}
              </div>
              <div>{formatBytes(session.files.reduce((sum, file) => sum + file.sizeBytes, 0))} total</div>
            </Card>
            {result ? <ResultCard result={result} onReset={() => setResult(null)} /> : null}
          </>
        }
      />
    </Screen>
  );
}


