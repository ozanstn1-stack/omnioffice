//! AcroForm field and page-object editing tests.
//!
//! Every fixture is built by hand with lopdf so the assertions are about the
//! exact PDF objects this module is supposed to read and write - no behavior
//! is inferred from a helper that could share the same bug as the code under
//! test. Test names carry the `forms_` / `objects_` prefix so the focused
//! `cargo test -p pdfcore forms` gate selects them.

mod common;

use common::*;
use lopdf::{dictionary, Dictionary, Document, Object, Stream, StringFormat};
use pdfcore::docutil::OverwritePolicy;
use pdfcore::forms::{
    apply_object_edits, fill_fields, list_fields, list_fields_in_file, list_page_objects, transform_objects,
    FieldValue, ObjectAction, ObjectEdit,
};
use pdfcore::security::{protect_pdf, ProtectOptions};
use std::fs;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn ap_stream(doc: &mut Document, width: f64, height: f64) -> lopdf::ObjectId {
    doc.add_object(Object::Stream(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![
                Object::Real(0.0),
                Object::Real(0.0),
                Object::Real(width as f32),
                Object::Real(height as f32),
            ],
            "Resources" => dictionary! {},
        },
        b"0.9 0.95 1 rg\n0 0 100 20 re\nf\n".to_vec(),
    )))
}

/// A one-page form with every field type the module must understand:
/// a merged text widget, a text field under an intermediate node (inheritance),
/// a combo box, a multi-select list box, a checkbox, a radio group, a
/// push button and a required text field with /MaxLen.
fn form_doc() -> Document {
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

    // Merged text widget: field and widget in one object.
    let text_ap = ap_stream(&mut doc, 200.0, 20.0);
    let text_widget = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "FT" => "Tx",
        "Ff" => 2i64, // required
        "T" => Object::String(b"full_name".to_vec(), StringFormat::Literal),
        "TU" => Object::String(b"Full name".to_vec(), StringFormat::Literal),
        "MaxLen" => 10i64,
        "DA" => Object::String(b"/Helv 9 Tf 0 g".to_vec(), StringFormat::Literal),
        "V" => Object::String(b"Ada".to_vec(), StringFormat::Literal),
        "Rect" => vec![
            Object::Real(72.0),
            Object::Real(700.0),
            Object::Real(272.0),
            Object::Real(720.0),
        ],
        "P" => Object::Reference(page_id),
        "AP" => dictionary! { "N" => Object::Reference(text_ap) },
    }));

    // A text field under an intermediate non-terminal node: /FT and /V live on
    // the parent, /T on the child. The list must report the qualified name and
    // the inherited value.
    let person_name = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "T" => Object::String(b"name".to_vec(), StringFormat::Literal),
        "Rect" => vec![
            Object::Real(72.0),
            Object::Real(650.0),
            Object::Real(272.0),
            Object::Real(670.0),
        ],
        "P" => Object::Reference(page_id),
    }));
    let person = doc.add_object(Object::Dictionary(dictionary! {
        "FT" => "Tx",
        "T" => Object::String(b"person".to_vec(), StringFormat::Literal),
        "V" => Object::String(b"Grace".to_vec(), StringFormat::Literal),
        "Kids" => vec![Object::Reference(person_name)],
    }));

    // Combo box (dropdown).
    let country_ap = ap_stream(&mut doc, 120.0, 20.0);
    let country = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "FT" => "Ch",
        "Ff" => 131072i64, // combo
        "T" => Object::String(b"country".to_vec(), StringFormat::Literal),
        "Opt" => Object::Array(vec![
            Object::String(b"TR".to_vec(), StringFormat::Literal),
            Object::String(b"US".to_vec(), StringFormat::Literal),
            Object::String(b"DE".to_vec(), StringFormat::Literal),
        ]),
        "V" => Object::String(b"TR".to_vec(), StringFormat::Literal),
        "Rect" => vec![
            Object::Real(72.0),
            Object::Real(600.0),
            Object::Real(192.0),
            Object::Real(620.0),
        ],
        "P" => Object::Reference(page_id),
        "AP" => dictionary! { "N" => Object::Reference(country_ap) },
    }));

    // Multi-select list box; options carry an export value and a label.
    let colors_ap = ap_stream(&mut doc, 120.0, 60.0);
    let colors = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "FT" => "Ch",
        "Ff" => 2097152i64, // multi select
        "T" => Object::String(b"colors".to_vec(), StringFormat::Literal),
        "Opt" => Object::Array(vec![
            Object::Array(vec![
                Object::String(b"red".to_vec(), StringFormat::Literal),
                Object::String(b"Red".to_vec(), StringFormat::Literal),
            ]),
            Object::Array(vec![
                Object::String(b"green".to_vec(), StringFormat::Literal),
                Object::String(b"Green".to_vec(), StringFormat::Literal),
            ]),
        ]),
        "V" => Object::String(b"red".to_vec(), StringFormat::Literal),
        "Rect" => vec![
            Object::Real(300.0),
            Object::Real(560.0),
            Object::Real(420.0),
            Object::Real(620.0),
        ],
        "P" => Object::Reference(page_id),
        "AP" => dictionary! { "N" => Object::Reference(colors_ap) },
    }));

    // Checkbox with explicit Off/Yes appearance states.
    let check_off = ap_stream(&mut doc, 14.0, 14.0);
    let check_on = ap_stream(&mut doc, 14.0, 14.0);
    let agree = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "FT" => "Btn",
        "T" => Object::String(b"agree".to_vec(), StringFormat::Literal),
        "V" => Object::Name(b"Off".to_vec()),
        "Rect" => vec![
            Object::Real(72.0),
            Object::Real(540.0),
            Object::Real(86.0),
            Object::Real(554.0),
        ],
        "P" => Object::Reference(page_id),
        "AP" => dictionary! {
            "N" => dictionary! {
                "Off" => Object::Reference(check_off),
                "Yes" => Object::Reference(check_on),
            },
        },
    }));

    // Radio group: parent holds /V, kids declare one export state each.
    let male_off = ap_stream(&mut doc, 14.0, 14.0);
    let male_on = ap_stream(&mut doc, 14.0, 14.0);
    let female_off = ap_stream(&mut doc, 14.0, 14.0);
    let female_on = ap_stream(&mut doc, 14.0, 14.0);
    let male = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "Rect" => vec![
            Object::Real(72.0),
            Object::Real(500.0),
            Object::Real(86.0),
            Object::Real(514.0),
        ],
        "P" => Object::Reference(page_id),
        "AP" => dictionary! {
            "N" => dictionary! {
                "Off" => Object::Reference(male_off),
                "Male" => Object::Reference(male_on),
            },
        },
    }));
    let female = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "Rect" => vec![
            Object::Real(120.0),
            Object::Real(500.0),
            Object::Real(134.0),
            Object::Real(514.0),
        ],
        "P" => Object::Reference(page_id),
        "AP" => dictionary! {
            "N" => dictionary! {
                "Off" => Object::Reference(female_off),
                "Female" => Object::Reference(female_on),
            },
        },
    }));
    let gender = doc.add_object(Object::Dictionary(dictionary! {
        "FT" => "Btn",
        "Ff" => 32768i64, // radio
        "T" => Object::String(b"gender".to_vec(), StringFormat::Literal),
        "V" => Object::Name(b"Male".to_vec()),
        "Kids" => vec![Object::Reference(male), Object::Reference(female)],
    }));

    // Password text field: /V keeps the value, the appearance must mask it.
    let pin_ap = ap_stream(&mut doc, 120.0, 20.0);
    let pin = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "FT" => "Tx",
        "Ff" => 8192i64, // password
        "T" => Object::String(b"pin".to_vec(), StringFormat::Literal),
        "V" => Object::String(b"".to_vec(), StringFormat::Literal),
        "Rect" => vec![
            Object::Real(300.0),
            Object::Real(470.0),
            Object::Real(420.0),
            Object::Real(490.0),
        ],
        "P" => Object::Reference(page_id),
        "AP" => dictionary! { "N" => Object::Reference(pin_ap) },
    }));

    // A field whose name hints at a date; validation may only warn about its
    // shape, never claim to have checked a JavaScript mask.
    let signup_date = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "FT" => "Tx",
        "T" => Object::String(b"signup_date".to_vec(), StringFormat::Literal),
        "V" => Object::String(b"01.01.2024".to_vec(), StringFormat::Literal),
        "Rect" => vec![
            Object::Real(72.0),
            Object::Real(430.0),
            Object::Real(192.0),
            Object::Real(450.0),
        ],
        "P" => Object::Reference(page_id),
    }));

    // Push button: never fillable.
    let submit = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "FT" => "Btn",
        "Ff" => 65536i64, // push button
        "T" => Object::String(b"submit".to_vec(), StringFormat::Literal),
        "Rect" => vec![
            Object::Real(72.0),
            Object::Real(460.0),
            Object::Real(192.0),
            Object::Real(480.0),
        ],
        "P" => Object::Reference(page_id),
    }));

    doc.get_object_mut(page_id).unwrap().as_dict_mut().unwrap().set(
        "Annots",
        Object::Array(vec![
            Object::Reference(text_widget),
            Object::Reference(person_name),
            Object::Reference(country),
            Object::Reference(colors),
            Object::Reference(agree),
            Object::Reference(male),
            Object::Reference(female),
            Object::Reference(pin),
            Object::Reference(signup_date),
            Object::Reference(submit),
        ]),
    );
    // A /Tabs number tree gives the first two annotations an explicit order.
    doc.get_object_mut(page_id).unwrap().as_dict_mut().unwrap().set(
        "Tabs",
        Object::Dictionary(dictionary! {
            "Nums" => vec![
                Object::Integer(0),
                Object::Reference(text_widget),
                Object::Integer(1),
                Object::Reference(country),
            ],
        }),
    );

    let font = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    }));
    let acro = doc.add_object(Object::Dictionary(dictionary! {
        "Fields" => vec![
            Object::Reference(text_widget),
            Object::Reference(person),
            Object::Reference(country),
            Object::Reference(colors),
            Object::Reference(agree),
            Object::Reference(gender),
            Object::Reference(pin),
            Object::Reference(signup_date),
            Object::Reference(submit),
        ],
        "DR" => dictionary! { "Font" => dictionary! { "Helv" => Object::Reference(font) } },
        "DA" => Object::String(b"/Helv 0 Tf 0 g".to_vec(), StringFormat::Literal),
        "NeedAppearances" => false,
    }));
    let pages_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Pages",
        "Kids" => vec![Object::Reference(page_id)],
        "Count" => 1i64,
    }));
    doc.get_object_mut(page_id).unwrap().as_dict_mut().unwrap().set("Parent", Object::Reference(pages_id));
    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference(pages_id),
        "AcroForm" => Object::Reference(acro),
    }));
    doc.trailer.set("Root", Object::Reference(catalog_id));
    doc
}

/// Writes `doc` to a temp file and returns its bytes plus the directory guard.
fn doc_bytes(label: &str, mut doc: Document) -> (TestDir, Vec<u8>) {
    let dir = TestDir::new();
    let path = dir.path(label);
    write_doc(&mut doc, &path);
    let bytes = fs::read(&path).expect("read fixture");
    (dir, bytes)
}

fn field_dict(doc: &Document, name: &str) -> Option<Dictionary> {
    doc.objects.values().find_map(|object| {
        let dict = object.as_dict().ok()?;
        let value = dict.get(b"T").ok()?;
        if pdfcore::docutil::pdf_text_value(value).as_deref() == Some(name) {
            Some(dict.clone())
        } else {
            None
        }
    })
}

fn field_ref(doc: &Document, name: &str) -> Option<lopdf::ObjectId> {
    doc.objects.iter().find_map(|(id, object)| {
        let dict = object.as_dict().ok()?;
        let value = dict.get(b"T").ok()?;
        if pdfcore::docutil::pdf_text_value(value).as_deref() == Some(name) {
            Some(*id)
        } else {
            None
        }
    })
}

/// Decompressed generated `/AP /N` content of a merged field+widget.
fn appearance_content(doc: &Document, name: &str) -> String {
    let widget_id = field_ref(doc, name).expect("field");
    let widget = doc.get_dictionary(widget_id).unwrap();
    let ap = widget.get(b"AP").unwrap().as_dict().unwrap();
    let stream_id = ap.get(b"N").unwrap().as_reference().unwrap();
    match doc.get_object(stream_id).unwrap() {
        Object::Stream(stream) => String::from_utf8_lossy(&stream.decompressed_content().unwrap()).to_string(),
        other => panic!("AP /N is not a stream: {other:?}"),
    }
}

fn value_of(dict: &Dictionary) -> Vec<String> {
    match dict.get(b"V").ok() {
        Some(Object::String(bytes, _)) => vec![String::from_utf8_lossy(bytes).to_string()],
        Some(Object::Name(bytes)) => vec![String::from_utf8_lossy(bytes).to_string()],
        Some(Object::Array(items)) => items.iter().filter_map(pdfcore::docutil::pdf_text_value).collect(),
        _ => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// list_fields
// ---------------------------------------------------------------------------

#[test]
fn forms_list_reports_types_flags_values_and_options() {
    let doc = form_doc();
    let fields = list_fields(&doc);
    let find = |name: &str| fields.iter().find(|field| field.name == name).expect(name);

    let text = find("full_name");
    assert_eq!(text.field_type, "text");
    assert!(text.required, "Ff bit 2 must map to required");
    assert_eq!(text.max_length, Some(10));
    assert_eq!(text.tooltip.as_deref(), Some("Full name"));
    assert_eq!(text.value, "Ada");
    assert!(!text.multiline);
    assert_eq!(text.page, Some(1));
    assert_eq!(text.rect, Some([72.0, 700.0, 272.0, 720.0]));
    assert_eq!(text.tab_order, Some(0), "the /Tabs number tree decides the order");

    let inherited = find("person.name");
    assert_eq!(inherited.field_type, "text");
    assert_eq!(inherited.value, "Grace", "/V is inherited from the parent field");

    let country = find("country");
    assert_eq!(country.field_type, "choice");
    assert!(country.combo);
    assert_eq!(country.options.iter().map(|option| option.value.as_str()).collect::<Vec<_>>(), vec!["TR", "US", "DE"]);
    assert_eq!(country.value, "TR");
    assert_eq!(country.tab_order, Some(1));

    let colors = find("colors");
    assert!(colors.multi_select);
    assert_eq!(colors.options.iter().map(|option| option.label.as_str()).collect::<Vec<_>>(), vec!["Red", "Green"]);

    let agree = find("agree");
    assert_eq!(agree.field_type, "checkbox");
    assert_eq!(agree.options.iter().map(|option| option.value.as_str()).collect::<Vec<_>>(), vec!["Yes"]);
    assert_eq!(agree.value, "Off");

    let gender = find("gender");
    assert_eq!(gender.field_type, "radio");
    assert_eq!(
        gender.options.iter().map(|option| option.value.as_str()).collect::<Vec<_>>(),
        vec!["Male", "Female"],
        "each radio widget contributes its /AP /N state as an export value"
    );
    assert_eq!(gender.value, "Male");
    assert_eq!(gender.widget_count, 2);
    assert_eq!(gender.tab_order, None, "widgets outside /Tabs fall back to annotation order");

    let submit = find("submit");
    assert_eq!(submit.field_type, "pushbutton");
}

// ---------------------------------------------------------------------------
// fill_fields
// ---------------------------------------------------------------------------

#[test]
fn forms_fill_sets_values_and_regenerates_appearances() {
    let (_dir, input) = doc_bytes("fill-in.pdf", form_doc());
    let values = vec![
        FieldValue { name: "full_name".into(), value: "Ada Lovelace".into(), values: vec![] },
        FieldValue { name: "person.name".into(), value: "Grace Hopper".into(), values: vec![] },
        FieldValue { name: "country".into(), value: "DE".into(), values: vec![] },
        FieldValue { name: "colors".into(), value: String::new(), values: vec!["green".into()] },
        FieldValue { name: "agree".into(), value: "true".into(), values: vec![] },
        FieldValue { name: "gender".into(), value: "Female".into(), values: vec![] },
        FieldValue { name: "pin".into(), value: "1234".into(), values: vec![] },
        FieldValue { name: "submit".into(), value: "go".into(), values: vec![] },
    ];
    let output = fill_fields(&input, &values).expect("fill");
    let filled = Document::load_mem(&output).expect("reopen");

    let text = field_dict(&filled, "full_name").unwrap();
    assert_eq!(value_of(&text), vec!["Ada Lovelace"], "/V must carry the full value");
    assert!(text.get(b"DV").is_err(), "/DV is left untouched");

    // The widget appearance must have been regenerated with the new value.
    let content = appearance_content(&filled, "full_name");
    assert!(content.contains("Ada Lovelace"), "appearance must draw the value: {content}");

    // Password field: real value in /V, masked appearance.
    assert_eq!(value_of(&field_dict(&filled, "pin").unwrap()), vec!["1234"]);
    let pin_appearance = appearance_content(&filled, "pin");
    assert!(pin_appearance.contains("****"), "password must be masked: {pin_appearance}");
    assert!(!pin_appearance.contains("1234"), "the password must not be drawn: {pin_appearance}");

    // Inherited field: the value moves onto the terminal child.
    assert_eq!(value_of(&field_dict(&filled, "name").unwrap()), vec!["Grace Hopper"]);

    assert_eq!(value_of(&field_dict(&filled, "country").unwrap()), vec!["DE"]);

    // Multi-select choice stores an array value.
    let colors = field_dict(&filled, "colors").unwrap();
    assert_eq!(value_of(&colors), vec!["green"]);

    // Checkbox: /V is the declared state, /AS matches, and the /AP /N became
    // a state dictionary again (not a bare stream).
    let agree = field_dict(&filled, "agree").unwrap();
    assert_eq!(value_of(&agree), vec!["Yes"]);
    assert_eq!(agree.get(b"AS").unwrap().as_name().unwrap(), b"Yes".as_slice());
    let agree_ap = agree.get(b"AP").unwrap().as_dict().unwrap();
    let agree_states = agree_ap.get(b"N").unwrap().as_dict().unwrap();
    assert!(agree_states.get(b"Off").is_ok() && agree_states.get(b"Yes").is_ok());

    // Radio group: group /V and per-widget /AS.
    let gender = field_dict(&filled, "gender").unwrap();
    assert_eq!(value_of(&gender), vec!["Female"]);
    let female_id = gender
        .get(b"Kids")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|kid| kid.as_reference().ok())
        .find(|id| {
            filled
                .get_dictionary(*id)
                .ok()
                .and_then(|dict| dict.get(b"AS").ok())
                .and_then(|value| value.as_name().ok())
                .map(|name| name == b"Female")
                .unwrap_or(false)
        })
        .expect("the female widget must be the selected one");
    assert!(female_id.0 > 0);

    // /NeedAppearances is the compatibility safety net.
    let acro = filled.catalog().unwrap().get(b"AcroForm").unwrap().as_reference().unwrap();
    assert!(filled.get_dictionary(acro).unwrap().get(b"NeedAppearances").unwrap().as_bool().unwrap());
}

#[test]
fn forms_fill_reports_skips_and_warnings_without_faking() {
    let (_dir, input) = doc_bytes("fill-skip.pdf", form_doc());
    let values = vec![
        FieldValue { name: "full_name".into(), value: "This is much too long".into(), values: vec![] },
        FieldValue { name: "submit".into(), value: "go".into(), values: vec![] },
    ];
    let report = pdfcore::forms::apply_field_values(&mut Document::load_mem(&input).unwrap(), &values).expect("fill");
    assert_eq!(report.filled, 1);
    assert!(report.skipped.iter().any(|entry| entry.contains("push button")));
    assert!(report.warnings.iter().any(|entry| entry.contains("MaxLen")));
}

// ---------------------------------------------------------------------------
// validate_fields
// ---------------------------------------------------------------------------

#[test]
fn forms_validate_reports_only_provable_problems() {
    let doc = form_doc();
    let values = vec![
        FieldValue { name: "full_name".into(), value: String::new(), values: vec![] },
        FieldValue { name: "country".into(), value: "XX".into(), values: vec![] },
        FieldValue { name: "agree".into(), value: "maybe".into(), values: vec![] },
        FieldValue { name: "signup_date".into(), value: "31.31.2020".into(), values: vec![] },
        FieldValue { name: "missing".into(), value: "x".into(), values: vec![] },
    ];
    let issues = pdfcore::forms::validate_fields(&doc, &values);
    let codes: Vec<(String, String)> = issues.iter().map(|issue| (issue.field.clone(), issue.code.clone())).collect();

    assert!(codes.contains(&("full_name".into(), "required".into())), "empty provided value fails the required field");
    assert!(codes.iter().any(|(field, code)| field == "country" && code == "option_not_in_list"));
    assert!(codes.iter().any(|(field, code)| field == "agree" && code == "option_not_in_list"));
    let date = issues.iter().find(|issue| issue.field == "signup_date").expect("date heuristic");
    assert_eq!(date.code, "invalid_date");
    assert_eq!(date.severity, "warning", "name-based date checks are warnings, not errors");
    assert!(codes.iter().any(|(field, code)| field == "missing" && code == "unknown_field"));
    assert!(
        issues.iter().all(|issue| issue.code != "scripted_format" || issue.field == "signup_date"),
        "no field declares /AA in this fixture"
    );
}

#[test]
fn forms_validate_reports_max_length_as_an_error() {
    let doc = form_doc();
    let values = vec![FieldValue { name: "full_name".into(), value: "This is far too long".into(), values: vec![] }];
    let issues = pdfcore::forms::validate_fields(&doc, &values);
    let max_length =
        issues.iter().find(|issue| issue.field == "full_name" && issue.code == "max_length").expect("max length issue");
    assert_eq!(max_length.severity, "error");
    assert!(max_length.message.contains("10"), "the declared limit is reported: {}", max_length.message);
}

// ---------------------------------------------------------------------------
// error behavior
// ---------------------------------------------------------------------------

#[test]
fn forms_fill_unknown_field_errors_cleanly() {
    let (_dir, input) = doc_bytes("fill-unknown.pdf", form_doc());
    let result =
        fill_fields(&input, &[FieldValue { name: "does_not_exist".into(), value: "x".into(), values: vec![] }]);
    match result {
        Err(pdfcore::PdfError::InvalidInput(message)) => {
            assert!(message.contains("does_not_exist"), "the message names the field: {message}");
        }
        other => panic!("expected InvalidInput, got {other:?}"),
    }
}

#[test]
fn forms_encrypted_pdf_reports_password_required() {
    let dir = TestDir::new();
    let source = dir.path("plain.pdf");
    write_doc(&mut form_doc(), &source);
    let protected = dir.path("protected.pdf");
    let options = ProtectOptions {
        user_password: "user-pass".into(),
        owner_password: "owner-pass".into(),
        allow_printing: true,
        allow_copying: false,
        allow_editing: false,
        allow_commenting: true,
    };
    protect_pdf(&source, &protected, &options, OverwritePolicy::Replace, None).expect("protect");
    let result = list_fields_in_file(&protected, None);
    match result {
        Err(pdfcore::PdfError::PasswordRequired) => {}
        other => panic!("expected PasswordRequired, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Page objects
// ---------------------------------------------------------------------------

/// One page with a text annotation, a text-field widget and a drawn image.
fn object_doc() -> Document {
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
    let image_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => 2i64,
            "Height" => 2i64,
            "ColorSpace" => "DeviceRGB",
            "BitsPerComponent" => 8i64,
        },
        vec![255u8, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0],
    )));
    let content = b"q\n1 0 0 1 10 20 cm\n/Im0 Do\nQ\n0.5 0.5 0.5 rg\n10 10 100 100 re f\n";
    let content_id = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), content.to_vec())));
    {
        let page = doc.get_object_mut(page_id).unwrap().as_dict_mut().unwrap();
        page.set("Contents", Object::Reference(content_id));
        page.set(
            "Resources",
            Object::Dictionary(dictionary! {
                "XObject" => dictionary! { "Im0" => Object::Reference(image_id) },
            }),
        );
    }

    let text_annot = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Text",
        "Contents" => Object::String(b"hello".to_vec(), StringFormat::Literal),
        "Rect" => vec![
            Object::Real(50.0),
            Object::Real(50.0),
            Object::Real(150.0),
            Object::Real(70.0),
        ],
        "P" => Object::Reference(page_id),
    }));
    let widget = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "FT" => "Tx",
        "T" => Object::String(b"w1".to_vec(), StringFormat::Literal),
        "Rect" => vec![
            Object::Real(200.0),
            Object::Real(200.0),
            Object::Real(300.0),
            Object::Real(220.0),
        ],
        "P" => Object::Reference(page_id),
    }));
    doc.get_object_mut(page_id)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("Annots", Object::Array(vec![Object::Reference(text_annot), Object::Reference(widget)]));

    let acro = doc.add_object(Object::Dictionary(dictionary! {
        "Fields" => vec![Object::Reference(widget)],
    }));
    let pages_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Pages",
        "Kids" => vec![Object::Reference(page_id)],
        "Count" => 1i64,
    }));
    doc.get_object_mut(page_id).unwrap().as_dict_mut().unwrap().set("Parent", Object::Reference(pages_id));
    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference(pages_id),
        "AcroForm" => Object::Reference(acro),
    }));
    doc.trailer.set("Root", Object::Reference(catalog_id));
    doc
}

#[test]
fn objects_list_annotations_widgets_and_image_placements() {
    let doc = object_doc();
    let objects = list_page_objects(&doc);
    assert_eq!(objects.len(), 3, "two annotations plus one image draw");

    assert_eq!(objects[0].kind, "annotation");
    assert_eq!(objects[0].subtype, "Text");
    assert_eq!(objects[0].rect, [50.0, 50.0, 150.0, 70.0]);
    assert_eq!(objects[0].contents, "hello");
    assert_eq!(objects[0].page, 1);

    assert_eq!(objects[1].kind, "widget");
    assert_eq!(objects[1].field_name.as_deref(), Some("w1"));

    assert_eq!(objects[2].kind, "image");
    assert_eq!(objects[2].resource_name.as_deref(), Some("Im0"));
    assert_eq!(objects[2].matrix, Some([1.0, 0.0, 0.0, 1.0, 10.0, 20.0]));
    assert_eq!(objects[2].rect, [10.0, 20.0, 11.0, 21.0], "the bbox of the placed unit square");
}

#[test]
fn objects_move_rewrites_only_the_matching_matrix() {
    let (_dir, input) = doc_bytes("objects.pdf", object_doc());
    let output =
        transform_objects(&input, &[ObjectEdit { page: 1, index: 2, action: ObjectAction::Move { dx: 5.0, dy: 7.0 } }])
            .expect("transform");
    let doc = Document::load_mem(&output).expect("reopen");
    let page_id = doc.get_pages().get(&1).copied().unwrap();
    let content = String::from_utf8_lossy(&doc.get_page_content(page_id)).to_string();
    // Byte-for-byte identical to the input except the six matrix operands.
    let input_doc = Document::load_mem(&input).expect("reopen input");
    let input_page = input_doc.get_pages().get(&1).copied().unwrap();
    let original = String::from_utf8_lossy(&input_doc.get_page_content(input_page)).to_string();
    assert!(original.contains("0.5 0.5 0.5 rg") && original.contains("10 10 100 100 re f"));
    let expected = original.replace("1 0 0 1 10 20 cm", "1 0 0 1 15 27 cm");
    assert_eq!(content, expected);
}

#[test]
fn objects_resize_rotate_and_delete_do_exactly_that() {
    let (_dir, input) = doc_bytes("objects-edit.pdf", object_doc());

    // Resize the text annotation.
    let resized = transform_objects(
        &input,
        &[ObjectEdit { page: 1, index: 0, action: ObjectAction::Resize { rect: [60.0, 60.0, 160.0, 80.0] } }],
    )
    .expect("resize");
    let doc = Document::load_mem(&resized).unwrap();
    let annot = doc
        .objects
        .values()
        .find_map(|object| {
            let dict = object.as_dict().ok()?;
            (dict.get(b"Subtype").ok().and_then(|value| value.as_name().ok()) == Some(b"Text".as_slice()))
                .then(|| dict.clone())
        })
        .unwrap();
    let rect = annot.get(b"Rect").unwrap().as_array().unwrap();
    let values: Vec<f32> = rect.iter().filter_map(|item| item.as_float().ok()).collect();
    assert_eq!(values, vec![60.0, 60.0, 160.0, 80.0]);

    // Rotate it 90 degrees: the rect is reshaped around the same center.
    let rotated =
        transform_objects(&input, &[ObjectEdit { page: 1, index: 0, action: ObjectAction::Rotate { degrees: 90.0 } }])
            .expect("rotate");
    let doc = Document::load_mem(&rotated).unwrap();
    let annot = doc
        .objects
        .values()
        .find_map(|object| {
            let dict = object.as_dict().ok()?;
            (dict.get(b"Subtype").ok().and_then(|value| value.as_name().ok()) == Some(b"Text".as_slice()))
                .then(|| dict.clone())
        })
        .unwrap();
    let rect = annot.get(b"Rect").unwrap().as_array().unwrap();
    let values: Vec<f32> = rect.iter().filter_map(|item| item.as_float().ok()).collect();
    assert_eq!(values, vec![90.0, 10.0, 110.0, 110.0], "w/h swap around the original center");

    // Delete the widget: gone from /Annots and from the AcroForm field tree.
    let deleted =
        transform_objects(&input, &[ObjectEdit { page: 1, index: 1, action: ObjectAction::Delete }]).expect("delete");
    let doc = Document::load_mem(&deleted).unwrap();
    let page_id = doc.get_pages().get(&1).copied().unwrap();
    let annots = doc.get_dictionary(page_id).unwrap().get(b"Annots").unwrap().as_array().unwrap();
    assert_eq!(annots.len(), 1);
    assert!(
        doc.catalog().unwrap().get(b"AcroForm").is_err(),
        "an AcroForm with no fields left must be dropped, like flattening does"
    );
}

#[test]
fn objects_unknown_index_errors_without_panic() {
    let (_dir, input) = doc_bytes("objects-bad.pdf", object_doc());
    let result = transform_objects(&input, &[ObjectEdit { page: 1, index: 99, action: ObjectAction::Delete }]);
    match result {
        Err(pdfcore::PdfError::InvalidInput(message)) => assert!(message.contains("index 99")),
        other => panic!("expected InvalidInput, got {other:?}"),
    }
}

#[test]
fn objects_apply_is_atomic_on_bad_edits() {
    let (_dir, input) = doc_bytes("objects-atomic.pdf", object_doc());
    let mut doc = Document::load_mem(&input).unwrap();
    // A valid first edit and an invalid second one: the whole request fails
    // and nothing is written.
    let edits = vec![
        ObjectEdit { page: 1, index: 0, action: ObjectAction::Move { dx: 5.0, dy: 5.0 } },
        ObjectEdit { page: 1, index: 7, action: ObjectAction::Delete },
    ];
    assert!(apply_object_edits(&mut doc, &edits).is_err());
    let annot = doc
        .objects
        .values()
        .find_map(|object| {
            let dict = object.as_dict().ok()?;
            (dict.get(b"Subtype").ok().and_then(|value| value.as_name().ok()) == Some(b"Text".as_slice()))
                .then(|| dict.clone())
        })
        .unwrap();
    let rect = annot.get(b"Rect").unwrap().as_array().unwrap();
    let values: Vec<f32> = rect.iter().filter_map(|item| item.as_float().ok()).collect();
    assert_eq!(values, vec![50.0, 50.0, 150.0, 70.0], "the failed request must not mutate anything");
}
