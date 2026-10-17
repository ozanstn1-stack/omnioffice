/**
 * KVKK Compliance Templates and Document Wizard (v5.4.0 - Issue #254)
 *
 * Provides ready-to-use, legally vetted Turkish KVKK templates:
 * 1. Çalışan Aydınlatma Metni (Employee Privacy Notice)
 * 2. Müşteri / Kullanıcı Aydınlatma Metni (Customer Privacy Notice)
 * 3. Ziyaretçi / Kamera Kayıt Aydınlatma Metni (Visitor CCTV Notice)
 * 4. Açık Rıza Beyan Formu (Explicit Consent Form)
 */

export interface KvkkTemplateField {
  companyName: string;
  representativeName?: string;
  contactEmail: string;
  address?: string;
  dpoContact?: string; // Veri İrtibat Kişisi
  purposeSummary?: string;
}

export type KvkkTemplateType = "employee" | "customer" | "visitor" | "consent";

/**
 * Generates official KVKK disclosure text based on template type and metadata.
 */
export function generateKvkkText(type: KvkkTemplateType, fields: KvkkTemplateField): string {
  const comp = fields.companyName.trim() || "ŞİRKET / VERİ SORUMLUSU";
  const email = fields.contactEmail.trim() || "kvkk@sirket.com";
  const addr = fields.address?.trim() || "Şirket Merkez Adresi";

  switch (type) {
    case "employee":
      return `KİŞİSEL VERİLERİN İŞLENMESİNE İLİŞKİN ÇALIŞAN AYDINLATMA METNİ

1. Veri Sorumlusu
6698 sayılı Kişisel Verilerin Korunması Kanunu ("KVKK") uyarınca, kişisel verileriniz veri sorumlusu sıfatıyla ${comp} ("Şirket") tarafından işbu metinde belirtilen çerçevede işlenebilecektir.

2. Kişisel Verilerin İşlenme Amaçları
Kişisel verileriniz; iş sözleşmesinin ifası, özlük dosyasının oluşturulması, SGK ve mevzuat bildirimlerinin yapılması, iş sağlığı ve güvenliği yükümlülükleri amaçlarıyla işlenmektedir.

3. Kişisel Verilerin Aktarılması
Kişisel verileriniz, kanunen yetkili kamu kurum ve kuruluşları ile adli makamlara mevzuat sınırları dahilinde aktarılabilecektir.

4. İlgili Kişi Hakları (Madde 11)
KVKK'nın 11. maddesi kapsamındaki taleplerinizi ${email} adresine veya şirketimizin ${addr} adresine yazılı olarak iletebilirsiniz.`;

    case "customer":
      return `MÜŞTERİ VE ÇEVRİMİÇİ ZİYARETÇİ AYDINLATMA METNİ

1. Veri Sorumlusunun Kimliği
${comp} ("Şirket") olarak, müşterilerimizin ve hizmetlerimizden faydalanan kullanıcılarımızın kişisel verilerinin güvenliğine azami önem vermekteyiz.

2. İşlenen Kişisel Veriler ve İşleme Amaçları
Sipariş, fatura, teslimat ve müşteri destek süreçlerinin yürütülmesi amacıyla kimlik (ad, soyad), iletişim (telefon, e-posta, teslimat adresi) ve finansal verileriniz işlenmektedir.

3. Hukuki Sebepler
Verileriniz, KVKK'nın 5. maddesinde yer alan "sözleşmenin kurulması ve ifası" ve "veri sorumlusunun hukuki yükümlülüğü" sebeplerine dayalı olarak işlenmektedir.

4. Başvuru ve Bilgi Edinme
Taleplerinizi ${email} e-posta adresimiz üzerinden Şirketimize bildirebilirsiniz.`;

    case "visitor":
      return `ZİYARETÇİ VE FİZİKİ GÜVENLİK (KAMERA) AYDINLATMA METNİ

1. Kamera ile Görüntü Kaydı Yapılması
${comp} hizmet binası, girişleri ve ortak alanlarında bina güvenliğinin sağlanması amacıyla güvenlik kamerası ile görüntü kaydı yapılmaktadır.

2. Hukuki Sebepler ve Saklama Süresi
Fiziki mekan güvenliği verileri KVKK Madde 5/2-f uyarınca meşru menfaat kapsamında toplanır ve 30 gün boyunca güvenli şekilde saklanarak süre sonunda imha edilir.

3. Veri Güvenliği ve İletişim
Kayıtlar yalnızca yetkili güvenlik birimince incelenir. İletişim: ${email}`;

    case "consent":
      return `KİŞİSEL VERİLERİN İŞLENMESİNE İLİŞKİN AÇIK RIZA BEYANI

${comp} tarafından tarafıma sunulan Aydınlatma Metnini okudum, anladım.

Özel nitelikli kişisel verilerim dahil olmak üzere; yukarıda belirtilen amaçlar ve mevzuat çerçevesinde işlenmesine, güvenli arşivlenmesine ve gerektiğinde ilgili resmi mercilere aktarılmasına:

[ ] AÇIK RIZA VERİYORUM
[ ] AÇIK RIZA VERMİYORUM

Adı Soyadı : ........................................
Tarih      : ........................................
İmza       : ........................................`;
  }
}
