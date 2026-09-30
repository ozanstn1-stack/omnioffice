import { useEffect, useState } from "react";
import { Eraser, Save } from "lucide-react";
import { Button, Card, Field, TextInput } from "../components/ui";
import { DropZone, FileList, InfoStrip, OutputBar, ResultCard } from "../components/files";
import { OptionCard, TwoColumn } from "../components/layout";
import { useT } from "../lib/i18n";
import { useTool } from "../lib/useTool";
import { editMetadata } from "../lib/api";

const EMPTY = {
  title: "",
  author: "",
  subject: "",
  keywords: "",
  creator: "",
  producer: "",
  creation_date: "",
  mod_date: "",
};

export function Metadata({ initialFiles, dragging }: { initialFiles?: string[]; dragging: boolean }) {
  const t = useT();
  const session = useTool({ suffix: "_metadata", accept: "pdf", initialPaths: initialFiles });
  const [form, setForm] = useState({ ...EMPTY });
  const [keepSignatures, setKeepSignatures] = useState(true);

  useEffect(() => {
    if (session.info) {
      setForm({
        title: session.info.metadata.title,
        author: session.info.metadata.author,
        subject: session.info.metadata.subject,
        keywords: session.info.metadata.keywords,
        creator: session.info.metadata.creator,
        producer: session.info.metadata.producer,
        creation_date: session.info.metadata.creation_date,
        mod_date: session.info.metadata.mod_date,
      });
    }
  }, [session.info]);

  const patch = (values: Partial<typeof form>) => setForm((previous) => ({ ...previous, ...values }));

  session.registerAutoRun(() => void save());
  const save = () =>
    session.run(async (jobId, overwrite) =>
      editMetadata(
        session.primary?.path ?? "",
        session.outputSpec(overwrite),
        form,
        false,
        jobId,
        session.password || undefined,
        keepSignatures,
      ),
    );

  const remove = () =>
    session.run(async (jobId, overwrite) =>
      editMetadata(
        session.primary?.path ?? "",
        session.outputSpec(overwrite),
        { ...EMPTY },
        true,
        jobId,
        session.password || undefined,
      ),
    );

  const fields: { key: keyof typeof form; label: string; readOnly?: boolean }[] = [
    { key: "title", label: t("metadata.fieldTitle") },
    { key: "author", label: t("metadata.author") },
    { key: "subject", label: t("metadata.subject") },
    { key: "keywords", label: t("metadata.keywords") },
    { key: "creator", label: t("metadata.creator") },
    { key: "producer", label: t("metadata.producer") },
    { key: "creation_date", label: t("metadata.creationDate") },
    { key: "mod_date", label: t("metadata.modDate") },
  ];

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
            <OptionCard title={t("metadata.signatures")}>
              <label className="row gap-2">
                <input
                  type="checkbox"
                  checked={keepSignatures}
                  onChange={(event) => setKeepSignatures(event.target.checked)}
                />
                <span className="small">{t("metadata.keepSignatures")}</span>
              </label>
              <p className="muted small">{t("metadata.keepSignaturesHint")}</p>
            </OptionCard>
            <OptionCard title={t("metadata.title")}>
              <div className="grid gap-3" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(240px, 1fr))" }}>
                {fields.map((field) => (
                  <Field key={field.key} label={field.label}>
                    <TextInput
                      value={form[field.key]}
                      onChange={(event) => patch({ [field.key]: event.target.value })}
                    />
                  </Field>
                ))}
              </div>
            </OptionCard>
          </>
        )
      }
      side={
        <>
          <OutputBar
            session={session}
            runLabel={t("metadata.save")}
            disabled={!session.primary}
            onRun={() => void save()}
          />
          <Card className="p-4 flex flex-col gap-2">
            <Button
              variant="danger"
              icon={<Eraser size={15} />}
              onClick={() => void remove()}
              disabled={!session.primary}
            >
              {t("metadata.remove")}
            </Button>
            <Button
              variant="ghost"
              icon={<Save size={15} />}
              onClick={() => void session.reloadInfo()}
              disabled={!session.primary}
            >
              {t("common.reset")}
            </Button>
          </Card>
          {session.result ? <ResultCard result={session.result} onReset={session.resetResult} /> : null}
        </>
      }
    />
  );
}
