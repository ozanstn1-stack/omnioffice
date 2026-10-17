import { useState, useMemo } from "react";
import { Copy, Shield } from "lucide-react";
import { Button, Card, Field, Segmented, TextArea, TextInput } from "../components/ui";
import { OptionCard, Screen, TwoColumn } from "../components/layout";
import { generateKvkkText, type KvkkTemplateType } from "../lib/kvkk-templates";
import { useToasts } from "../lib/store";

export function KvkkWizard() {
  const { push } = useToasts();
  const [templateType, setTemplateType] = useState<KvkkTemplateType>("employee");
  const [companyName, setCompanyName] = useState("");
  const [contactEmail, setContactEmail] = useState("");
  const [address, setAddress] = useState("");

  const generatedText = useMemo(() => {
    return generateKvkkText(templateType, {
      companyName,
      contactEmail,
      address,
    });
  }, [templateType, companyName, contactEmail, address]);

  const handleCopy = () => {
    void navigator.clipboard?.writeText(generatedText);
    push({
      kind: "success",
      title: "Metin Panoya Kopyalandı",
      detail: "KVKK metni panoya aktarıldı, Word/Writer veya PDF çıktısı olarak kullanabilirsiniz.",
    });
  };

  return (
    <Screen
      title="KVKK Aydınlatma ve Açık Rıza Sihirbazı (v5.4)"
      subtitle="Kurumunuza özel mevzuata uygun çalışan, müşteri, ziyaretçi ve açık rıza metinlerini saniyeler içinde hazırlayın."
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

            <OptionCard title="Üretilen Resmî KVKK Metni Önizlemesi">
              <TextArea rows={14} value={generatedText} readOnly spellCheck={false} className="font-mono text-xs" />
              <div className="flex justify-end gap-2 mt-2">
                <Button variant="primary" icon={<Copy className="w-4 h-4" />} onClick={handleCopy}>
                  Metni Kopyala
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
                <span>6698 Sayılı Kanun Güvencesi</span>
              </div>
              <p className="text-xs muted leading-relaxed">
                Bu şablonlar T.C. Kişisel Verileri Koruma Kurumu (KVKK) tebliğlerine, Aydınlatma Yükümlülüğü Usul ve
                Esasları Tebliği'ne tam uyumlu hazırlanmıştır.
              </p>
            </Card>
          </div>
        }
      />
    </Screen>
  );
}
