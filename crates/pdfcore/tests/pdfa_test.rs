//! PDF/A validation and conversion tests.

mod common;

use common::*;
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdfcore::pdfa::{convert_pdfa, validate_pdfa, PdfaLevel, PdfaReport};
use pdfcore::progress::CancelToken;

fn check_status(report: &PdfaReport, id: &str) -> String {
    report
        .checks
        .iter()
        .find(|entry| entry.id == id)
        .map(|entry| entry.status.clone())
        .unwrap_or_else(|| panic!("missing check {id}"))
}

/// Adds a `FontFile2` program to the fixture's base font so the font check
/// can pass. The bytes are not parsed by the validator, which only checks that
/// the descriptor carries a font program.
fn embed_font_program(doc: &mut Document) {
    let font_id = doc
        .objects
        .iter()
        .find_map(|(id, object)| {
            let dict = object.as_dict().ok()?;
            dict.get(b"BaseFont").ok()?;
            Some(*id)
        })
        .expect("fixture font");
    let program = doc.add_object(Object::Stream(Stream::new(
        dictionary! { "Length1" => 64i64 },
        vec![0u8; 64],
    )));
    let descriptor = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => "Helvetica",
        "Flags" => 32i64,
        "FontFile2" => Object::Reference(program),
    }));
    doc.get_object_mut(font_id)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("FontDescriptor", Object::Reference(descriptor));
}

fn set_xmp(doc: &mut Document, level: PdfaLevel) {
    let packet = format!(
        "<?xpacket begin=\"\u{feff}\"?><x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\
         <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\
         <rdf:Description rdf:about=\"\" xmlns:pdfaid=\"http://www.aiim.org/pdfa/ns/id/\">\
         <pdfaid:part>{}</pdfaid:part><pdfaid:conformance>B</pdfaid:conformance>\
         </rdf:Description>\
         <rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\
         <dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">Sample</rdf:li></rdf:Alt></dc:title>\
         </rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end=\"w\"?>",
        level.part()
    );
    let id = doc.add_object(Object::Stream(Stream::new(
        dictionary! { "Type" => "Metadata", "Subtype" => "XML" },
        packet.into_bytes(),
    )));
    doc.catalog_mut().unwrap().set("Metadata", Object::Reference(id));
}

fn add_output_intent(doc: &mut Document) {
    let id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "OutputIntent",
        "S" => "GTS_PDFA1",
        "OutputConditionIdentifier" => Object::String(b"sRGB".to_vec(), StringFormat::Literal),
    }));
    doc.catalog_mut()
        .unwrap()
        .set("OutputIntents", Object::Array(vec![Object::Reference(id)]));
}

fn add_trailer_id(doc: &mut Document) {
    doc.trailer.set(
        "ID",
        vec![
            Object::String(b"0123456789abcdef".to_vec(), StringFormat::Literal),
            Object::String(b"0123456789abcdef".to_vec(), StringFormat::Literal),
        ],
    );
}

fn archival_doc(level: PdfaLevel) -> Document {
    let mut doc = build_text_doc(1, "Ready", "Archival sample");
    embed_font_program(&mut doc);
    set_xmp(&mut doc, level);
    add_output_intent(&mut doc);
    add_trailer_id(&mut doc);
    doc
}

#[test]
fn a_document_with_unembedded_fonts_fails_validation() {
    let dir = TestDir::new();
    let path = dir.path("plain.pdf");
    write_doc(&mut build_text_doc(1, "Plain", "Plain document"), &path);
    let report = validate_pdfa(&path, PdfaLevel::A2b).expect("validate");
    assert!(!report.valid, "a base14 font must not count as embedded");
    assert_eq!(check_status(&report, "pdfa.fonts-embedded"), "fail");
    assert_eq!(check_status(&report, "pdfa.xmp-pdfaid"), "fail");
    assert_eq!(check_status(&report, "pdfa.encryption"), "pass");
    assert!(report.failures >= 1);
    assert_eq!(report.level, "A-2b");
}

#[test]
fn conversion_writes_xmp_but_stays_honest_about_fonts() {
    let dir = TestDir::new();
    let source = dir.path("plain.pdf");
    write_doc(&mut build_text_doc(1, "Plain", "Plain document"), &source);
    let output = dir.path("converted.pdf");
    let report = convert_pdfa(
        &source,
        &output,
        PdfaLevel::A2b,
        &no_progress,
        &CancelToken::new(),
    )
    .expect("convert");

    assert!(report.converted, "the report must say it came from a conversion");
    assert!(
        !report.valid,
        "fonts are still unembedded, so the conversion must not claim validity"
    );
    assert_eq!(check_status(&report, "pdfa.fonts-embedded"), "fail");
    assert_eq!(check_status(&report, "pdfa.xmp-pdfaid"), "pass");
    assert_eq!(check_status(&report, "pdfa.encryption"), "pass");
    assert_eq!(check_status(&report, "pdfa.javascript"), "pass");

    let converted = Document::load(&output).expect("reopen converted file");
    let metadata_id = converted
        .catalog()
        .unwrap()
        .get(b"Metadata")
        .expect("metadata")
        .as_reference()
        .unwrap();
    let stream = converted.get_object(metadata_id).unwrap().as_stream().unwrap();
    let xmp = String::from_utf8_lossy(&stream.decompressed_content().unwrap()).to_string();
    assert!(xmp.contains("<pdfaid:part>2</pdfaid:part>"), "xmp was: {xmp}");
    assert!(xmp.contains("<pdfaid:conformance>B</pdfaid:conformance>"));
    assert!(xmp.contains("dc:title"));
    assert!(converted.catalog().unwrap().get(b"OutputIntents").is_ok());

    let metadata = pdfcore::metadata::read_metadata_from_file(&output).expect("metadata");
    assert_eq!(metadata.title, "Plain document", "the Info title must survive conversion");
}

#[test]
fn a_complete_document_passes_for_a2b() {
    let dir = TestDir::new();
    let path = dir.path("archival.pdf");
    write_doc(&mut archival_doc(PdfaLevel::A2b), &path);
    let report = validate_pdfa(&path, PdfaLevel::A2b).expect("validate");
    assert!(
        report.valid,
        "expected a pass, failures: {:?}",
        report
            .checks
            .iter()
            .filter(|entry| entry.status == "fail")
            .map(|entry| format!("{}: {}", entry.id, entry.message))
            .collect::<Vec<_>>()
    );
    assert_eq!(report.failures, 0);
    assert_eq!(check_status(&report, "pdfa.fonts-embedded"), "pass");
    assert_eq!(check_status(&report, "pdfa.output-intent"), "pass");
    assert_eq!(check_status(&report, "pdfa.xmp-pdfaid"), "pass");

    // The same file is not PDF/A-1: the XMP declares part 2, not 1.
    let wrong_level = validate_pdfa(&path, PdfaLevel::A1b).expect("validate");
    assert!(!wrong_level.valid);
    assert_eq!(check_status(&wrong_level, "pdfa.xmp-pdfaid"), "fail");
}

#[test]
fn unknown_levels_are_rejected() {
    assert!(PdfaLevel::parse("9z").is_err());
    assert!(PdfaLevel::parse("").is_err());
    assert_eq!(PdfaLevel::parse("A-3b").unwrap(), PdfaLevel::A3b);
    assert_eq!(PdfaLevel::parse("2b").unwrap().as_str(), "A-2b");
    assert_eq!(PdfaLevel::parse("PDF/A-1B").unwrap().part(), 1);
}
