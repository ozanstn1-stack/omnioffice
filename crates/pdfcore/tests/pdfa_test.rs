//! PDF/A validation and conversion tests.

mod common;

use common::*;
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream, StringFormat};
use pdfcore::fontembed::embed_missing_fonts;
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

fn check_message(report: &PdfaReport, id: &str) -> String {
    report
        .checks
        .iter()
        .find(|entry| entry.id == id)
        .map(|entry| entry.message.clone())
        .unwrap_or_else(|| panic!("missing check {id}"))
}

/// Adds a `FontFile2` program to the fixture's base font so the font check
/// can pass. The bytes are not parsed by the validator, which only checks that
/// the descriptor carries a font program.
fn embed_font_program(doc: &mut Document) {
    let font_id = find_font_id(doc);
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

/// The fixture's only font object (the synthetic documents give each page one
/// `/F1` font).
fn find_font_id(doc: &Document) -> ObjectId {
    doc.objects
        .iter()
        .find_map(|(id, object)| {
            let dict = object.as_dict().ok()?;
            dict.get(b"BaseFont").ok()?;
            Some(*id)
        })
        .expect("fixture font")
}

fn find_font_by_base(doc: &Document, base_prefix: &str) -> (ObjectId, Dictionary) {
    doc.objects
        .iter()
        .find_map(|(id, object)| {
            let dict = object.as_dict().ok()?;
            let base = dict.get(b"BaseFont").ok()?.as_name().ok()?;
            if base.starts_with(base_prefix.as_bytes()) {
                Some((*id, dict.clone()))
            } else {
                None
            }
        })
        .unwrap_or_else(|| panic!("font with base {base_prefix}"))
}

fn font_descriptor(doc: &Document, font: &Dictionary) -> Dictionary {
    let id = font
        .get(b"FontDescriptor")
        .expect("font descriptor")
        .as_reference()
        .expect("indirect descriptor");
    doc.get_dictionary(id).expect("descriptor dictionary").clone()
}

fn widths_of(font: &Dictionary) -> Vec<i64> {
    font.get(b"Widths")
        .expect("widths")
        .as_array()
        .expect("widths array")
        .iter()
        .map(|value| value.as_i64().expect("integer width"))
        .collect()
}

fn set_explicit_widths(doc: &mut Document, widths: &[i64]) {
    let font_id = find_font_id(doc);
    let font = doc.get_dictionary_mut(font_id).expect("font dict");
    font.set("FirstChar", 32i64);
    font.set("LastChar", (32 + widths.len() - 1) as i64);
    font.set(
        "Widths",
        widths.iter().map(|value| Object::Integer(*value)).collect::<Vec<_>>(),
    );
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

/// A complete output intent, including the `/DestOutputProfile` ICC stream the
/// upgraded validator requires.
fn add_output_intent(doc: &mut Document) {
    let profile = doc.add_object(Object::Stream(Stream::new(
        dictionary! { "N" => 3i64 },
        pdfcore::pdfa::srgb_v4_icc_profile(),
    )));
    let id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "OutputIntent",
        "S" => "GTS_PDFA1",
        "OutputConditionIdentifier" => Object::String(b"sRGB".to_vec(), StringFormat::Literal),
        "DestOutputProfile" => Object::Reference(profile),
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

// ---------------------------------------------------------------------------
// ICC structural walker (test-side, independent of the generator)
// ---------------------------------------------------------------------------

fn be32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes([bytes[offset], bytes[offset + 1], bytes[offset + 2], bytes[offset + 3]])
}

struct IccTag {
    signature: [u8; 4],
    type_signature: [u8; 4],
    offset: usize,
    size: usize,
}

/// Walks the profile exactly like a strict reader would: header size field,
/// `acsp` signature, tag table bounds and 4-byte alignment, type signatures,
/// and per-type payload sizes. Panics with a precise message on any violation.
fn walk_icc(profile: &[u8]) -> Vec<IccTag> {
    assert!(profile.len() >= 132, "profile too small: {} bytes", profile.len());
    assert_eq!(be32(profile, 0) as usize, profile.len(), "header size field must match the real size");
    assert_eq!(&profile[12..16], b"mntr", "device class must be a display profile");
    assert_eq!(&profile[16..20], b"RGB ", "data colour space must be RGB");
    assert_eq!(&profile[20..24], b"XYZ ", "PCS must be XYZ");
    assert_eq!(be32(profile, 8) >> 24, 4, "ICC major version must be 4");
    assert_eq!(&profile[36..40], b"acsp", "profile signature");

    let count = be32(profile, 128) as usize;
    assert!(count >= 9, "expected at least the 9 required tags, found {count}");
    let table_end = 132 + count * 12;
    let mut tags: Vec<IccTag> = Vec::with_capacity(count);
    for index in 0..count {
        let entry = 132 + index * 12;
        let mut signature = [0u8; 4];
        signature.copy_from_slice(&profile[entry..entry + 4]);
        let offset = be32(profile, entry + 4) as usize;
        let size = be32(profile, entry + 8) as usize;
        // The ICC spec requires tag data to start on a 4-byte boundary and to
        // fit inside the profile, after the tag table.
        assert_eq!(offset % 4, 0, "tag {:?} data is not 4-byte aligned", signature);
        assert!(offset >= table_end, "tag {:?} overlaps the tag table", signature);
        assert!(offset + size <= profile.len(), "tag {:?} runs past the profile end", signature);
        let mut type_signature = [0u8; 4];
        type_signature.copy_from_slice(&profile[offset..offset + 4]);
        tags.push(IccTag { signature, type_signature, offset, size });
    }

    // The tag table is required to be sorted by signature.
    let mut sorted: Vec<[u8; 4]> = tags.iter().map(|tag| tag.signature).collect();
    let original = sorted.clone();
    sorted.sort();
    assert_eq!(sorted, original, "tag table must be sorted by signature");

    for tag in &tags {
        match &tag.signature {
            b"rXYZ" | b"gXYZ" | b"bXYZ" | b"wtpt" => {
                assert_eq!(&tag.type_signature, b"XYZ ", "colourant tags are XYZType");
                assert_eq!(tag.size, 20, "an XYZType with one value is 20 bytes");
            }
            b"rTRC" | b"gTRC" | b"bTRC" => {
                assert_eq!(&tag.type_signature, b"para", "the sRGB curve is parametric");
                assert_eq!(tag.size, 40, "a 7-parameter parametric curve is 40 bytes");
                let function_type = u16::from_be_bytes([profile[tag.offset + 8], profile[tag.offset + 9]]);
                assert_eq!(function_type, 4, "the sRGB curve is parametric type 4");
            }
            b"desc" | b"cprt" => {
                assert_eq!(&tag.type_signature, b"mluc", "ICC v4 text uses multiLocalizedUnicode");
            }
            other => panic!("unexpected ICC tag {other:?}"),
        }
    }
    tags
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

#[test]
fn pdfa_unembedded_fonts_fail_validation() {
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
    assert!(report.font_embedding.is_empty(), "validation alone embeds nothing");
}

// ---------------------------------------------------------------------------
// Conversion: font embedding
// ---------------------------------------------------------------------------

#[test]
fn pdfa_conversion_embeds_missing_fonts_and_writes_xmp() {
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
        report.valid,
        "all checks should pass after embedding; failures: {:?}",
        report
            .checks
            .iter()
            .filter(|entry| entry.status == "fail")
            .map(|entry| format!("{}: {}", entry.id, entry.message))
            .collect::<Vec<_>>()
    );
    assert_eq!(check_status(&report, "pdfa.fonts-embedded"), "pass");
    assert_eq!(check_status(&report, "pdfa.output-intent"), "pass");
    assert_eq!(check_status(&report, "pdfa.xmp-pdfaid"), "pass");
    assert_eq!(check_status(&report, "pdfa.javascript"), "pass");

    // The report must name what was embedded and how compatible it is.
    let entry = report
        .font_embedding
        .iter()
        .find(|entry| entry.base_font == "Helvetica")
        .expect("the Helvetica font must be reported");
    assert!(entry.embedded, "the base14 font must now be embedded: {entry:?}");
    assert_eq!(entry.substitute.as_deref(), Some("LiberationSans"));
    assert_eq!(entry.metric_compatible, Some(true));
    assert!(entry.skipped_reason.is_none());
    assert!(entry.warning.is_none(), "the metric-compatible family needs no warning");

    // The written file must really carry the program.
    let converted = Document::load(&output).expect("reopen converted file");
    let (_, font) = find_font_by_base(&converted, "LiberationSans");
    assert_eq!(
        font.get(b"Subtype").unwrap().as_name().unwrap(),
        b"TrueType",
        "a TrueType program belongs to a /TrueType font dictionary"
    );
    assert_eq!(font.get(b"Encoding").unwrap().as_name().unwrap(), b"WinAnsiEncoding");
    let descriptor = font_descriptor(&converted, &font);
    let program_id = descriptor
        .get(b"FontFile2")
        .expect("FontFile2")
        .as_reference()
        .expect("indirect program");
    let stream = converted.get_object(program_id).unwrap().as_stream().unwrap();
    let bytes = stream.decompressed_content().unwrap();
    assert!(bytes.len() > 100_000, "the full Liberation face was embedded ({} bytes)", bytes.len());
    assert_eq!(
        stream.dict.get(b"Length1").unwrap().as_i64().unwrap(),
        bytes.len() as i64,
        "/Length1 is the program length"
    );

    // XMP and metadata survive, as they did before font embedding landed.
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

    // Re-validating the written file must not report the font failure again.
    let revalidated = validate_pdfa(&output, PdfaLevel::A2b).expect("revalidate");
    assert_eq!(check_status(&revalidated, "pdfa.fonts-embedded"), "pass");
    assert!(!revalidated
        .checks
        .iter()
        .any(|entry| entry.status == "fail" && entry.message.to_lowercase().contains("font")));
}

#[test]
fn pdfa_metric_compatible_substitution_keeps_the_widths_array() {
    let mut doc = build_text_doc(1, "Metric", "Metric sample");
    let widths: Vec<i64> = (0..95).map(|index| 200 + index * 3).collect();
    set_explicit_widths(&mut doc, &widths);
    let mut source = Vec::new();
    doc.save_to(&mut source).expect("save source bytes");

    let (rewritten, reports) = embed_missing_fonts(&source);
    let entry = reports.first().expect("one font report");
    assert!(entry.embedded, "the font must be embedded: {entry:?}");
    assert_eq!(entry.substitute.as_deref(), Some("LiberationSans"));
    assert_eq!(entry.metric_compatible, Some(true));

    let converted = Document::load_mem(&rewritten).expect("load rewritten bytes");
    let (_, font) = find_font_by_base(&converted, "LiberationSans");
    assert_eq!(font.get(b"FirstChar").unwrap().as_i64().unwrap(), 32, "FirstChar was not touched");
    assert_eq!(font.get(b"LastChar").unwrap().as_i64().unwrap(), 126, "LastChar was not touched");
    assert_eq!(widths_of(&font), widths, "the original /Widths must be kept byte for byte");
    let descriptor = font_descriptor(&converted, &font);
    assert!(descriptor.get(b"FontFile2").is_ok(), "the descriptor must carry the program");
    assert!(descriptor.get(b"FontBBox").is_ok(), "the descriptor must carry real metrics");
    assert!(descriptor.get(b"Ascent").is_ok());
    assert!(descriptor.get(b"CapHeight").is_ok());
}

#[test]
fn pdfa_times_falls_back_to_pt_sans_with_a_warning() {
    let mut doc = build_text_doc(1, "Times", "Times sample");
    {
        let font_id = find_font_id(&doc);
        doc.get_dictionary_mut(font_id)
            .unwrap()
            .set("BaseFont", "Times-Roman");
    }
    let mut source = Vec::new();
    doc.save_to(&mut source).expect("save source bytes");
    let (_rewritten, reports) = embed_missing_fonts(&source);
    let entry = reports.first().expect("one font report");
    assert!(entry.embedded, "the Times substitute must be embedded: {entry:?}");
    assert_eq!(entry.substitute.as_deref(), Some("PTSans-Regular"));
    assert_eq!(entry.metric_compatible, Some(false), "PT Sans is not metric-compatible");
    let warning = entry.warning.as_deref().unwrap_or_default();
    assert!(
        warning.contains("not metric-compatible"),
        "the report must warn about the metric mismatch: {warning}"
    );
}

#[test]
fn pdfa_malformed_bytes_are_reported_not_panicked_on() {
    let garbage = b"this is not a PDF at all".to_vec();
    let (output, reports) = embed_missing_fonts(&garbage);
    assert_eq!(output, garbage, "malformed input must come back unchanged");
    assert_eq!(reports.len(), 1);
    assert!(reports[0].skipped_reason.is_some(), "the parse failure must be reported");
}

#[test]
fn pdfa_type0_font_is_skipped_and_validation_stays_honest() {
    let dir = TestDir::new();
    let mut doc = build_text_doc(1, "CID", "CID sample");
    {
        let font_id = find_font_id(&doc);
        let font = doc.get_dictionary_mut(font_id).unwrap();
        font.set("Subtype", "Type0");
        font.set("BaseFont", "NotoSansCJK");
        font.set("Encoding", "Identity-H");
    }
    let source = dir.path("cid.pdf");
    write_doc(&mut doc, &source);
    let output = dir.path("cid-converted.pdf");
    let report = convert_pdfa(
        &source,
        &output,
        PdfaLevel::A2b,
        &no_progress,
        &CancelToken::new(),
    )
    .expect("convert");

    assert!(!report.valid, "an unembeddable CID font must keep the conversion honest");
    assert_eq!(check_status(&report, "pdfa.fonts-embedded"), "fail");
    let entry = report
        .font_embedding
        .iter()
        .find(|entry| entry.base_font == "NotoSansCJK")
        .expect("the CID font must be reported");
    assert!(!entry.embedded);
    let reason = entry.skipped_reason.as_deref().unwrap_or_default();
    assert!(
        reason.contains("Type0") && reason.contains("CID"),
        "the skip reason must name the real limitation: {reason}"
    );

    let revalidated = validate_pdfa(&output, PdfaLevel::A2b).expect("revalidate");
    assert_eq!(check_status(&revalidated, "pdfa.fonts-embedded"), "fail");
    assert!(
        !check_message(&revalidated, "pdfa.fonts-embedded").is_empty(),
        "the font failure must name the offending resources"
    );
}

// ---------------------------------------------------------------------------
// Conversion: ICC output intent
// ---------------------------------------------------------------------------

#[test]
fn pdfa_icc_profile_is_structurally_valid() {
    let profile = pdfcore::pdfa::srgb_v4_icc_profile();
    let tags = walk_icc(&profile);
    for required in [
        b"rXYZ".as_slice(),
        b"gXYZ".as_slice(),
        b"bXYZ".as_slice(),
        b"wtpt".as_slice(),
        b"rTRC".as_slice(),
        b"gTRC".as_slice(),
        b"bTRC".as_slice(),
        b"desc".as_slice(),
        b"cprt".as_slice(),
    ] {
        assert!(
            tags.iter().any(|tag| tag.signature.as_slice() == required),
            "missing required tag {:?}",
            String::from_utf8_lossy(required)
        );
    }
    // The same deterministic bytes must come back on every call.
    assert_eq!(profile, pdfcore::pdfa::srgb_v4_icc_profile());
}

#[test]
fn pdfa_converted_output_intent_carries_an_n3_icc_profile() {
    let dir = TestDir::new();
    let source = dir.path("icc.pdf");
    write_doc(&mut build_text_doc(1, "ICC", "ICC sample"), &source);
    let output = dir.path("icc-converted.pdf");
    convert_pdfa(&source, &output, PdfaLevel::A2b, &no_progress, &CancelToken::new())
        .expect("convert");

    let converted = Document::load(&output).expect("reopen converted file");
    let intents = converted
        .catalog()
        .unwrap()
        .get(b"OutputIntents")
        .expect("output intents")
        .as_array()
        .unwrap()
        .clone();
    let intent = decoded_dict(&converted, intents.first().expect("one intent"));
    assert_eq!(intent.get(b"S").unwrap().as_name().unwrap(), b"GTS_PDFA1");
    let profile_id = intent
        .get(b"DestOutputProfile")
        .expect("DestOutputProfile")
        .as_reference()
        .expect("indirect ICC stream");
    let stream = converted.get_object(profile_id).unwrap().as_stream().unwrap();
    assert_eq!(stream.dict.get(b"N").unwrap().as_i64().unwrap(), 3, "/N must be 3 for RGB");
    let bytes = stream.decompressed_content().unwrap();
    assert_eq!(bytes, pdfcore::pdfa::srgb_v4_icc_profile());
    walk_icc(&bytes);
}

fn decoded_dict(doc: &Document, value: &Object) -> Dictionary {
    match value {
        Object::Reference(id) => doc.get_dictionary(*id).expect("dictionary").clone(),
        Object::Dictionary(dict) => dict.clone(),
        _ => panic!("not a dictionary"),
    }
}

// ---------------------------------------------------------------------------
// Existing suite (kept green)
// ---------------------------------------------------------------------------

#[test]
fn pdfa_complete_document_passes_for_a2b() {
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
fn pdfa_output_intent_without_a_profile_is_reported_exactly() {
    let dir = TestDir::new();
    let mut doc = build_text_doc(1, "Legacy", "Legacy sample");
    embed_font_program(&mut doc);
    set_xmp(&mut doc, PdfaLevel::A1b);
    // An output intent with only the condition identifier: the old validator
    // accepted this, the upgraded one must name the missing profile.
    let id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "OutputIntent",
        "S" => "GTS_PDFA1",
        "OutputConditionIdentifier" => Object::String(b"sRGB".to_vec(), StringFormat::Literal),
    }));
    doc.catalog_mut()
        .unwrap()
        .set("OutputIntents", Object::Array(vec![Object::Reference(id)]));
    let path = dir.path("no-profile.pdf");
    write_doc(&mut doc, &path);

    let a1 = validate_pdfa(&path, PdfaLevel::A1b).expect("validate");
    assert_eq!(check_status(&a1, "pdfa.output-intent"), "fail");
    assert!(check_message(&a1, "pdfa.output-intent").contains("DestOutputProfile"));

    let a2 = validate_pdfa(&path, PdfaLevel::A2b).expect("validate");
    assert_eq!(check_status(&a2, "pdfa.output-intent"), "warning");
    assert!(check_message(&a2, "pdfa.output-intent").contains("DestOutputProfile"));
}

#[test]
fn pdfa_unknown_levels_are_rejected() {
    assert!(PdfaLevel::parse("9z").is_err());
    assert!(PdfaLevel::parse("").is_err());
    assert_eq!(PdfaLevel::parse("A-3b").unwrap(), PdfaLevel::A3b);
    assert_eq!(PdfaLevel::parse("2b").unwrap().as_str(), "A-2b");
    assert_eq!(PdfaLevel::parse("PDF/A-1B").unwrap().part(), 1);
}
