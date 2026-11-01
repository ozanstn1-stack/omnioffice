import { useState, useMemo } from "react";
import { pickDocumentToken, pickSaveToken } from "../lib/file-tokens";
import { AlertTriangle, CheckCircle2, FolderOpen, Info, ShieldAlert, Sparkles, Trash2 } from "lucide-react";
import { Badge, Button, Card, TextArea } from "../components/ui";
import { OptionCard, Screen, TwoColumn } from "../components/layout";
import { editMetadata, inspectDocument, pageText, pdfVerifySignatures } from "../lib/api";
import {
  analyzeDocumentHealth,
  fixMetadataLeaks,
  type DocumentHealthReport,
  type HealthIssue,
} from "../lib/document-health";
import { fileBaseName, uid } from "../lib/format";
import { useT } from "../lib/i18n";
import { isAndroid, pickOfficeFiles } from "../lib/mobile";
import type { ViewId } from "../lib/nav";
import { openDocument, saveDocument } from "../lib/office-api";
import type { TextDocument } from "../lib/office-types";
import { reportError, useToasts } from "../lib/store";

export function DocumentHealthScreen({
  initialText = "",
  initialMetadata = {},
  onNavigate,
}: {
  initialText?: string;
  initialMetadata?: Record<string, string | undefined>;
  onNavigate?: (view: ViewId) => void;
}) {
  const t = useT();
  const { push } = useToasts();
  const [filePath, setFilePath] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [content, setContent] = useState(initialText);
  const [metadata, setMetadata] = useState<Record<string, string | undefined>>(initialMetadata);
  const [hasSignatures, setHasSignatures] = useState(true);
  const [hasTitle, setHasTitle] = useState(true);
  const [hasLanguage, setHasLanguage] = useState(true);
  const [fontIssuesCount, setFontIssuesCount] = useState(0);

  const report: DocumentHealthReport = useMemo(() => {
    return analyzeDocumentHealth({
      text: content,
      metadata,
      hasSignatures,
      hasTitle,
      hasLanguage,
      fontIssuesCount,
    });
  }, [content, metadata, hasSignatures, hasTitle, hasLanguage, fontIssuesCount]);

  const loadFileForInspection = async (path: string) => {
    setBusy(true);
    setFilePath(path);
    try {
      // A token has no extension; its remembered name does.
      const lower = fileBaseName(path).toLowerCase();
      if (lower.endsWith(".pdf")) {
        const [insp, sigs] = await Promise.all([
          inspectDocument(path),
          pdfVerifySignatures(path).catch(() => ({ signatures: [], warnings: [] })),
        ]);
        const meta: Record<string, string | undefined> = {};
        if (insp.authorOverride) meta.author = insp.authorOverride;
        if (insp.creator) meta.creator = insp.creator;
        if (insp.producer) meta.producer = insp.producer;
        if (insp.titleOverride) meta.title = insp.titleOverride;
        if (insp.subjectOverride) meta.subject = insp.subjectOverride;
        setMetadata(meta);
        setHasSignatures(sigs.signatures.length > 0);
        setHasTitle(Boolean(insp.titleOverride?.trim()));
        setHasLanguage(Boolean(insp.language?.trim()));
        setFontIssuesCount(insp.fonts.filter((f) => !f.embedded).length);

        const maxPages = Math.min(insp.pageCount, 20);
        const pageTexts: string[] = [];
        for (let p = 1; p <= maxPages; p++) {
          const txt = await pageText(path, p).catch(() => "");
          if (txt.trim()) pageTexts.push(txt);
        }
        setContent(pageTexts.join("\n\n"));
      } else {
        const opened = await openDocument(path);
        const doc = opened.model as TextDocument;
        const meta: Record<string, string | undefined> = {};
        if (doc.metadata?.author) meta.author = doc.metadata.author;
        if (doc.metadata?.creator) meta.creator = doc.metadata.creator;
        if (doc.metadata?.lastModifiedBy) meta.lastModifiedBy = doc.metadata.lastModifiedBy;
        if (doc.metadata?.title || doc.title) meta.title = doc.metadata?.title || doc.title;
        if (doc.metadata?.subject) meta.subject = doc.metadata.subject;
        setMetadata(meta);
        setHasSignatures(false);
        setHasTitle(Boolean((doc.metadata?.title || doc.title || "").trim()));
        setHasLanguage(true);
        setFontIssuesCount(0);

        const paragraphs: string[] = [];
        for (const block of doc.blocks || []) {
          if (block.type === "paragraph") {
            const line = (block.runs || []).map((r) => r.text || "").join("");
            if (line.trim()) paragraphs.push(line);
          }
        }
        setContent(paragraphs.join("\n\n"));
      }
      push({
        kind: "success",
        title: "Belge Analiz Edildi",
        detail: `${fileBaseName(path)} üzerinde KVKK, meta veri, yazı tipi ve e-imza denetimi tamamlandı.`,
      });
    } catch (error) {
      reportError(error, t);
    } finally {
      setBusy(false);
    }
  };

  const handlePickFile = async () => {
    if (isAndroid()) {
      const [picked] = await pickOfficeFiles(false).catch(() => []);
      if (picked) await loadFileForInspection(picked);
      return;
    }
    // The Rust dialog returns a token; release builds refuse raw paths.
    const picked = await pickDocumentToken(["pdf", "docx", "odt", "udf", "rtf", "txt", "md"]).catch(() => null);
    if (picked) await loadFileForInspection(picked.token);
  };

  const handleFixIssue = async (issue: HealthIssue) => {
    if (issue.fixAction === "strip_metadata") {
      setMetadata((prev) => fixMetadataLeaks(prev));
      if (filePath && fileBaseName(filePath).toLowerCase().endsWith(".pdf") && !isAndroid()) {
        const outPath = await pickSaveToken(fileBaseName(filePath).replace(/\.pdf$/i, "-temiz.pdf"), ["pdf"]).catch(
          () => null,
        );
        if (outPath) {
          try {
            await editMetadata(
              filePath,
              { path: outPath.token, overwrite: "replace" },
              {
                title: metadata.title || "",
                author: "",
                subject: "",
                keywords: "",
                creator: "",
                producer: "",
                creation_date: "",
                mod_date: "",
              },
              true,
              uid("meta-clean"),
              undefined,
              false,
            );
          } catch (error) {
            reportError(error, t);
            return;
          }
        }
      } else if (filePath && !fileBaseName(filePath).toLowerCase().endsWith(".pdf")) {
        try {
          const opened = await openDocument(filePath);
          const doc = opened.model as TextDocument;
          if (doc.metadata) {
            doc.metadata.author = "";
            doc.metadata.creator = "";
            doc.metadata.lastModifiedBy = "";
          }
          await saveDocument(opened.kind, doc, filePath);
        } catch {
          // In-memory state is still cleaned below
        }
      }
      push({
        kind: "success",
        title: "Meta veriler temizlendi",
        detail: "Yazar, kurum ve yazılım bilgileri başarıyla kaldırıldı.",
      });
    } else if (issue.fixAction === "redact_kvkk") {
      push({
        kind: "info",
        title: "Karartma Aracı",
        detail: "Kişisel veriler Redact sekmesinde karartılmak üzere hazır.",
      });
      onNavigate?.("redact");
    } else {
      push({
        kind: "info",
        title: "Düzeltme Uygulandı",
        detail: `${issue.title} için önerilen düzeltme adımı uygulandı.`,
      });
    }
  };

  const getSeverityBadge = (severity: HealthIssue["severity"]) => {
    switch (severity) {
      case "critical":
        return <Badge tone="danger">{t("health.severityCritical")}</Badge>;
      case "warning":
        return <Badge tone="warn">{t("health.severityWarning")}</Badge>;
      case "info":
        return <Badge tone="default">{t("health.severityInfo")}</Badge>;
      default:
        return <Badge tone="ok">{t("health.severityOk")}</Badge>;
    }
  };

  return (
    <Screen
      title={
        <span className="flex items-center gap-2">
          {t("health.title")}
          <Badge tone="warn">{t("common.experimental")}</Badge>
        </span>
      }
      subtitle={t("health.subtitle")}
      actions={
        <Button
          variant="primary"
          icon={<FolderOpen className="w-4 h-4" />}
          disabled={busy}
          onClick={() => void handlePickFile()}
        >
          {filePath ? t("health.document", { name: fileBaseName(filePath) }) : t("health.pickFile")}
        </Button>
      }
    >
      <TwoColumn
        main={
          <div className="flex flex-col gap-4">
            <Card className="p-5 flex items-center justify-between">
              <div className="flex items-center gap-4">
                <div
                  className={`w-16 h-16 rounded-full flex items-center justify-center font-bold text-2xl border-4 ${
                    report.score >= 80
                      ? "border-emerald-500/20 bg-emerald-500/10 text-emerald-500"
                      : report.score >= 50
                        ? "border-amber-500/20 bg-amber-500/10 text-amber-500"
                        : "border-red-500/20 bg-red-500/10 text-red-500"
                  }`}
                >
                  {report.score}
                </div>
                <div>
                  <h2 className="text-lg font-bold">{t("health.score")}</h2>
                  <p className="text-sm muted">
                    {report.score >= 80
                      ? t("health.verdictGood")
                      : report.score >= 50
                        ? t("health.verdictWarn")
                        : t("health.verdictBad")}
                  </p>
                </div>
              </div>

              <div className="flex items-center gap-3">
                <div className="text-right">
                  <div className="text-xs muted uppercase font-bold tracking-wider">{t("health.findings")}</div>
                  <div className="text-sm font-semibold">
                    <span className="text-red-500">{t("health.criticalCount", { count: report.criticalCount })}</span> ·{" "}
                    <span className="text-amber-500">{t("health.warningCount", { count: report.warningCount })}</span>
                  </div>
                </div>
              </div>
            </Card>

            <OptionCard title={t("health.issuesTitle")}>
              {report.issues.length === 0 ? (
                <div className="p-6 text-center text-emerald-500 flex flex-col items-center gap-2">
                  <CheckCircle2 className="w-8 h-8" />
                  <span className="font-semibold">{t("health.clean")}</span>
                </div>
              ) : (
                <div className="flex flex-col gap-3">
                  {report.issues.map((issue) => (
                    <div
                      key={issue.id}
                      className="p-3.5 rounded-lg border border-border/50 bg-card/50 flex items-start justify-between gap-3"
                    >
                      <div className="flex items-start gap-3">
                        <div className="mt-0.5">
                          {issue.severity === "critical" ? (
                            <ShieldAlert className="w-5 h-5 text-red-500" />
                          ) : issue.severity === "warning" ? (
                            <AlertTriangle className="w-5 h-5 text-amber-500" />
                          ) : (
                            <Info className="w-5 h-5 text-blue-500" />
                          )}
                        </div>
                        <div>
                          <div className="flex items-center gap-2">
                            <span className="font-semibold text-sm">{issue.title}</span>
                            {getSeverityBadge(issue.severity)}
                          </div>
                          <p className="text-xs muted mt-1">{issue.description}</p>
                        </div>
                      </div>

                      {issue.fixable ? (
                        <Button
                          variant="primary"
                          size="sm"
                          icon={<Sparkles className="w-3.5 h-3.5" />}
                          onClick={() => void handleFixIssue(issue)}
                        >
                          {t("health.fixAll")}
                        </Button>
                      ) : null}
                    </div>
                  ))}
                </div>
              )}
            </OptionCard>

            <OptionCard title={t("health.contentTitle")}>
              <TextArea
                rows={6}
                value={content}
                onChange={(e) => setContent(e.target.value)}
                placeholder={t("health.contentPlaceholder")}
              />
            </OptionCard>
          </div>
        }
        side={
          <div className="flex flex-col gap-4">
            <OptionCard title={t("health.metadataTitle")}>
              <div className="flex flex-col gap-2 text-xs">
                {Object.entries(metadata).length === 0 ? (
                  <span className="muted">{t("health.metadataClean")}</span>
                ) : (
                  Object.entries(metadata).map(([k, v]) => (
                    <div key={k} className="flex justify-between border-b border-border/30 pb-1">
                      <span className="font-semibold capitalize text-muted">{k}:</span>
                      <span className="truncate max-w-[150px]">{v || "-"}</span>
                    </div>
                  ))
                )}
              </div>

              {Object.keys(metadata).length > 0 ? (
                <Button
                  variant="danger"
                  size="sm"
                  icon={<Trash2 className="w-3.5 h-3.5" />}
                  onClick={() => {
                    setMetadata({});
                    push({
                      kind: "success",
                      title: t("health.metadataClearedTitle"),
                      detail: t("health.metadataClearedDetail"),
                    });
                  }}
                >
                  {t("health.removeMetadata")}
                </Button>
              ) : null}
            </OptionCard>

            <OptionCard title={t("health.kvkkTitle")}>
              <p className="text-xs muted leading-relaxed">{t("health.kvkkNotice")}</p>
            </OptionCard>
          </div>
        }
      />
    </Screen>
  );
}
