import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { appCacheDir, join } from "@tauri-apps/api/path";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { FileText, Download, Edit3 } from "lucide-react";
import { Button, Card, Field, TextInput } from "../components/ui";
import { Screen } from "../components/layout";
import { useT } from "../lib/i18n";
import { flagEnabled } from "../lib/flags";
import { uid } from "../lib/format";
import { isAndroid, pickAndroidSaveTarget, publishOutputs } from "../lib/mobile";
import type { ViewId } from "../lib/nav";
import { useOfficeTabs } from "../lib/office-store";
import { reportError, useToasts } from "../lib/store";
import { saveDocument, exportPdf } from "../lib/office-api";
import { validateTCKN, createPetitionDocument, type PetitionData } from "../lib/petition";

/**
 * Petition Wizard (Dilekçe Sihirbazı): Guided petition creation with
 * TCKN mod-11 validation and export to UDF, DOCX, PDF, or Writer (v5.3 - Issue #247).
 */
export function PetitionWizard({ onNavigate }: { onNavigate?: (view: ViewId) => void }) {
  const t = useT();
  const [data, setData] = useState<PetitionData>({
    recipientAuthority: "",
    subject: "",
    bodyText: "",
    demands: "Gereğini saygılarımla arz ederim.",
    attachments: "",
    fullName: "",
    tckn: "",
    address: "",
    phone: "",
    date: new Date().toLocaleDateString("tr-TR"),
  });
  const [busy, setBusy] = useState(false);

  const update = (field: keyof PetitionData, value: string) => {
    setData((prev) => ({ ...prev, [field]: value }));
  };

  const validateForm = (): boolean => {
    if (!data.recipientAuthority.trim() || !data.subject.trim() || !data.fullName.trim() || !data.bodyText.trim()) {
      useToasts.getState().push({
        kind: "info",
        title: "Lütfen makam, konu, ad soyad ve açıklama alanlarını doldurun.",
      });
      return false;
    }

    if (data.tckn.trim() && !validateTCKN(data.tckn)) {
      useToasts.getState().push({
        kind: "error",
        title: "Geçersiz T.C. Kimlik Numarası!",
      });
      return false;
    }

    return true;
  };

  const handleOpenInWriter = () => {
    if (!validateForm()) return;
    const doc = createPetitionDocument(data);
    useOfficeTabs.getState().create("writer", data.subject.trim() || "Dilekçe", doc);
    onNavigate?.("office");
  };

  const handleExport = async (format: "udf" | "docx" | "pdf") => {
    if (!validateForm()) return;

    const doc = createPetitionDocument(data);
    const defaultName = `${data.subject.trim() || "dilekce"}.${format}`;

    if (isAndroid()) {
      const target = await pickAndroidSaveTarget(defaultName, format);
      if (!target) return;
      setBusy(true);
      try {
        const cache = await appCacheDir();
        const dir = await join(cache, "petition-export");
        await invoke("ensure_dir", { path: dir });
        const tempPath = await join(dir, `${uid("petition")}.${format}`);
        if (format === "pdf") {
          await exportPdf("writer", doc, tempPath);
        } else {
          await saveDocument("writer", doc, tempPath);
        }
        const failures = await publishOutputs([tempPath], { file: target });
        if (failures.length) {
          useToasts.getState().push({ kind: "error", title: t("errors.title"), detail: failures[0].error });
        } else {
          useToasts.getState().push({
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
        format === "udf"
          ? [{ name: "UYAP Doküman Formatı (*.udf)", extensions: ["udf"] }]
          : format === "docx"
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
      useToasts.getState().push({
        kind: "success",
        title: `${format.toUpperCase()} belgesi başarıyla kaydedildi!`,
      });
    } catch (error) {
      reportError(error, t);
    } finally {
      setBusy(false);
    }
  };

  const udfEnabled = flagEnabled("experimentalUdf");

  return (
    <Screen
      title="Dilekçe Sihirbazı"
      subtitle={
        udfEnabled
          ? "Resmî makamlara sunulmak üzere standartlara uygun UDF, DOCX ve PDF dilekçe oluşturun."
          : "Resmî makamlara sunulmak üzere standartlara uygun DOCX ve PDF dilekçe oluşturun."
      }
    >
      <div className="flex flex-col gap-4 max-w-4xl mx-auto">
        <Card className="p-5 flex flex-col gap-4">
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <Field label="Tarih" hint="Belgenin sağ üst köşesinde yer alacak tarih.">
              <TextInput value={data.date} onChange={(e) => update("date", e.target.value)} />
            </Field>

            <Field label="Muhatap Makam / Kurum" hint="Örn: ÇANKAYA KAYMAKAMLIĞINA">
              <TextInput
                value={data.recipientAuthority}
                onChange={(e) => update("recipientAuthority", e.target.value)}
                placeholder="Örn: ANKARA BÜYÜKŞEHİR BELEDİYE BAŞKANLIĞINA"
              />
            </Field>
          </div>

          <Field label="Dilekçe Konusu">
            <TextInput
              value={data.subject}
              onChange={(e) => update("subject", e.target.value)}
              placeholder="Örn: İkametgah Adres Düzeltme Talebi"
            />
          </Field>

          <Field label="Açıklamalar / Olay Anlatımı">
            <textarea
              className="w-full min-h-[140px] p-3 rounded-lg border border-hairline bg-surface text-text resize-y text-sm focus:outline-none focus:ring-1 focus:ring-accent"
              value={data.bodyText}
              onChange={(e) => update("bodyText", e.target.value)}
              placeholder="Dilekçenizin ana metnini buraya yazın..."
            />
          </Field>

          <Field label="Sonuç ve İstem">
            <TextInput
              value={data.demands}
              onChange={(e) => update("demands", e.target.value)}
              placeholder="Örn: Gereğini saygılarımla arz ederim."
            />
          </Field>

          <div className="border-t border-hairline pt-4 grid grid-cols-1 md:grid-cols-2 gap-4">
            <Field label="Ad Soyad">
              <TextInput value={data.fullName} onChange={(e) => update("fullName", e.target.value)} />
            </Field>

            <Field label="T.C. Kimlik No" hint="11 haneli TCKN (Mod-11 doğrulanır)">
              <TextInput
                value={data.tckn}
                onChange={(e) => update("tckn", e.target.value)}
                placeholder="11 haneli kimlik numarası"
              />
            </Field>

            <Field label="İletişim Adresi">
              <TextInput value={data.address} onChange={(e) => update("address", e.target.value)} />
            </Field>

            <Field label="Telefon Numarası">
              <TextInput value={data.phone} onChange={(e) => update("phone", e.target.value)} />
            </Field>
          </div>

          <Field label="Ekler (İsteğe bağlı)" hint="Varsa ek belgeleri listeleyin.">
            <TextInput
              value={data.attachments}
              onChange={(e) => update("attachments", e.target.value)}
              placeholder="Örn: 1. Kimlik Fotokopisi, 2. Fatura"
            />
          </Field>

          <div className="flex flex-wrap items-center justify-end gap-3 pt-4 border-t border-hairline">
            <Button variant="default" icon={<Edit3 className="w-4 h-4" />} disabled={busy} onClick={handleOpenInWriter}>
              Writer&apos;da Aç
            </Button>
            <Button
              variant="default"
              icon={<FileText className="w-4 h-4" />}
              disabled={busy}
              onClick={() => void handleExport("docx")}
            >
              DOCX Olarak Kaydet
            </Button>
            {udfEnabled ? (
              <Button
                variant="default"
                icon={<FileText className="w-4 h-4" />}
                disabled={busy}
                onClick={() => void handleExport("udf")}
              >
                UYAP UDF Olarak Kaydet
              </Button>
            ) : null}
            <Button
              variant="primary"
              icon={<Download className="w-4 h-4" />}
              disabled={busy}
              onClick={() => void handleExport("pdf")}
            >
              PDF Olarak Kaydet
            </Button>
          </div>
        </Card>
      </div>
    </Screen>
  );
}
