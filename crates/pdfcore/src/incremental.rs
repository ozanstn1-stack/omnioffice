//! Incremental updates: edits that must not touch a signed revision.
//!
//! A PDF can be extended by appending a revision (ISO 32000-1, 7.5.6): the
//! original bytes stay exactly as they were, and only the added or replaced
//! objects plus a new cross-reference section follow them. That is what makes
//! a counter-signature, an archived validation block or a stamp added after
//! signing possible without invalidating anything - each signature still
//! covers precisely the revision it signed.
//!
//! [`apply_difference`] turns an ordinary in-memory edit into such an update by
//! comparing the edited document with the original one. An edit that *removes*
//! objects is refused: the format cannot drop an object from an earlier
//! revision, so the only honest outcomes are a full rewrite (which invalidates
//! signatures, and the caller has to say so) or no edit at all.

use crate::error::{PdfError, PdfResult};
use lopdf::{Document, IncrementalDocument, Object, ObjectId};

/// How many signature dictionaries the document carries.
///
/// This is the cheap object scan - a signature dictionary always carries
/// /ByteRange and /Contents. Use `sign::verify_signatures` when the
/// cryptographic result is what matters.
pub fn signature_count(pdf: &[u8]) -> usize {
    match Document::load_mem(pdf) {
        Ok(doc) => doc.objects.values().filter(|object| looks_like_signature(object)).count(),
        Err(_) => 0,
    }
}

/// True when the document carries at least one signature dictionary.
pub fn has_signatures(pdf: &[u8]) -> bool {
    signature_count(pdf) > 0
}

fn looks_like_signature(object: &Object) -> bool {
    object.as_dict().map(|dict| dict.get(b"ByteRange").is_ok() && dict.get(b"Contents").is_ok()).unwrap_or(false)
}

/// Appends the difference between `original` and `edited` as a new revision.
///
/// Every byte of `original` is preserved, which is exactly what keeps an
/// existing signature valid. On success the result always starts with
/// `original`; that invariant is checked here rather than trusted.
pub fn apply_difference(original: &[u8], edited: &Document) -> PdfResult<Vec<u8>> {
    let previous = Document::load_mem(original).map_err(|error| PdfError::from_lopdf(error, None))?;
    if previous.is_encrypted() || previous.was_encrypted() {
        return Err(PdfError::PasswordRequired);
    }
    if previous.trailer.get(b"Root").is_err() {
        return Err(PdfError::InvalidPdf("the document has no catalog".into()));
    }

    let mut changed: Vec<ObjectId> = Vec::new();
    for (id, object) in &edited.objects {
        match previous.objects.get(id) {
            Some(older) if older == object => {}
            _ => changed.push(*id),
        }
    }
    let removed = previous.objects.keys().filter(|id| !edited.objects.contains_key(id)).count();
    if removed > 0 {
        return Err(PdfError::InvalidInput(format!(
            "this change removes {removed} object(s); an incremental update can only add or replace \
             them, so keeping existing signatures would need a full rewrite"
        )));
    }

    let mut inc = IncrementalDocument::create_from(original.to_vec(), previous);
    for id in changed {
        if let Some(object) = edited.objects.get(&id) {
            inc.new_document.set_object(id, object.clone());
        }
    }

    // The trailer carries /Root, /Info and /ID, so it follows the edit - but
    // /Prev is the chain link lopdf computed when the update was created, and
    // dropping it would orphan every earlier revision.
    let prev_link = inc.new_document.trailer.get(b"Prev").ok().cloned();
    let mut trailer = edited.trailer.clone();
    if let Some(link) = prev_link {
        trailer.set("Prev", link);
    }
    inc.new_document.trailer = trailer;
    if edited.max_id > inc.new_document.max_id {
        inc.new_document.max_id = edited.max_id;
    }

    let mut output = Vec::new();
    inc.save_to(&mut output)
        .map_err(|error| PdfError::ProcessingFailed(format!("could not write the incremental update: {error}")))?;
    if !output.starts_with(original) {
        return Err(PdfError::Internal("the incremental update rewrote the previous revision".into()));
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    fn sample() -> (Vec<u8>, Document) {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(dictionary! { "Type" => "Page", "Parent" => pages_id, "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()] });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(
                dictionary! { "Type" => "Pages", "Kids" => vec![Object::Reference(page_id)], "Count" => 1 },
            ),
        );
        let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).expect("save");
        // Callers edit the document they loaded from these bytes, so the test
        // does the same: a hand-built Document can differ from its own
        // round trip (lopdf adds trailer entries on save).
        let reloaded = Document::load_mem(&bytes).expect("reload");
        (bytes, reloaded)
    }

    #[test]
    fn the_original_bytes_are_preserved() {
        let (bytes, mut edited) = sample();
        let info = edited.add_object(dictionary! { "Title" => Object::string_literal("After") });
        edited.trailer.set("Info", info);
        let output = apply_difference(&bytes, &edited).expect("update");
        assert!(output.starts_with(&bytes));
        assert!(output.len() > bytes.len());
        // The appended revision is a readable PDF that carries the new value.
        let reloaded = Document::load_mem(&output).expect("reload");
        let info_id = reloaded.trailer.get(b"Info").and_then(Object::as_reference).expect("info");
        let info = reloaded.get_dictionary(info_id).expect("info dict");
        assert_eq!(info.get(b"Title").and_then(|value| value.as_str()).unwrap_or_default(), b"After");
        // And the appended trailer links back at the original revision: a
        // reader that ignores the update would still see the signed document.
        let appended = &output[bytes.len()..];
        assert!(appended.windows(5).any(|window| window == b"/Prev"), "the new trailer must carry a /Prev link");
    }

    #[test]
    fn an_unchanged_document_appends_an_empty_revision() {
        let (bytes, doc) = sample();
        let output = apply_difference(&bytes, &doc).expect("update");
        assert!(output.starts_with(&bytes));
        let reloaded = Document::load_mem(&output).expect("reload");
        assert_eq!(reloaded.get_pages().len(), 1);
    }

    #[test]
    fn a_removal_is_refused() {
        let (bytes, mut edited) = sample();
        let doomed = edited.get_pages().values().next().copied().expect("page");
        edited.objects.remove(&doomed);
        let error = apply_difference(&bytes, &edited).expect_err("removal must be refused");
        assert!(error.to_string().contains("removes"), "unexpected error: {error}");
    }

    #[test]
    fn signature_detection_is_a_cheap_object_scan() {
        let (bytes, _) = sample();
        assert_eq!(signature_count(&bytes), 0);
        assert!(!has_signatures(&bytes));
        assert_eq!(signature_count(b"not a pdf"), 0);
    }
}
