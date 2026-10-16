//! Annotation and form flattening tests.

mod common;

use common::*;
use lopdf::{dictionary, Dictionary, Document, Object, Stream, StringFormat};
use pdfcore::flatten::{flatten_pdf, FlattenOptions};
use pdfcore::progress::CancelToken;

/// A one-page document with an AcroForm text field whose widget carries a red
/// rectangle appearance stream.
fn widget_doc() -> Document {
    let mut doc = build_text_doc(1, "Form", "Form document");
    let page_id = doc.get_pages().get(&1).copied().expect("page");
    let appearance = doc.add_object(Object::Stream(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![
                Object::Real(0.0),
                Object::Real(0.0),
                Object::Real(200.0),
                Object::Real(40.0),
            ],
            "Resources" => Object::Dictionary(Dictionary::new()),
        },
        b"0.85 0.1 0.1 rg\n0 0 200 40 re\nf\n".to_vec(),
    )));
    let widget = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "FT" => "Tx",
        "T" => Object::String(b"name".to_vec(), StringFormat::Literal),
        "V" => Object::String(b"Ada".to_vec(), StringFormat::Literal),
        "Rect" => vec![
            Object::Real(100.0),
            Object::Real(100.0),
            Object::Real(300.0),
            Object::Real(140.0),
        ],
        "P" => Object::Reference(page_id),
        "AP" => Object::Dictionary(dictionary! {
            "N" => Object::Reference(appearance),
        }),
    }));
    doc.get_object_mut(page_id)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("Annots", Object::Array(vec![Object::Reference(widget)]));

    let catalog_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
    let acro = doc.add_object(Object::Dictionary(dictionary! {
        "Fields" => vec![Object::Reference(widget)],
        "NeedAppearances" => Object::Boolean(true),
    }));
    doc.get_object_mut(catalog_id).unwrap().as_dict_mut().unwrap().set("AcroForm", Object::Reference(acro));
    doc
}

fn widget_id(doc: &Document) -> lopdf::ObjectId {
    doc.objects
        .iter()
        .find_map(|(id, object)| {
            let dict = object.as_dict().ok()?;
            let subtype = dict.get(b"Subtype").ok().and_then(|value| value.as_name().ok());
            if subtype == Some(b"Widget".as_slice()) {
                Some(*id)
            } else {
                None
            }
        })
        .expect("widget")
}

fn annots(doc: &Document, page: u32) -> Vec<Object> {
    let page_id = doc.get_pages().get(&page).copied().expect("page");
    doc.get_dictionary(page_id)
        .ok()
        .and_then(|dict| dict.get(b"Annots").ok())
        .and_then(|value| match value {
            Object::Array(items) => Some(items.clone()),
            Object::Reference(id) => match doc.get_object(*id).ok()? {
                Object::Array(items) => Some(items.clone()),
                _ => None,
            },
            _ => None,
        })
        .unwrap_or_default()
}

#[test]
fn flattening_a_widget_burns_the_appearance_and_removes_the_form() {
    let dir = TestDir::new();
    let source = dir.path("form.pdf");
    write_doc(&mut widget_doc(), &source);
    let output = dir.path("flat.pdf");
    let report =
        flatten_pdf(&source, &output, &FlattenOptions::default(), &no_progress, &CancelToken::new()).expect("flatten");

    assert_eq!(report.annotations_flattened, 1);
    assert_eq!(report.fields_flattened, 1);
    assert_eq!(report.pages_touched, 1);

    let flat = Document::load(&output).expect("reopen");
    assert_eq!(flat.get_pages().len(), 1, "the page count must be preserved");
    assert!(annots(&flat, 1).is_empty(), "/Annots must be empty after flattening");
    assert!(flat.catalog().unwrap().get(b"AcroForm").is_err(), "/AcroForm must be gone once every field is flattened");

    let page_id = flat.get_pages().get(&1).copied().unwrap();
    let content = flat.get_page_content(page_id);
    let content = String::from_utf8_lossy(&content).to_string();
    assert!(content.contains("Do"), "the appearance XObject must be drawn: {content}");
    assert!(content.contains("cm"), "the appearance must be placed with a matrix: {content}");

    let text = page_text(&output, 1);
    assert!(text.contains("Form"), "page text must stay extractable: {text:?}");
}

#[test]
fn flattening_changes_what_the_page_renders() {
    if !pdfcore::render::is_available() {
        eprintln!("pdfium is not available; skipping the render comparison");
        return;
    }
    let dir = TestDir::new();
    let source = dir.path("form.pdf");
    write_doc(&mut widget_doc(), &source);
    let output = dir.path("flat.pdf");
    flatten_pdf(&source, &output, &FlattenOptions::default(), &no_progress, &CancelToken::new()).expect("flatten");

    // Control: the same document with the widget deleted but nothing burned in.
    let mut control = widget_doc();
    let page_id = control.get_pages().get(&1).copied().unwrap();
    control.get_object_mut(page_id).unwrap().as_dict_mut().unwrap().remove(b"Annots");
    let catalog_id = control.trailer.get(b"Root").unwrap().as_reference().unwrap();
    control.get_object_mut(catalog_id).unwrap().as_dict_mut().unwrap().remove(b"AcroForm");
    let control_path = dir.path("control.pdf");
    write_doc(&mut control, &control_path);

    let options = pdfcore::render::RenderOptions { dpi: 96.0, max_width: Some(900), max_height: Some(900) };
    let before = pdfcore::render::render_page(&control_path, None, 1, &options).expect("render control");
    let after = pdfcore::render::render_page(&output, None, 1, &options).expect("render flattened");
    assert!(
        changed_pixels(&before, &after, 8) > 0,
        "the burned-in appearance must be visible where the control shows nothing"
    );
}

#[test]
fn appearances_only_generates_a_missing_text_field_appearance() {
    let dir = TestDir::new();
    let mut doc = widget_doc();
    let widget = widget_id(&doc);
    doc.get_object_mut(widget).unwrap().as_dict_mut().unwrap().remove(b"AP");
    let source = dir.path("no-ap.pdf");
    write_doc(&mut doc, &source);
    let output = dir.path("generated.pdf");
    let options = FlattenOptions { annotations: false, forms: false, appearances: true };
    let report = flatten_pdf(&source, &output, &options, &no_progress, &CancelToken::new()).expect("appearances");
    assert_eq!(report.annotations_flattened, 0, "nothing must be flattened");

    let result = Document::load(&output).expect("reopen");
    let widget = widget_id(&result);
    let widget_dict = result.get_dictionary(widget).expect("widget");
    assert!(widget_dict.get(b"AP").is_ok(), "an appearance must be generated");
    assert_eq!(annots(&result, 1).len(), 1, "the annotation must stay");
    let acro = result.catalog().unwrap().get(b"AcroForm").expect("acroform").as_reference().unwrap();
    assert!(
        result.get_dictionary(acro).unwrap().get(b"NeedAppearances").is_err(),
        "/NeedAppearances must be off once appearances are generated"
    );
}

/// A `/V` string in the PDF's own UTF-16BE form, the way a real form stores
/// non-ASCII text.
fn utf16_string(text: &str) -> Object {
    let mut bytes = vec![0xFE, 0xFF];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    Object::String(bytes, StringFormat::Literal)
}

#[test]
fn a_generated_appearance_writes_turkish_with_the_embedded_font() {
    // The generated appearance used base-14 Helvetica + WinAnsi, which cannot
    // represent Turkish letters: it wrote them as '?'.
    let dir = TestDir::new();
    let mut doc = widget_doc();
    let widget = widget_id(&doc);
    {
        let dict = doc.get_object_mut(widget).unwrap().as_dict_mut().unwrap();
        dict.remove(b"AP");
        dict.set("V", utf16_string("Şişli Güneş İĞDIR ı"));
    }
    let source = dir.path("turkish-no-ap.pdf");
    write_doc(&mut doc, &source);
    let output = dir.path("turkish-generated.pdf");
    let options = FlattenOptions { annotations: false, forms: false, appearances: true };
    flatten_pdf(&source, &output, &options, &no_progress, &CancelToken::new()).expect("appearances");

    let result = Document::load(&output).expect("reopen");
    let widget = widget_id(&result);
    let widget_dict = result.get_dictionary(widget).expect("widget");
    let appearance = widget_dict.get(b"AP").unwrap().as_dict().unwrap().get(b"N").unwrap().as_reference().unwrap();
    let stream = result.get_object(appearance).unwrap().as_stream().unwrap();
    // The appearance draws with an embedded Type0/CID font, not WinAnsi text.
    let fonts = stream.dict.get(b"Resources").unwrap().as_dict().unwrap().get(b"Font").unwrap().as_dict().unwrap();
    let font = fonts.iter().next().expect("one font resource").1.as_reference().unwrap();
    let font_dict = result.get_dictionary(font).unwrap();
    assert_eq!(
        font_dict.get(b"Subtype").and_then(Object::as_name).unwrap(),
        b"Type0",
        "the appearance must use the embedded font"
    );
    let content = String::from_utf8_lossy(&stream.decompressed_content().unwrap()).to_string();
    assert!(content.contains(" Tj"), "content: {content}");
    assert!(!content.contains("Şişli"), "the text must be CID-encoded, not literal bytes");

    // Burn the appearance into the page and read the text back through ToUnicode.
    let burned = dir.path("turkish-burned.pdf");
    let burn = FlattenOptions::default();
    flatten_pdf(&output, &burned, &burn, &no_progress, &CancelToken::new()).expect("flatten");
    // pdfium descends into the burned-in XObject; lopdf's `extract_text` does
    // not, which is why the shared `page_text` helper sees only the page text.
    if pdfcore::render::is_available() {
        let text = pdfcore::render::extract_page_text(&burned, None, 1).unwrap_or_default();
        assert!(text.contains("Şişli Güneş İĞDIR ı"), "extracted: {text}");
    }
}
