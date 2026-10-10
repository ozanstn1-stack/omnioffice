import { useState } from "react";
import { Crop, FileOutput, Hash, RotateCw, Scaling, Stamp, Trash2 } from "lucide-react";
import { Button, Card, ColorInput, Field, Segmented, Slider, TextInput, Toggle } from "../components/ui";
import { DropZone, FileList, InfoStrip, OutputBar, ResultCard } from "../components/files";
import { PageCanvas, Pager } from "../components/pages";
import { OptionCard, Screen, TwoColumn } from "../components/layout";
import { useT } from "../lib/i18n";
import { useTool } from "../lib/useTool";
import { parsePageList } from "../lib/format";
import {
  addPageNumbers,
  cropPages,
  deletePages,
  extractPages,
  nupPdf,
  resizePages,
  rotatePages,
  stampPdf,
} from "../lib/api";
import type {
  BatesOptions,
  CropItem,
  HeaderFooterOptions,
  NumberingOptions,
  NupOptions,
  ResizeOptions,
  WatermarkPosition,
} from "../lib/types";

type Tab = "extract" | "delete" | "rotate" | "resize" | "crop" | "numbering" | "stamp" | "nup";

export function PageTools({
  tab: initialTab = "extract",
  initialFiles,
  dragging,
}: {
  tab?: Tab;
  initialFiles?: string[];
  dragging: boolean;
}) {
  const t = useT();
  const [tab, setTab] = useState<Tab>(initialTab);
  const titles: Record<Tab, string> = {
    extract: t("pageTools.extractTitle"),
    delete: t("pageTools.deleteTitle"),
    rotate: t("pageTools.rotateTitle"),
    resize: t("pageTools.resizeTitle"),
    crop: t("pageTools.cropTitle"),
    numbering: t("pageTools.numberingTitle"),
    stamp: t("pageTools.stampTitle"),
    nup: t("pageTools.nupTitle"),
  };
  const subtitles: Record<Tab, string> = {
    extract: t("pageTools.extractSubtitle"),
    delete: t("pageTools.deleteSubtitle"),
    rotate: t("pageTools.rotateSubtitle"),
    resize: t("pageTools.resizeSubtitle"),
    crop: t("pageTools.cropSubtitle"),
    numbering: t("pageTools.numberingSubtitle"),
    stamp: t("pageTools.stampSubtitle"),
    nup: t("pageTools.nupSubtitle"),
  };

  return (
    <Screen
      title={titles[tab]}
      subtitle={subtitles[tab]}
      actions={
        <Segmented<Tab>
          value={tab}
          onChange={setTab}
          options={[
            { value: "extract", label: t("common.extract") },
            { value: "delete", label: t("common.delete") },
            { value: "rotate", label: t("common.rotate") },
            { value: "resize", label: t("common.size") },
            { value: "crop", label: t("common.crop") },
            { value: "numbering", label: t("pageTools.numberingTitle") },
            { value: "stamp", label: t("pageTools.stampTitle") },
            { value: "nup", label: t("pageTools.nupTitle") },
          ]}
        />
      }
    >
      {tab === "extract" ? <Extract initialFiles={initialFiles} dragging={dragging} /> : null}
      {tab === "delete" ? <Delete initialFiles={initialFiles} dragging={dragging} /> : null}
      {tab === "rotate" ? <Rotate initialFiles={initialFiles} dragging={dragging} /> : null}
      {tab === "resize" ? <Resize initialFiles={initialFiles} dragging={dragging} /> : null}
      {tab === "crop" ? <CropTool initialFiles={initialFiles} dragging={dragging} /> : null}
      {tab === "numbering" ? <Numbering initialFiles={initialFiles} dragging={dragging} /> : null}
      {tab === "stamp" ? <StampTool initialFiles={initialFiles} dragging={dragging} /> : null}
      {tab === "nup" ? <NupTool initialFiles={initialFiles} dragging={dragging} /> : null}
    </Screen>
  );
}

function InputColumn({ session, dragging }: { session: ReturnType<typeof useTool>; dragging: boolean }) {
  const t = useT();
  if (!session.primary) {
    return <DropZone onPaths={(paths) => void session.addPaths(paths)} dragging={dragging} accept="pdf" />;
  }
  return (
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
    </>
  );
}

function SelectionField({
  value,
  onChange,
  pageCount,
  disabled = false,
}: {
  value: string;
  onChange: (value: string) => void;
  pageCount: number;
  disabled?: boolean;
}) {
  const t = useT();
  return (
    <Field
      label={t("pageTools.selection")}
      hint={`${t("pageTools.selectionHint")} ${t("common.example")} 1,3,5-8 · ${pageCount} ${t("common.pages")}`}
    >
      <TextInput
        value={value}
        onChange={(event) => onChange(event.target.value)}
        placeholder={disabled ? t("common.all") : "1,3,5-8"}
        spellCheck={false}
      />
    </Field>
  );
}

function Extract({ initialFiles, dragging }: { initialFiles?: string[]; dragging: boolean }) {
  const t = useT();
  const session = useTool({ suffix: "_extracted", accept: "pdf", initialPaths: initialFiles });
  const [selection, setSelection] = useState("1");

  return (
    <TwoColumn
      main={<InputColumn session={session} dragging={dragging} />}
      side={
        <>
          <OutputBar
            session={session}
            runLabel={t("pageTools.extractTitle")}
            disabled={!session.primary || !selection.trim()}
            onRun={() =>
              void session.run(async (jobId, overwrite) =>
                extractPages({
                  input: session.primary?.path ?? "",
                  selection,
                  output: session.outputSpec(overwrite),
                  password: session.password || undefined,
                  jobId,
                }),
              )
            }
          />
          <OptionCard title={t("pageTools.extractTitle")}>
            <SelectionField value={selection} onChange={setSelection} pageCount={session.info?.pageCount ?? 0} />
            <p className="text-xs muted flex items-center gap-1.5">
              <FileOutput size={12} /> {t("common.output")}: {session.outputPath.split(/[\\/]/).pop()}
            </p>
          </OptionCard>
          {session.result ? <ResultCard result={session.result} onReset={session.resetResult} /> : null}
        </>
      }
    />
  );
}

function Delete({ initialFiles, dragging }: { initialFiles?: string[]; dragging: boolean }) {
  const t = useT();
  const session = useTool({ suffix: "_cleaned", accept: "pdf", initialPaths: initialFiles });
  const [selection, setSelection] = useState("");
  const [confirming, setConfirming] = useState(false);

  return (
    <TwoColumn
      main={
        <>
          <InputColumn session={session} dragging={dragging} />
          {confirming ? (
            <Card className="p-4 flex items-center gap-3" soft>
              <Trash2 size={16} style={{ color: "var(--danger)" }} />
              <p className="text-[13px] flex-1">
                {t("organize.deleteConfirm", { count: selection.split(",").filter(Boolean).length })}
              </p>
              <Button size="sm" variant="ghost" onClick={() => setConfirming(false)}>
                {t("common.cancel")}
              </Button>
              <Button
                size="sm"
                variant="primary"
                onClick={() =>
                  void session.run(async (jobId, overwrite) => {
                    setConfirming(false);
                    return deletePages({
                      input: session.primary?.path ?? "",
                      selection,
                      output: session.outputSpec(overwrite),
                      password: session.password || undefined,
                      jobId,
                    });
                  })
                }
              >
                {t("common.delete")}
              </Button>
            </Card>
          ) : null}
        </>
      }
      side={
        <>
          <OutputBar
            session={session}
            runLabel={t("pageTools.deleteTitle")}
            disabled={!session.primary || !selection.trim()}
            onRun={() => setConfirming(true)}
          />
          <OptionCard title={t("pageTools.deleteTitle")}>
            <SelectionField value={selection} onChange={setSelection} pageCount={session.info?.pageCount ?? 0} />
          </OptionCard>
          {session.result ? <ResultCard result={session.result} onReset={session.resetResult} /> : null}
        </>
      }
    />
  );
}

function Rotate({ initialFiles, dragging }: { initialFiles?: string[]; dragging: boolean }) {
  const t = useT();
  const session = useTool({ suffix: "_rotated", accept: "pdf", initialPaths: initialFiles });
  const [selection, setSelection] = useState("");
  const [degrees, setDegrees] = useState<90 | 180 | 270>(90);

  return (
    <TwoColumn
      main={<InputColumn session={session} dragging={dragging} />}
      side={
        <>
          <OutputBar
            session={session}
            runLabel={t("pageTools.rotateTitle")}
            disabled={!session.primary}
            onRun={() =>
              void session.run(async (jobId, overwrite) =>
                rotatePages({
                  input: session.primary?.path ?? "",
                  selection: selection.trim() || undefined,
                  pages: selection.trim() ? undefined : [],
                  degrees,
                  output: session.outputSpec(overwrite),
                  password: session.password || undefined,
                  jobId,
                }),
              )
            }
          />
          <OptionCard title={t("pageTools.rotateTitle")}>
            <Field label={t("pageTools.degrees")}>
              <div className="flex gap-2">
                {[90, 180, 270].map((value) => (
                  <Button
                    key={value}
                    variant={degrees === value ? "primary" : "default"}
                    size="sm"
                    icon={<RotateCw size={14} />}
                    onClick={() => setDegrees(value as 90 | 180 | 270)}
                  >
                    {value}°
                  </Button>
                ))}
              </div>
            </Field>
            <SelectionField
              value={selection}
              onChange={setSelection}
              pageCount={session.info?.pageCount ?? 0}
              disabled
            />
            <p className="text-xs muted">{t("common.all")}</p>
          </OptionCard>
          {session.result ? <ResultCard result={session.result} onReset={session.resetResult} /> : null}
        </>
      }
    />
  );
}

function Resize({ initialFiles, dragging }: { initialFiles?: string[]; dragging: boolean }) {
  const t = useT();
  const session = useTool({ suffix: "_resized", accept: "pdf", initialPaths: initialFiles });
  const [options, setOptions] = useState<ResizeOptions>({
    page_size: "a4",
    custom_width_pt: 595.28,
    custom_height_pt: 841.89,
    orientation: "auto",
    mode: "fit",
    pages: [],
  });
  const [selection, setSelection] = useState("");

  const patch = (values: Partial<ResizeOptions>) => setOptions((previous) => ({ ...previous, ...values }));

  return (
    <TwoColumn
      main={<InputColumn session={session} dragging={dragging} />}
      side={
        <>
          <OutputBar
            session={session}
            runLabel={t("pageTools.resizeTitle")}
            disabled={!session.primary}
            onRun={() =>
              void session.run(async (jobId, overwrite) =>
                resizePages(
                  session.primary?.path ?? "",
                  session.outputSpec(overwrite),
                  options,
                  jobId,
                  session.password || undefined,
                ),
              )
            }
          />
          <OptionCard title={t("convert.pageSize")}>
            <Field label={t("convert.pageSize")}>
              <select
                className="select"
                value={options.page_size}
                onChange={(event) => patch({ page_size: event.target.value })}
              >
                <option value="a4">A4</option>
                <option value="letter">Letter</option>
                <option value="legal">Legal</option>
                <option value="a3">A3</option>
                <option value="a5">A5</option>
                <option value="custom">{t("convert.custom")}</option>
              </select>
            </Field>
            {options.page_size === "custom" ? (
              <div className="grid grid-cols-2 gap-2">
                <Field label={t("convert.widthPt")}>
                  <TextInput
                    type="number"
                    value={options.custom_width_pt}
                    onChange={(event) => patch({ custom_width_pt: Number(event.target.value) })}
                  />
                </Field>
                <Field label={t("convert.heightPt")}>
                  <TextInput
                    type="number"
                    value={options.custom_height_pt}
                    onChange={(event) => patch({ custom_height_pt: Number(event.target.value) })}
                  />
                </Field>
              </div>
            ) : null}
            <Field label={t("convert.orientation")}>
              <Segmented<ResizeOptions["orientation"]>
                value={options.orientation}
                onChange={(value) => patch({ orientation: value })}
                options={[
                  { value: "auto", label: t("convert.auto") },
                  { value: "portrait", label: t("convert.portrait") },
                  { value: "landscape", label: t("convert.landscape") },
                ]}
              />
            </Field>
            <Field label={t("pageTools.mode")}>
              <Segmented<ResizeOptions["mode"]>
                value={options.mode}
                onChange={(value) => patch({ mode: value })}
                options={[
                  { value: "fit", label: t("pageTools.fit") },
                  { value: "stretch", label: t("pageTools.stretch") },
                ]}
              />
            </Field>
            <SelectionField
              value={selection}
              onChange={setSelection}
              pageCount={session.info?.pageCount ?? 0}
              disabled
            />
            <p className="text-xs muted flex items-center gap-1.5">
              <Scaling size={12} /> {t("pageTools.resizeHint")}
            </p>
          </OptionCard>
          {session.result ? <ResultCard result={session.result} onReset={session.resetResult} /> : null}
        </>
      }
    />
  );
}

function CropTool({ initialFiles, dragging }: { initialFiles?: string[]; dragging: boolean }) {
  const t = useT();
  const session = useTool({ suffix: "_cropped", accept: "pdf", initialPaths: initialFiles });
  const [page, setPage] = useState(1);
  const [rect, setRect] = useState<{ x: number; y: number; w: number; h: number } | null>(null);
  const [applyAll, setApplyAll] = useState(false);

  const pageCount = session.info?.pageCount ?? 0;
  const geometry = session.info?.pageGeometries.find((entry) => entry.page === page);
  const renderWidth = 1100;

  const toPoints = (value: number, dimension: number) => (value / renderWidth) * dimension;

  const apply = () =>
    session.run(async (jobId, overwrite) => {
      if (!rect || !geometry) throw { code: "invalid_input", message: t("pageTools.cropHint") };
      const scale = geometry.display_width_pt / renderWidth;
      const crop: CropItem = {
        page,
        x: rect.x * scale,
        y: rect.y * scale,
        w: rect.w * scale,
        h: rect.h * scale,
      };
      const crops: CropItem[] = applyAll
        ? Array.from({ length: pageCount }, (_, index) => ({ ...crop, page: index + 1 }))
        : [crop];
      return cropPages(
        session.primary?.path ?? "",
        session.outputSpec(overwrite),
        crops,
        jobId,
        session.password || undefined,
      );
    });

  return (
    <TwoColumn
      main={
        !session.primary ? (
          <DropZone onPaths={(paths) => void session.addPaths(paths)} dragging={dragging} accept="pdf" />
        ) : (
          <>
            <Card className="p-4 flex items-center gap-3">
              <InfoStrip info={session.info} error={session.infoError} />
              <div className="ml-auto">
                <Pager page={page} pageCount={pageCount || 1} onChange={setPage} />
              </div>
            </Card>
            <Card className="p-4">
              <p className="text-xs muted mb-2">{t("pageTools.cropHint")}</p>
              <div className="mx-auto" style={{ maxWidth: 560 }}>
                <PageCanvas
                  path={session.primary.path}
                  page={page}
                  password={session.password || undefined}
                  maxWidth={renderWidth}
                  onDragRect={setRect}
                />
              </div>
              {rect ? (
                <p className="text-xs muted mt-2 tabular-nums">
                  {Math.round(rect.x)}×{Math.round(rect.y)} · {Math.round(rect.w)}×{Math.round(rect.h)} px
                </p>
              ) : null}
            </Card>
          </>
        )
      }
      side={
        <>
          <OutputBar
            session={session}
            runLabel={t("pageTools.applyCrop")}
            disabled={!session.primary || !rect}
            onRun={() => void apply()}
          />
          <OptionCard title={t("common.crop")}>
            <Toggle checked={applyAll} onChange={setApplyAll} label={t("common.all")} />
            <Button size="sm" variant="ghost" icon={<Crop size={14} />} onClick={() => setRect(null)} disabled={!rect}>
              {t("pageTools.clearCrop")}
            </Button>
            {rect && geometry ? (
              <p className="text-xs muted tabular-nums">
                {t("convert.widthPt")}: {toPoints(rect.w, geometry.display_width_pt).toFixed(0)} ·{" "}
                {t("convert.heightPt")}: {toPoints(rect.h, geometry.display_height_pt).toFixed(0)}
              </p>
            ) : null}
          </OptionCard>
          {session.result ? <ResultCard result={session.result} onReset={session.resetResult} /> : null}
        </>
      }
    />
  );
}

const POSITIONS: WatermarkPosition[] = [
  "top_left",
  "top_center",
  "top_right",
  "bottom_left",
  "bottom_center",
  "bottom_right",
];

const BATES_POSITIONS: BatesOptions["position"][] = [
  "topLeft",
  "topCenter",
  "topRight",
  "bottomLeft",
  "bottomCenter",
  "bottomRight",
];

function batesPositionLabel(position: BatesOptions["position"]): string {
  return position.replace(/([A-Z])/g, " $1").replace(/^./, (character) => character.toUpperCase());
}

function StampTool({ initialFiles, dragging }: { initialFiles?: string[]; dragging: boolean }) {
  const t = useT();
  const session = useTool({ suffix: "_stamped", accept: "pdf", initialPaths: initialFiles });
  const [keepSignatures, setKeepSignatures] = useState(true);
  const [options, setOptions] = useState<HeaderFooterOptions>({
    headerLeft: "",
    headerCenter: "",
    headerRight: "",
    footerLeft: "",
    footerCenter: "",
    footerRight: "",
    fontSizePt: 10,
    color: "#333333",
    marginPt: 28,
    pages: [],
    startNumber: 1,
    countFromStart: true,
  });
  const [bates, setBates] = useState<BatesOptions>({
    prefix: "",
    suffix: "",
    start: 1,
    digits: 6,
    position: "bottomRight",
    fontSizePt: 10,
    color: "#333333",
    marginPt: 28,
    pages: [],
  });
  const [selection, setSelection] = useState("");

  const patch = (values: Partial<HeaderFooterOptions>) => setOptions((previous) => ({ ...previous, ...values }));
  const patchBates = (values: Partial<BatesOptions>) => setBates((previous) => ({ ...previous, ...values }));
  // One style for both stamps: the shared controls update header/footer and Bates.
  const patchStyle = (values: Partial<Pick<HeaderFooterOptions, "fontSizePt" | "color" | "marginPt">>) => {
    setOptions((previous) => ({ ...previous, ...values }));
    setBates((previous) => ({ ...previous, ...values }));
  };

  const headerFilled = [
    options.headerLeft,
    options.headerCenter,
    options.headerRight,
    options.footerLeft,
    options.footerCenter,
    options.footerRight,
  ].some((value) => value.trim() !== "");
  const batesFilled = bates.prefix.trim() !== "" || bates.suffix.trim() !== "";

  const apply = () =>
    session.run(async (jobId, overwrite) => {
      const pages = parsePageList(selection, session.info?.pageCount ?? 0) ?? [];
      return stampPdf(
        session.primary?.path ?? "",
        session.outputSpec(overwrite),
        headerFilled ? { ...options, pages } : null,
        batesFilled ? { ...bates, pages } : null,
        jobId,
        session.password || undefined,
        keepSignatures,
      );
    });

  const templates: [keyof HeaderFooterOptions, string][] = [
    ["headerLeft", t("pageTools.headerLeft")],
    ["headerCenter", t("pageTools.headerCenter")],
    ["headerRight", t("pageTools.headerRight")],
    ["footerLeft", t("pageTools.footerLeft")],
    ["footerCenter", t("pageTools.footerCenter")],
    ["footerRight", t("pageTools.footerRight")],
  ];

  return (
    <TwoColumn
      main={
        <>
          <InputColumn session={session} dragging={dragging} />
          <Card className="p-4 flex flex-col gap-3">
            <p className="text-xs muted flex items-center gap-1.5">
              <Stamp size={12} /> {t("pageTools.tokensHint")}
            </p>
            <div className="grid grid-cols-1 sm:grid-cols-3 gap-2">
              {templates.map(([key, label]) => (
                <Field key={key} label={label}>
                  <TextInput
                    className="input-sm"
                    value={String(options[key] ?? "")}
                    aria-label={label}
                    spellCheck={false}
                    onChange={(event) => patch({ [key]: event.target.value })}
                  />
                </Field>
              ))}
            </div>
          </Card>
        </>
      }
      side={
        <>
          <OutputBar
            session={session}
            runLabel={t("pageTools.applyStamp")}
            disabled={!session.primary || !(headerFilled || batesFilled)}
            onRun={() => void apply()}
          />
          <OptionCard title={t("pageTools.stampTitle")}>
            <Field label={t("annotate.fontSize")}>
              <Slider
                value={options.fontSizePt}
                min={6}
                max={24}
                onChange={(value) => patchStyle({ fontSizePt: value })}
              />
            </Field>
            <Field label={t("common.color")}>
              <ColorInput value={options.color} onChange={(value) => patchStyle({ color: value })} />
            </Field>
            <Field label={t("convert.margin")}>
              <Slider value={options.marginPt} min={6} max={90} onChange={(value) => patchStyle({ marginPt: value })} />
            </Field>
            <Field label={t("pageTools.startNumber")}>
              <TextInput
                type="number"
                value={options.startNumber}
                onChange={(event) => patch({ startNumber: Math.max(1, Number(event.target.value) || 1) })}
              />
            </Field>
            <Toggle
              checked={options.countFromStart}
              onChange={(value) => patch({ countFromStart: value })}
              label={t("pageTools.countFromStart")}
            />
            <SelectionField
              value={selection}
              onChange={setSelection}
              pageCount={session.info?.pageCount ?? 0}
              disabled
            />
          </OptionCard>
          <OptionCard title="Bates">
            <div className="grid grid-cols-2 gap-2">
              <Field label={t("pageTools.batesPrefix")}>
                <TextInput
                  className="input-sm"
                  value={bates.prefix}
                  aria-label={t("pageTools.batesPrefix")}
                  spellCheck={false}
                  onChange={(event) => patchBates({ prefix: event.target.value })}
                />
              </Field>
              <Field label={t("pageTools.batesSuffix")}>
                <TextInput
                  className="input-sm"
                  value={bates.suffix}
                  aria-label={t("pageTools.batesSuffix")}
                  spellCheck={false}
                  onChange={(event) => patchBates({ suffix: event.target.value })}
                />
              </Field>
              <Field label={t("pageTools.batesStart")}>
                <TextInput
                  type="number"
                  className="input-sm"
                  value={bates.start}
                  onChange={(event) => patchBates({ start: Math.max(1, Number(event.target.value) || 1) })}
                />
              </Field>
              <Field label={t("pageTools.batesDigits")}>
                <TextInput
                  type="number"
                  className="input-sm"
                  value={bates.digits}
                  onChange={(event) => patchBates({ digits: Math.max(1, Number(event.target.value) || 1) })}
                />
              </Field>
            </div>
            <Field label={t("pageTools.numberingPosition")}>
              <select
                className="select"
                value={bates.position}
                onChange={(event) => patchBates({ position: event.target.value as BatesOptions["position"] })}
              >
                {BATES_POSITIONS.map((position) => (
                  <option key={position} value={position}>
                    {batesPositionLabel(position)}
                  </option>
                ))}
              </select>
            </Field>
          </OptionCard>
          <OptionCard title={t("metadata.signatures")}>
            <Toggle checked={keepSignatures} onChange={setKeepSignatures} label={t("metadata.keepSignatures")} />
            <p className="muted small">{t("annotate.keepSignaturesHint")}</p>
          </OptionCard>
          {session.result ? <ResultCard result={session.result} onReset={session.resetResult} /> : null}
        </>
      }
    />
  );
}

function NupTool({ initialFiles, dragging }: { initialFiles?: string[]; dragging: boolean }) {
  const t = useT();
  const session = useTool({ suffix: "_nup", accept: "pdf", initialPaths: initialFiles });
  const [options, setOptions] = useState<NupOptions>({
    perSheet: 2,
    booklet: false,
    orientation: "portrait",
    pageSize: "source",
    marginPt: 18,
    gutterPt: 0,
    border: false,
    pages: [],
  });
  const [selection, setSelection] = useState("");

  const patch = (values: Partial<NupOptions>) => setOptions((previous) => ({ ...previous, ...values }));

  const apply = () =>
    session.run(async (jobId, overwrite) =>
      nupPdf(
        session.primary?.path ?? "",
        session.outputSpec(overwrite),
        {
          ...options,
          booklet: options.perSheet === 4 ? false : options.booklet,
          pages: parsePageList(selection, session.info?.pageCount ?? 0) ?? [],
        },
        jobId,
        session.password || undefined,
      ),
    );

  return (
    <TwoColumn
      main={<InputColumn session={session} dragging={dragging} />}
      side={
        <>
          <OutputBar
            session={session}
            runLabel={t("pageTools.applyNup")}
            disabled={!session.primary}
            onRun={() => void apply()}
          />
          <OptionCard title={t("pageTools.nupTitle")}>
            <Field label={t("pageTools.nupPerSheet")}>
              <Segmented<"2" | "4">
                value={options.perSheet === 4 ? "4" : "2"}
                onChange={(value) =>
                  patch({ perSheet: Number(value), booklet: value === "4" ? false : options.booklet })
                }
                options={[
                  { value: "2", label: "2" },
                  { value: "4", label: "4" },
                ]}
              />
            </Field>
            <div
              aria-disabled={options.perSheet === 4}
              className={options.perSheet === 4 ? "opacity-50 cursor-not-allowed" : undefined}
            >
              <Toggle
                checked={options.booklet}
                onChange={(value) => {
                  if (options.perSheet === 4) return;
                  patch({ booklet: value });
                }}
                label={t("pageTools.nupBooklet")}
              />
            </div>
            <Field label={t("convert.orientation")}>
              <Segmented<NupOptions["orientation"]>
                value={options.orientation}
                onChange={(value) => patch({ orientation: value })}
                options={[
                  { value: "portrait", label: t("convert.portrait") },
                  { value: "landscape", label: t("convert.landscape") },
                ]}
              />
            </Field>
            <Field label={t("pageTools.nupPageSize")}>
              <select
                className="select"
                value={options.pageSize}
                onChange={(event) => patch({ pageSize: event.target.value as NupOptions["pageSize"] })}
              >
                <option value="source">{t("pageTools.nupSource")}</option>
                <option value="a4">A4</option>
                <option value="letter">Letter</option>
              </select>
            </Field>
            <Field label={t("pageTools.nupMargin")}>
              <Slider value={options.marginPt} min={0} max={72} onChange={(value) => patch({ marginPt: value })} />
            </Field>
            <Field label={t("pageTools.nupGutter")}>
              <Slider value={options.gutterPt} min={0} max={48} onChange={(value) => patch({ gutterPt: value })} />
            </Field>
            <Toggle
              checked={options.border}
              onChange={(value) => patch({ border: value })}
              label={t("pageTools.nupBorder")}
            />
            <SelectionField
              value={selection}
              onChange={setSelection}
              pageCount={session.info?.pageCount ?? 0}
              disabled
            />
          </OptionCard>
          {session.result ? <ResultCard result={session.result} onReset={session.resetResult} /> : null}
        </>
      }
    />
  );
}

function Numbering({ initialFiles, dragging }: { initialFiles?: string[]; dragging: boolean }) {
  const t = useT();
  const session = useTool({ suffix: "_numbered", accept: "pdf", initialPaths: initialFiles });
  const [keepSignatures, setKeepSignatures] = useState(true);
  const [options, setOptions] = useState<NumberingOptions>({
    position: "bottom_center",
    format: "n",
    start_number: 1,
    font_size_pt: 11,
    color: "#333333",
    margin_pt: 28,
    pages: [],
    count_from_start: true,
  });
  const [selection, setSelection] = useState("");

  const patch = (values: Partial<NumberingOptions>) => setOptions((previous) => ({ ...previous, ...values }));

  return (
    <TwoColumn
      main={<InputColumn session={session} dragging={dragging} />}
      side={
        <>
          <OutputBar
            session={session}
            runLabel={t("pageTools.applyNumbers")}
            disabled={!session.primary}
            onRun={() =>
              void session.run(async (jobId, overwrite) =>
                addPageNumbers(
                  session.primary?.path ?? "",
                  session.outputSpec(overwrite),
                  options,
                  jobId,
                  session.password || undefined,
                  keepSignatures,
                ),
              )
            }
          />
          <OptionCard title={t("metadata.signatures")}>
            <Toggle checked={keepSignatures} onChange={setKeepSignatures} label={t("metadata.keepSignatures")} />
            <p className="muted small">{t("annotate.keepSignaturesHint")}</p>
          </OptionCard>
          <OptionCard title={t("pageTools.numberingTitle")}>
            <Field label={t("pageTools.numberingPosition")}>
              <select
                className="select"
                value={options.position}
                onChange={(event) => patch({ position: event.target.value as WatermarkPosition })}
              >
                {POSITIONS.map((position) => (
                  <option key={position} value={position}>
                    {position
                      .split("_")
                      .map((part) => part[0].toUpperCase() + part.slice(1))
                      .join(" ")}
                  </option>
                ))}
              </select>
            </Field>
            <Field label={t("pageTools.numberingFormat")}>
              <select
                className="select"
                value={options.format}
                onChange={(event) => patch({ format: event.target.value as NumberingOptions["format"] })}
              >
                <option value="n">1, 2, 3</option>
                <option value="page_n">Page 1</option>
                <option value="n_of_total">1 / 20</option>
                <option value="page_n_of_total">Page 1 of 20</option>
              </select>
            </Field>
            <Field label={t("pageTools.startNumber")}>
              <TextInput
                type="number"
                value={options.start_number}
                onChange={(event) => patch({ start_number: Math.max(1, Number(event.target.value) || 1) })}
              />
            </Field>
            <Field label={t("annotate.fontSize")}>
              <Slider
                value={options.font_size_pt}
                min={7}
                max={28}
                onChange={(value) => patch({ font_size_pt: value })}
              />
            </Field>
            <Field label={t("convert.margin")}>
              <Slider value={options.margin_pt} min={6} max={90} onChange={(value) => patch({ margin_pt: value })} />
            </Field>
            <Toggle
              checked={options.count_from_start}
              onChange={(value) => patch({ count_from_start: value })}
              label={t("pageTools.countFromStart")}
            />
            <SelectionField
              value={selection}
              onChange={setSelection}
              pageCount={session.info?.pageCount ?? 0}
              disabled
            />
            <p className="text-xs muted flex items-center gap-1.5">
              <Hash size={12} /> {t("common.example")}{" "}
              {options.format === "n" ? "1" : options.format === "page_n" ? "Page 1" : "1 / 20"}
            </p>
          </OptionCard>
          {session.result ? <ResultCard result={session.result} onReset={session.resetResult} /> : null}
        </>
      }
    />
  );
}
