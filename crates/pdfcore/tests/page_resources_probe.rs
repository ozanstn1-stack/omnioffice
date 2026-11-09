//! What a page's resource dictionary looks like after this app has drawn on it.
//!
//! Every path that draws its own text - header/footer, Bates numbering, page
//! numbers, bundle labels - registers a font in the page's `/Font` dictionary
//! (ISO 32000-1 7.8.3). The page is the user's, so an existing entry has to
//! survive: a name already in use means the new font takes another one, not the
//! old font's place. Two pages that share a resource dictionary must not be able
//! to spoil each other either, and a page that inherits its resources from the
//! page tree must end up owning the ones it uses (7.7.3.4).

use lopdf::{dictionary, Document, Object, Stream};
use pdfcore::docutil::OverwritePolicy;
use pdfcore::progress::CancelToken;
use pdfcore::stamp::{stamp_pdf, BatesOptions, HeaderFooterOptions};

fn rect(values: [f64; 4]) -> Object {
    Object::Array(values.iter().map(|value| Object::Real(*value as f32)).collect())
}

/// A one-page document with an own font `/F1`, whose `/Resources` is either inline
/// or inherited from the page tree.
fn document(name: &str, inherited: bool) -> std::path::PathBuf {
    let mut doc = Document::with_version("1.7");
    let root = doc.new_object_id();
    let content = doc.add_object(Stream::new(dictionary! {}, b"BT /F1 12 Tf (exhibit) Tj ET".to_vec()));
    let font = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Courier",
    }));
    let resources = doc.add_object(dictionary! { "Font" => dictionary! { "F1" => Object::Reference(font) } });
    let mut page = dictionary! {
        "Type" => "Page",
        "Parent" => Object::Reference(root),
        "MediaBox" => rect([0.0, 0.0, 595.0, 842.0]),
        "Contents" => Object::Reference(content),
    };
    if !inherited {
        page.set("Resources", Object::Reference(resources));
    }
    let page_id = doc.add_object(Object::Dictionary(page));
    let mut tree = dictionary! {
        "Type" => "Pages",
        "Count" => Object::Integer(1),
        "Kids" => Object::Array(vec![Object::Reference(page_id)]),
    };
    if inherited {
        tree.set("Resources", Object::Reference(resources));
    }
    doc.objects.insert(root, Object::Dictionary(tree));
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(root) });
    doc.trailer.set("Root", Object::Reference(catalog));
    let path = std::env::temp_dir().join(format!("omnioffice-page-resources-{name}.pdf"));
    doc.save(&path).expect("save the fixture");
    path
}

fn header_footer() -> HeaderFooterOptions {
    HeaderFooterOptions {
        header_center: "ÇĞ".into(),
        footer_left: "İ".into(),
        margin_pt: 24.0,
        font_size_pt: 9.0,
        ..HeaderFooterOptions::default()
    }
}

fn bates() -> BatesOptions {
    BatesOptions {
        prefix: "ABC".into(),
        suffix: String::new(),
        start: 1,
        digits: 6,
        position: "bottomRight".into(),
        font_size_pt: 9.0,
        color: "#334155".into(),
        margin_pt: 24.0,
        pages: Vec::new(),
    }
}

fn stamped(input: &std::path::Path, name: &str) -> Vec<u8> {
    let output = std::env::temp_dir().join(format!("omnioffice-page-resources-{name}-out.pdf"));
    stamp_pdf(
        input,
        &output,
        Some(&header_footer()),
        Some(&bates()),
        OverwritePolicy::Replace,
        None,
        &|_| {},
        &CancelToken::new(),
    )
    .expect("stamp");
    std::fs::read(&output).expect("read the output")
}

/// The page's own `/Font` dictionary, resolved through references.
fn fonts_of(doc: &Document, page: u32) -> lopdf::Dictionary {
    let page_id = *doc.get_pages().get(&page).expect("the page");
    let page_dict = doc.get_dictionary(page_id).expect("page dict");
    let resources = match page_dict.get(b"Resources").expect("resources") {
        Object::Reference(id) => doc.get_dictionary(*id).expect("resources dict").clone(),
        Object::Dictionary(dict) => dict.clone(),
        other => panic!("unexpected /Resources: {other:?}"),
    };
    match resources.get(b"Font").expect("font dictionary") {
        Object::Reference(id) => doc.get_dictionary(*id).expect("font dict").clone(),
        Object::Dictionary(dict) => dict.clone(),
        other => panic!("unexpected /Font: {other:?}"),
    }
}

#[test]
fn a_page_keeps_its_own_font_when_a_stamp_registers_its_own() {
    let path = document("own-font", false);
    let output = stamped(&path, "own-font");
    let doc = Document::load_mem(&output).expect("reload the output");
    let fonts = fonts_of(&doc, 1);
    let names: Vec<String> = fonts.iter().map(|(key, _)| String::from_utf8_lossy(key).to_string()).collect();
    assert!(fonts.has(b"F1"), "the page's own font survives: {names:?}");
    assert_eq!(names.len(), 2, "the page's font plus one per stamp font: {names:?}");
    assert!(names.iter().any(|name| name == "OmniTr"), "the stamp registers its font: {names:?}");
    // The page's own font is still the one its text was drawn with.
    let original = fonts.get(b"F1").expect("F1").as_reference().expect("a reference");
    let base =
        doc.get_dictionary(original).and_then(|font| font.get(b"BaseFont")).and_then(Object::as_name).unwrap_or(b"");
    assert_eq!(base, b"Courier", "the page's font object is untouched");
}

#[test]
fn an_inherited_font_is_resolved_onto_the_page_that_uses_it() {
    // The page inherits `/Resources` from the page tree; whichever way the stamp
    // materializes it, the page has to end up with the font it draws with
    // (ISO 32000-1 7.7.3.4).
    let path = document("inherited", true);
    let output = stamped(&path, "inherited");
    let doc = Document::load_mem(&output).expect("reload the output");
    let fonts = fonts_of(&doc, 1);
    assert!(fonts.has(b"F1"), "the inherited font is resolved onto the page");
    let problem = &doc.get_page_content(*doc.get_pages().get(&1).expect("page"));
    assert!(String::from_utf8_lossy(problem).contains("/OmniTr"), "and the stamp draws with its own font");
}

#[test]
fn two_pages_sharing_a_resource_dictionary_do_not_spoil_each_other() {
    // Both pages point at one `/Resources` object. Registering a font for page 1
    // must not change what page 2 draws with, so each page ends up with its own
    // copy (ISO 32000-1 7.8.3).
    let mut doc = Document::with_version("1.7");
    let root = doc.new_object_id();
    let font = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Courier",
    }));
    let shared =
        Object::Reference(doc.add_object(dictionary! { "Font" => dictionary! { "F1" => Object::Reference(font) } }));
    let mut kids = Vec::new();
    for index in 0..2 {
        let content =
            doc.add_object(Stream::new(dictionary! {}, format!("BT /F1 12 Tf (page {index}) Tj ET").into_bytes()));
        let page = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(root),
            "MediaBox" => rect([0.0, 0.0, 595.0, 842.0]),
            "Contents" => Object::Reference(content),
            "Resources" => shared.clone(),
        });
        kids.push(Object::Reference(page));
    }
    doc.objects.insert(
        root,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => Object::Integer(2),
            "Kids" => Object::Array(kids),
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(root) });
    doc.trailer.set("Root", Object::Reference(catalog));
    let path = std::env::temp_dir().join("omnioffice-page-resources-shared.pdf");
    doc.save(&path).expect("save");

    let output = stamped(&path, "shared");
    let doc = Document::load_mem(&output).expect("reload the output");
    let mut seen = Vec::new();
    for page in 1..=2 {
        let page_id = *doc.get_pages().get(&page).expect("the page");
        let entry = doc.get_dictionary(page_id).expect("page dict").get(b"Resources").expect("resources").clone();
        seen.push(entry);
        let fonts = fonts_of(&doc, page);
        assert!(fonts.has(b"F1"), "page {page} keeps the shared font");
    }
    assert_ne!(seen[0], seen[1], "each page ends up with its own resource dictionary");
}

#[test]
fn a_second_stamp_does_not_take_the_first_one_s_font() {
    // The file this app just stamped carries `/OmniTr`. Stamping it again - a
    // Bates number on top of a header - must not overwrite that resource, or the
    // first stamp's text is drawn with the second font (ISO 32000-1 7.8.3).
    let path = document("twice", false);
    let once = stamped(&path, "twice-1");
    let first = std::env::temp_dir().join("omnioffice-page-resources-twice-1.pdf");
    std::fs::write(&first, &once).expect("write the intermediate file");
    let twice = stamped(&first, "twice-2");

    let doc = Document::load_mem(&twice).expect("reload the output");
    let fonts = fonts_of(&doc, 1);
    let names: Vec<String> = fonts.iter().map(|(key, _)| String::from_utf8_lossy(key).to_string()).collect();
    assert!(names.iter().any(|name| name == "OmniTr"), "the first stamp keeps its name: {names:?}");
    assert!(
        names.iter().any(|name| name != "OmniTr" && name.starts_with("OmniTr")),
        "the second stamp registers under a free name: {names:?}"
    );
}
