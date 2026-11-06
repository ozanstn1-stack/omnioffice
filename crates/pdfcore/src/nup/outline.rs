//! Carries the document outline (bookmarks) onto the imposed sheets.
//!
//! Imposition replaces the page tree, so an outline entry that pointed at a
//! source page would dangle. Every entry is rewritten through
//! [`super::dests::DestMap`] - an internal destination becomes a destination on
//! the sheet page that carries the source page, a URI entry keeps its action,
//! and an entry whose page is outside the selection loses its target. Entries
//! that end up with neither a target nor a child are dropped, so the outline
//! never shows a bookmark that goes nowhere.
//!
//! The tree is rebuilt with `/Parent`, `/Prev`, `/Next`, `/First`, `/Last` and
//! the signed `/Count` rule of the specification; a cycle in the source, an
//! absurd depth or an absurd number of entries truncates the copy instead of
//! failing the imposition.

use super::dests::{resolve, DestMap};
use lopdf::{Dictionary, Document, Object, ObjectId};
use std::collections::HashSet;

/// Deeper than any real outline; a broken `/First` chain stops here.
const MAX_DEPTH: usize = 64;
/// A truncated outline still beats a failed imposition.
const MAX_NODES: usize = 10_000;

/// What happened while carrying the outline, for tests and logs.
pub(super) struct OutlineReport {
    pub kept: usize,
    pub dropped: usize,
    pub truncated: bool,
}

/// One outline entry with its children, already mapped onto the sheets.
struct Node {
    title: Object,
    /// A rewritten destination array, or a URI action dictionary.
    target: Option<Object>,
    color: Option<Object>,
    flags: Option<Object>,
    open: bool,
    children: Vec<Node>,
}

/// Copies the source outline onto the imposed page tree.
///
/// Never fails: a broken entry is dropped, a broken chain is cut, and the
/// imposition goes on.
pub(super) fn carry_outline(doc: &mut Document, dests: &DestMap) -> OutlineReport {
    let mut report = OutlineReport { kept: 0, dropped: 0, truncated: false };
    let page_mode = doc.catalog().ok().and_then(|catalog| catalog.get(b"PageMode").ok().cloned());
    let nodes = prune(read_outline(doc, dests, &mut report), &mut report);
    if nodes.is_empty() {
        // The source outline pointed at pages this imposition removed; leaving
        // it behind would keep a set of bookmarks that go nowhere.
        if let Ok(catalog) = doc.catalog_mut() {
            catalog.remove(b"Outlines");
        }
        return report;
    }
    let root = doc.new_object_id();
    let Some((first, last, visible, total)) = write_level(doc, root, &nodes) else { return report };
    let mut root_dict = Dictionary::new();
    root_dict.set("Type", Object::Name(b"Outlines".to_vec()));
    root_dict.set("First", Object::Reference(first));
    root_dict.set("Last", Object::Reference(last));
    root_dict.set("Count", Object::Integer(visible));
    doc.objects.insert(root, Object::Dictionary(root_dict));
    if let Ok(catalog) = doc.catalog_mut() {
        catalog.set("Outlines", Object::Reference(root));
        // The source asked for the outline panel; it still has one now.
        if let Some(mode) = page_mode {
            catalog.set("PageMode", mode);
        }
    }
    report.kept = total as usize;
    report
}

// ---------------------------------------------------------------------------
// Reading the source outline
// ---------------------------------------------------------------------------

fn read_outline(doc: &Document, dests: &DestMap, report: &mut OutlineReport) -> Vec<Node> {
    let Ok(catalog) = doc.catalog() else { return Vec::new() };
    let Ok(outlines) = catalog.get(b"Outlines") else { return Vec::new() };
    let first = resolve(doc, outlines).as_dict().ok().and_then(|dict| reference(dict, b"First"));
    let mut seen = HashSet::new();
    let mut budget = MAX_NODES;
    read_level(doc, first, 0, &mut seen, &mut budget, dests, report)
}

fn read_level(
    doc: &Document,
    first: Option<ObjectId>,
    depth: usize,
    seen: &mut HashSet<ObjectId>,
    budget: &mut usize,
    dests: &DestMap,
    report: &mut OutlineReport,
) -> Vec<Node> {
    let mut nodes = Vec::new();
    let mut current = first;
    while let Some(id) = current {
        if depth > MAX_DEPTH || *budget == 0 {
            report.truncated = true;
            break;
        }
        // A `/Next` chain that loops back is cut here, not followed.
        if !seen.insert(id) {
            report.truncated = true;
            break;
        }
        *budget -= 1;
        let Ok(dict) = doc.get_dictionary(id) else {
            report.dropped += 1;
            break;
        };
        let next = reference(dict, b"Next");
        let title = match dict.get(b"Title") {
            Ok(Object::String(..)) | Ok(Object::Name(_)) => dict.get(b"Title").cloned().ok(),
            _ => None,
        };
        let Some(title) = title else {
            report.dropped += 1;
            current = next;
            continue;
        };
        let children = read_level(doc, reference(dict, b"First"), depth + 1, seen, budget, dests, report);
        let open = dict.get(b"Count").ok().and_then(|value| value.as_i64().ok()).is_some_and(|count| count > 0);
        let color = dict.get(b"C").ok().filter(|value| is_color(value)).cloned();
        let flags = dict.get(b"F").ok().and_then(|value| value.as_i64().ok()).map(Object::Integer);
        nodes.push(Node { title, target: resolve_target(doc, dict, dests), color, flags, open, children });
        current = next;
    }
    nodes
}

/// The entry's destination, rewritten onto the sheets when it can be.
fn resolve_target(doc: &Document, dict: &Dictionary, dests: &DestMap) -> Option<Object> {
    if let Ok(dest) = dict.get(b"Dest") {
        return dests.destination(doc, dest);
    }
    let action = resolve(doc, dict.get(b"A").ok()?).as_dict().ok()?;
    match action.get(b"S").ok()?.as_name().ok()? {
        b"URI" => {
            let uri = action.get(b"URI").ok()?;
            let mut out = Dictionary::new();
            out.set("S", Object::Name(b"URI".to_vec()));
            out.set("URI", uri.clone());
            Some(Object::Dictionary(out))
        }
        b"GoTo" => dests.destination(doc, action.get(b"D").ok()?),
        // `GoToR`, `Launch`, `JavaScript` and the rest would point at another
        // file or run a script: the entry loses its target on purpose.
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Writing the outline
// ---------------------------------------------------------------------------

/// Drops entries that have neither a target nor a surviving child.
fn prune(nodes: Vec<Node>, report: &mut OutlineReport) -> Vec<Node> {
    let mut kept = Vec::with_capacity(nodes.len());
    for mut node in nodes {
        node.children = prune(node.children, report);
        if node.target.is_some() || !node.children.is_empty() {
            kept.push(node);
        } else {
            report.dropped += 1;
        }
    }
    kept
}

/// Writes one level of siblings, returning `(first, last, visible, total)`
/// where `visible` counts what a reader sees and `total` counts every entry
/// below the parent (the number a closed parent reports negative).
fn write_level(doc: &mut Document, parent: ObjectId, nodes: &[Node]) -> Option<(ObjectId, ObjectId, i64, i64)> {
    // Ids first: siblings link to each other before either is written.
    let ids: Vec<ObjectId> = nodes.iter().map(|_| doc.new_object_id()).collect();
    let mut visible = 0i64;
    let mut total = 0i64;
    for (index, node) in nodes.iter().enumerate() {
        let id = ids[index];
        let mut dict = Dictionary::new();
        dict.set("Title", node.title.clone());
        dict.set("Parent", Object::Reference(parent));
        if index > 0 {
            dict.set("Prev", Object::Reference(ids[index - 1]));
        }
        if index + 1 < ids.len() {
            dict.set("Next", Object::Reference(ids[index + 1]));
        }
        if let Some(target) = &node.target {
            if target.as_dict().is_ok() {
                dict.set("A", target.clone());
            } else {
                dict.set("Dest", target.clone());
            }
        }
        if let Some(color) = &node.color {
            dict.set("C", color.clone());
        }
        if let Some(flags) = &node.flags {
            dict.set("F", flags.clone());
        }
        if !node.children.is_empty() {
            if let Some((first, last, child_visible, child_total)) = write_level(doc, id, &node.children) {
                dict.set("First", Object::Reference(first));
                dict.set("Last", Object::Reference(last));
                // Open reports what is visible, closed reports the whole
                // subtree with a negative sign.
                dict.set("Count", Object::Integer(if node.open { child_visible } else { -child_total }));
                if node.open {
                    visible += child_visible;
                }
                total += child_total;
            }
        }
        doc.objects.insert(id, Object::Dictionary(dict));
        visible += 1;
        total += 1;
    }
    Some((*ids.first()?, *ids.last()?, visible, total))
}

fn reference(dict: &Dictionary, key: &[u8]) -> Option<ObjectId> {
    match dict.get(key).ok()? {
        Object::Reference(id) => Some(*id),
        _ => None,
    }
}

fn is_color(value: &Object) -> bool {
    value.as_array().map(|items| items.len() == 3 && items.iter().all(number)).unwrap_or(false)
}

fn number(value: &Object) -> bool {
    matches!(value, Object::Integer(_) | Object::Real(_))
}
