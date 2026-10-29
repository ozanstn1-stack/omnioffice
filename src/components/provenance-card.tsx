/**
 * Source manifest card (C8): shows what a document was made from and lets the
 * user add the record to a copy. The heavy lifting is the local
 * `pdf_provenance_*` commands; this card only picks sources and renders the
 * answer, including the "changed after the manifest" verdict.
 */
import { useCallback, useEffect, useState } from "react";
import { FilePlus2, Fingerprint, ListTree, ShieldAlert, ShieldCheck } from "lucide-react";
import { Badge, Button, Card, Modal, Spinner, TextArea } from "./ui";
import { FileList } from "./files";
import { appInfo, toAppError } from "../lib/api";
import type { FileToken } from "../lib/file-tokens";
import { fileBaseName, formatBytes } from "../lib/format";
import { useT } from "../lib/i18n";
import { isAndroid } from "../lib/mobile";
import { pickDocumentTokens, pickSaveToken } from "../lib/file-tokens";
import { readProvenance, writeProvenance, type ProvenanceManifest, type ProvenanceReport } from "../lib/provenance-api";
import { reportError, useToasts } from "../lib/store";

function shortDigest(digest: string): string {
  return digest.length > 16 ? `${digest.slice(0, 16)}…` : digest;
}

/** What a written manifest says; shown after a save so the user sees the result. */
interface SavedManifest {
  path: string;
  manifest: ProvenanceManifest;
}

export function ProvenanceCard({ path }: { path: string }) {
  const t = useT();
  const push = useToasts((state) => state.push);
  const [report, setReport] = useState<ProvenanceReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [open, setOpen] = useState(false);
  const [sources, setSources] = useState<FileToken[]>([]);
  const [note, setNote] = useState("");
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState<SavedManifest | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setReport(await readProvenance(path));
    } catch (caught) {
      setReport(null);
      setError(toAppError(caught).message);
    } finally {
      setLoading(false);
    }
  }, [path]);

  useEffect(() => {
    // The loader owns the report/loading state; a new document starts a new read.
    // eslint-disable-next-line react-hooks/set-state-in-effect -- the loader owns the report state
    setSaved(null);
    void load();
  }, [load]);

  const pickSources = async () => {
    const picked = await pickDocumentTokens(undefined, true).catch(() => []);
    if (!picked.length) return;
    setSources((previous) => {
      const known = new Set(previous.map((source) => source.token));
      return [...previous, ...picked.filter((source) => !known.has(source.token))];
    });
  };

  const save = async () => {
    const suggested = `${fileBaseName(path).replace(/\.pdf$/i, "")}-provenance.pdf`;
    const target = await pickSaveToken(suggested, ["pdf"]).catch(() => null);
    if (!target) return;
    setSaving(true);
    try {
      const info = await appInfo().catch(() => null);
      const result = await writeProvenance({
        input: path,
        output: { path: target.token },
        module: t("provenance.moduleManual"),
        note: note.trim(),
        appVersion: info?.appVersion ?? "",
        sources: sources.map((source) => ({ path: source.token, name: source.name, role: "source" })),
        ai: null,
      });
      setSaved({ path: result.result.path, manifest: result.manifest });
      setOpen(false);
      setSources([]);
      setNote("");
      push({ kind: "success", title: t("provenance.saved"), detail: result.result.path });
    } catch (caught) {
      reportError(caught, t);
    } finally {
      setSaving(false);
    }
  };

  const manifest = saved?.manifest ?? report?.manifest ?? null;
  const intact = saved ? true : (report?.intact ?? false);

  return (
    <Card>
      <div className="flex items-center justify-between gap-2 flex-wrap">
        <div className="row">
          <ListTree size={16} />
          <strong>{t("provenance.title")}</strong>
          {manifest ? (
            <Badge tone={intact ? "ok" : "danger"}>{intact ? t("provenance.intact") : t("provenance.changed")}</Badge>
          ) : null}
        </div>
        {!isAndroid() && !saved ? (
          <Button size="sm" icon={<FilePlus2 size={14} />} disabled={loading} onClick={() => setOpen(true)}>
            {t("provenance.add")}
          </Button>
        ) : null}
      </div>

      {loading ? (
        <div className="row mt-2">
          <Spinner size={14} /> <span className="muted small">{t("provenance.loading")}</span>
        </div>
      ) : null}
      {error ? (
        <p className="small mt-2" role="status">
          <Badge tone="danger">{t("errors.title")}</Badge> {error}
        </p>
      ) : null}

      {!loading && !error && manifest ? (
        <div className="stack mt-3">
          {!intact ? <p className="small">{t("provenance.changedHint")}</p> : null}
          {saved ? <p className="muted small">{t("provenance.savedTo", { path: saved.path })}</p> : null}
          <div className="flex flex-wrap gap-4">
            <p className="muted small">
              {t("provenance.module")}: <strong>{manifest.module}</strong>
            </p>
            <p className="muted small">
              {t("provenance.createdAt")}: <strong>{manifest.createdAt}</strong>
            </p>
            <p className="muted small">
              {t("provenance.appVersion")}: <strong>{manifest.appVersion || "?"}</strong>
            </p>
          </div>
          {manifest.note ? <p className="small">{manifest.note}</p> : null}
          {manifest.sources.length ? (
            <div className="stack">
              <p className="text-xs muted">{t("provenance.sources")}</p>
              {manifest.sources.map((source, position) => (
                <p key={position} className="muted small">
                  {source.fileName} · {formatBytes(source.size)} · {shortDigest(source.sha256)}
                  {source.role ? ` · ${source.role}` : ""}
                </p>
              ))}
            </div>
          ) : null}
          {manifest.ai ? (
            <p className="muted small">
              {t("provenance.ai")}: {manifest.ai.provider} · {manifest.ai.model}
              {manifest.ai.citations ? ` · ${t("provenance.citations", { count: manifest.ai.citations })}` : ""}
            </p>
          ) : null}
          <p className="muted small row">
            <Fingerprint size={13} /> {t("provenance.digest")}: {shortDigest(manifest.documentSha256)}
          </p>
        </div>
      ) : null}

      {!loading && !error && !manifest ? (
        <div className="stack mt-2">
          <p className="muted small">{t("provenance.absent")}</p>
          <p className="muted small">{isAndroid() ? t("provenance.android") : t("provenance.absentHint")}</p>
        </div>
      ) : null}

      {open ? (
        <Modal
          title={t("provenance.add")}
          onClose={() => setOpen(false)}
          width={720}
          footer={
            <div className="row" style={{ justifyContent: "flex-end", flexWrap: "wrap" }}>
              <Button size="sm" variant="ghost" onClick={() => setOpen(false)}>
                {t("provenance.close")}
              </Button>
              <Button size="sm" variant="primary" disabled={saving} onClick={() => void save()}>
                {saving ? t("provenance.saving") : t("provenance.save")}
              </Button>
            </div>
          }
        >
          <div className="stack">
            <p className="muted small">{t("provenance.subtitle")}</p>
            <FileList
              files={sources.map((source) => ({ path: source.token, name: source.name, sizeBytes: 0 }))}
              onRemove={(index) => setSources((previous) => previous.filter((_, position) => position !== index))}
              onAdd={() => void pickSources()}
              addLabel={t("provenance.pickSources")}
            />
            <label className="text-xs muted" htmlFor="provenance-note">
              {t("provenance.noteLabel")}
            </label>
            <TextArea
              id="provenance-note"
              rows={3}
              value={note}
              placeholder={t("provenance.notePlaceholder")}
              onChange={(event) => setNote(event.target.value)}
            />
            {manifest ? (
              <p className="muted small">
                <ShieldCheck size={13} /> {t("provenance.replaces")}
              </p>
            ) : null}
            <p className="muted small row">
              <ShieldAlert size={13} /> {t("provenance.signedNote")}
            </p>
          </div>
        </Modal>
      ) : null}
    </Card>
  );
}
