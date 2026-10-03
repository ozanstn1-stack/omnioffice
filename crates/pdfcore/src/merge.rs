//! Merging documents and combining page selections from several files.

use crate::docutil::*;
use crate::error::{PdfError, PdfResult};
use crate::progress::{CancelToken, ProgressCallback, ProgressEvent};
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct MergeOptions {
    /// Copy the Info dictionary of the first input into the merged output.
    #[serde(default = "default_true")]
    pub preserve_metadata: bool,
}

fn default_true() -> bool {
    true
}

/// Merges already loaded documents (in the given order) into one new document.
pub fn merge_documents(sources: Vec<(String, Document)>, options: &MergeOptions) -> PdfResult<Document> {
    if sources.is_empty() {
        return Err(PdfError::InvalidInput("no documents to merge".into()));
    }
    let mut out = Document::new();
    out.version = "1.7".to_string();
    let mut page_ids: Vec<ObjectId> = Vec::new();
    let mut offset: u32 = 1;
    let mut first_info: Option<Dictionary> = None;

    for (path, mut src) in sources {
        materialize_all_pages(&mut src).map_err(|e| PdfError::ProcessingFailed(format!("{path}: {e}")))?;
        let pages_in_order: Vec<ObjectId> = src.get_pages().values().copied().collect();
        if pages_in_order.is_empty() {
            continue;
        }
        if first_info.is_none() && options.preserve_metadata {
            first_info = src
                .trailer
                .get(b"Info")
                .ok()
                .and_then(|o| o.as_reference().ok())
                .and_then(|id| src.get_dictionary(id).ok())
                .cloned();
        }
        let mut rename: BTreeMap<ObjectId, ObjectId> = BTreeMap::new();
        let mut max_old: u32 = 0;
        for id in src.objects.keys() {
            max_old = max_old.max(id.0);
        }
        for id in src.objects.keys() {
            rename.insert(*id, (id.0 + offset, id.1));
        }
        for (id, obj) in src.objects.iter() {
            let mut copy = obj.clone();
            rename_refs(&mut copy, &rename);
            out.objects.insert(rename[id], copy);
        }
        out.max_id = out.max_id.max(offset + max_old);
        offset += max_old + 1;
        for pid in pages_in_order {
            page_ids.push(rename[&pid]);
        }
    }
    if page_ids.is_empty() {
        return Err(PdfError::InvalidPdf("inputs contain no pages".into()));
    }

    let pages_id = out.add_object(Object::Dictionary(dictionary! {
        "Type" => "Pages",
        "Kids" => Vec::<Object>::new(),
        "Count" => 0i64,
    }));
    let kids: Vec<Object> = page_ids.iter().map(|id| Object::Reference(*id)).collect();
    {
        let pages = out.get_object_mut(pages_id)?.as_dict_mut()?;
        pages.set("Kids", Object::Array(kids.clone()));
        pages.set("Count", Object::Integer(kids.len() as i64));
    }
    for id in &page_ids {
        if let Ok(dict) = out.get_object_mut(*id).and_then(Object::as_dict_mut) {
            dict.set("Parent", Object::Reference(pages_id));
        }
    }
    let mut catalog = dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    };
    if let Some(version) = out.version.clone().strip_prefix("PDF-") {
        let _ = version;
    }
    catalog.set("PageMode", Object::Name(b"UseNone".to_vec()));
    let catalog_id = out.add_object(Object::Dictionary(catalog));
    out.trailer.set("Root", Object::Reference(catalog_id));
    if let Some(mut info) = first_info {
        info.set("Producer", pdf_text_object("PDF Swiss Army Knife"));
        info.set("ModDate", pdf_text_object(&lopdf_date_now()));
        let info_id = out.add_object(Object::Dictionary(info));
        out.trailer.set("Info", Object::Reference(info_id));
    } else {
        let info_id = out.add_object(Object::Dictionary(dictionary! {
            "Producer" => pdf_text_object("PDF Swiss Army Knife"),
            "ModDate" => pdf_text_object(&lopdf_date_now()),
        }));
        out.trailer.set("Info", Object::Reference(info_id));
    }
    out.trailer.set("Size", Object::Integer((out.max_id + 1) as i64));
    Ok(out)
}

pub fn lopdf_date_now() -> String {
    // PDF date string: D:YYYYMMDDHHmmSS+HH'mm'
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    let (y, mo, d, h, mi, s) = civil_from_unix(now as i64);
    format!("D:{y:04}{mo:02}{d:02}{h:02}{mi:02}{s:02}Z")
}

/// Days-from-civil algorithm (Howard Hinnant), UTC.
fn civil_from_unix(unix: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = unix.div_euclid(86400);
    let secs = unix.rem_euclid(86400);
    let h = (secs / 3600) as u32;
    let mi = ((secs % 3600) / 60) as u32;
    let s = (secs % 60) as u32;
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d, h, mi, s)
}

fn rename_refs(obj: &mut Object, rename: &BTreeMap<ObjectId, ObjectId>) {
    match obj {
        Object::Reference(id) => {
            if let Some(new_id) = rename.get(id) {
                *id = *new_id;
            } else {
                *obj = Object::Null;
            }
        }
        Object::Array(items) => {
            for item in items.iter_mut() {
                rename_refs(item, rename);
            }
        }
        Object::Dictionary(dict) => {
            for (_, value) in dict.iter_mut() {
                rename_refs(value, rename);
            }
        }
        Object::Stream(stream) => {
            for (_, value) in stream.dict.iter_mut() {
                rename_refs(value, rename);
            }
        }
        _ => {}
    }
}

/// Merges PDF files from disk in the given order.
pub fn merge_files(
    inputs: &[PathBuf],
    output: &Path,
    options: &MergeOptions,
    policy: OverwritePolicy,
    progress: &ProgressCallback,
    cancel: &CancelToken,
) -> PdfResult<(PathBuf, u32)> {
    if inputs.len() < 2 {
        return Err(PdfError::InvalidInput("select at least two PDF files".into()));
    }
    let mut sources = Vec::with_capacity(inputs.len());
    for (index, path) in inputs.iter().enumerate() {
        cancel.check()?;
        progress(ProgressEvent::new("merge.load", index as u64, inputs.len() as u64));
        let doc = load_document(path, None)?;
        sources.push((path.display().to_string(), doc));
    }
    cancel.check()?;
    progress(ProgressEvent::new("merge.write", 0, 1));
    let mut merged = merge_documents(sources, options)?;
    let final_path = resolve_output_path(output, policy)?;
    save_document(&mut merged, &final_path, true)?;
    let pages = merged.get_pages().len() as u32;
    progress(ProgressEvent::new("merge.write", 1, 1));
    Ok((final_path, pages))
}
