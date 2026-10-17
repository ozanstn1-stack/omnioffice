//! Generates the license-clean compatibility corpus under `tests/corpus/`.
//!
//! Every file is written by this repository's own writers from synthetic
//! Turkish content: no third-party material, no personal data. The
//! `manifest.json` it writes is the input of
//! `crates/officecore/tests/corpus_test.rs`, which imports every sample through
//! the reader the manifest names.
//!
//!   cargo run -p officecore --example make-corpus
//!
//! The corpus proves the **importers** read what our writers produce; real
//! third-party files (UYAP, a bank's e-Fatura, Word/Excel output) are still
//! required for external compatibility and are documented in `docs/corpus.md`.

use officecore::model::*;
use officecore::{docx, odf, pptx, udf, xlsx};
use std::path::PathBuf;

fn sample_png() -> Vec<u8> {
    let mut image = image::RgbaImage::new(160, 90);
    for (x, _y, pixel) in image.enumerate_pixels_mut() {
        let blue = (x as f32 / 160.0 * 255.0) as u8;
        *pixel = image::Rgba([37, 99, blue, 255]);
    }
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image).write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn cell(text: &str) -> Cell {
    Cell { value: CellValue::Text(text.to_string()), ..Default::default() }
}

fn number(value: f64, format: &str) -> Cell {
    Cell {
        value: CellValue::Number(value),
        style: CellStyle { number_format: format.to_string(), ..Default::default() },
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Writer variants
// ---------------------------------------------------------------------------

fn writer_basic() -> TextDocument {
    let mut document = TextDocument::new_blank("Korpus: temel belge");
    document.blocks = vec![
        Block::heading("Temel belge", 1),
        Block::paragraph("Bu belge OmniOffice'in kendi yazıcısıyla üretilmiştir; içerik tamamen kurgusaldır."),
        Block::paragraph("İkinci paragraf Türkçe karakterleri sınar: ğ, ü, ş, i, ö, ç, ı, İ."),
    ];
    document
}

fn writer_rich() -> TextDocument {
    let mut document = TextDocument::new_blank("Korpus: tablo ve görsel");
    let mut table = TableData::simple(4, 3, 460.0);
    for (index, header) in ["Ürün", "Adet", "Fiyat"].iter().enumerate() {
        table.rows[0].cells[index].blocks = vec![Block::paragraph(header)];
    }
    for (index, (name, quantity, price)) in
        [("Kalem", "10", "5,50"), ("Defter", "3", "12,00"), ("Ataç", "25", "0,40")].iter().enumerate()
    {
        let row = index + 1;
        table.rows[row].cells[0].blocks = vec![Block::paragraph(name)];
        table.rows[row].cells[1].blocks = vec![Block::paragraph(quantity)];
        table.rows[row].cells[2].blocks = vec![Block::paragraph(price)];
    }
    document.blocks = vec![
        Block::heading("Tablo ve görsel", 1),
        Block::paragraph("Aşağıda bir tablo ve üretilmiş bir görsel bulunur."),
        Block::heading("Tablo", 2),
        Block::Table { table },
        Block::heading("Görsel", 2),
        Block::Image {
            image: ImageData::from_bytes("korpus.png", &sample_png()),
            width_pt: 240.0,
            height_pt: 135.0,
            align: "center".into(),
            caption: "Üretilmiş örnek görsel".into(),
            wrap: "inline".into(),
        },
    ];
    document
}

fn writer_notes() -> TextDocument {
    let mut document = TextDocument::new_blank("Korpus: liste ve dipnot");
    let note = Footnote {
        id: "fn1".into(),
        runs: vec![Run { text: "Dipnot metni: kurgusal bir açıklama.".into(), ..Default::default() }],
        ..Default::default()
    };
    document.footnotes = vec![note];
    document.blocks = vec![
        Block::heading("Liste ve dipnot", 1),
        Block::Paragraph {
            props: ParaProps {
                list: Some(ListInfo { kind: "bullet".into(), level: 0, start: 1, marker: "•".into() }),
                ..Default::default()
            },
            runs: vec![Run { text: "Birinci madde".into(), ..Default::default() }],
        },
        Block::Paragraph {
            props: ParaProps {
                list: Some(ListInfo { kind: "number".into(), level: 0, start: 1, marker: "1.".into() }),
                ..Default::default()
            },
            runs: vec![
                Run { text: "Numaralı madde".into(), ..Default::default() },
                Run { text: "1".into(), footnote: Some("fn1".into()), ..Default::default() },
            ],
        },
        Block::paragraph("Listeden sonra gelen kapanış paragrafı."),
    ];
    document
}

// ---------------------------------------------------------------------------
// Calc variants
// ---------------------------------------------------------------------------

fn calc_basic() -> Workbook {
    let mut workbook = Workbook::new_blank("Korpus: temel çizelge");
    workbook.sheets.clear();
    let mut sheet = Sheet::new("Sayfa1");
    for (index, header) in ["Ay", "Gelir", "Gider"].iter().enumerate() {
        let address = officecore::address::format(0, index as u32);
        let mut header_cell = cell(header);
        header_cell.style = CellStyle { bold: true, ..Default::default() };
        sheet.set(&address, header_cell);
    }
    for (index, (month, income, expense)) in
        [("Ocak", 1200.0, 800.0), ("Şubat", 1350.0, 910.0), ("Mart", 1280.0, 860.0)].iter().enumerate()
    {
        let row = index as u32 + 1;
        sheet.set(&officecore::address::format(row, 0), cell(month));
        sheet.set(&officecore::address::format(row, 1), number(*income, "#,##0.00"));
        sheet.set(&officecore::address::format(row, 2), number(*expense, "#,##0.00"));
    }
    workbook.sheets.push(sheet);
    workbook
}

fn calc_formulas() -> Workbook {
    let mut workbook = Workbook::new_blank("Korpus: formüller");
    workbook.sheets.clear();
    let mut sheet = Sheet::new("Hesap");
    for (index, header) in ["Kalem", "Adet", "Birim", "Tutar"].iter().enumerate() {
        let address = officecore::address::format(0, index as u32);
        let mut header_cell = cell(header);
        header_cell.style = CellStyle { bold: true, fill: Some("#EEF2FF".into()), ..Default::default() };
        sheet.set(&address, header_cell);
    }
    for row in 1..=5u32 {
        sheet.set(&officecore::address::format(row, 0), cell(&format!("Kalem {row}")));
        sheet.set(&officecore::address::format(row, 1), number(row as f64 * 2.0, "0"));
        sheet.set(&officecore::address::format(row, 2), number(row as f64 + 0.5, "#,##0.00"));
        sheet.set(
            &officecore::address::format(row, 3),
            Cell {
                value: CellValue::Number(0.0),
                formula: Some(format!("=B{}*C{}", row + 1, row + 1)),
                style: CellStyle { number_format: "#,##0.00".into(), ..Default::default() },
                ..Default::default()
            },
        );
    }
    sheet.set("F1", cell("Toplam"));
    sheet.set(
        "G1",
        Cell {
            value: CellValue::Number(0.0),
            formula: Some("=SUM(D2:D6)".into()),
            style: CellStyle { number_format: "#,##0.00".into(), ..Default::default() },
            ..Default::default()
        },
    );
    sheet.set("F2", cell("Ortalama"));
    sheet.set(
        "G2",
        Cell {
            value: CellValue::Number(0.0),
            formula: Some("=AVERAGE(D2:D6)".into()),
            style: CellStyle { number_format: "#,##0.00".into(), ..Default::default() },
            ..Default::default()
        },
    );
    sheet.set("F3", cell("Durum"));
    sheet.set(
        "G3",
        Cell {
            value: CellValue::Text(String::new()),
            formula: Some("=IF(G1>50,\"Yüksek\",\"Düşük\")".into()),
            ..Default::default()
        },
    );
    sheet.freeze_rows = 1;
    workbook.sheets.push(sheet);
    workbook
}

fn calc_multisheet() -> Workbook {
    let mut workbook = Workbook::new_blank("Korpus: çok sayfa");
    workbook.sheets.clear();
    let mut data = Sheet::new("Veri");
    data.set("A1", cell("Değer"));
    for row in 1..=4u32 {
        data.set(&officecore::address::format(row, 0), number(row as f64 * 10.0, "#,##0.00"));
    }
    let mut summary = Sheet::new("Özet");
    summary.set("A1", cell("Veri toplamı"));
    summary.set(
        "B1",
        Cell {
            value: CellValue::Number(0.0),
            formula: Some("=SUM(Veri!A2:A5)".into()),
            style: CellStyle { number_format: "#,##0.00".into(), ..Default::default() },
            ..Default::default()
        },
    );
    summary.set("A2", cell("Sayfa sayısı"));
    summary.set("B2", number(2.0, "0"));
    workbook.sheets.push(data);
    workbook.sheets.push(summary);
    workbook
}

// ---------------------------------------------------------------------------
// Impress variants
// ---------------------------------------------------------------------------

fn text_object(x: f64, y: f64, w: f64, h: f64, text: &str, size_pt: f64, bullet: bool) -> SlideObject {
    let mut object = SlideObject::new("text", x, y, w, h);
    object.text = Some(TextFrame {
        paragraphs: vec![TextParagraph {
            text: text.to_string(),
            size_pt: Some(size_pt),
            bullet,
            ..Default::default()
        }],
        ..Default::default()
    });
    object
}

fn deck_basic() -> Deck {
    let mut deck = Deck::new_blank("Korpus: temel sunum");
    deck.slides = vec![
        Slide {
            objects: vec![text_object(60.0, 250.0, 840.0, 120.0, "Temel sunum", 40.0, false)],
            ..Default::default()
        },
        Slide {
            objects: vec![
                text_object(60.0, 50.0, 840.0, 90.0, "Gündem", 32.0, false),
                text_object(70.0, 170.0, 700.0, 200.0, "Birinci madde", 20.0, true),
                text_object(70.0, 230.0, 700.0, 200.0, "İkinci madde", 20.0, true),
            ],
            ..Default::default()
        },
    ];
    deck
}

fn deck_media() -> Deck {
    let mut deck = Deck::new_blank("Korpus: görselli sunum");
    let mut picture = SlideObject::new("image", 620.0, 170.0, 260.0, 150.0);
    picture.image = Some(ImageData::from_bytes("korpus.png", &sample_png()));
    let mut shape = SlideObject::new("rect", 620.0, 340.0, 260.0, 80.0);
    shape.style = Some(ShapeStyle { fill: Some("#2563EB".into()), corner_radius_pt: 8.0, ..Default::default() });
    deck.slides = vec![Slide {
        objects: vec![text_object(60.0, 50.0, 840.0, 90.0, "Görsel ve şekil", 32.0, false), picture, shape],
        notes: "Konuşmacı notu: kurgusal.".into(),
        ..Default::default()
    }];
    deck
}

fn deck_table() -> Deck {
    let mut deck = Deck::new_blank("Korpus: tablolu sunum");
    let mut table_object = SlideObject::new("table", 70.0, 170.0, 700.0, 220.0);
    table_object.table = Some(TableData::simple(3, 3, 700.0));
    deck.slides = vec![Slide {
        objects: vec![text_object(60.0, 50.0, 840.0, 90.0, "Tablo", 32.0, false), table_object],
        ..Default::default()
    }];
    deck
}

// ---------------------------------------------------------------------------
// UDF variants (the UYAP writer's own output; external compatibility is a
// separate question - see docs/corpus.md)
// ---------------------------------------------------------------------------

fn udf_basic() -> TextDocument {
    let mut document = TextDocument::new_blank("Korpus: temel UDF");
    document.blocks =
        vec![Block::heading("Temel UDF", 1), Block::paragraph("UYAP biçiminde üretilmiş kurgusal bir metin.")];
    document
}

fn udf_table() -> TextDocument {
    let mut document = TextDocument::new_blank("Korpus: tablolu UDF");
    document.blocks = vec![Block::heading("Tablolu UDF", 1), Block::Table { table: TableData::simple(3, 2, 420.0) }];
    document
}

fn udf_long() -> TextDocument {
    let mut document = TextDocument::new_blank("Korpus: uzun UDF");
    let mut blocks = vec![Block::heading("Uzun metin", 1)];
    for index in 1..=12 {
        blocks.push(Block::paragraph(&format!(
            "Paragraf {index}: Türkçe karakterler (ğüşiöçıİ) ve rakamlar 0123456789 içeren kurgusal bir cümle."
        )));
    }
    document.blocks = blocks;
    document
}

// ---------------------------------------------------------------------------

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("tests").join("corpus");
    std::fs::create_dir_all(&root)?;

    let mut manifest: Vec<serde_json::Value> = Vec::new();
    let mut write = |file: &str,
                     format: &str,
                     kind: &str,
                     reader: &str,
                     notes: &str,
                     bytes: Vec<u8>|
     -> Result<(), Box<dyn std::error::Error>> {
        std::fs::write(root.join(file), &bytes)?;
        manifest.push(serde_json::json!({
            "file": file,
            "format": format,
            "kind": kind,
            "reader": reader,
            "origin": "generated by crates/officecore/examples/make-corpus.rs",
            "license": "MIT (this repository)",
            "content": "synthetic Turkish, no personal data",
            "notes": notes,
        }));
        Ok(())
    };

    for (file, document, notes) in [
        ("writer-basic.docx", writer_basic(), "headings and paragraphs"),
        ("writer-rich.docx", writer_rich(), "table and image"),
        ("writer-notes.docx", writer_notes(), "bullet list and footnote"),
    ] {
        write(file, "docx", "writer", "docx::read_docx", notes, docx::write_docx(&document)?)?;
    }
    for (file, document, notes) in [
        ("writer-basic.odt", writer_basic(), "headings and paragraphs"),
        ("writer-rich.odt", writer_rich(), "table and image"),
        ("writer-notes.odt", writer_notes(), "bullet list and footnote"),
    ] {
        write(file, "odt", "writer", "odf::read_odt", notes, odf::write_odt(&document)?)?;
    }
    for (file, workbook, notes) in [
        ("calc-basic.xlsx", calc_basic(), "header row and numbers"),
        ("calc-formulas.xlsx", calc_formulas(), "SUM/AVERAGE/IF and number formats"),
        ("calc-multisheet.xlsx", calc_multisheet(), "two sheets and a cross-sheet formula"),
    ] {
        write(file, "xlsx", "calc", "xlsx::read_workbook_bytes", notes, xlsx::write_xlsx(&workbook)?)?;
    }
    for (file, deck, notes) in [
        ("deck-basic.pptx", deck_basic(), "title and bullet slides"),
        ("deck-media.pptx", deck_media(), "image, shape and speaker notes"),
        ("deck-table.pptx", deck_table(), "table object"),
    ] {
        write(file, "pptx", "impress", "pptx::read_pptx", notes, pptx::write_pptx(&deck)?)?;
    }
    for (file, document, notes) in [
        ("uyap-basic.udf", udf_basic(), "heading and paragraph"),
        ("uyap-table.udf", udf_table(), "table"),
        ("uyap-long.udf", udf_long(), "twelve paragraphs"),
    ] {
        write(file, "udf", "writer", "udf::read_udf", notes, udf::write_udf(&document)?)?;
    }

    let manifest_json = serde_json::to_string_pretty(&manifest)?;
    std::fs::write(root.join("manifest.json"), format!("{manifest_json}\n"))?;
    println!("corpus written to {} ({} samples)", root.display(), manifest.len());
    Ok(())
}
