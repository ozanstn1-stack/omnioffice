//! Which annotations Flatten may paint.
//!
//! The annotation flags are the only thing that decides it today: the Hidden bit
//! means the viewer never shows the annotation, so baking it in would add
//! something the user never saw. The label is here too, for the warnings.

use lopdf::Dictionary;

/// Bit 2 of an annotation's `/F`: hidden, so never displayed or printed.
pub(super) const HIDDEN_FLAG: i64 = 2;

/// An annotation's `/F`, or zero when it has none.
fn annotation_flags(annot: &Dictionary) -> i64 {
    annot.get(b"F").ok().and_then(|value| value.as_i64().ok()).unwrap_or(0)
}

/// Whether the viewer hides this annotation, so flatten must not paint it.
pub(super) fn is_hidden(annot: &Dictionary) -> bool {
    annotation_flags(annot) & HIDDEN_FLAG != 0
}

/// The warning a hidden annotation leaves behind.
pub(super) fn hidden_warning(page: u32, annot: &Dictionary) -> String {
    let label = annotation_label(annot);
    format!("Page {page}: the {label} annotation is hidden and was left in place.")
}

pub(super) fn annotation_label(annot: &Dictionary) -> String {
    annot
        .get(b"Subtype")
        .ok()
        .and_then(|value| value.as_name().ok())
        .map(|value| String::from_utf8_lossy(value).to_string())
        .unwrap_or_else(|| "annotation".into())
}
