import { useState } from "react";
import { Highlighter, Image as ImageIcon, Minus, Square, Trash2, Type } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { isAndroid, pickAndroidFiles } from "../lib/mobile";
import { Badge, Button, Card, ColorInput, Field, Slider, Toggle } from "../components/ui";
import { DropZone, InfoStrip, OutputBar, ResultCard } from "../components/files";
import { PageCanvas, Pager } from "../components/pages";
import { OptionCard, TwoColumn } from "../components/layout";
import { useT } from "../lib/i18n";
import { useTool } from "../lib/useTool";
import { annotatePdf } from "../lib/api";
import type { Annotation, AnnotationKind } from "../lib/types";

const KINDS: { value: AnnotationKind; icon: React.ReactNode; labelKey: string }[] = [
  { value: "text", icon: <Type size={14} />, labelKey: "annotate.textTool" },
  { value: "image", icon: <ImageIcon size={14} />, labelKey: "annotate.imageTool" },
  { value: "rect", icon: <Square size={14} />, labelKey: "annotate.rectTool" },
  { value: "highlight", icon: <Highlighter size={14} />, labelKey: "annotate.highlightTool" },
  { value: "line", icon: <Minus size={14} />, labelKey: "annotate.lineTool" },
];

export function Annotate({ initialFiles, dragging }: { initialFiles?: string[]; dragging: boolean }) {
  const t = useT();
  const session = useTool({ suffix: "_annotated", accept: "pdf", initialPaths: initialFiles });
  const [kind, setKind] = useState<AnnotationKind>("text");
  const [text, setText] = useState("");
  const [fontSize, setFontSize] = useState(16);
  const [bold, setBold] = useState(false);
  const [color, setColor] = useState("#e11d48");
  const [opacity, setOpacity] = useState(0.35);
  const [lineWidth, setLineWidth] = useState(2);
  const [imagePath, setImagePath] = useState<string | null>(null);
  const [keepSignatures, setKeepSignatures] = useState(true);
  const [page, setPage] = useState(1);
  const [annotations, setAnnotations] = useState<Annotation[]>([]);

  const pageCount = session.info?.pageCount ?? 1;
  const geometry = session.info?.pageGeometries.find((entry) => entry.page === page);

  const addAnnotation = (x: number, y: number, existing?: Annotation) => {
    if (!geometry) return;
    const width = geometry.display_width_pt;
    const height = geometry.display_height_pt;
    const base: Annotation = existing ?? {
      kind,
      page,
      x: x * width,
      y: y * height,
      w: kind === "text" ? width * 0.4 : width * 0.3,
      h: kind === "text" ? fontSize * 2 : height * 0.08,
      text,
      font_size_pt: fontSize,
      bold,
      color,
      opacity: kind === "highlight" ? opacity : 1,
      image_path: imagePath,
      line_width_pt: lineWidth,
      x2: kind === "line" ? x * width + width * 0.25 : null,
      y2: kind === "line" ? y * height + height * 0.1 : null,
    };
    setAnnotations((previous) => [...previous, base]);
  };

  const patchLast = (values: Partial<Annotation>) => {
    setAnnotations((previous) =>
      previous.map((item, index) => (index === previous.length - 1 ? { ...item, ...values } : item)),
    );
  };

  const run = () =>
    session.run(async (jobId, overwrite) =>
      annotatePdf(
        session.primary?.path ?? "",
        session.outputSpec(overwrite),
        annotations,
        jobId,
        session.password || undefined,
        keepSignatures,
      ),
    );

  const pickImage = async () => {
    if (isAndroid()) {
      const paths = await pickAndroidFiles({ multiple: false, accept: "image" }).catch(() => []);
      if (paths.length) setImagePath(paths[0]);
      return;
    }
    const picked = await open({ multiple: false, filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg"] }] });
    if (picked) setImagePath(String(picked));
  };

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
              <p className="text-xs muted mb-2">{t("annotate.placeHint")}</p>
              <div className="mx-auto" style={{ maxWidth: 560 }}>
                <PageCanvas
                  path={session.primary.path}
                  page={page}
                  password={session.password || undefined}
                  maxWidth={1100}
                  dragging
                  onPointClick={(x, y) => {
                    if (kind === "image" && !imagePath) return;
                    addAnnotation(x, y);
                  }}
                  overlay={
                    <>
                      {annotations
                        .filter((annotation) => annotation.page === page && geometry)
                        .map((annotation, index) => {
                          const width = geometry?.display_width_pt ?? 1;
                          const height = geometry?.display_height_pt ?? 1;
                          const left = `${(annotation.x / width) * 100}%`;
                          const top = `${(annotation.y / height) * 100}%`;
                          const boxWidth = `${(annotation.w / width) * 100}%`;
                          const boxHeight = `${(annotation.h / height) * 100}%`;
                          if (annotation.kind === "line") {
                            return null;
                          }
                          return (
                            <div
                              key={index}
                              style={{
                                position: "absolute",
                                left,
                                top,
                                width: boxWidth,
                                height: boxHeight,
                                border:
                                  annotation.kind === "rect"
                                    ? `${annotation.line_width_pt}px solid ${annotation.color}`
                                    : "none",
                                background:
                                  annotation.kind === "highlight"
                                    ? annotation.color
                                    : annotation.kind === "text"
                                      ? "transparent"
                                      : "transparent",
                                opacity: annotation.kind === "highlight" ? annotation.opacity : 1,
                                color: annotation.color,
                                fontSize: `${Math.max(8, annotation.font_size_pt * 0.72)}px`,
                                fontWeight: annotation.bold ? 700 : 400,
                                overflow: "hidden",
                                pointerEvents: "none",
                              }}
                            >
                              {annotation.kind === "text" ? annotation.text : null}
                              {annotation.kind === "image" ? "▣" : null}
                            </div>
                          );
                        })}
                    </>
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
            runLabel={t("annotate.apply")}
            onRun={() => void run()}
            disabled={!session.primary || !annotations.length}
          />
          <OptionCard title={t("metadata.signatures")}>
            <Toggle checked={keepSignatures} onChange={setKeepSignatures} label={t("metadata.keepSignatures")} />
            <p className="muted small">{t("annotate.keepSignaturesHint")}</p>
          </OptionCard>
          <OptionCard title={t("annotate.tools")}>
            <div className="flex flex-wrap gap-1.5">
              {KINDS.map((entry) => (
                <Button
                  key={entry.value}
                  size="sm"
                  variant={kind === entry.value ? "primary" : "default"}
                  icon={entry.icon}
                  onClick={() => setKind(entry.value)}
                >
                  {t(entry.labelKey)}
                </Button>
              ))}
            </div>
            {kind === "text" ? (
              <>
                <Field label={t("annotate.textPlaceholder")}>
                  <textarea
                    className="textarea"
                    rows={2}
                    value={text}
                    onChange={(event) => setText(event.target.value)}
                  />
                </Field>
                <Field label={t("annotate.fontSize")}>
                  <Slider value={fontSize} min={8} max={48} onChange={setFontSize} />
                </Field>
                <Toggle checked={bold} onChange={setBold} label={t("annotate.bold")} />
              </>
            ) : null}
            {kind === "image" ? (
              <Button size="sm" variant="ghost" icon={<ImageIcon size={14} />} onClick={() => void pickImage()}>
                {imagePath ? imagePath.split(/[\\/]/).pop() : t("annotate.imageFile")}
              </Button>
            ) : null}
            {kind === "rect" || kind === "line" ? (
              <Field label={t("annotate.lineWidth")}>
                <Slider value={lineWidth} min={1} max={10} onChange={setLineWidth} />
              </Field>
            ) : null}
            <Field label="Color">
              <ColorInput value={color} onChange={setColor} />
            </Field>
            {kind === "highlight" ? (
              <Field label="Opacity">
                <Slider
                  value={opacity * 100}
                  min={10}
                  max={90}
                  onChange={(value) => setOpacity(value / 100)}
                  format={(value) => `${value}%`}
                />
              </Field>
            ) : null}
          </OptionCard>

          <OptionCard title={`${t("annotate.list")} · ${annotations.length}`}>
            {annotations.length === 0 ? (
              <p className="text-xs muted">{t("annotate.none")}</p>
            ) : (
              <div className="flex flex-col gap-1.5 max-h-[220px] overflow-y-auto">
                {annotations.map((annotation, index) => (
                  <div key={index} className="card-soft flex items-center gap-2 px-2.5 py-1.5 text-[12.5px]">
                    <Badge tone="accent">{annotation.kind}</Badge>
                    <span className="truncate flex-1">
                      {t("common.page")} {annotation.page}
                      {annotation.text ? ` · ${annotation.text.slice(0, 24)}` : ""}
                    </span>
                    <Button
                      size="sm"
                      variant="ghost"
                      icon={<Trash2 size={12} />}
                      onClick={() => setAnnotations((previous) => previous.filter((_, i) => i !== index))}
                    />
                  </div>
                ))}
              </div>
            )}
            {annotations.length ? (
              <div className="flex gap-2">
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => {
                    const last = annotations[annotations.length - 1];
                    if (last) {
                      setPage(last.page);
                      patchLast({ w: last.w * 1.25, h: last.h * 1.25 });
                    }
                  }}
                >
                  {t("common.size")} +
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => {
                    const last = annotations[annotations.length - 1];
                    if (last) patchLast({ w: last.w * 0.8, h: last.h * 0.8 });
                  }}
                >
                  {t("common.size")} −
                </Button>
              </div>
            ) : null}
          </OptionCard>

          {session.result ? <ResultCard result={session.result} onReset={session.resetResult} /> : null}
        </>
      }
    />
  );
}
