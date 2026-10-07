//! PDF/A font embedding: glyph-preserving subsets and Type0 fonts.

mod common;

use std::collections::BTreeSet;

use ab_glyph::{Font, FontRef, GlyphId};
use common::*;
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};
use pdfcore::fontembed::EmbeddedFontReport;
use pdfcore::pdfa::{convert_pdfa, validate_pdfa, PdfaLevel, PdfaReport};
use pdfcore::progress::CancelToken;
use pdfcore::ttfsubset::subset_truetype;

const LIBERATION_SANS: &[u8] = include_bytes!("../assets/fonts/LiberationSans-Regular.ttf");

fn check_status(report: &PdfaReport, id: &str) -> String {
    report
        .checks
        .iter()
        .find(|entry| entry.id == id)
        .map(|entry| entry.status.clone())
        .unwrap_or_else(|| panic!("missing check {id}"))
}

fn failures(report: &PdfaReport) -> Vec<String> {
    report
        .checks
        .iter()
        .filter(|entry| entry.status == "fail")
        .map(|entry| format!("{}: {}", entry.id, entry.message))
        .collect()
}

fn font_id(doc: &Document) -> ObjectId {
    doc.objects
        .iter()
        .find_map(|(id, object)| {
            let dict = object.as_dict().ok()?;
            dict.get(b"BaseFont").ok()?;
            Some(*id)
        })
        .expect("fixture font")
}

fn page_content_id(doc: &Document) -> ObjectId {
    let page = *doc.get_pages().values().next().expect("page");
    doc.get_page_contents(page).first().copied().expect("page content")
}

fn set_content(doc: &mut Document, content: &str) {
    let id = page_content_id(doc);
    doc.get_object_mut(id).unwrap().as_stream_mut().unwrap().set_plain_content(content.as_bytes().to_vec());
}

fn convert(doc: &mut Document, dir: &TestDir, name: &str) -> (PdfaReport, Document) {
    let source = dir.path(&format!("{name}-in.pdf"));
    write_doc(doc, &source);
    let output = dir.path(&format!("{name}-out.pdf"));
    let report = convert_pdfa(&source, &output, PdfaLevel::A2b, &no_progress, &CancelToken::new()).expect("convert");
    let revalidated = validate_pdfa(&output, PdfaLevel::A2b).expect("revalidate");
    assert_eq!(
        check_status(&revalidated, "pdfa.fonts-embedded"),
        check_status(&report, "pdfa.fonts-embedded"),
        "the written file validates like the conversion report"
    );
    (report, Document::load(&output).expect("reopen converted file"))
}

fn report_for<'a>(report: &'a PdfaReport, base_font: &str) -> &'a EmbeddedFontReport {
    report
        .font_embedding
        .iter()
        .find(|entry| entry.base_font == base_font)
        .unwrap_or_else(|| panic!("no font report for {base_font}: {:?}", report.font_embedding))
}

fn base_font_of(dict: &Dictionary) -> String {
    String::from_utf8_lossy(dict.get(b"BaseFont").expect("BaseFont").as_name().expect("name")).to_string()
}

fn assert_subset_tag(name: &str) -> &str {
    let bytes = name.as_bytes();
    assert!(
        bytes.len() > 7 && bytes[6] == b'+' && bytes[..6].iter().all(u8::is_ascii_uppercase),
        "expected an ABCDEF+Name subset tag in {name}"
    );
    &name[7..]
}

/// The decompressed FontFile2 program of a font descriptor.
fn program_of(doc: &Document, descriptor: &Dictionary) -> Vec<u8> {
    let id = descriptor.get(b"FontFile2").expect("FontFile2").as_reference().expect("indirect program");
    let stream = doc.get_object(id).unwrap().as_stream().unwrap();
    let bytes = stream.decompressed_content().unwrap();
    assert_eq!(stream.dict.get(b"Length1").unwrap().as_i64().unwrap(), bytes.len() as i64, "/Length1");
    bytes
}

fn descriptor_of(doc: &Document, font: &Dictionary) -> Dictionary {
    let id = font.get(b"FontDescriptor").expect("descriptor").as_reference().expect("indirect descriptor");
    doc.get_dictionary(id).unwrap().clone()
}

fn has_outline(font: &FontRef<'_>, character: char) -> bool {
    font.outline(font.glyph_id(character)).is_some()
}

// ---------------------------------------------------------------------------
// Simple fonts
// ---------------------------------------------------------------------------

#[test]
fn helvetica_is_embedded_as_a_small_glyph_preserving_subset() {
    let dir = TestDir::new();
    let mut doc = build_text_doc(1, "Plain", "Plain document");
    let (report, converted) = convert(&mut doc, &dir, "helvetica");
    assert!(report.valid, "failures: {:?}", failures(&report));
    assert_eq!(check_status(&report, "pdfa.fonts-embedded"), "pass");

    let entry = report_for(&report, "Helvetica");
    assert!(entry.embedded && entry.subset, "{entry:?}");
    assert_eq!(entry.substitute.as_deref(), Some("LiberationSans"));
    assert!(entry.warning.is_none(), "a subset of a metric-compatible face needs no warning: {entry:?}");

    let font = converted.get_dictionary(font_id(&converted)).unwrap().clone();
    let name = base_font_of(&font);
    assert_eq!(assert_subset_tag(&name), "LiberationSans");
    let descriptor = descriptor_of(&converted, &font);
    assert_eq!(descriptor.get(b"FontName").unwrap().as_name().unwrap(), name.as_bytes(), "FontName carries the tag");

    let program = program_of(&converted, &descriptor);
    assert!(
        program.len() * 5 < LIBERATION_SANS.len(),
        "the subset ({} bytes) must be a fraction of the full face ({} bytes)",
        program.len(),
        LIBERATION_SANS.len()
    );

    // The program parses, keeps the used glyphs at their original ids and has
    // emptied the rest.
    let full = FontRef::try_from_slice(LIBERATION_SANS).unwrap();
    let subset = FontRef::try_from_slice(&program).expect("the subset parses");
    for character in "Plain page 1".chars().filter(|character| *character != ' ') {
        assert_eq!(subset.glyph_id(character), full.glyph_id(character), "glyph id of {character}");
        assert!(has_outline(&subset, character), "{character} must keep its outline");
    }
    for character in ['Z', 'q', '7', 'W'] {
        assert_eq!(subset.glyph_id(character), full.glyph_id(character), "ids never move");
        assert!(has_outline(&full, character));
        assert!(!has_outline(&subset, character), "{character} is unused and must be emptied");
    }
}

#[test]
fn fonts_used_only_inside_forms_and_appearances_are_embedded_with_their_glyphs() {
    let dir = TestDir::new();
    let mut doc = build_text_doc(1, "Page", "Forms");
    let page_id = *doc.get_pages().values().next().unwrap();
    // A form XObject with its own font resource and text the page never shows.
    let form_font = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Arial-BoldMT",
        "Encoding" => "WinAnsiEncoding",
    }));
    let form = doc.add_object(Object::Stream(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), 300.into(), 50.into()],
            "Resources" => dictionary! { "Font" => dictionary! { "FB" => Object::Reference(form_font) } },
        },
        b"BT /FB 12 Tf 10 10 Td (Xylophone) Tj ET".to_vec(),
    )));
    add_resource_entry_xobject(&mut doc, page_id, "Fm1", form);
    let mut content = doc.get_object(page_content_id(&doc)).unwrap().as_stream().unwrap().content.clone();
    content.extend_from_slice(b"\nq 1 0 0 1 72 500 cm /Fm1 Do Q\n");
    set_content(&mut doc, &String::from_utf8(content).unwrap());

    let (report, converted) = convert(&mut doc, &dir, "form");
    assert!(report.valid, "failures: {:?}", failures(&report));
    let entry = report_for(&report, "Arial-BoldMT");
    assert!(entry.embedded && entry.subset, "{entry:?}");

    let (_, font) = converted
        .objects
        .iter()
        .filter_map(|(id, object)| Some((*id, object.as_dict().ok()?.clone())))
        .find(|(_, dict)| {
            dict.get(b"BaseFont")
                .ok()
                .and_then(|name| name.as_name().ok())
                .is_some_and(|name| name.ends_with(b"+LiberationSans-Bold"))
        })
        .expect("the bold form font is embedded and tagged");
    let program = program_of(&converted, &descriptor_of(&converted, &font));
    let subset = FontRef::try_from_slice(&program).unwrap();
    for character in "Xylophone".chars() {
        assert!(has_outline(&subset, character), "{character} is drawn by the form and must be kept");
    }
    assert!(!has_outline(&subset, 'Q'), "unused glyphs stay empty");
}

fn add_resource_entry_xobject(doc: &mut Document, page_id: ObjectId, name: &str, form: ObjectId) {
    pdfcore::docutil::add_resource_entry(doc, page_id, b"XObject", name, Object::Reference(form)).unwrap();
}

#[test]
fn acroform_default_resource_fonts_keep_the_full_program() {
    let dir = TestDir::new();
    let mut doc = build_text_doc(1, "Form", "Form");
    let id = font_id(&doc);
    let acro = doc.add_object(Object::Dictionary(dictionary! {
        "Fields" => Object::Array(vec![]),
        "DR" => dictionary! { "Font" => dictionary! { "Helv" => Object::Reference(id) } },
    }));
    let root = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
    doc.get_dictionary_mut(root).unwrap().set("AcroForm", Object::Reference(acro));
    let (report, converted) = convert(&mut doc, &dir, "acroform");
    let entry = report_for(&report, "Helvetica");
    assert!(entry.embedded && !entry.subset, "a field may show any character: {entry:?}");
    assert!(entry.warning.as_deref().unwrap_or_default().contains("full font program"), "{entry:?}");
    let font = converted.get_dictionary(font_id(&converted)).unwrap().clone();
    let name = base_font_of(&font);
    assert_eq!(name, "LiberationSans", "no tag on a full program");
    assert!(program_of(&converted, &descriptor_of(&converted, &font)).len() > 100_000);
}

// ---------------------------------------------------------------------------
// ttfsubset
// ---------------------------------------------------------------------------

#[test]
fn subsetting_keeps_glyph_ids_composites_and_drops_layout_tables() {
    let full = FontRef::try_from_slice(LIBERATION_SANS).unwrap();
    let eacute = full.glyph_id('\u{e9}').0;
    let wanted = BTreeSet::from([eacute, full.glyph_id('A').0]);
    let subset = subset_truetype(LIBERATION_SANS, &wanted).expect("subset");
    assert!(subset.glyphs.contains(&0), ".notdef is always kept");
    assert!(subset.glyphs.len() >= 4, "the components of the composite e-acute must be kept too: {:?}", subset.glyphs);
    let parsed = FontRef::try_from_slice(&subset.program).expect("the subset parses");
    for character in ['\u{e9}', 'A', 'e'] {
        assert_eq!(parsed.glyph_id(character), full.glyph_id(character));
        assert!(has_outline(&parsed, character), "{character}");
    }
    assert!(!has_outline(&parsed, 'B'));
    assert_eq!(parsed.glyph_id('B'), GlyphId(full.glyph_id('B').0), "ids never move");
    let count = u16::from_be_bytes([subset.program[4], subset.program[5]]) as usize;
    let tags: Vec<&[u8]> = (0..count).map(|index| &subset.program[12 + index * 16..16 + index * 16]).collect();
    for dropped in [b"GSUB", b"GPOS", b"GDEF", b"kern", b"DSIG"] {
        assert!(!tags.contains(&dropped.as_slice()), "{} must be dropped", String::from_utf8_lossy(dropped));
    }
    for kept in [b"cmap".as_slice(), b"glyf", b"head", b"hhea", b"hmtx", b"loca", b"maxp"] {
        assert!(tags.contains(&kept), "{} must be kept", String::from_utf8_lossy(kept));
    }
}

// ---------------------------------------------------------------------------
// Type0 / CID fonts
// ---------------------------------------------------------------------------

fn to_unicode_cmap(pairs: &[(u16, char)]) -> Vec<u8> {
    let mut text = String::from("/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n");
    text.push_str("1 begincodespacerange <0000> <FFFF> endcodespacerange\n");
    text.push_str(&format!("{} beginbfchar\n", pairs.len()));
    for (cid, character) in pairs {
        text.push_str(&format!("<{cid:04X}> <{:04X}>\n", *character as u32));
    }
    text.push_str("endbfchar\nendcmap\nCMapName currentdict /CMap defineresource pop end end\n");
    text.into_bytes()
}

/// Turns the fixture's font into a Type0/Identity-H font whose CIDFontType2
/// descendant has no program, and makes the page show `shown` CIDs.
fn make_type0(doc: &mut Document, mapping: &[(u16, char)], shown: &[u16], with_to_unicode: bool) {
    let id = font_id(doc);
    let cid_font = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "CIDFontType2",
        "BaseFont" => "ArialMT",
        "CIDSystemInfo" => dictionary! {
            "Registry" => Object::string_literal("Adobe"),
            "Ordering" => Object::string_literal("Identity"),
            "Supplement" => 0,
        },
    }));
    let to_unicode = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), to_unicode_cmap(mapping))));
    let font = doc.get_dictionary_mut(id).unwrap();
    font.set("Subtype", "Type0");
    font.set("BaseFont", "ArialMT");
    font.set("Encoding", "Identity-H");
    font.remove(b"Widths");
    font.set("DescendantFonts", Object::Array(vec![Object::Reference(cid_font)]));
    if with_to_unicode {
        font.set("ToUnicode", Object::Reference(to_unicode));
    }
    let hex: String = shown.iter().map(|cid| format!("{cid:04X}")).collect();
    set_content(doc, &format!("BT\n/F1 24 Tf\n72 700 Td\n<{hex}> Tj\nET\n"));
}

#[test]
fn type0_identity_h_with_to_unicode_gets_a_cid_to_gid_map() {
    let dir = TestDir::new();
    let mut doc = build_text_doc(1, "CID", "CID sample");
    // CID 7 = H, CID 9 = i, CID 300 = a; CID 8 = Z exists but is not shown.
    make_type0(&mut doc, &[(7, 'H'), (8, 'Z'), (9, 'i'), (300, 'a')], &[7, 9, 9, 300], true);
    let (report, converted) = convert(&mut doc, &dir, "type0");
    assert!(report.valid, "failures: {:?}", failures(&report));
    assert_eq!(check_status(&report, "pdfa.fonts-embedded"), "pass");
    let entry = report_for(&report, "ArialMT");
    assert!(entry.embedded && entry.subset, "{entry:?}");

    let type0 = converted.get_dictionary(font_id(&converted)).unwrap().clone();
    assert_eq!(type0.get(b"Subtype").unwrap().as_name().unwrap(), b"Type0");
    let name = base_font_of(&type0);
    assert_eq!(assert_subset_tag(&name), "LiberationSans");
    let descendant_id = type0.get(b"DescendantFonts").unwrap().as_array().unwrap()[0].as_reference().unwrap();
    let descendant = converted.get_dictionary(descendant_id).unwrap().clone();
    assert_eq!(descendant.get(b"Subtype").unwrap().as_name().unwrap(), b"CIDFontType2");
    assert_eq!(base_font_of(&descendant), name);
    let descriptor = descriptor_of(&converted, &descendant);
    assert_eq!(descriptor.get(b"FontName").unwrap().as_name().unwrap(), name.as_bytes());

    // CIDToGIDMap: 2 bytes per CID, the shown CIDs point at the used glyphs.
    let map_id = descendant.get(b"CIDToGIDMap").expect("CIDToGIDMap").as_reference().expect("a stream, not /Identity");
    let map = converted.get_object(map_id).unwrap().as_stream().unwrap().decompressed_content().unwrap();
    let full = FontRef::try_from_slice(LIBERATION_SANS).unwrap();
    let gid = |cid: usize| u16::from_be_bytes([map[cid * 2], map[cid * 2 + 1]]);
    assert_eq!(gid(7), full.glyph_id('H').0);
    assert_eq!(gid(9), full.glyph_id('i').0);
    assert_eq!(gid(300), full.glyph_id('a').0);
    assert_eq!(gid(8), 0, "an unshown CID is not mapped");

    // /W was missing, so it is built from the substitute's advances.
    let widths = descendant.get(b"W").expect("W").as_array().unwrap();
    assert!(!widths.is_empty());

    // The program keeps exactly the shown glyphs.
    let program = program_of(&converted, &descriptor);
    let subset = FontRef::try_from_slice(&program).unwrap();
    for character in ['H', 'i', 'a'] {
        assert!(has_outline(&subset, character), "{character}");
        assert_eq!(subset.glyph_id(character), full.glyph_id(character));
    }
    assert!(!has_outline(&subset, 'Z'), "CID 8 is mapped by ToUnicode but never shown");
    assert!(program.len() * 5 < LIBERATION_SANS.len());
}

#[test]
fn type0_cids_without_a_to_unicode_character_are_refused_honestly() {
    let dir = TestDir::new();
    let mut doc = build_text_doc(1, "CID", "CID sample");
    make_type0(&mut doc, &[(7, 'H')], &[7, 11], true);
    let (report, _) = convert(&mut doc, &dir, "type0-gap");
    assert!(!report.valid);
    assert_eq!(check_status(&report, "pdfa.fonts-embedded"), "fail");
    let entry = report_for(&report, "ArialMT");
    assert!(!entry.embedded);
    assert!(entry.skipped_reason.as_deref().unwrap_or_default().contains("CID"), "{entry:?}");
}

#[test]
fn type0_without_to_unicode_stays_skipped_with_the_cid_reason() {
    let dir = TestDir::new();
    let mut doc = build_text_doc(1, "CID", "CID sample");
    make_type0(&mut doc, &[(7, 'H')], &[7], false);
    let (report, _) = convert(&mut doc, &dir, "type0-nomap");
    assert!(!report.valid);
    let entry = report_for(&report, "ArialMT");
    assert!(!entry.embedded);
    let reason = entry.skipped_reason.as_deref().unwrap_or_default();
    assert!(reason.contains("Type0") && reason.contains("CID") && reason.contains("ToUnicode"), "{reason}");
}
