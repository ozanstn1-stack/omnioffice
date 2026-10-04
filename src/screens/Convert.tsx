import { useMemo, useState } from "react";
import { FileImage, FolderOpen, Images, RotateCw } from "lucide-react";
import { isAndroid, revealAnyFile } from "../lib/mobile";
import { Button, Card, Field, Segmented, Select, Slider, Toggle } from "../components/ui";
import { DropZone, FileList, InfoStrip, OutputBar, ResultCard } from "../components/files";
import { OptionCard, Screen, TwoColumn } from "../components/layout";
import { useT } from "../lib/i18n";
import { useTool } from "../lib/useTool";
import { imagesToPdf, pdfToImages } from "../lib/api";
import { formatBytes } from "../lib/format";
import { useSettings } from "../lib/store";
import type { ImageToPdfOptions } from "../lib/types";

type Tab = "pdfToImages" | "imagesToPdf";

export function Convert({
  tab: initialTab,
  initialFiles,
  dragging,
}: {
  tab: Tab;
  initialFiles?: string[];
  dragging: boolean;
}) {
  const t = useT();
  const [tab, setTab] = useState<Tab>(initialTab);

  return (
    <Screen
      title={t("nav.convert")}
      subtitle={tab === "pdfToImages" ? t("convert.pdfToImagesSubtitle") : t("convert.imagesToPdfSubtitle")}
      actions={
        <Segmented<Tab>
          value={tab}
          onChange={setTab}
          options={[
            { value: "pdfToImages", label: t("nav.pdfToImages") },
            { value: "imagesToPdf", label: t("nav.imagesToPdf") },
          ]}
        />
      }
    >
      {tab === "pdfToImages" ? (
        <PdfToImages initialFiles={initialFiles} dragging={dragging} />
      ) : (
        <ImagesToPdf initialFiles={initialFiles} dragging={dragging} />
      )}
    </Screen>
  );
}

function PdfToImages({ initialFiles, dragging }: { initialFiles?: string[]; dragging: boolean }) {
  const t = useT();
  const settings = useSettings((s) => s.settings);
  const session = useTool({
    suffix: "_images",
    accept: "pdf",
    multiOutput: true,
    initialPaths: initialFiles,
  });
  const [format, setFormat] = useState<"jpeg" | "png">(settings.defaultExportFormat === "png" ? "png" : "jpeg");
  const [dpi, setDpi] = useState(settings.defaultImageDpi || 150);
  const [quality, setQuality] = useState(90);
  const [grayscale, setGrayscale] = useState(false);
  const [prefix, setPrefix] = useState("page");
  const [files, setFiles] = useState<
    { path: string; page: number; bytes: number; width: number; height: number }[] | null
  >(null);

  const run = () =>
    session.run(async (jobId, overwrite) => {
      const result = await pdfToImages({
        input: session.primary?.path ?? "",
        outputDir: session.outputDir || session.outputPath.replace(/[^\\/]*$/, ""),
        format,
        dpi,
        jpegQuality: quality,
        grayscale,
        namePrefix: prefix,
        pages: [],
        overwrite,
        password: session.password || undefined,
        jobId,
      });
      setFiles(result.files);
      await session.publish(result.files.map((file) => file.path));
      return {
        path: result.files[0]?.path ?? session.outputDir,
        pageCount: result.files.length,
        outputBytes: result.totalBytes,
        message: `${result.files.length} ${t("common.pages")}`,
      };
    });

  return (
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
            {files ? (
              <Card className="p-4 fade-in">
                <div className="flex items-center justify-between mb-3">
                  <p className="font-semibold text-[13.5px]">
                    {files.length} {t("common.pages")} · {formatBytes(files.reduce((sum, file) => sum + file.bytes, 0))}
                  </p>
                  <Button
                    size="sm"
                    icon={<FolderOpen size={14} />}
                    onClick={() => void revealAnyFile(files[0]?.path ?? "").catch(() => undefined)}
                  >
                    {isAndroid() ? t("common.share") : t("common.openFolder")}
                  </Button>
                </div>
                <div className="flex flex-col gap-1 max-h-[240px] overflow-y-auto text-[13px]">
                  {files.slice(0, 50).map((file) => (
                    <div
                      key={file.path}
                      className="flex items-center justify-between px-2 py-1 rounded hover:bg-[var(--surface-2)]"
                    >
                      <span className="truncate" title={file.path}>
                        {file.path.split(/[\\/]/).pop()}
                      </span>
                      <span className="muted shrink-0 ml-3">
                        {file.width}×{file.height} · {formatBytes(file.bytes)}
                      </span>
                    </div>
                  ))}
                  {files.length > 50 ? <p className="muted px-2 py-1">…</p> : null}
                </div>
              </Card>
            ) : null}
          </>
        )
      }
      side={
        <>
          <OutputBar
            session={session}
            runLabel={t("convert.run")}
            outputKind="folder"
            showOverwrite
            onRun={() => void run()}
            disabled={!session.primary}
          />
          <OptionCard title={t("convert.pdfToImagesTitle")}>
            <Field label={t("convert.format")}>
              <Segmented<"jpeg" | "png">
                value={format}
                onChange={setFormat}
                options={[
                  { value: "jpeg", label: "JPG" },
                  { value: "png", label: "PNG" },
                ]}
              />
            </Field>
            <Field label={t("convert.resolution")}>
              <Select<string>
                value={String(dpi)}
                onChange={(value) => setDpi(Number(value))}
                options={[
                  { value: "72", label: "72 DPI" },
                  { value: "150", label: "150 DPI" },
                  { value: "200", label: "200 DPI" },
                  { value: "300", label: "300 DPI" },
                  { value: "450", label: "450 DPI" },
                ]}
              />
            </Field>
            {format === "jpeg" ? (
              <Field label={t("compress.quality")}>
                <Slider value={quality} min={40} max={100} onChange={setQuality} format={(value) => `${value}%`} />
              </Field>
            ) : null}
            <Field label={t("convert.namePrefix")}>
              <input
                className="input"
                value={prefix}
                onChange={(event) => setPrefix(event.target.value.replace(/[\\/:*?"<>|]/g, ""))}
              />
            </Field>
            <Toggle checked={grayscale} onChange={setGrayscale} label={t("compress.grayscale")} />
            <p className="text-xs muted">
              {t("common.example")} {prefix}_001.{format === "jpeg" ? "jpg" : "png"}
            </p>
          </OptionCard>
        </>
      }
    />
  );
}

function ImagesToPdf({ initialFiles, dragging }: { initialFiles?: string[]; dragging: boolean }) {
  const t = useT();
  const session = useTool({
    suffix: "_album",
    accept: "image",
    multiple: true,
    loadInfo: false,
    initialPaths: initialFiles,
  });
  const [rotations, setRotations] = useState<Record<string, number>>({});
  const [options, setOptions] = useState<ImageToPdfOptions>({
    page_size: "a4",
    custom_width_pt: 595.28,
    custom_height_pt: 841.89,
    orientation: "auto",
    fit: "fit",
    margin_pt: 0,
    dpi: 96,
    jpeg_quality: 92,
  });

  const patch = (values: Partial<ImageToPdfOptions>) => setOptions((previous) => ({ ...previous, ...values }));

  const items = useMemo(
    () =>
      session.files.map((file) => ({
        path: file.path,
        rotation_delta: rotations[file.path] ?? 0,
      })),
    [rotations, session.files],
  );

  const rotate = (path: string) => {
    setRotations((previous) => ({ ...previous, [path]: ((previous[path] ?? 0) + 90) % 360 }));
  };

  const run = () =>
    session.run(async (jobId, overwrite) => {
      return imagesToPdf(items, session.outputSpec(overwrite), options, jobId);
    });

  return (
    <TwoColumn
      main={
        !session.files.length ? (
          <DropZone onPaths={(paths) => void session.addPaths(paths)} dragging={dragging} multiple accept="image" />
        ) : (
          <>
            <OptionCard title={t("convert.imagesOrder")}>
              <div className="flex flex-col gap-2">
                {session.files.map((file, index) => (
                  <div key={file.path} className="card-soft flex items-center gap-3 px-3 py-2">
                    <span
                      className="w-6 h-6 rounded-md flex items-center justify-center text-xs font-semibold"
                      style={{ background: "var(--accent-weak)", color: "var(--accent)" }}
                    >
                      {index + 1}
                    </span>
                    <div className="min-w-0 flex-1">
                      <p className="truncate text-[13.5px]">{file.name}</p>
                      <p className="text-xs muted">
                        {formatBytes(file.sizeBytes)}
                        {rotations[file.path] ? ` · ${t("common.rotation")}: ${rotations[file.path]}°` : ""}
                      </p>
                    </div>
                    <Button size="sm" variant="ghost" icon={<RotateCw size={14} />} onClick={() => rotate(file.path)}>
                      90°
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() => session.moveFile(index, Math.max(0, index - 1))}
                      disabled={index === 0}
                    >
                      ↑
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() => session.moveFile(index, Math.min(session.files.length - 1, index + 1))}
                      disabled={index === session.files.length - 1}
                    >
                      ↓
                    </Button>
                    <Button size="sm" variant="ghost" onClick={() => session.removeFile(index)}>
                      ✕
                    </Button>
                  </div>
                ))}
                <Button variant="ghost" size="sm" icon={<Images size={14} />} onClick={session.pickFiles}>
                  {t("common.addImages")}
                </Button>
              </div>
            </OptionCard>
            {session.result ? <ResultCard result={session.result} onReset={session.resetResult} /> : null}
          </>
        )
      }
      side={
        <>
          <OutputBar
            session={session}
            runLabel={t("convert.run")}
            onRun={() => void run()}
            disabled={!session.files.length}
          />
          <OptionCard title={t("convert.imagesToPdfTitle")}>
            <Field label={t("convert.pageSize")}>
              <Select<string>
                value={options.page_size}
                onChange={(value) => patch({ page_size: value })}
                options={[
                  { value: "a4", label: "A4" },
                  { value: "letter", label: "Letter" },
                  { value: "legal", label: "Legal" },
                  { value: "a3", label: "A3" },
                  { value: "a5", label: "A5" },
                  { value: "original", label: t("common.original") },
                  { value: "custom", label: t("convert.custom") },
                ]}
              />
            </Field>
            {options.page_size === "custom" ? (
              <div className="grid grid-cols-2 gap-2">
                <Field label={t("convert.widthPt")}>
                  <input
                    type="number"
                    className="input"
                    value={options.custom_width_pt}
                    onChange={(event) => patch({ custom_width_pt: Number(event.target.value) })}
                  />
                </Field>
                <Field label={t("convert.heightPt")}>
                  <input
                    type="number"
                    className="input"
                    value={options.custom_height_pt}
                    onChange={(event) => patch({ custom_height_pt: Number(event.target.value) })}
                  />
                </Field>
              </div>
            ) : null}
            <Field label={t("convert.orientation")}>
              <Segmented<ImageToPdfOptions["orientation"]>
                value={options.orientation}
                onChange={(value) => patch({ orientation: value })}
                options={[
                  { value: "auto", label: t("convert.auto") },
                  { value: "portrait", label: t("convert.portrait") },
                  { value: "landscape", label: t("convert.landscape") },
                ]}
              />
            </Field>
            <Field label={t("convert.fit")}>
              <Select<ImageToPdfOptions["fit"]>
                value={options.fit}
                onChange={(value) => patch({ fit: value })}
                options={[
                  { value: "fit", label: t("convert.fitContain") },
                  { value: "fill", label: t("convert.fitCover") },
                  { value: "actual", label: t("convert.fitActual") },
                ]}
              />
            </Field>
            <div className="grid grid-cols-2 gap-2">
              <Field label={t("convert.margin")}>
                <input
                  type="number"
                  className="input"
                  value={options.margin_pt}
                  onChange={(event) => patch({ margin_pt: Number(event.target.value) })}
                />
              </Field>
              <Field label={t("convert.assumedDpi")}>
                <input
                  type="number"
                  className="input"
                  value={options.dpi}
                  onChange={(event) => patch({ dpi: Number(event.target.value) })}
                />
              </Field>
            </div>
            <p className="text-xs muted flex items-center gap-1.5">
              <FileImage size={12} />
              {t("common.example")} {session.files.length || 0} → {session.files.length || 0} {t("common.pages")}
            </p>
          </OptionCard>
        </>
      }
    />
  );
}
