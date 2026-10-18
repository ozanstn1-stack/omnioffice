//! Acceptance tests for the offline Turkish spell checker: valid inflections
//! pass, classic errors are caught (>= 90%), suggestions find the right word,
//! and a clean Turkish text stays under the 2% false-positive budget.

use turkish::{check_text, check_word, suggestions};

#[test]
fn valid_inflected_forms_are_accepted() {
    let valid = [
        "ev",
        "evler",
        "evlerimiz",
        "evlerimizde",
        "evlerimizdekiler",
        "kitap",
        "kitabı",
        "kitabım",
        "kitaplarımızdan",
        "kitaba",
        "ağaç",
        "ağacı",
        "ağaçlar",
        "çocuk",
        "çocuğu",
        "çocuklarımız",
        "okul",
        "okuldan",
        "okullardaki",
        "gitmek",
        "gidiyorum",
        "gittim",
        "gidiyor",
        "gidecek",
        "giderim",
        "gitmedi",
        "gitmeliyim",
        "okumak",
        "okuyorum",
        "okudum",
        "okumuş",
        "okuyacak",
        "yapmak",
        "yapıyor",
        "yaptı",
        "yapmış",
        "yapacağım",
        "yapabilir",
        "gelmek",
        "geliyor",
        "geldi",
        "gelmiş",
        "gelecek",
        "geleceğim",
        "araba",
        "arabayı",
        "arabam",
        "arabamı",
        "arabalar",
        "arabamız",
        "arabamızdaki",
        "oğul",
        "oğlu",
        "oğluma",
        "oğlumuz",
        "saat",
        "saati",
        "saatler",
        "kalp",
        "kalbi",
        "ağız",
        "ağzı",
        "ülke",
        "ülkesi",
        "ülkelerin",
        "ülkemizde",
        "devlet",
        "devletin",
        "devletler",
        "bilgisayar",
        "bilgisayarım",
        "bilgisayarlarda",
        "telefon",
        "telefonu",
        "telefonlar",
        "arkadaş",
        "arkadaşı",
        "arkadaşlarım",
        "öğrenci",
        "öğrenciler",
        "öğrencinin",
        "öğrencilerine",
        "öğretmen",
        "öğretmenim",
        "öğretmenler",
        "büyük",
        "büyükler",
        "büyüğü",
        "küçük",
        "küçüğü",
        "yemek",
        "yiyorum",
        "yedim",
        "demek",
        "diyorum",
        "etmek",
        "ediyorum",
        "ettim",
        "işleyen",
        "düzenlenen",
        "yayımlandı",
        "aldıkları",
        "etmelidir",
        "müşterilerine",
        "çalışanlar",
        "okunmalı",
        "katılanlar",
        "projenin",
        "geleceği",
        "görüşlerini",
        "paylaştı",
        "araştırmalar",
        "çalışmanın",
        "başarıyı",
        "artırdığını",
        "gösteriyor",
        "zamanında",
        "gönderilmez",
    ];
    let mut rejected = Vec::new();
    for word in valid {
        if !check_word(word) {
            rejected.push(word);
        }
    }
    assert!(rejected.is_empty(), "geçerli sayılmalıydı: {rejected:?}");
}

#[test]
fn broken_forms_are_rejected() {
    let broken = [
        "evlar",
        "kitapda",
        "yapdı",
        "evdan",
        "xyzabc",
        "asdfgh",
        "qwerty",
        "blabla",
        "kkkkkk",
        "telefonnnn",
        "öğretmenn",
        "kitapp",
        "güzell",
        "arkadaşş",
        "heryer",
    ];
    let mut accepted = Vec::new();
    for word in broken {
        if check_word(word) {
            accepted.push(word);
        }
    }
    assert!(accepted.is_empty(), "yanlış sayılmalıydı: {accepted:?}");
}

#[test]
fn common_misspellings_are_caught() {
    let cases: &[(&str, &str)] = &[
        ("herşey", "her şey"),
        ("birşey", "bir şey"),
        ("hiçbirşey", "hiçbir şey"),
        ("herkez", "herkes"),
        ("yanlız", "yalnız"),
        ("yanlızca", "yalnızca"),
        ("yalnış", "yanlış"),
        ("klavuz", "kılavuz"),
        ("şöför", "şoför"),
        ("şarz", "şarj"),
        ("orjinal", "orijinal"),
        ("labaratuvar", "laboratuvar"),
        ("üniverste", "üniversite"),
        ("proğram", "program"),
        ("acenta", "acente"),
        ("ambülans", "ambulans"),
        ("egzos", "egzoz"),
        ("ayrıyeten", "ayrıca"),
        ("faliyet", "faaliyet"),
        ("malesef", "maalesef"),
        ("ilkönce", "ilk önce"),
        ("iyiki", "iyi ki"),
        ("yada", "ya da"),
        ("makina", "makine"),
        ("döküman", "doküman"),
        ("eylence", "eğlence"),
        ("itibariyle", "itibarıyla"),
        ("telefonnn", "telefon"),
        ("güzelll", "güzel"),
        ("bilgisayarr", "bilgisayar"),
    ];
    let caught = cases.iter().filter(|(wrong, _)| !check_word(wrong)).count();
    let ratio = caught as f64 / cases.len() as f64;
    assert!(ratio >= 0.90, "yakalanan {caught}/{} = {ratio:.2}", cases.len());
}

#[test]
fn suggestions_include_the_correct_form() {
    let cases: &[(&str, &str)] = &[
        ("yanlız", "yalnız"),
        ("yalnış", "yanlış"),
        ("klavuz", "kılavuz"),
        ("şöför", "şoför"),
        ("orjinal", "orijinal"),
        ("labaratuvar", "laboratuvar"),
        ("üniverste", "üniversite"),
        ("proğram", "program"),
        ("acenta", "acente"),
        ("egzos", "egzoz"),
        ("malesef", "maalesef"),
        ("herkez", "herkes"),
        ("makina", "makine"),
        ("döküman", "doküman"),
        ("eylence", "eğlence"),
        ("itibariyle", "itibarıyla"),
        ("telefonnn", "telefon"),
        ("güzelll", "güzel"),
        ("bilgisayarr", "bilgisayar"),
        ("öğretmenn", "öğretmen"),
    ];
    for (wrong, right) in cases {
        let list = suggestions(wrong, 5);
        assert!(list.contains(&right.to_string()), "{wrong} önerileri: {list:?}");
    }
}

#[test]
fn clean_text_has_few_false_positives() {
    let text = "OmniOffice belgelerinizi yerel olarak işleyen bir ofis uygulamasıdır. \
        Uygulama PDF DOCX XLSX PPTX ve ODF dosyalarını açabilir düzenleyebilir ve dönüştürebilir. \
        Kullanıcılar dosyalarını güvenle saklayabilir çünkü hiçbir veri sunucuya gönderilmez. \
        Ankara'da düzenlenen toplantıda 2026 yılı hedefleri konuşuldu. \
        TBMM tarafından kabul edilen kanun resmî gazetede yayımlandı. \
        Öğrenciler öğretmenlerinden aldıkları ödevleri zamanında teslim etmelidir. \
        Şirketimiz müşterilerine daha iyi hizmet vermek için yeni yazılımlar geliştiriyor. \
        Bu belge tüm çalışanlar tarafından okunmalı ve imzalanmalıdır. \
        Toplantıya katılanlar projenin geleceği hakkında görüşlerini paylaştı. \
        Yapılan araştırmalar düzenli çalışmanın başarıyı artırdığını gösteriyor.";
    let misspellings = check_text(text);
    let tokens = text.split_whitespace().count();
    let ratio = misspellings.len() as f64 / tokens as f64;
    assert!(ratio < 0.02, "yanlış pozitif {}/{} = {ratio:.3}: {:?}", misspellings.len(), tokens, misspellings);
}

#[test]
fn utf16_offsets_align_after_multibyte_turkish_characters() {
    let text = "Çok güzel yanlız bir gün.";
    let misspellings = check_text(text);
    assert_eq!(misspellings.len(), 1);
    assert_eq!(misspellings[0].word, "yanlız");
    // "Çok güzel " is 10 UTF-16 code units (even though Ç and ü are 2 UTF-8 bytes each)
    assert_eq!(misspellings[0].start, 10);
    assert_eq!(misspellings[0].end, 16);
}
