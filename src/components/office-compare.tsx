/**
 * Office document comparison inside the Compare screen (C13): pick two office
 * files, open them as models and show the structural diff. PDF comparison
 * stays the screen's other mode; this component never renders pages.
 */
import { useState } from "react";
import { Download, FileSearch, Play } from "lucide-react";
import { Badge, Button, Card, Field } from "./ui";
import { OfficeDiffView } from "./office-diff-view";
import { pickDocumentToken, pickSaveToken, type FileToken } from "../lib/file-tokens";
import { useT } from "../lib/i18n";
import { asWriterModel, openDocument, saveDocument } from "../lib/office-api";
import { diffOfficeDocuments, type OfficeDiff } from "../lib/office-diff";
import { diffDocuments } from "../office/writer/compare";
import type { TextDocument } from "../lib/office-types";
import { officeDocumentExtensions } from "../lib/format";
import { reportError } from "../lib/store";

function FileRow({ label, picked, onPick }: { label: string; picked: FileToken | null; onPick: () => void }) {
  const t = useT();
  return (
    <Field label={label}>
      <div className="row">
        <span className="small" style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
          {picked ? picked.name : t("diff.notPicked")}
        </span>
        <Button size="sm" variant="ghost" icon={<FileSearch size={13} />} onClick={onPick}>
          {t("diff.pick")}
        </Button>
      </div>
    </Field>
  );
}

export function OfficeCompare() {
  const t = useT();
  const [left, setLeft] = useState<FileToken | null>(null);
  const [right, setRight] = useState<FileToken | null>(null);
  const [busy, setBusy] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [savedName, setSavedName] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [diff, setDiff] = useState<OfficeDiff | null>(null);
  const [writerPair, setWriterPair] = useState<{ base: TextDocument; current: TextDocument } | null>(null);

  const pick = async (setter: (picked: FileToken) => void) => {
    const picked = await pickDocumentToken([...officeDocumentExtensions()]).catch(() => null);
    if (picked) {
      setter(picked);
      setDiff(null);
      setWriterPair(null);
      setSavedName(null);
    }
  };

  const run = async () => {
    if (!left || !right) return;
    setBusy(true);
    setError(null);
    setDiff(null);
    setWriterPair(null);
    setSavedName(null);
    try {
      const [base, current] = await Promise.all([openDocument(left.token), openDocument(right.token)]);
      if (base.kind !== current.kind) {
        setError(t("diff.differentKinds", { left: base.kind, right: current.kind }));
        return;
      }
      setDiff(diffOfficeDocuments(base.kind, base.model, current.model));
      if (base.kind === "writer") {
        setWriterPair({ base: asWriterModel(base.model), current: asWriterModel(current.model) });
      }
    } catch (caught) {
      reportError(caught, t);
    } finally {
      setBusy(false);
    }
  };

  const exportRedline = async () => {
    if (!writerPair) return;
    const baseName = (right?.name ?? "document").replace(/\.[^.]+$/, "");
    const target = await pickSaveToken(`${baseName}-redline.docx`, ["docx"]).catch(() => null);
    if (!target) return;
    setExporting(true);
    setSavedName(null);
    try {
      const redline = diffDocuments(writerPair.base, writerPair.current, "Comparison");
      await saveDocument("writer", redline, target.token);
      setSavedName(target.name);
    } catch (caught) {
      reportError(caught, t);
    } finally {
      setExporting(false);
    }
  };

  return (
    <div className="stack">
      <Card>
        <p className="muted small">{t("diff.officeHint")}</p>
        <div className="stack mt-2">
          <FileRow label={t("diff.left")} picked={left} onPick={() => void pick(setLeft)} />
          <FileRow label={t("diff.right")} picked={right} onPick={() => void pick(setRight)} />
          <div className="row" style={{ flexWrap: "wrap", gap: 8 }}>
            <Button
              size="sm"
              variant="primary"
              icon={<Play size={13} />}
              disabled={busy || !left || !right}
              onClick={() => void run()}
            >
              {t("diff.run")}
            </Button>
            {writerPair ? (
              <Button
                size="sm"
                variant="secondary"
                icon={<Download size={13} />}
                disabled={exporting}
                onClick={() => void exportRedline()}
              >
                {t("diff.exportRedlineDocx")}
              </Button>
            ) : null}
            {savedName ? <Badge tone="ok">{t("diff.redlineSaved", { name: savedName })}</Badge> : null}
          </div>
          {error ? (
            <p className="small" style={{ color: "var(--danger, #ef4444)" }}>
              {error}
            </p>
          ) : null}
        </div>
      </Card>
      {diff ? <OfficeDiffView diff={diff} leftLabel={left?.name} rightLabel={right?.name} /> : null}
    </div>
  );
}
