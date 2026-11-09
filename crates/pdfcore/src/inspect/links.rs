//! What a document's links and its declared restrictions actually say.
//!
//! A `/Link` annotation carries either an action (`/A`, ISO 32000-1 12.6.2) or a
//! destination (`/Dest`, 12.3.2.3). An internal destination names a page, and a
//! document that was edited by another tool can name a page that no longer
//! exists - the reader then finds a link that goes nowhere. The trailer's
//! `/Encrypt` may also declare `/P` restrictions (7.6.4.2): it is the document's
//! own statement of what it allows, not something this app enforces.

use std::collections::HashSet;

use lopdf::{Document, Object};

/// The page number a destination points at, if it points at one.
pub(crate) fn destination_page(doc: &Document, value: &Object) -> Option<u32> {
    let array = match value {
        Object::Array(items) => items.clone(),
        Object::Dictionary(entry) => match entry.get(b"D").ok() {
            Some(Object::Array(items)) => items.clone(),
            _ => return None,
        },
        _ => return None,
    };
    // ISO 32000-1 destinations put the page reference first (`[page /XYZ ...]`);
    // some producers keep a leading `null`, so a second lookup covers them.
    let page_id = [0usize, 1].into_iter().find_map(|index| array.get(index)?.as_reference().ok())?;
    doc.get_pages().iter().find_map(|(number, candidate)| (*candidate == page_id).then_some(*number))
}

/// A link target, once resolved as far as the document allows.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct LinkCounts {
    pub(super) total: u32,
    pub(super) external: u32,
    pub(super) internal: u32,
    pub(super) broken: u32,
    pub(super) other: u32,
}

/// Counts the links of a document by where they lead.
pub(super) fn count_links(doc: &Document) -> LinkCounts {
    let mut counts = LinkCounts::default();
    for page_id in doc.get_pages().values() {
        let Ok(page) = doc.get_dictionary(*page_id) else { continue };
        let annots = match page.get(b"Annots") {
            Ok(Object::Array(items)) => items.clone(),
            Ok(Object::Reference(id)) => match doc.get_object(*id).and_then(Object::as_array) {
                Ok(items) => items.clone(),
                Err(_) => continue,
            },
            _ => continue,
        };
        for item in annots {
            let Ok(annot) = item.as_reference().and_then(|id| doc.get_dictionary(id)) else { continue };
            if annot.get(b"Subtype").and_then(Object::as_name).unwrap_or(b"") != b"Link" {
                continue;
            }
            counts.total += 1;
            // `/A` wins when a producer writes both (12.6.2): the action runs.
            let action = match annot.get(b"A").ok() {
                Some(Object::Reference(id)) => doc.get_dictionary(*id).ok(),
                Some(Object::Dictionary(dict)) => Some(dict),
                _ => None,
            };
            if let Some(action) = action {
                if action.get(b"S").and_then(Object::as_name).unwrap_or(b"") == b"URI" {
                    counts.external += 1;
                    continue;
                }
                if action.get(b"S").and_then(Object::as_name).unwrap_or(b"") != b"GoTo" {
                    counts.other += 1;
                    continue;
                }
                match action.get(b"D").ok() {
                    Some(destination) => count_destination(doc, destination, &mut counts),
                    None => counts.broken += 1,
                }
                continue;
            }
            match annot.get(b"Dest").ok() {
                Some(destination) => count_destination(doc, destination, &mut counts),
                None => counts.other += 1,
            }
        }
    }
    counts
}

fn count_destination(doc: &Document, destination: &Object, counts: &mut LinkCounts) {
    match destination {
        // A name is resolved through the document's own name tree.
        Object::Name(_) | Object::String(_, _) => match named_page(doc, destination) {
            Some(_) => counts.internal += 1,
            None => counts.broken += 1,
        },
        _ => match destination_page(doc, destination) {
            Some(_) => counts.internal += 1,
            None => counts.broken += 1,
        },
    }
}

/// The page a named destination points at, following `/Names /Dests` (12.3.2.4).
fn named_page(doc: &Document, key: &Object) -> Option<u32> {
    let catalog = doc.catalog().ok()?;
    let names = match catalog.get(b"Names").ok()? {
        Object::Reference(id) => doc.get_dictionary(*id).ok()?,
        Object::Dictionary(dict) => dict,
        _ => return None,
    };
    let mut seen = HashSet::new();
    walk_names(doc, names.get(b"Dests").ok()?, key, &mut seen)
}

/// Walks a name-tree node, following `/Kids` when the key is not in `/Names`.
/// The tree is sorted, but a wrong sort order in the file would make a binary
/// search wrong, so every kid is visited - each node at most once.
fn walk_names(doc: &Document, node: &Object, key: &Object, seen: &mut HashSet<lopdf::ObjectId>) -> Option<u32> {
    let dict = match node {
        Object::Reference(id) => {
            if !seen.insert(*id) {
                return None;
            }
            doc.get_dictionary(*id).ok()?.clone()
        }
        Object::Dictionary(dict) => dict.clone(),
        _ => return None,
    };
    if let Ok(pairs) = dict.get(b"Names").and_then(Object::as_array) {
        for pair in pairs.chunks(2) {
            let [name, value] = pair else { continue };
            if value_matches(name, key) {
                let target = match value {
                    Object::Dictionary(entry) => entry.get(b"D").ok(),
                    other => Some(other),
                };
                return target.and_then(|target| destination_page(doc, target));
            }
        }
    }
    let kids = dict.get(b"Kids").and_then(Object::as_array).ok()?;
    kids.iter().find_map(|kid| walk_names(doc, kid, key, seen))
}

fn value_matches(name: &Object, key: &Object) -> bool {
    match (name, key) {
        (Object::String(left, _), Object::String(right, _)) => left == right,
        (Object::Name(left), Object::Name(right)) => left == right,
        (Object::String(left, _), Object::Name(right)) => left == right,
        (Object::Name(left), Object::String(right, _)) => left == right,
        _ => false,
    }
}

/// The restrictions a document declares in its `/Encrypt` dictionary.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct Permissions {
    /// The permission bits that are **cleared**, in reading order.
    pub(super) denied: Vec<&'static str>,
    /// The raw `/P` value, for the report.
    pub(super) raw: i64,
}

/// Reads `/P` from the file's own `/Encrypt` dictionary (ISO 32000-1 7.6.4.2).
///
/// The parsed document cannot answer this: a PDF library decrypts on load and
/// drops the trailer's `/Encrypt`, so the restriction bits are only in the bytes.
/// Returns `None` when no restrictions are declared - no dictionary, no `/P`, or
/// every permission granted (bit 1-2 are reserved and always clear, so a `/P` of
/// `-4` grants all eight and means "nothing denied").
pub(super) fn declared_permissions(path: &std::path::Path) -> Option<Permissions> {
    let bytes = std::fs::read(path).ok()?;
    let raw = read_permission_bits(&bytes)?;
    let bits = (raw as u32) & 0x0FFF;
    const NAMES: [(u32, &str); 8] = [
        (1 << 2, "printing"),
        (1 << 3, "modifying"),
        (1 << 4, "copying"),
        (1 << 5, "annotating"),
        (1 << 8, "filling forms"),
        (1 << 9, "extracting for accessibility"),
        (1 << 10, "assembling"),
        (1 << 11, "high-quality printing"),
    ];
    let denied: Vec<&'static str> = NAMES.iter().filter(|(bit, _)| bits & bit == 0).map(|(_, name)| *name).collect();
    if denied.is_empty() {
        return None;
    }
    Some(Permissions { denied, raw })
}

/// The `/P` value of the encryption dictionary, read from the raw file.
fn read_permission_bits(bytes: &[u8]) -> Option<i64> {
    let tail_start = bytes.len().saturating_sub(8192);
    let tail = &bytes[tail_start..];
    let offset = rfind(tail, b"/Encrypt")? + tail_start;
    let after = &bytes[offset + b"/Encrypt".len()..];
    let (number, _) = read_two_ints(after)?;
    // The dictionary itself: `<number> <generation> obj` anywhere in the file.
    let needle = format!("{number} 0 obj");
    let object = find(bytes, needle.as_bytes())?;
    let window = &bytes[object..bytes.len().min(object + 4096)];
    let p = find(window, b"/P")?;
    let value = &window[p + 2..];
    let text = String::from_utf8_lossy(&value[..value.len().min(24)]);
    let token = text.split(|c: char| !(c.is_ascii_digit() || c == '-')).find(|part| !part.is_empty())?;
    token.parse().ok()
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

fn rfind(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).rposition(|window| window == needle)
}

/// The first two integers in `text`, which is how a PDF writes an indirect
/// reference (`12 0 R`).
fn read_two_ints(text: &[u8]) -> Option<(i64, i64)> {
    let text = String::from_utf8_lossy(&text[..text.len().min(32)]);
    let mut numbers = text
        .split(|c: char| !(c.is_ascii_digit() || c == '-'))
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<i64>().ok());
    Some((numbers.next()?, numbers.next()?))
}

/// The findings a document's links and declared restrictions earn.
///
/// Reading the file again is deliberate: `evaluate_findings` runs on a finished
/// report, and the pages' `/Annots` are gone by then.
pub(super) fn findings(path: &str) -> Vec<super::Finding> {
    // Loaded without a password on purpose: the decrypted copy lopdf hands back
    // no longer carries the trailer's `/Encrypt`, which is where `/P` lives.
    let Ok(doc) = Document::load(path) else { return Vec::new() };
    let mut out = Vec::new();
    if let Some(permissions) = declared_permissions(std::path::Path::new(path)) {
        out.push(super::Finding {
            severity: super::Severity::Info,
            code: "security.permissions".into(),
            title: "The document declares restrictions".into(),
            detail: format!(
                "Its own /Encrypt dictionary denies: {} (ISO 32000-1 7.6.4.2). This is the document's statement about itself, not a setting of this app.",
                permissions.denied.join(", ")
            ),
        });
    }
    // An encrypted file keeps its strings encrypted, so names and URIs cannot be
    // read from it; only the structure would be visible, which is not enough to
    // call a link broken.
    if doc.is_encrypted() {
        return out;
    }
    let counts = count_links(&doc);
    if counts.broken > 0 {
        out.push(super::Finding {
            severity: super::Severity::Warning,
            code: "link.broken-internal".into(),
            title: "A link points at a page the document no longer has".into(),
            detail: format!(
                "{} of {} internal link(s) name a page outside the document, so a reader opens them onto nothing.",
                counts.broken,
                counts.internal + counts.broken
            ),
        });
    }
    if counts.external > 0 {
        out.push(super::Finding {
            severity: super::Severity::Info,
            code: "link.external".into(),
            title: "The document links to the web".into(),
            detail: format!("{} web link(s). They open in the reader's browser, not inside this app.", counts.external),
        });
    }
    out
}
