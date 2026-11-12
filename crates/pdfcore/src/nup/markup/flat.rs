//! The synthesized appearance of a single annotation, for flattening.
//!
//! Imposition paints markup without an `/AP` from its own geometry; flattening
//! needs the same drawing as a standalone form XObject so it can be burned into
//! the page. The form's content is in page space (its `/BBox` is the area the
//! drawing covers), so it is placed with the identity matrix, not fitted to the
//! annotation's `/Rect`.

use super::{appearance, install, links::read_rect};
use lopdf::{Dictionary, Document, Object, ObjectId};
use std::collections::BTreeMap;

/// Builds the appearance of an annotation that has none and adds it to the
/// document as a form XObject. `None` for an annotation that ships an `/AP`
/// (a broken one is never guessed at), a kind that cannot be drawn from its
/// geometry, or unreadable geometry.
pub(crate) fn synthesized_appearance(doc: &mut Document, annot: &Dictionary) -> Option<ObjectId> {
    if annot.has(b"AP") {
        return None;
    }
    let subtype = annot.get(b"Subtype").ok()?.as_name().ok()?.to_vec();
    let rect = read_rect(doc, annot.get(b"Rect").ok()?)?;
    let mut fonts = appearance::FontCache::new();
    let synth = appearance::synthesize(doc, annot, &subtype, rect, &mut fonts)?;
    let form = install::synthesized_form(doc, &synth, &mut BTreeMap::new());
    Some(doc.add_object(Object::Stream(form)))
}
