//! AcroForm field builder: creates real interactive form fields (`/Tx`, `/Btn`,
//! `/Ch`, `/Sig`) as merged Widget/Field dictionaries on PDF pages.

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, StringFormat};
use serde::{Deserialize, Serialize};

use crate::error::{PdfError, PdfResult};

const FLAG_REQUIRED: i64 = 2;
const FLAG_COMBO: i64 = 1 << 17;

/// Specification for a new interactive AcroForm field to add to a PDF page.
#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewFormField {
    pub name: String,
    /// `text` | `checkbox` | `choice` | `signature`.
    pub field_type: String,
    /// 1-based page number.
    pub page: u32,
    /// Page-space rectangle `[x0, y0, x1, y1]` (bottom-left origin, points).
    #[cfg_attr(feature = "bindings", specta(type = [specta_typescript::Number; 4]))]
    pub rect: [f64; 4],
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default_value: Option<String>,
    #[serde(default)]
    pub options: Vec<String>,
}

fn ensure_acroform(doc: &mut Document) -> PdfResult<ObjectId> {
    let catalog_id = doc
        .trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .map_err(|_| PdfError::Internal("PDF has no catalog Root".into()))?;

    let existing = doc.get_dictionary(catalog_id).ok().and_then(|cat| cat.get(b"AcroForm").ok().cloned());

    match existing {
        Some(Object::Reference(id)) => Ok(id),
        Some(Object::Dictionary(mut dict)) => {
            if !dict.has(b"Fields") {
                dict.set("Fields", Object::Array(Vec::new()));
            }
            if !dict.has(b"NeedAppearances") {
                dict.set("NeedAppearances", Object::Boolean(true));
            }
            let id = doc.add_object(Object::Dictionary(dict));
            if let Ok(Object::Dictionary(cat)) = doc.get_object_mut(catalog_id) {
                cat.set("AcroForm", Object::Reference(id));
            }
            Ok(id)
        }
        _ => {
            let acro = dictionary! {
                "Fields" => Object::Array(Vec::new()),
                "NeedAppearances" => Object::Boolean(true),
                "DA" => Object::String(b"/Helv 11 Tf 0 g".to_vec(), StringFormat::Literal),
            };
            let id = doc.add_object(Object::Dictionary(acro));
            if let Ok(Object::Dictionary(cat)) = doc.get_object_mut(catalog_id) {
                cat.set("AcroForm", Object::Reference(id));
            }
            Ok(id)
        }
    }
}

fn append_to_page_annots(doc: &mut Document, page_id: ObjectId, widget_id: ObjectId) -> PdfResult<()> {
    let annots_ref = doc.get_dictionary(page_id).ok().and_then(|page| page.get(b"Annots").ok().cloned());

    match annots_ref {
        Some(Object::Reference(arr_id)) => {
            if let Ok(Object::Array(arr)) = doc.get_object_mut(arr_id) {
                arr.push(Object::Reference(widget_id));
            }
        }
        Some(Object::Array(mut arr)) => {
            arr.push(Object::Reference(widget_id));
            if let Ok(Object::Dictionary(page)) = doc.get_object_mut(page_id) {
                page.set("Annots", Object::Array(arr));
            }
        }
        _ => {
            if let Ok(Object::Dictionary(page)) = doc.get_object_mut(page_id) {
                page.set("Annots", Object::Array(vec![Object::Reference(widget_id)]));
            }
        }
    }
    Ok(())
}

/// Adds the requested interactive form fields to `doc` and returns the count added.
pub fn create_form_fields(doc: &mut Document, fields: &[NewFormField]) -> PdfResult<usize> {
    if fields.is_empty() {
        return Ok(0);
    }
    let pages = doc.get_pages();
    let acroform_id = ensure_acroform(doc)?;
    let mut created = 0usize;

    for spec in fields {
        let name = spec.name.trim();
        if name.is_empty() {
            continue;
        }
        let page_id =
            *pages.get(&spec.page).ok_or_else(|| PdfError::Internal(format!("page {} does not exist", spec.page)))?;

        let [x0, y0, x1, y1] = spec.rect;
        let rect = vec![
            Object::Real(x0.min(x1) as f32),
            Object::Real(y0.min(y1) as f32),
            Object::Real(x0.max(x1) as f32),
            Object::Real(y0.max(y1) as f32),
        ];

        let mut flags = if spec.required { FLAG_REQUIRED } else { 0 };
        let ft = match spec.field_type.as_str() {
            "checkbox" => "Btn",
            "choice" => {
                flags |= FLAG_COMBO;
                "Ch"
            }
            "signature" => "Sig",
            _ => "Tx",
        };

        let mut dict = Dictionary::new();
        dict.set("Type", Object::Name(b"Annot".to_vec()));
        dict.set("Subtype", Object::Name(b"Widget".to_vec()));
        dict.set("FT", Object::Name(ft.as_bytes().to_vec()));
        dict.set("T", Object::String(name.as_bytes().to_vec(), StringFormat::Literal));
        dict.set("Rect", Object::Array(rect));
        dict.set("P", Object::Reference(page_id));
        dict.set("F", Object::Integer(4));
        if flags != 0 {
            dict.set("Ff", Object::Integer(flags));
        }
        dict.set("DA", Object::String(b"/Helv 11 Tf 0 g".to_vec(), StringFormat::Literal));
        dict.set("BS", Object::Dictionary(dictionary! { "W" => 1, "S" => Object::Name(b"S".to_vec()) }));
        dict.set(
            "MK",
            Object::Dictionary(dictionary! {
                "BC" => vec![Object::Real(0.25), Object::Real(0.30), Object::Real(0.38)],
                "BG" => vec![Object::Real(0.98), Object::Real(0.98), Object::Real(1.0)],
            }),
        );

        if ft == "Btn" {
            let checked = spec.default_value.as_deref().is_some_and(|v| v == "Yes" || v == "true" || v == "On");
            let state = if checked { b"Yes".to_vec() } else { b"Off".to_vec() };
            dict.set("V", Object::Name(state.clone()));
            dict.set("AS", Object::Name(state));
        } else if let Some(val) = spec.default_value.as_deref().filter(|v| !v.is_empty()) {
            dict.set("V", Object::String(val.as_bytes().to_vec(), StringFormat::Literal));
            dict.set("DV", Object::String(val.as_bytes().to_vec(), StringFormat::Literal));
        }

        if ft == "Ch" && !spec.options.is_empty() {
            let opts: Vec<Object> =
                spec.options.iter().map(|opt| Object::String(opt.as_bytes().to_vec(), StringFormat::Literal)).collect();
            dict.set("Opt", Object::Array(opts));
        }

        let widget_id = doc.add_object(Object::Dictionary(dict));
        append_to_page_annots(doc, page_id, widget_id)?;

        if let Ok(Object::Dictionary(acro)) = doc.get_object_mut(acroform_id) {
            if let Ok(Object::Array(arr)) = acro.get_mut(b"Fields") {
                arr.push(Object::Reference(widget_id));
            } else {
                acro.set("Fields", Object::Array(vec![Object::Reference(widget_id)]));
            }
            acro.set("NeedAppearances", Object::Boolean(true));
        }
        created += 1;
    }

    Ok(created)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forms::list_fields;

    fn blank_pdf() -> Document {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Count" => 1,
                "Kids" => vec![page_id.into()],
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);
        doc
    }

    #[test]
    fn creates_text_checkbox_choice_and_signature_fields() {
        let mut doc = blank_pdf();
        let count = create_form_fields(
            &mut doc,
            &[
                NewFormField {
                    name: "fullName".into(),
                    field_type: "text".into(),
                    page: 1,
                    rect: [72.0, 700.0, 300.0, 724.0],
                    required: true,
                    default_value: Some("Ada".into()),
                    options: Vec::new(),
                },
                NewFormField {
                    name: "agree".into(),
                    field_type: "checkbox".into(),
                    page: 1,
                    rect: [72.0, 660.0, 88.0, 676.0],
                    required: false,
                    default_value: None,
                    options: Vec::new(),
                },
                NewFormField {
                    name: "city".into(),
                    field_type: "choice".into(),
                    page: 1,
                    rect: [72.0, 610.0, 220.0, 634.0],
                    required: false,
                    default_value: Some("Ankara".into()),
                    options: vec!["Ankara".into(), "Istanbul".into(), "Izmir".into()],
                },
                NewFormField {
                    name: "sig1".into(),
                    field_type: "signature".into(),
                    page: 1,
                    rect: [72.0, 520.0, 260.0, 570.0],
                    required: false,
                    default_value: None,
                    options: Vec::new(),
                },
            ],
        )
        .expect("create fields");

        assert_eq!(count, 4);
        let listed = list_fields(&doc);
        assert_eq!(listed.len(), 4);
        assert_eq!(listed[0].name, "fullName");
        assert_eq!(listed[0].field_type, "text");
        assert!(listed[0].required);
        assert_eq!(listed[0].value, "Ada");
        assert_eq!(listed[1].field_type, "checkbox");
        assert_eq!(listed[2].field_type, "choice");
        assert_eq!(listed[2].options.len(), 3);
        assert_eq!(listed[3].field_type, "signature");
    }
}
