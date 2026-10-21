//! Writes the synthetic KVKK redaction fixture (samples/redaction-sample.pdf).
//!
//! The page carries a valid-checksum TCKN, a TR IBAN, an e-mail and a phone
//! number, so the Redact screen's detector and the lawyer persona E2E have a
//! deterministic document. No real personal data is involved.
//!
//! Usage: cargo run -p pdfcore --example make-redaction-sample -- <output-dir>

use lopdf::{dictionary, Dictionary, Document, Object, Stream};
use pdfcore::docutil::{add_resource_entry, pdf_text_object};
use std::path::PathBuf;

fn main() {
    let out_dir = std::env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("samples"));
    std::fs::create_dir_all(&out_dir).expect("create output dir");
    let mut doc = Document::new();
    doc.version = "1.7".to_string();
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Page",
        "MediaBox" => vec![
            Object::Real(0.0),
            Object::Real(0.0),
            Object::Real(595.28),
            Object::Real(841.89),
        ],
    }));
    let font_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    }));
    add_resource_entry(&mut doc, page_id, b"Font", "F1", Object::Reference(font_id)).unwrap();
    let content = "BT\n/F1 16 Tf\n72 740 Td\n(Ornek dilekce - KVKK testi) Tj\nET\n\
        BT\n/F1 12 Tf\n72 700 Td\n(T.C. Kimlik No: 10000000146) Tj\nET\n\
        BT\n/F1 12 Tf\n72 680 Td\n(IBAN: TR33 0006 1005 1978 6457 8413 26) Tj\nET\n\
        BT\n/F1 12 Tf\n72 660 Td\n(E-posta: avukat@example.com) Tj\nET\n\
        BT\n/F1 12 Tf\n72 640 Td\n(Telefon: 0532 123 45 67) Tj\nET\n";
    let content_id = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), content.as_bytes().to_vec())));
    doc.get_object_mut(page_id).unwrap().as_dict_mut().unwrap().set("Contents", Object::Reference(content_id));

    let pages_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Pages",
        "Kids" => vec![Object::Reference(page_id)],
        "Count" => 1,
    }));
    doc.get_object_mut(page_id).unwrap().as_dict_mut().unwrap().set("Parent", Object::Reference(pages_id));
    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    }));
    doc.trailer.set("Root", Object::Reference(catalog_id));
    let info_id = doc.add_object(Object::Dictionary(dictionary! {
        "Title" => pdf_text_object("Redaction sample"),
        "Author" => pdf_text_object("PDF Swiss Army Knife samples"),
        "Subject" => pdf_text_object("Synthetic test document"),
    }));
    doc.trailer.set("Info", Object::Reference(info_id));

    let path = out_dir.join("redaction-sample.pdf");
    doc.save(&path).expect("save sample");
    println!("wrote {}", path.display());
}
