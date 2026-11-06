//! Decides which form-field widgets an imposition may print.
//!
//! A widget is the visual half of an AcroForm field, and painting it is the
//! same problem as painting a highlight - its `/AP /N` stream goes onto the
//! sheet. What differs is the meaning: a text field whose appearance was never
//! regenerated can print an old value, and an unsigned signature field is a
//! placeholder that would look like a signature.
//!
//! The rules are therefore conservative. A widget without a print flag, or one
//! whose field type says its appearance may be stale, stays behind exactly as
//! it did before this pass existed.

use super::super::dests::resolve;
use lopdf::{Dictionary, Document, Object, ObjectId};
use std::collections::HashSet;

/// Deeper than any real field tree; a broken `/Parent` chain stops here.
const MAX_DEPTH: usize = 32;

/// Document-wide field context, read once per imposition.
pub(super) struct AcroCtx {
    need_appearances: bool,
}

impl AcroCtx {
    /// Reads `Root /AcroForm /NeedAppearances`; anything unreadable means "no".
    pub(super) fn from_doc(doc: &Document) -> Self {
        let flag = doc
            .catalog()
            .ok()
            .and_then(|catalog| catalog.get(b"AcroForm").ok().cloned())
            .and_then(|form| resolve(doc, &form).as_dict().ok().cloned())
            .and_then(|form| form.get(b"NeedAppearances").ok().and_then(|value| value.as_bool().ok()));
        AcroCtx { need_appearances: flag.unwrap_or(false) }
    }
}

/// The inherited field properties a widget carries.
#[derive(Default)]
struct FieldInfo {
    /// `/FT` of the nearest ancestor (or the widget) that names one.
    kind: Option<Vec<u8>>,
    /// Whether a `/V` (value) is set anywhere in the chain.
    has_value: bool,
}

/// What the markup pass should do with a widget.
#[derive(PartialEq, Eq)]
pub(super) enum Decision {
    Draw,
    /// Only tests read the reason; the imposition drops the widget silently.
    Skip(&'static str),
}

/// Classifies one widget: `Draw` when its appearance can be trusted.
pub(super) fn classify(doc: &Document, widget: &Dictionary, context: &AcroCtx) -> Decision {
    let info = field_info(doc, widget);
    match info.kind.as_deref() {
        // An unsigned signature field is a placeholder, not a signature.
        Some(b"Sig") if !info.has_value => Decision::Skip("unsigned signature field"),
        // The stored appearance may predate the value; printing it could show
        // text the field no longer holds.
        Some(b"Tx") | Some(b"Ch") if context.need_appearances => Decision::Skip("stale appearance"),
        // Buttons carry a state appearance per value, so `/AS` is trustworthy.
        _ => Decision::Draw,
    }
}

/// `/FT`, `/Ff` and `/V`, inherited through the `/Parent` chain.
fn field_info(doc: &Document, widget: &Dictionary) -> FieldInfo {
    let mut info = FieldInfo::default();
    let mut seen: HashSet<ObjectId> = HashSet::new();
    collect(doc, widget, &mut info, 0, &mut seen);
    info
}

fn collect(doc: &Document, dict: &Dictionary, info: &mut FieldInfo, depth: usize, seen: &mut HashSet<ObjectId>) {
    if depth > MAX_DEPTH {
        return;
    }
    if info.kind.is_none() {
        if let Ok(kind) = dict.get(b"FT").and_then(|value| value.as_name()) {
            info.kind = Some(kind.to_vec());
        }
    }
    if !info.has_value && dict.get(b"V").is_ok() {
        info.has_value = true;
    }
    let Ok(Object::Reference(parent)) = dict.get(b"Parent") else { return };
    if !seen.insert(*parent) {
        return;
    }
    if let Ok(parent) = doc.get_dictionary(*parent) {
        collect(doc, parent, info, depth + 1, seen);
    }
}
