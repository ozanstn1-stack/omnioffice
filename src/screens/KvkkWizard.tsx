import { useState, useMemo } from "react";
import { call } from "../lib/typed-call";
import { appCacheDir, join } from "@tauri-apps/api/path";
import { AlertTriangle, Copy, Download, Edit3, FileText, Shield } from "lucide-react";
import { Badge, Button, Card, Field, Segmented, TextArea, TextInput } from "../components/ui";
import { OptionCard, Screen, TwoColumn } from "../components/layout";
import { generateKvkkText, type KvkkTemplateType } from "../lib/kvkk-templates";
import { useT } from "../lib/i18n";
import { fileBaseName, uid } from "../lib/format";
import { pickSaveToken } from "../lib/file-tokens";
import { isAndroid, pickAndroidSaveTarget, publishOutputs } from "../lib/mobile";
import type { ViewId } from "../lib/nav";
import { defaultParaProps, newTextDocument, type Block, type TextDocument } from "../lib/office-types";
import { exportPdf, saveDocument } from "../lib/office-api";
import { useOfficeTabs } from "../lib/office-store";
import { reportError, useToasts } from "../lib/store";

function createKvkkDocument(title: string, text: string): TextDocument {
  const doc = newTextDocument(title);
  doc.page.marginTopPt = 71;
  doc.page.marginBottomPt = 71;
  doc.page.marginLeftPt = 71;
  doc.page.marginRightPt = 71;
  const paragraphs = text.split(/\n\n+/);
  const blocks: Block[] = paragraphs.map((para, idx) => {
    const isHeading = idx === 0 || /^[0-9]+\.\s+[A-ZÇĞİÖŞÜ]/.test(para.trim());
    return {
      type: "paragraph",
      props: {
        ...defaultParaProps(),
        align: idx === 0 ? "center" : "justify",
        lineSpacing: 1.15,
        spaceBeforePt: isHeading ? 6 : 0,
        spaceAfterPt: 6,
      },
      runs: [
        {
          text: para.trim(),
          font: "Times New Roman",
          sizePt: 12,
          bold: isHeading,
          italic: false,
          underline: false,
          strike: false,
          color: null,
          highlight: null,
          link: null,
          comment: null,
          superscript: false,
          subscript: false,
        },
      ],
    };
  });
  doc.blocks = blocks;
  return doc;
}

export function KvkkWizard({ onNavigate }: { onNavigate?: (view: ViewId) => void }) {
  const t = useT();
  const { push } = useToasts();
  const [templateType, setTemplateType] = useState<KvkkTemplateType>("employee");
  const [companyName, setCompanyName] = useState("");
  const [contactEmail, setContactEmail] = useState("");
  const [address, setAddress] = useState("");
  const [busy, setBusy] = useState(false);

  const generatedText = useMemo(() => {
    return generateKvkkText(templateType, {
      companyName,
      contactEmail,
      address,
    });
  }, [templateType, companyName, contactEmail, address]);

  const docTitle = useMemo(() => {
    const labels: Record<KvkkTemplateType, string> = {
      employee: "KVKK Çalışan Aydınlatma Metni",
      customer: "KVKK Müşteri Aydınlatma Metni",
      visitor: "KVKK Ziyaretçi Aydınlatma Metni",
      consent: "KVKK Açık Rıza Beyanı",
    };
    return labels[templateType];
  }, [templateType]);

  const handleCopy = () => {
    void navigator.clipboard?.writeText(generatedText);
    push({
      kind: "success",
      title: "Metin Panoya Kopyalandı",
      detail: "KVKK metni panoya aktarıldı, Word/Writer veya PDF çıktısı olarak kullanabilirsiniz.",
    });
  };

  const handleOpenInWriter = () => {
    const doc = createKvkkDocument(docTitle, generatedText);
    useOfficeTabs.getState().create("writer", docTitle, doc);
    onNavigate?.("office");
  };

  const handleExport = async (format: "docx" | "pdf") => {
    const doc = createKvkkDocument(docTitle, generatedText);
    const defaultName = `kvkk-${templateType}.${format}`;

    if (isAndroid()) {
      const target = await pickAndroidSaveTarget(defaultName, format);
      if (!target) return;
      setBusy(true);
      try {
        const cache = await appCacheDir();
        const dir = await join(cache, "kvkk-export");
        await call((c) => c.ensureDir(dir));
        const tempPath = await join(dir, `${uid("kvkk")}.${format}`);
        if (format === "pdf") {
          await exportPdf("writer", doc, tempPath);
        } else {
          await saveDocument("writer", doc, tempPath);
        }
        const failures = await publishOutputs([tempPath], { file: target });
        if (failures.length) {
          push({ kind: "error", title: t("errors.title"), detail: failures[0].error });
        } else {
          push({
            kind: "success",
            title: `${format.toUpperCase()} belgesi başarıyla kaydedildi!`,
            detail: target.name,
          });
        }
      } catch (error) {
        reportError(error, t);
      } finally {
        setBusy(false);
      }
      return;
    }

    const extension = format === "docx" ? "docx" : "pdf";
    const target = await pickSaveToken(fileBaseName(defaultName), [extension]).catch(() => null);
    if (!target) return;

    setBusy(true);
    try {
      if (format === "pdf") {
        await exportPdf("writer", doc, target.token);
      } else {
        await saveDocument("writer", doc, target.token);
      }
      push({
        kind: "success",
        title: `${format.toUpperCase()} belgesi başarıyla kaydedildi!`,
      });
    } catch (error) {
      reportError(error, t);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Screen
      title={
        <span className="flex items-center gap-2">
          {t("kvkk.wizardTitle")}
          <Badge tone="warn">{t("common.experimental")}</Badge>
        </span>
      }
      subtitle="Kurumunuza özel çalışan, müşteri, ziyaretçi ve açık rıza taslak metinlerini saniyeler içinde hazırlayın."
    >
      <TwoColumn
        main={
          <div className="flex flex-col gap-4">
            <OptionCard title={t("kvkk.templateTypeTitle")}>
              <Segmented<KvkkTemplateType>
                value={templateType}
                onChange={setTemplateType}
                options={[
                  { value: "employee", label: "Çalışan" },
                  { value: "customer", label: "Müşteri / Web" },
                  { value: "visitor", label: "Ziyaretçi (Kamera)" },
                  { value: "consent", label: "Açık Rıza" },
                ]}
              />
            </OptionCard>

            <OptionCard title={t("kvkk.previewTitle")}>
              <TextArea rows={14} value={generatedText} readOnly spellCheck={false} className="font-mono text-xs" />
              <div className="flex flex-wrap justify-end gap-2 mt-2">
                <Button variant="default" icon={<Copy className="w-4 h-4" />} onClick={handleCopy}>
                  {t("common.copy")}
                </Button>
                <Button
                  variant="default"
                  icon={<Edit3 className="w-4 h-4" />}
                  disabled={busy}
                  onClick={handleOpenInWriter}
                >
                  {t("common.openInWriter")}
                </Button>
                <Button
                  variant="default"
                  icon={<FileText className="w-4 h-4" />}
                  disabled={busy}
                  onClick={() => void handleExport("docx")}
                >
                  {t("kvkk.downloadDocx")}
                </Button>
                <Button
                  variant="primary"
                  icon={<Download className="w-4 h-4" />}
                  disabled={busy}
                  onClick={() => void handleExport("pdf")}
                >
                  {t("kvkk.downloadPdf")}
                </Button>
              </div>
            </OptionCard>
          </div>
        }
        side={
          <div className="flex flex-col gap-4">
            <OptionCard title={t("kvkk.responsibleTitle")}>
              <Field label="Şirket / Kurum Unvanı">
                <TextInput
                  value={companyName}
                  onChange={(e) => setCompanyName(e.target.value)}
                  placeholder={t("kvkk.companyPlaceholder")}
                />
              </Field>

              <Field label="KVKK İrtibat E-Postası">
                <TextInput
                  value={contactEmail}
                  onChange={(e) => setContactEmail(e.target.value)}
                  placeholder={t("kvkk.emailPlaceholder")}
                />
              </Field>

              <Field label="Şirket Adresi">
                <TextInput
                  value={address}
                  onChange={(e) => setAddress(e.target.value)}
                  placeholder={t("kvkk.addressPlaceholder")}
                />
              </Field>
            </OptionCard>

            <Card className="p-4 flex flex-col gap-2">
              <div className="flex items-center gap-2 font-semibold text-sm">
                <Shield className="w-4 h-4 text-emerald-500" />
                <span>{t("kvkk.lawTitle")}</span>
              </div>
              <p className="text-xs muted leading-relaxed">t("kvkk.lawBody")</p>
            </Card>

            <Card className="p-4 flex flex-col gap-2 border-amber-500/40 bg-amber-500/5">
              <div className="flex items-center gap-2 font-semibold text-sm text-amber-500">
                <AlertTriangle className="w-4 h-4" />
                <span>{t("kvkk.legalWarningTitle")}</span>
              </div>
              <p className="text-xs muted leading-relaxed">t("kvkk.legalWarningBody")</p>
            </Card>
          </div>
        }
      />
    </Screen>
  );
}
