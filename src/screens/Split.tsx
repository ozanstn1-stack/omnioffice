import { useState } from "react";
import { FolderOpen, Scissors } from "lucide-react";
import { isAndroid, openAnyFile, revealAnyFile } from "../lib/mobile";
import { Button, Card, Field, Segmented, TextArea, TextInput } from "../components/ui";
import { DropZone, FileList, InfoStrip, OutputBar } from "../components/files";
import { OptionCard, Screen, TwoColumn } from "../components/layout";
import { useT } from "../lib/i18n";
import { useTool } from "../lib/useTool";
import { splitPdf } from "../lib/api";
import { parsePageList } from "../lib/format";
import type { SplitMode } from "../lib/types";

type Mode = "ranges" | "every_n" | "individual" | "at_pages";

export function Split({ initialFiles, dragging }: { initialFiles?: string[]; dragging: boolean }) {
  const t = useT();
  const session = useTool({
    suffix: "_split",
    accept: "pdf",
    multiOutput: true,
    initialPaths: initialFiles,
  });
  const [mode, setMode] = useState<Mode>("ranges");
  const [rangesText, setRangesText] = useState("1-5\n6-10");
  const [everyN, setEveryN] = useState(5);
  const [atPagesText, setAtPagesText] = useState("5");
  const [parts, setParts] = useState<{ path: string; first_page: number; last_page: number }[] | null>(null);
  const [validationError, setValidationError] = useState<string | null>(null);

  const pageCount = session.info?.pageCount ?? 0;

  const buildMode = (): SplitMode | null => {
    setValidationError(null);
    if (mode === "ranges") {
      const ranges = rangesText
        .split(/[\n,;]/)
        .map((value) => value.trim())
        .filter(Boolean);
      if (!ranges.length) {
        setValidationError(t("errors.invalid_input"));
        return null;
      }
      return { mode: "ranges", ranges };
    }
    if (mode === "every_n") {
      if (everyN < 1) {
        setValidationError(t("errors.invalid_input"));
        return null;
      }
      return { mode: "every_n", n: everyN };
    }
    if (mode === "at_pages") {
      const pages = parsePageList(atPagesText, pageCount || 100000);
      if (!pages) {
        setValidationError(t("errors.invalid_input"));
        return null;
      }
      return { mode: "at_pages", pages };
    }
    return { mode: "individual" };
  };

  session.registerAutoRun(() => void run());
  const run = () =>
    session.run(async (jobId, overwrite) => {
      const splitMode = buildMode();
      if (!splitMode) throw { code: "invalid_input", message: t("errors.invalid_input") };
      const result = await splitPdf(
        session.primary?.path ?? "",
        splitMode,
        session.outputDir,
        overwrite,
        jobId,
        session.password || undefined,
      );
      setParts(result.parts);
      await session.publish(result.parts.map((part) => part.path));
      return {
        path: `${result.parts.length ? result.parts[0].path : session.outputDir}`,
        pageCount: result.parts.length,
        message: t("split.created", { count: result.parts.length }),
      };
    });

  return (
    <Screen
      title={t("split.title")}
      subtitle={t("split.subtitle")}
      actions={
        session.primary ? (
          <Button variant="ghost" size="sm" icon={<Scissors size={14} />} onClick={session.clearFiles}>
            {t("common.clear")}
          </Button>
        ) : null
      }
    >
      <TwoColumn
        main={
          <>
            {!session.primary ? (
              <DropZone onPaths={(paths) => void session.addPaths(paths)} dragging={dragging} accept="pdf" />
            ) : (
              <>
                <OptionCard title={t("common.selectFile")}>
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
                <OptionCard title={t("split.title")}>
                  <Field label={t("common.pageRange")}>
                    <Segmented<Mode>
                      value={mode}
                      onChange={setMode}
                      options={[
                        { value: "ranges", label: t("split.modeRanges") },
                        { value: "every_n", label: t("split.modeEveryN") },
                        { value: "individual", label: t("split.modeIndividual") },
                        { value: "at_pages", label: t("split.modeAtPages") },
                      ]}
                    />
                  </Field>
                  {mode === "ranges" ? (
                    <Field label={t("common.pageRange")} hint={t("split.rangesHint")}>
                      <TextArea
                        value={rangesText}
                        onChange={(event) => setRangesText(event.target.value)}
                        rows={4}
                        spellCheck={false}
                      />
                    </Field>
                  ) : null}
                  {mode === "every_n" ? (
                    <Field label={t("split.everyN")}>
                      <TextInput
                        type="number"
                        min={1}
                        value={everyN}
                        onChange={(event) => setEveryN(Math.max(1, Number(event.target.value) || 1))}
                      />
                    </Field>
                  ) : null}
                  {mode === "at_pages" ? (
                    <Field label={t("common.pageRange")} hint={t("split.atPagesHint")}>
                      <TextInput
                        value={atPagesText}
                        onChange={(event) => setAtPagesText(event.target.value)}
                        placeholder="4, 10"
                      />
                    </Field>
                  ) : null}
                  {mode === "individual" ? (
                    <p className="text-xs muted">
                      {pageCount ? `${pageCount} ${t("common.pages")} → ${pageCount} PDF` : t("common.loading")}
                    </p>
                  ) : null}
                  {validationError ? (
                    <p className="text-xs" style={{ color: "var(--danger)" }}>
                      {validationError}
                    </p>
                  ) : null}
                </OptionCard>
              </>
            )}
            {parts ? (
              <Card className="p-4 fade-in">
                <div className="flex items-center justify-between mb-3">
                  <p className="font-semibold text-[13.5px]">{t("split.created", { count: parts.length })}</p>
                  <Button
                    size="sm"
                    icon={<FolderOpen size={14} />}
                    onClick={() =>
                      void revealAnyFile(parts[0] ? parts[0].path : session.outputDir).catch(() => undefined)
                    }
                  >
                    {isAndroid() ? t("common.share") : t("common.openFolder")}
                  </Button>
                </div>
                <div className="flex flex-col gap-1 max-h-[280px] overflow-y-auto">
                  {parts.map((part) => (
                    <button
                      key={part.path}
                      className="text-left text-[13px] px-2.5 py-1.5 rounded-lg hover:bg-[var(--surface-2)] truncate"
                      onClick={() => void openAnyFile(part.path).catch(() => undefined)}
                      title={part.path}
                    >
                      {part.path.split(/[\\/]/).pop()}{" "}
                      <span className="muted">
                        ({part.first_page}-{part.last_page})
                      </span>
                    </button>
                  ))}
                </div>
              </Card>
            ) : null}
          </>
        }
        side={
          <>
            <OutputBar
              session={session}
              runLabel={t("split.run")}
              outputKind="folder"
              showOverwrite
              disabled={!session.primary}
              onRun={() => void run()}
            />
          </>
        }
      />
    </Screen>
  );
}
