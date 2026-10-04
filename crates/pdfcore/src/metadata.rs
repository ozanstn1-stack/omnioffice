//! Document metadata (Info dictionary) reading, editing and removal.

use crate::docutil::*;
use crate::error::{PdfError, PdfResult};
use lopdf::{dictionary, Dictionary, Document, Object};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const INFO_KEYS: [&str; 8] =
    ["Title", "Author", "Subject", "Keywords", "Creator", "Producer", "CreationDate", "ModDate"];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PdfMetadata {
    pub title: String,
    pub author: String,
    pub subject: String,
    pub keywords: String,
    pub creator: String,
    pub producer: String,
    pub creation_date: String,
    pub mod_date: String,
}

impl PdfMetadata {
    pub fn is_empty(&self) -> bool {
        self.title.is_empty()
            && self.author.is_empty()
            && self.subject.is_empty()
            && self.keywords.is_empty()
            && self.creator.is_empty()
            && self.producer.is_empty()
            && self.creation_date.is_empty()
            && self.mod_date.is_empty()
    }
}

pub fn read_metadata(doc: &Document) -> PdfMetadata {
    let mut meta = PdfMetadata::default();
    let info =
        doc.trailer.get(b"Info").ok().and_then(|o| o.as_reference().ok()).and_then(|id| doc.get_dictionary(id).ok());
    let Some(info) = info else {
        return meta;
    };
    let get = |key: &str| -> String { info.get(key.as_bytes()).ok().and_then(pdf_text_value).unwrap_or_default() };
    meta.title = get("Title");
    meta.author = get("Author");
    meta.subject = get("Subject");
    meta.keywords = get("Keywords");
    meta.creator = get("Creator");
    meta.producer = get("Producer");
    meta.creation_date = get("CreationDate");
    meta.mod_date = get("ModDate");
    meta
}

pub fn read_metadata_from_file(path: &Path) -> PdfResult<PdfMetadata> {
    let doc = load_document(path, None)?;
    Ok(read_metadata(&doc))
}

/// Writes the given metadata into the document, creating the Info dictionary
/// when needed.
pub fn write_metadata(doc: &mut Document, meta: &PdfMetadata) -> PdfResult<()> {
    let existing_id = doc.trailer.get(b"Info").ok().and_then(|o| o.as_reference().ok());
    let mut info = match existing_id {
        Some(id) => doc.get_dictionary(id).cloned().unwrap_or_default(),
        None => Dictionary::new(),
    };
    let pairs = [
        ("Title", &meta.title),
        ("Author", &meta.author),
        ("Subject", &meta.subject),
        ("Keywords", &meta.keywords),
        ("Creator", &meta.creator),
        ("Producer", &meta.producer),
        ("CreationDate", &meta.creation_date),
        ("ModDate", &meta.mod_date),
    ];
    for (key, value) in pairs {
        if value.is_empty() {
            info.remove(key.as_bytes());
        } else {
            info.set(key, pdf_text_object(value));
        }
    }
    let info_id = doc.add_object(Object::Dictionary(info));
    doc.trailer.set("Info", Object::Reference(info_id));
    Ok(())
}

/// Removes the Info dictionary and any XMP metadata stream in the catalog.
pub fn remove_metadata(doc: &mut Document) -> PdfResult<()> {
    doc.trailer.remove(b"Info");
    if let Ok(catalog_id) = doc.catalog().and_then(|c| {
        c.get(b"Metadata").ok().and_then(|o| o.as_reference().ok()).ok_or(lopdf::Error::ObjectNotFound((0, 0)))
    }) {
        let _ = doc.objects.remove(&catalog_id);
    }
    if let Ok(catalog) = doc.catalog_mut() {
        catalog.remove(b"Metadata");
    }
    Ok(())
}

/// High level: edit metadata of `input`, writing to `output`.
pub fn edit_metadata_file(
    input: &Path,
    output: &Path,
    meta: &PdfMetadata,
    remove: bool,
    policy: OverwritePolicy,
    password: Option<&str>,
) -> PdfResult<PathBuf> {
    let mut doc = load_document(input, password)?;
    if remove {
        remove_metadata(&mut doc)?;
    } else {
        write_metadata(&mut doc, meta)?;
    }
    let final_path = resolve_output_path(output, policy)?;
    save_document(&mut doc, &final_path, true)?;
    Ok(final_path)
}

/// Applies metadata changes as an incremental update.
///
/// Nothing in the original bytes is touched, so a document that is already
/// signed keeps every signature valid: each one still covers exactly the
/// revision it signed, and the metadata change is visible as a later revision.
pub fn edit_metadata_incremental(input: &[u8], meta: &PdfMetadata) -> PdfResult<Vec<u8>> {
    let mut edited = Document::load_mem(input).map_err(|error| PdfError::from_lopdf(error, None))?;
    if edited.is_encrypted() || edited.was_encrypted() {
        return Err(PdfError::PasswordRequired);
    }
    write_metadata(&mut edited, meta)?;
    crate::incremental::apply_difference(input, &edited)
}

/// Convenience used by other operations that want a fresh Producer stamp.
pub fn stamp_producer(doc: &mut Document, producer: &str) {
    let existing_id = doc.trailer.get(b"Info").ok().and_then(|o| o.as_reference().ok());
    let mut info = existing_id.and_then(|id| doc.get_dictionary(id).ok().cloned()).unwrap_or_else(|| dictionary! {});
    info.set("Producer", pdf_text_object(producer));
    let info_id = doc.add_object(Object::Dictionary(info));
    doc.trailer.set("Info", Object::Reference(info_id));
}

/// Creation date is kept when re-saving; this is used by tests.
pub fn read_title(path: &Path) -> Option<String> {
    read_metadata_from_file(path).ok().map(|m| m.title).filter(|t| !t.is_empty())
}
