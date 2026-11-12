//! Deleting annotations from a page together with what belongs to them.
//!
//! A markup annotation's `/Popup` is its own annotation in `/Annots` with
//! `/Parent` pointing back, and a reply is an annotation whose `/IRT` points at
//! the comment it answers (12.5.6.2-12.5.6.4). Removing only the comment left
//! its popup and its replies on the page, now attached to nothing.

use lopdf::{Document, Object, ObjectId};
use std::collections::BTreeSet;

/// The `/Annots` array without the entries at `indexes` (positions in `items`)
/// and without every popup or reply that hangs off a removed annotation,
/// followed transitively (a reply to a reply goes too).
pub(super) fn without_deleted(doc: &Document, items: Vec<Object>, indexes: &BTreeSet<usize>) -> Vec<Object> {
    let mut removed: BTreeSet<ObjectId> =
        indexes.iter().filter_map(|index| items.get(*index)?.as_reference().ok()).collect();
    loop {
        let before = removed.len();
        for item in &items {
            let id = item.as_reference().ok();
            if id.is_some_and(|id| removed.contains(&id)) {
                continue;
            }
            let dict = match item {
                Object::Reference(id) => doc.get_dictionary(*id).ok(),
                Object::Dictionary(dict) => Some(dict),
                _ => None,
            };
            let Some(dict) = dict else { continue };
            let points_at_removed = |key: &[u8]| {
                dict.get(key)
                    .ok()
                    .and_then(|value| value.as_reference().ok())
                    .is_some_and(|target| removed.contains(&target))
            };
            if points_at_removed(b"Parent") || points_at_removed(b"IRT") {
                // An inline popup or reply cannot be pointed at; it is dropped below.
                if let Some(id) = id {
                    removed.insert(id);
                }
            }
        }
        if removed.len() == before {
            break;
        }
    }
    items
        .into_iter()
        .enumerate()
        .filter(|(index, item)| {
            if indexes.contains(index) {
                return false;
            }
            if item.as_reference().is_ok_and(|id| removed.contains(&id)) {
                return false;
            }
            // Inline entries that hang off a removed annotation.
            !matches!(item, Object::Dictionary(dict) if [b"Parent".as_slice(), b"IRT"].iter().any(|key| {
                dict.get(key).ok().and_then(|value| value.as_reference().ok()).is_some_and(|target| removed.contains(&target))
            }))
        })
        .map(|(_, item)| item)
        .collect()
}
