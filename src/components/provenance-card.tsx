/**
 * Source manifest card (C8, D7): shows what a document was made from and lets
 * the user add the record to a copy.
 *
 * The heavy lifting is the local `provenance_read_chain` / `provenance_write`
 * commands: they sniff a PDF or DOCX container and return one derivation chain
 * (sources → transformation → output) with an integrity verdict. A digest
 * mismatch is shown as a tamper warning before the recorded sources, because a
 * changed document makes every recorded source claim worthless.
 */
import { useCallback, useEffect, useState } from "react";
import { FilePlus2, ListTree } from "lucide-react";
import { Badge, Button, Card, Spinner } from "./ui";
import { appInfo, toAppError } from "../lib/api";
import type { FileToken } from "../lib/file-tokens";
import { fileBaseName } from "../lib/format";
import { useT } from "../lib/i18n";
import { ProvenanceAddDialog } from "./provenance-add-dialog";
import { ProvenanceChain } from "./provenance-chain";
import { isAndroid } from "../lib/mobile";
import { pickDocumentTokens, pickSaveToken } from "../lib/file-tokens";
import { readSourceChain, writeProvenance, type SourceChainReport } from "../lib/provenance-api";
import { reportError, useToasts } from "../lib/store";

const TONE: Record<string, "ok" | "danger" | "warn" | "accent"> = {
  intact: "ok",
  changed: "danger",
  unsupported: "warn",
  absent: "accent",
};

export function ProvenanceCard({ path }: { path: string }) {
  const t = useT();
  const push = useToasts((state) => state.push);
  const [chain, setChain] = useState<SourceChainReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [open, setOpen] = useState(false);
  const [sources, setSources] = useState<FileToken[]>([]);
  const [note, setNote] = useState("");
  const [saving, setSaving] = useState(false);
  const [savedTo, setSavedTo] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setChain(await readSourceChain(path));
    } catch (caught) {
      setChain(null);
      setError(toAppError(caught).message);
    } finally {
      setLoading(false);
    }
  }, [path]);

  useEffect(() => {
    // The loader owns the chain state; a new document starts a new read.
    // eslint-disable-next-line react-hooks/set-state-in-effect -- the loader owns the chain state
    setSavedTo(null);
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
    const docx = /\.docx$/i.test(path);
    const extension = docx ? "docx" : "pdf";
    const suggested = `${fileBaseName(path).replace(/\.(pdf|docx)$/i, "")}-provenance.${extension}`;
    const target = await pickSaveToken(suggested, [extension]).catch(() => null);
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
      setSavedTo(result.result.path);
      setOpen(false);
      setSources([]);
      setNote("");
      await load();
      push({ kind: "success", title: t("provenance.saved"), detail: result.result.path });
    } catch (caught) {
      reportError(caught, t);
    } finally {
      setSaving(false);
    }
  };

  const verdict = chain?.verdict ?? "absent";
  const present = Boolean(chain?.present);
  const changed = verdict === "changed";
  const docx = /\.docx$/i.test(path);

  return (
    <Card>
      <div className="flex items-center justify-between gap-2 flex-wrap">
        <div className="row">
          <ListTree size={16} />
          <strong>{t("provenance.title")}</strong>
          {present ? (
            <Badge tone={TONE[verdict] ?? "accent"}>{t(`provenance.${changed ? "changed" : "intact"}`)}</Badge>
          ) : null}
          {chain && chain.format ? <Badge tone="accent">{chain.format.toUpperCase()}</Badge> : null}
        </div>
        {!isAndroid() ? (
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

      {!loading && !error && present && chain ? <ProvenanceChain chain={chain} savedTo={savedTo} /> : null}

      {!loading && !error && chain && !present ? (
        <div className="stack mt-2">
          {verdict === "unsupported" ? (
            <p className="muted small">
              <Badge tone="warn">{t("provenance.chainUnsupported")}</Badge>
            </p>
          ) : (
            <>
              <p className="muted small">{t("provenance.absent")}</p>
              <p className="muted small">{isAndroid() ? t("provenance.android") : t("provenance.absentHint")}</p>
            </>
          )}
        </div>
      ) : null}

      <ProvenanceAddDialog
        open={open}
        docx={docx}
        sources={sources}
        note={note}
        saving={saving}
        onClose={() => setOpen(false)}
        onPickSources={() => void pickSources()}
        onRemoveSource={(index) => setSources((previous) => previous.filter((_, position) => position !== index))}
        onNote={setNote}
        onSave={() => void save()}
      />
    </Card>
  );
}
