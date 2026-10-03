import { Combine } from "lucide-react";
import { Button, Card } from "../components/ui";
import { DropZone, FileList, InfoStrip, OutputBar, ResultCard } from "../components/files";
import { OptionCard, Screen, TwoColumn } from "../components/layout";
import { useT } from "../lib/i18n";
import { useTool } from "../lib/useTool";
import { useSettings } from "../lib/store";
import { useState } from "react";
import { mergePdfs } from "../lib/api";

export function Merge({ initialFiles, dragging }: { initialFiles?: string[]; dragging: boolean }) {
  const t = useT();
  const session = useTool({
    suffix: "_merged",
    multiple: true,
    accept: "pdf",
    loadInfo: true,
    initialPaths: initialFiles,
  });
  const [preserveMetadata, setPreserveMetadata] = useState(true);
  const defaultCompression = useSettings((s) => s.settings);

  session.registerAutoRun(() => void run());
  const run = () =>
    session.run(async (jobId, overwrite) => {
      const paths = session.files.map((file) => file.path);
      return mergePdfs(paths, session.outputSpec(overwrite), preserveMetadata, jobId);
    });

  return (
    <Screen
      title={t("merge.title")}
      subtitle={t("merge.subtitle")}
      actions={
        session.files.length ? (
          <Button variant="ghost" size="sm" icon={<Combine size={14} />} onClick={session.pickFiles}>
            {t("common.addFiles")}
          </Button>
        ) : null
      }
    >
      <TwoColumn
        main={
          <>
            {session.files.length === 0 ? (
              <DropZone onPaths={(paths) => void session.addPaths(paths)} dragging={dragging} multiple accept="pdf" />
            ) : (
              <OptionCard title={`${session.files.length} ${t("common.pages")}`}>
                <p className="text-xs muted -mt-1">{t("merge.dragHint")}</p>
                <FileList
                  files={session.files}
                  reorder
                  onRemove={session.removeFile}
                  onMove={session.moveFile}
                  onAdd={session.pickFiles}
                  addLabel={t("common.addPdf")}
                />
              </OptionCard>
            )}
            {session.info ? (
              <Card className="p-4">
                <InfoStrip info={session.info} error={session.infoError} />
              </Card>
            ) : null}
            {session.result ? <ResultCard result={session.result} onReset={session.resetResult} /> : null}
          </>
        }
        side={
          <>
            <OutputBar
              session={session}
              runLabel={t("merge.run")}
              onRun={() => void run()}
              disabled={session.files.length < 2}
            />
            <Card className="p-4 flex flex-col gap-3">
              <label className="checkbox">
                <input
                  type="checkbox"
                  checked={preserveMetadata}
                  onChange={(event) => setPreserveMetadata(event.target.checked)}
                />
                <span>{t("merge.preserveMetadata")}</span>
              </label>
              <p className="text-xs muted">
                {defaultCompression.language === "tr"
                  ? "Birleştirme tamamen yerel olarak yapılır; dosyalarınız cihazdan ayrılmaz."
                  : "Merging happens fully offline; your documents never leave this device."}
              </p>
            </Card>
          </>
        }
      />
    </Screen>
  );
}
