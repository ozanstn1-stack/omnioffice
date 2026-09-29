//! Tools that bridge the office modules and the PDF engine: image <-> PDF
//! conversion, PDF text export and the PDF form creator.
//!
//! These complement the existing PDF tools; nothing here replaces them.

use crate::office::OfficeErrorPayload;
use lopdf::dictionary;
use officecore::error::OfficeError;
use pdfcore::docutil::OverwritePolicy;
use pdfcore::images::{ImageItem, ImageToPdfOptions};
use pdfcore::progress::{CancelToken, ProgressEvent};

fn silent(_event: ProgressEvent) {}
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::AppHandle;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ImagesToPdfRequest {
    pub images: Vec<String>,
    pub output: String,
    pub page_size: Option<String>,
    pub orientation: Option<String>,
    pub fit: Option<String>,
    pub margin_pt: Option<f64>,
    pub overwrite: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PdfToImagesRequest {
    pub input: String,
    pub output_dir: String,
    pub format: Option<String>,
    pub dpi: Option<u32>,
    pub jpeg_quality: Option<u8>,
    pub grayscale: Option<bool>,
    pub prefix: Option<String>,
    pub overwrite: Option<String>,
}

fn policy(value: &Option<String>) -> OverwritePolicy {
    match value.as_deref() {
        Some("replace") => OverwritePolicy::Replace,
        Some("unique_name") => OverwritePolicy::UniqueName,
        _ => OverwritePolicy::Error,
    }
}

fn payload(error: OfficeError) -> OfficeErrorPayload {
    error.into()
}

fn pdf_error(error: pdfcore::error::PdfError) -> OfficeErrorPayload {
    let fallback = error.to_string();
    match serde_json::to_value(&error) {
        Ok(value) => OfficeErrorPayload {
            code: value.get("code").and_then(|item| item.as_str()).unwrap_or("internal").to_string(),
            message: value.get("message").and_then(|item| item.as_str()).unwrap_or(&fallback).to_string(),
        },
        Err(_) => OfficeErrorPayload { code: "internal".into(), message: fallback },
    }
}

#[tauri::command]
pub async fn office_images_to_pdf(request: ImagesToPdfRequest) -> Result<String, OfficeErrorPayload> {
    let task = tauri::async_runtime::spawn_blocking(move || {
        if request.images.is_empty() {
            return Err(payload(OfficeError::invalid("Select at least one image.")));
        }
        let mut options = ImageToPdfOptions::default();
        if let Some(size) = request.page_size {
            options.page_size = size;
        }
        if let Some(orientation) = request.orientation {
            options.orientation = orientation;
        }
        if let Some(fit) = request.fit {
            options.fit = fit;
        }
        if let Some(margin) = request.margin_pt {
            options.margin_pt = margin;
        }
        let items: Vec<ImageItem> = request
            .images
            .iter()
            .map(|path| ImageItem { path: path.clone(), rotation_delta: 0 })
            .collect();
        let output = PathBuf::from(&request.output);
        let result = pdfcore::images::images_to_pdf(&items, &options, &output, policy(&request.overwrite), &silent, &CancelToken::new())
            .map_err(pdf_error)?;
        Ok(result.to_string_lossy().to_string())
    });
    task.await
        .map_err(|error| payload(OfficeError::internal(format!("worker thread failed: {error}"))))?
}

#[tauri::command]
pub async fn office_pdf_to_images(request: PdfToImagesRequest) -> Result<Vec<String>, OfficeErrorPayload> {
    let task = tauri::async_runtime::spawn_blocking(move || {
        let format = match request.format.as_deref() {
            Some("png") => pdfcore::images::ImageFormat::Png,
            _ => pdfcore::images::ImageFormat::Jpeg,
        };
        let result = pdfcore::convert::pdf_to_images(
            Path::new(&request.input),
            Path::new(&request.output_dir),
            format,
            request.dpi.unwrap_or(150),
            request.jpeg_quality.unwrap_or(85),
            request.grayscale.unwrap_or(false),
            request.prefix.as_deref().unwrap_or("page"),
            &[],
            policy(&request.overwrite),
            None,
            &silent,
            &CancelToken::new(),
        )
        .map_err(pdf_error)?;
        Ok(result.files.iter().map(|file| file.path.clone()).collect())
    });
    task.await
        .map_err(|error| payload(OfficeError::internal(format!("worker thread failed: {error}"))))?
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PdfTextRequest {
    pub input: String,
    pub output: String,
    pub password: Option<String>,
}

#[tauri::command]
pub async fn office_pdf_to_text(request: PdfTextRequest) -> Result<String, OfficeErrorPayload> {
    let task = tauri::async_runtime::spawn_blocking(move || {
        let input = Path::new(&request.input);
        let pages = pdfcore::render::page_geometries(input, request.password.as_deref()).map_err(pdf_error)?;
        let mut text = String::new();
        for page in &pages {
            let content = pdfcore::render::extract_page_text(input, request.password.as_deref(), page.page).map_err(pdf_error)?;
            text.push_str(&content);
            text.push_str("\n\n");
        }
        officecore::io::write_atomic(Path::new(&request.output), text.as_bytes()).map_err(payload)?;
        Ok(request.output)
    });
    task.await
        .map_err(|error| payload(OfficeError::internal(format!("worker thread failed: {error}"))))?
}

// ---------------------------------------------------------------------------
// PDF form creator (AcroForm)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PdfFormField {
    /// text | checkbox | radio | dropdown
    pub kind: String,
    pub name: String,
    pub page: u32,
    /// Position and size in PDF points, top-left origin.
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub value: String,
    pub options: Vec<String>,
    pub font_size: Option<f64>,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PdfFormRequest {
    pub input: String,
    pub output: String,
    pub fields: Vec<PdfFormField>,
    pub overwrite: Option<String>,
    pub password: Option<String>,
}

#[tauri::command]
pub async fn office_pdf_add_form(request: PdfFormRequest) -> Result<String, OfficeErrorPayload> {
    let task = tauri::async_runtime::spawn_blocking(move || {
        if request.fields.is_empty() {
            return Err(payload(OfficeError::invalid("Add at least one form field.")));
        }
        let mut document = pdfcore::docutil::load_document(Path::new(&request.input), request.password.as_deref())
            .map_err(pdf_error)?;
        let output = pdfcore::docutil::resolve_output_path(Path::new(&request.output), policy(&request.overwrite)).map_err(pdf_error)?;

        let mut field_refs: Vec<lopdf::Object> = Vec::new();
        for (index, field) in request.fields.iter().enumerate() {
            let page_id = document
                .get_pages()
                .get(&field.page.max(1))
                .copied()
                .ok_or_else(|| payload(OfficeError::invalid(format!("Page {} does not exist.", field.page))))?;
            let media = pdfcore::docutil::page_mediabox(&document, page_id).map_err(pdf_error)?;
            let page_height = media[3] - media[1];
            let rect = vec![
                field.x.into(),
                (page_height - field.y - field.h).into(),
                (field.x + field.w).into(),
                (page_height - field.y).into(),
            ];
            let field_kind = field.kind.as_str();
            let mut widget = lopdf::Dictionary::new();
            widget.set("Type", lopdf::Object::Name(b"Annot".to_vec()));
            widget.set("Subtype", lopdf::Object::Name(b"Widget".to_vec()));
            widget.set("Rect", lopdf::Object::Array(rect));
            widget.set("F", 4); // print
            widget.set("T", lopdf::Object::string_literal(field.name.clone()));
            widget.set("DA", lopdf::Object::string_literal(format!("/Helv {}", field.font_size.unwrap_or(11.0))));
            if field.required {
                widget.set("Ff", 2);
            }
            // Every arm that produces an appearance assigns it; the initial
            // None is only the fall-through value.
            #[allow(unused_assignments)]
            let mut appearance: Option<String> = None;
            match field_kind {
                "checkbox" => {
                    widget.set("FT", lopdf::Object::Name(b"Btn".to_vec()));
                    widget.set("V", lopdf::Object::Name(if field.value == "true" || field.value == "1" || field.value.eq_ignore_ascii_case("on") { b"Yes".to_vec() } else { b"Off".to_vec() }));
                    let width = field.w.min(field.h);
                    appearance = Some(format!(
                        "q\n0.9 0.9 0.9 rg\n{:.2} {:.2} {:.2} {:.2} re f\n0.2 0.2 0.2 RG\n1 w\n{:.2} {:.2} {:.2} {:.2} re S\nQ\n",
                        field.x + 1.0,
                        field.y + 1.0,
                        width - 2.0,
                        width - 2.0,
                        field.x + 0.5,
                        field.y + 0.5,
                        width - 1.0,
                        width - 1.0
                    ));
                }
                "radio" => {
                    widget.set("FT", lopdf::Object::Name(b"Btn".to_vec()));
                    widget.set("Ff", 32768);
                    appearance = Some(format!(
                        "q\n1 1 1 rg\n{:.2} {:.2} {:.2} {:.2} re f\n0.25 0.3 0.9 RG\n1 w\n{:.2} {:.2} m\n{:.2} {:.2} l\nS\nQ\n",
                        field.x,
                        field.y,
                        field.w,
                        field.w,
                        field.x + field.w * 0.3,
                        field.y + field.w * 0.55,
                        field.x + field.w * 0.75,
                        field.y + field.w * 0.2
                    ));
                }
                "dropdown" => {
                    widget.set("FT", lopdf::Object::Name(b"Ch".to_vec()));
                    let options: Vec<lopdf::Object> = field.options.iter().map(|value| lopdf::Object::string_literal(value.clone())).collect();
                    widget.set("Opt", lopdf::Object::Array(options));
                    if !field.value.is_empty() {
                        widget.set("V", lopdf::Object::string_literal(field.value.clone()));
                    }
                    appearance = Some(format!(
                        "q\n0.97 0.98 1 rg\n{:.2} {:.2} {:.2} {:.2} re f\n0.4 0.45 0.55 RG\n0.7 w\n{:.2} {:.2} {:.2} {:.2} re S\n0.3 0.35 0.45 rg\n{:.2} {:.2} m\n{:.2} {:.2} l\n{:.2} {:.2} l\nf\nQ\n",
                        field.x,
                        field.y,
                        field.w,
                        field.h,
                        field.x + 0.5,
                        field.y + 0.5,
                        field.w - 1.0,
                        field.h - 1.0,
                        field.x + field.w - 10.0,
                        field.y + field.h / 2.0 + 2.0,
                        field.x + field.w - 5.0,
                        field.y + field.h / 2.0 + 2.0,
                        field.x + field.w - 7.5,
                        field.y + field.h / 2.0 - 2.0
                    ));
                }
                _ => {
                    widget.set("FT", lopdf::Object::Name(b"Tx".to_vec()));
                    if !field.value.is_empty() {
                        widget.set("V", lopdf::Object::string_literal(field.value.clone()));
                    }
                    appearance = Some(format!(
                        "q\n1 1 1 rg\n{:.2} {:.2} {:.2} {:.2} re f\n0.45 0.5 0.6 RG\n0.7 w\n{:.2} {:.2} {:.2} {:.2} re S\nQ\n",
                        field.x,
                        field.y,
                        field.w,
                        field.h,
                        field.x + 0.5,
                        field.y + 0.5,
                        field.w - 1.0,
                        field.h - 1.0
                    ));
                }
            }
            if let Some(stream) = appearance {
                let stream_id = document.add_object(lopdf::Stream::new(
                    dictionary! {
                        "Type" => "XObject",
                        "Subtype" => "Form",
                        "BBox" => vec![0.into(), 0.into(), field.w.into(), field.h.into()],
                        "Resources" => dictionary! {},
                    },
                    stream.into_bytes(),
                ).with_compression(true));
                let mut resources = lopdf::Dictionary::new();
                resources.set("XObject", dictionary! { "FRM" => lopdf::Object::Reference(stream_id) });
                widget.set("AP", dictionary! { "N" => lopdf::Object::Reference(stream_id) });
                let _ = resources;
            }
            let id = document.add_object(widget);
            // Register the widget on its page.
            let page = document
                .get_dictionary_mut(page_id)
                .map_err(|error| payload(OfficeError::internal(error.to_string())))?;
            let annots = page.get_mut(b"Annots").ok();
            match annots {
                Some(lopdf::Object::Array(array)) => array.push(lopdf::Object::Reference(id)),
                _ => {
                    page.set("Annots", lopdf::Object::Array(vec![lopdf::Object::Reference(id)]));
                }
            }
            field_refs.push(lopdf::Object::Reference(id));
            let _ = index;
        }

        // AcroForm dictionary with the standard Helvetica font for appearances.
        let font_id = document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => "WinAnsiEncoding",
        });
        let acro_form = dictionary! {
            "Fields" => lopdf::Object::Array(field_refs),
            "DA" => lopdf::Object::string_literal("/Helv 0 Tf 0 g"),
            "DR" => dictionary! {
                "Font" => dictionary! { "Helv" => lopdf::Object::Reference(font_id) },
            },
            "NeedAppearances" => true,
        };
        let catalog_id = match document.trailer.get(b"Root") {
            Ok(lopdf::Object::Reference(id)) => *id,
            _ => return Err(payload(OfficeError::internal("The PDF has no catalog."))),
        };
        if let Ok(catalog) = document.get_dictionary_mut(catalog_id) {
            catalog.set("AcroForm", lopdf::Object::Dictionary(acro_form));
        }
        pdfcore::docutil::save_document(&mut document, &output, true).map_err(pdf_error)?;
        Ok(output.to_string_lossy().to_string())
    });
    task.await
        .map_err(|error| payload(OfficeError::internal(format!("worker thread failed: {error}"))))?
}

/// Reading form fields back out of a PDF (used for previews and batch flows).
///
/// Delegates to `pdfcore::forms::list_fields`, which walks the AcroForm field
/// tree properly (parent `/Kids`, `/FT` and `/V` inheritance, widget state
/// dictionaries) instead of the earlier shallow read that reported every
/// field as page 1, dropped options and guessed the type from `/FT` alone.
/// Coordinates are converted back to the top-left origin the office form
/// editor uses, so listing a created form and saving it again leaves every
/// field where it was.
#[tauri::command]
pub async fn office_pdf_list_form(input: String) -> Result<Vec<PdfFormField>, OfficeErrorPayload> {
    let task = tauri::async_runtime::spawn_blocking(move || {
        let document = pdfcore::docutil::load_document(Path::new(&input), None).map_err(pdf_error)?;
        let pages = document.get_pages();
        let fields = pdfcore::forms::list_fields(&document)
            .into_iter()
            .map(|info| {
                let page = info.page.unwrap_or(1);
                let (x, y, w, h) = match (info.page, info.rect) {
                    (Some(page_number), Some(rect)) => {
                        let height = pages
                            .get(&page_number)
                            .and_then(|page_id| pdfcore::docutil::page_mediabox(&document, *page_id).ok())
                            .map(|media| media[3] - media[1])
                            .unwrap_or(0.0);
                        let left = rect[0].min(rect[2]);
                        let bottom = rect[1].min(rect[3]);
                        let right = rect[0].max(rect[2]);
                        let top = rect[1].max(rect[3]);
                        (left, height - top, right - left, top - bottom)
                    }
                    _ => (0.0, 0.0, 0.0, 0.0),
                };
                // The simple office builder only edits text/checkbox/radio and
                // dropdown fields; everything else is shown as text rather
                // than inventing an editor for it.
                let kind = match info.field_type.as_str() {
                    "checkbox" => "checkbox",
                    "radio" => "radio",
                    "choice" => "dropdown",
                    _ => "text",
                };
                PdfFormField {
                    kind: kind.to_string(),
                    name: info.name,
                    page,
                    x,
                    y,
                    w,
                    h,
                    value: info.value,
                    options: info.options.into_iter().map(|option| option.value).collect(),
                    font_size: None,
                    required: info.required,
                }
            })
            .collect();
        Ok(fields)
    });
    task.await
        .map_err(|error| payload(OfficeError::internal(format!("worker thread failed: {error}"))))?
}

#[allow(dead_code)]
fn unused(_app: AppHandle) {}
