import { useState, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { appCacheDir, join } from "@tauri-apps/api/path";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { AlertTriangle, Copy, Download, Edit3, FileText, Shield } from "lucide-react";
import { Badge, Button, Card, Field, Segmented, TextArea, TextInput } from "../components/ui";
import { OptionCard, Screen, TwoColumn } from "../components/layout";
import { generateKvkkText, type KvkkTemplateType } from "../lib/kvkk-templates";
import { useT } from "../lib/i18n";
import { uid } from "../lib/format";
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
        await invoke("ensure_dir", { path: dir });
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

    const path = await saveDialog({
      defaultPath: defaultName,
      filters:
        format === "docx"
          ? [{ name: "Word Belgesi (*.docx)", extensions: ["docx"] }]
          : [{ name: "PDF Belgesi (*.pdf)", extensions: ["pdf"] }],
    });

    if (!path || typeof path !== "string") return;

    setBusy(true);
    try {
      if (format === "pdf") {
        await exportPdf("writer", doc, path);
      } else {
        await saveDocument("writer", doc, path);
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
          KVKK Aydınlatma ve Açık Rıza Sihirbazı
          <Badge tone="warn">{t("common.experimental")}</Badge>
        </span>
      }
      subtitle="Kurumunuza özel çalışan, müşteri, ziyaretçi ve açık rıza taslak metinlerini saniyeler içinde hazırlayın."
    >
      <TwoColumn
        main={
          <div className="flex flex-col gap-4">
            <OptionCard title="Şablon Türü Seçimi">
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

            <OptionCard title="Üretilen KVKK Metni Önizlemesi">
              <TextArea rows={14} value={generatedText} readOnly spellCheck={false} className="font-mono text-xs" />
              <div className="flex flex-wrap justify-end gap-2 mt-2">
                <Button variant="default" icon={<Copy className="w-4 h-4" />} onClick={handleCopy}>
                  Metni Kopyala
                </Button>
                <Button
                  variant="default"
                  icon={<Edit3 className="w-4 h-4" />}
                  disabled={busy}
                  onClick={handleOpenInWriter}
                >
                  Writer&apos;da Aç
                </Button>
                <Button
                  variant="default"
                  icon={<FileText className="w-4 h-4" />}
                  disabled={busy}
                  onClick={() => void handleExport("docx")}
                >
                  DOCX İndir
                </Button>
                <Button
                  variant="primary"
                  icon={<Download className="w-4 h-4" />}
                  disabled={busy}
                  onClick={() => void handleExport("pdf")}
                >
                  PDF İndir
                </Button>
              </div>
            </OptionCard>
          </div>
        }
        side={
          <div className="flex flex-col gap-4">
            <OptionCard title="Kurum & Veri Sorumlusu Bilgileri">
              <Field label="Şirket / Kurum Unvanı">
                <TextInput
                  value={companyName}
                  onChange={(e) => setCompanyName(e.target.value)}
                  placeholder="Örn: Örnek Bilişim Tic. A.Ş."
                />
              </Field>

              <Field label="KVKK İrtibat E-Postası">
                <TextInput
                  value={contactEmail}
                  onChange={(e) => setContactEmail(e.target.value)}
                  placeholder="kvkk@sirketiniz.com"
                />
              </Field>

              <Field label="Şirket Adresi">
                <TextInput value={address} onChange={(e) => setAddress(e.target.value)} placeholder="İlçe / İl" />
              </Field>
            </OptionCard>

            <Card className="p-4 flex flex-col gap-2">
              <div className="flex items-center gap-2 font-semibold text-sm">
                <Shield className="w-4 h-4 text-emerald-500" />
                <span>6698 Sayılı Kanun Çerçevesi</span>
              </div>
              <p className="text-xs muted leading-relaxed">
                Bu şablonlar 6698 sayılı Kişisel Verilerin Korunması Kanunu ve Aydınlatma Yükümlülüğünün Yerine
                Getirilmesinde Uyulacak Usul ve Esaslar Hakkında Tebliğ esas alınarak hazırlanmıştır.
              </p>
            </Card>

            <Card className="p-4 flex flex-col gap-2 border-amber-500/40 bg-amber-500/5">
              <div className="flex items-center gap-2 font-semibold text-sm text-amber-500">
                <AlertTriangle className="w-4 h-4" />
                <span>Hukuki Uyarı</span>
              </div>
              <p className="text-xs muted leading-relaxed">
                Bu şablonlar genel bilgilendirme amacıyla hazırlanmış örnek taslaklardır; hukuki danışmanlık veya
                bağlayıcı görüş niteliği taşımaz. Kurumunuzun veri işleme envanterine göre hukuk müşaviriniz tarafından
                incelenmelidir.
              </p>
            </Card>
          </div>
        }
      />
    </Screen>
  );
}
