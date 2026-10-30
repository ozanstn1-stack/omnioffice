/**
 * Office document comparison inside the Compare screen (C13): pick two office
 * files, open them as models and show the structural diff. PDF comparison
 * stays the screen's other mode; this component never renders pages.
 */
import { useState } from "react";
import { FileSearch, Play } from "lucide-react";
import { Button, Card, Field } from "./ui";
import { OfficeDiffView } from "./office-diff-view";
import { pickDocumentToken, type FileToken } from "../lib/file-tokens";
import { useT } from "../lib/i18n";
import { openDocument } from "../lib/office-api";
import { diffOfficeDocuments, type OfficeDiff } from "../lib/office-diff";
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
  const [error, setError] = useState<string | null>(null);
  const [diff, setDiff] = useState<OfficeDiff | null>(null);

  const pick = async (setter: (picked: FileToken) => void) => {
    const picked = await pickDocumentToken([...officeDocumentExtensions()]).catch(() => null);
    if (picked) {
      setter(picked);
      setDiff(null);
    }
  };

  const run = async () => {
    if (!left || !right) return;
    setBusy(true);
    setError(null);
    setDiff(null);
    try {
      const [base, current] = await Promise.all([openDocument(left.token), openDocument(right.token)]);
      if (base.kind !== current.kind) {
        setError(t("diff.differentKinds", { left: base.kind, right: current.kind }));
        return;
      }
      setDiff(diffOfficeDocuments(base.kind, base.model, current.model));
    } catch (caught) {
      reportError(caught, t);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="stack">
      <Card>
        <p className="muted small">{t("diff.officeHint")}</p>
        <div className="stack mt-2">
          <FileRow label={t("diff.left")} picked={left} onPick={() => void pick(setLeft)} />
          <FileRow label={t("diff.right")} picked={right} onPick={() => void pick(setRight)} />
          <div className="row">
            <Button
              size="sm"
              variant="primary"
              icon={<Play size={13} />}
              disabled={busy || !left || !right}
              onClick={() => void run()}
            >
              {t("diff.run")}
            </Button>
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
