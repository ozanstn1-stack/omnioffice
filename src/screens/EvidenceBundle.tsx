import { useState } from "react";
import { FolderCheck, ShieldCheck } from "lucide-react";
import { Badge, Button, Card } from "../components/ui";
import { DropZone, FileList, OutputBar, ResultCard } from "../components/files";
import { OptionCard, Screen, TwoColumn } from "../components/layout";
import {
  buildEvidenceBundle,
  matchEvidenceOriginals,
  verifyEvidenceBundle,
  type EvidenceManifest,
  type EvidenceVerification,
} from "../lib/evidence-api";
import { pickDocumentTokens } from "../lib/file-tokens";
import { fileBaseName } from "../lib/format";
import { useT } from "../lib/i18n";
import { isAndroid, pickAndroidFiles } from "../lib/mobile";
import { reportError, useSettings } from "../lib/store";
import { useTool } from "../lib/useTool";

/** Picks PDFs: the system picker on Android, file tokens on the desktop. */
async function pickPdfs(multiple: boolean): Promise<{ path: string; name: string }[]> {
  if (isAndroid()) {
    const paths = await pickAndroidFiles({ multiple, accept: "pdf" });
    return paths.map((path) => ({ path, name: fileBaseName(path) }));
  }
  const refs = await pickDocumentTokens(["pdf"], multiple);
  return refs.map((ref) => ({ path: ref.token, name: ref.name }));
}

const defaultLabel = (name: string) => name.replace(/\.pdf$/i, "");

const DETAIL_FIELDS = [
  ["title", "evidence.caseTitle"],
  ["caseReference", "evidence.caseReference"],
  ["court", "evidence.court"],
  ["submittedBy", "evidence.submittedBy"],
] as const;

type DetailKey = (typeof DETAIL_FIELDS)[number][0];

/** The manifest of a bundle just built or just verified. */
function ManifestCard({ manifest }: { manifest: EvidenceManifest }) {
  const t = useT();
  return (
    <Card className="p-4 flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        <Badge tone="accent">{manifest.id}</Badge>
        <span className="text-xs muted">
          {t("evidence.summary", { exhibits: manifest.entries.length, pages: manifest.totalPages })}
        </span>
      </div>
      <ol className="flex flex-col gap-2">
        {manifest.entries.map((entry) => (
          <li key={entry.number} className="min-w-0">
            <div className="flex flex-wrap justify-between gap-2 text-sm">
              <span className="font-semibold">
                {t("evidence.exhibit", { n: entry.number })} {entry.label}
              </span>
              <span className="muted">
                {entry.firstPage === entry.lastPage
                  ? t("evidence.page", { page: entry.firstPage })
                  : t("evidence.pages", { first: entry.firstPage, last: entry.lastPage })}
              </span>
            </div>
            <p className="text-xs muted break-all font-mono">{entry.sha256}</p>
          </li>
        ))}
      </ol>
    </Card>
  );
}

export function EvidenceBundleScreen({ initialFiles, dragging }: { initialFiles?: string[]; dragging: boolean }) {
  const t = useT();
  const language = useSettings((s) => s.settings.language);
  const session = useTool({ suffix: "_evidence", multiple: true, accept: "pdf", initialPaths: initialFiles });
  const [labels, setLabels] = useState<Record<string, string>>({});
  const [details, setDetails] = useState<Record<DetailKey, string>>({
    title: "",
    caseReference: "",
    court: "",
    submittedBy: "",
  });
  const [manifest, setManifest] = useState<EvidenceManifest | null>(null);
  const [checked, setChecked] = useState<{ path: string; verification: EvidenceVerification } | null>(null);
  const [matches, setMatches] = useState<{ name: string; exhibit: number | null }[] | null>(null);
  const [busy, setBusy] = useState(false);

  const labelOf = (path: string, name: string) => labels[path] ?? defaultLabel(name);

  const run = () =>
    session.run(async (_jobId, overwrite) => {
      setManifest(null);
      const exhibits = session.files.map((file) => ({
        path: file.path,
        label: labelOf(file.path, file.name),
        name: file.name,
      }));
      const built = await buildEvidenceBundle(
        exhibits,
        { ...details, language: language === "tr" ? "tr" : "en" },
        session.outputSpec(overwrite),
      );
      setManifest(built.manifest);
      return built.result;
    });
  session.registerAutoRun(() => void run());

  const verify = async () => {
    const [bundle] = await pickPdfs(false);
    if (!bundle) return;
    setBusy(true);
    setMatches(null);
    try {
      setChecked({ path: bundle.path, verification: await verifyEvidenceBundle(bundle.path) });
    } catch (error) {
      reportError(error, t);
    } finally {
      setBusy(false);
    }
  };

  const compareOriginals = async () => {
    if (!checked) return;
    const originals = await pickPdfs(true);
    if (!originals.length) return;
    setBusy(true);
    try {
      const found = await matchEvidenceOriginals(
        checked.path,
        originals.map((original) => original.path),
      );
      setMatches(found.map((match, index) => ({ name: originals[index]?.name ?? match.path, exhibit: match.exhibit })));
    } catch (error) {
      reportError(error, t);
    } finally {
      setBusy(false);
    }
  };

  const verification = checked?.verification;
  return (
    <Screen title={t("evidence.title")} subtitle={t("evidence.subtitle")}>
      <TwoColumn
        main={
          <>
            {session.files.length === 0 ? (
              <DropZone onPaths={(paths) => void session.addPaths(paths)} dragging={dragging} multiple accept="pdf" />
            ) : (
              <OptionCard title={t("evidence.exhibits", { n: session.files.length })}>
                <FileList
                  files={session.files}
                  reorder
                  onRemove={session.removeFile}
                  onMove={session.moveFile}
                  onAdd={session.pickFiles}
                  addLabel={t("common.addPdf")}
                />
                <div className="flex flex-col gap-2 mt-2">
                  {session.files.map((file, index) => (
                    <div key={file.path} className="flex items-center gap-2 min-w-0">
                      <span className="text-xs muted shrink-0 w-12">{t("evidence.exhibit", { n: index + 1 })}</span>
                      <input
                        className="input min-w-0 flex-1"
                        aria-label={t("evidence.labelFor", { n: index + 1 })}
                        value={labelOf(file.path, file.name)}
                        maxLength={160}
                        onChange={(event) => setLabels((current) => ({ ...current, [file.path]: event.target.value }))}
                      />
                    </div>
                  ))}
                </div>
              </OptionCard>
            )}
            {session.result ? <ResultCard result={session.result} onReset={session.resetResult} /> : null}
            {manifest ? (
              <div data-testid="evidence-manifest">
                <ManifestCard manifest={manifest} />
              </div>
            ) : null}

            <OptionCard title={t("evidence.verifyTitle")}>
              <p className="text-xs muted -mt-1">{t("evidence.verifyHint")}</p>
              <div className="flex flex-wrap gap-2">
                <Button
                  variant="default"
                  icon={<ShieldCheck size={14} />}
                  disabled={busy}
                  onClick={() => void verify()}
                >
                  {t("evidence.verifyPick")}
                </Button>
                {verification?.present ? (
                  <Button
                    variant="default"
                    icon={<FolderCheck size={14} />}
                    disabled={busy}
                    onClick={() => void compareOriginals()}
                  >
                    {t("evidence.originalsPick")}
                  </Button>
                ) : null}
              </div>
              {verification ? (
                <p className="text-sm" role="status">
                  {!verification.present ? (
                    t("evidence.verifyNone")
                  ) : verification.contentMatches ? (
                    <Badge tone="ok">{t("evidence.verifyOk", { id: verification.manifest?.id ?? "" })}</Badge>
                  ) : (
                    <Badge tone="danger">{t("evidence.verifyChanged")}</Badge>
                  )}
                </p>
              ) : null}
              {matches ? (
                <ul className="flex flex-col gap-1 text-sm">
                  {matches.map((match, index) => (
                    <li key={`${match.name}-${index}`}>
                      {match.exhibit
                        ? t("evidence.originalMatch", { name: match.name, n: match.exhibit })
                        : t("evidence.originalNoMatch", { name: match.name })}
                    </li>
                  ))}
                </ul>
              ) : null}
              {verification?.present && verification.manifest ? (
                <ManifestCard manifest={verification.manifest} />
              ) : null}
            </OptionCard>
          </>
        }
        side={
          <>
            <OutputBar
              session={session}
              runLabel={t("evidence.run")}
              onRun={() => void run()}
              disabled={session.files.length === 0}
            />
            <Card className="p-4 flex flex-col gap-3">
              <h3 className="text-sm font-semibold">{t("evidence.details")}</h3>
              {DETAIL_FIELDS.map(([key, labelKey]) => (
                <div key={key}>
                  <label className="label" htmlFor={`evidence-${key}`}>
                    {t(labelKey)}
                  </label>
                  <input
                    id={`evidence-${key}`}
                    className="input"
                    value={details[key]}
                    maxLength={160}
                    onChange={(event) => setDetails((current) => ({ ...current, [key]: event.target.value }))}
                  />
                </div>
              ))}
              <p className="text-xs muted">{t("evidence.localNote")}</p>
            </Card>
          </>
        }
      />
    </Screen>
  );
}
