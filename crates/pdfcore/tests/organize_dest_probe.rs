//! What happens to everything that pointed at a page the user deleted.
//!
//! A `/Dest` names a page (ISO 32000-1 12.3.2.2). Delete that page and the
//! reference dangles: a link opens onto nothing, an outline entry jumps nowhere -
//! and, worse, the page object can stay reachable in the file, so the content the
//! user believed was gone is still in it.

use lopdf::{dictionary, Document, Object, Stream};
use pdfcore::docutil::OverwritePolicy;
use pdfcore::organize::delete_pages;

const MARKS: [&str; 3] = ["MARK_A", "MARK_B", "MARK_C"];

/// Three pages, each with its own mark in the content stream.
struct Fixture {
    doc: Document,
    pages: Vec<lopdf::ObjectId>,
}

impl Fixture {
    fn new() -> Self {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let mut kids = Vec::new();
        let mut pages = Vec::new();
        for mark in MARKS {
            let content = doc
                .add_object(Stream::new(dictionary! {}, format!("BT /F1 12 Tf 72 720 Td ({mark}) Tj ET").into_bytes()));
            let page = doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => Object::Reference(pages_id),
                "MediaBox" => Object::Array(vec![0.into(), 0.into(), 595.into(), 842.into()]),
                "Contents" => Object::Reference(content),
            });
            kids.push(Object::Reference(page));
            pages.push(page);
        }
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Count" => Object::Integer(3),
                "Kids" => Object::Array(kids),
            }),
        );
        let catalog = doc.new_object_id();
        doc.objects.insert(
            catalog,
            Object::Dictionary(dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages_id) }),
        );
        doc.trailer.set("Root", Object::Reference(catalog));
        Self { doc, pages }
    }

    fn catalog(&self) -> lopdf::ObjectId {
        self.doc.trailer.get(b"Root").and_then(Object::as_reference).expect("root")
    }

    fn catalog_set(&mut self, key: &str, value: Object) {
        let catalog_id = self.catalog();
        self.doc.get_object_mut(catalog_id).and_then(Object::as_dict_mut).expect("catalog").set(key, value);
    }

    /// A link annotation on `page` pointing at `dest`.
    fn link(&mut self, page: lopdf::ObjectId, dest: Object) {
        let annot = self.doc.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Link",
            "Rect" => Object::Array(vec![10.into(), 10.into(), 100.into(), 40.into()]),
            "Dest" => dest,
        });
        let page_dict = self.doc.get_object_mut(page).and_then(Object::as_dict_mut).expect("page");
        match page_dict.get_mut(b"Annots") {
            Ok(Object::Array(items)) => items.push(Object::Reference(annot)),
            _ => page_dict.set("Annots", Object::Array(vec![Object::Reference(annot)])),
        }
    }

    fn save(&mut self, name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("omnioffice-organize-{name}.pdf"));
        self.doc.save(&path).expect("save");
        path
    }
}

fn dest_to(page: lopdf::ObjectId) -> Object {
    Object::Array(vec![Object::Reference(page), Object::Name(b"Fit".to_vec())])
}

fn pruned(input: &std::path::Path, name: &str, pages: &[u32]) -> Document {
    let output = std::env::temp_dir().join(format!("omnioffice-organize-{name}-out.pdf"));
    delete_pages(input, pages, &output, OverwritePolicy::Replace, None).expect("delete pages");
    Document::load(&output).expect("reload the output")
}

/// Every object id reachable from the trailer, i.e. actually part of the file a
/// reader sees. An orphaned object still sitting in the object table is not.
fn reachable_ids(doc: &Document) -> std::collections::HashSet<lopdf::ObjectId> {
    fn walk(object: &Object, out: &mut std::collections::HashSet<lopdf::ObjectId>) {
        match object {
            Object::Reference(id) => {
                out.insert(*id);
            }
            Object::Array(items) => items.iter().for_each(|item| walk(item, out)),
            Object::Dictionary(dict) => dict.iter().for_each(|(_, value)| walk(value, out)),
            Object::Stream(stream) => stream.dict.iter().for_each(|(_, value)| walk(value, out)),
            _ => {}
        }
    }
    let mut out = std::collections::HashSet::new();
    for (_, value) in doc.trailer.iter() {
        walk(value, &mut out);
    }
    let mut seen: std::collections::HashSet<lopdf::ObjectId> = std::collections::HashSet::new();
    let mut frontier: Vec<lopdf::ObjectId> = out.iter().copied().collect();
    while let Some(id) = frontier.pop() {
        if !seen.insert(id) {
            continue;
        }
        if let Ok(object) = doc.get_object(id) {
            let before = out.len();
            walk(object, &mut out);
            if out.len() != before {
                let fresh: Vec<lopdf::ObjectId> = out.iter().copied().filter(|id| !seen.contains(id)).collect();
                frontier.extend(fresh);
            }
        }
    }
    out
}

fn annotations_of(doc: &Document, page: u32) -> usize {
    let page_id = *doc.get_pages().get(&page).expect("the page");
    match doc.get_dictionary(page_id).and_then(|page| page.get(b"Annots")) {
        Ok(Object::Array(items)) => items.len(),
        Ok(Object::Reference(id)) => {
            doc.get_object(*id).and_then(Object::as_array).map(|items| items.len()).unwrap_or(0)
        }
        _ => 0,
    }
}

#[test]
fn a_link_to_the_deleted_page_goes_and_a_link_to_a_kept_page_stays() {
    let mut fixture = Fixture::new();
    let b = fixture.pages[1];
    let c = fixture.pages[2];
    let a_page = fixture.pages[0];
    fixture.link(a_page, dest_to(b));
    fixture.link(c, dest_to(a_page));
    let input = fixture.save("links");

    let doc = pruned(&input, "links", &[2]);
    // The link on page A pointed at B, which is gone; the link on C still points
    // at A, which is page one of the result.
    assert_eq!(annotations_of(&doc, 1), 0, "the dead link is removed");
    assert_eq!(annotations_of(&doc, 2), 1, "the link to a kept page stays");
    assert_eq!(doc.get_pages().len(), 2, "two pages left");
}

#[test]
fn an_open_action_on_the_deleted_page_is_dropped() {
    let mut fixture = Fixture::new();
    let b = fixture.pages[1];
    fixture.catalog_set("OpenAction", dest_to(b));
    let input = fixture.save("openaction");

    let doc = pruned(&input, "openaction", &[2]);
    let catalog = doc.catalog().expect("catalog");
    assert!(catalog.get(b"OpenAction").is_err(), "a catalog action cannot point at a page that is gone");
}

#[test]
fn an_outline_entry_on_the_deleted_page_keeps_its_title_but_loses_its_target() {
    let mut fixture = Fixture::new();
    let (a_page, b, c) = (fixture.pages[0], fixture.pages[1], fixture.pages[2]);
    // O1 -> A, O2 -> B (with a child O2a -> C), O3 -> C.
    let root = fixture.doc.new_object_id();
    let o3 = fixture.doc.add_object(dictionary! {
        "Title" => Object::String(b"Dokumanin sonu".to_vec(), lopdf::StringFormat::Literal),
        "Parent" => Object::Reference(root),
        "Dest" => dest_to(c),
    });
    let child = fixture.doc.add_object(dictionary! {
        "Title" => Object::String(b"C sayfasi".to_vec(), lopdf::StringFormat::Literal),
        "Parent" => Object::Reference(root),
        "Dest" => dest_to(c),
    });
    let o2 = fixture.doc.add_object(dictionary! {
        "Title" => Object::String(b"B sayfasi".to_vec(), lopdf::StringFormat::Literal),
        "Parent" => Object::Reference(root),
        "Dest" => dest_to(b),
        "First" => Object::Reference(child),
        "Last" => Object::Reference(child),
    });
    let o1 = fixture.doc.add_object(dictionary! {
        "Title" => Object::String(b"A sayfasi".to_vec(), lopdf::StringFormat::Literal),
        "Parent" => Object::Reference(root),
        "Dest" => dest_to(a_page),
        "Next" => Object::Reference(o2),
    });
    fixture.doc.objects.insert(
        root,
        Object::Dictionary(dictionary! {
            "Type" => "Outlines",
            "First" => Object::Reference(o1),
            "Last" => Object::Reference(o3),
            "Count" => Object::Integer(4),
        }),
    );
    fixture.doc.get_object_mut(o2).and_then(Object::as_dict_mut).expect("o2").set("Prev", Object::Reference(o1));
    fixture.doc.get_object_mut(o3).and_then(Object::as_dict_mut).expect("o3").set("Prev", Object::Reference(o2));
    fixture.catalog_set("Outlines", Object::Reference(root));
    let input = fixture.save("outline");

    let doc = pruned(&input, "outline", &[2]);
    // Object numbers change when the tree is rebuilt, so the entry is found by its
    // title rather than by the id it had in the input.
    let catalog = doc.catalog().expect("catalog");
    let root = catalog.get(b"Outlines").and_then(Object::as_reference).expect("outlines");
    let mut current = doc.get_dictionary(root).expect("outline root").get(b"First").and_then(Object::as_reference).ok();
    let mut found = None;
    while let Some(id) = current {
        let entry = doc.get_dictionary(id).expect("entry");
        if entry.get(b"Title").and_then(Object::as_str).unwrap_or_default() == b"B sayfasi" {
            found = Some(entry.clone());
            break;
        }
        current = entry.get(b"Next").and_then(Object::as_reference).ok();
    }
    let entry = found.expect("the entry survives as a title");
    let dest = entry.get(b"Dest").ok().cloned();
    assert!(dest.is_none() && entry.get(b"A").is_err(), "the entry no longer points at the deleted page");
}

#[test]
fn nothing_in_the_output_still_reaches_the_deleted_pages_content() {
    let mut fixture = Fixture::new();
    let b = fixture.pages[1];
    let a_page = fixture.pages[0];
    fixture.link(a_page, dest_to(b));
    let input = fixture.save("privacy");

    let doc = pruned(&input, "privacy", &[2]);
    let reachable = reachable_ids(&doc);
    // Object numbers are reassigned when the file is rewritten, so the check is on
    // what the reachable objects draw, not on the id the page had before.
    let _ = b;
    let text: String = doc
        .objects
        .iter()
        .filter(|(id, _)| reachable.contains(id))
        .filter_map(|(_, object)| object.as_stream().ok())
        .map(|stream| stream.decompressed_content().unwrap_or_default())
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .collect();
    assert!(text.contains("MARK_A"), "the kept pages are still drawn");
    assert!(text.contains("MARK_C"), "the kept pages are still drawn");
    assert!(!text.contains("MARK_B"), "the deleted page's content must not survive in the file");
}

#[test]
fn a_named_destination_on_the_deleted_page_goes_and_the_other_one_stays() {
    // `/Names /Dests` is where a producer keeps destinations it refers to by name
    // (ISO 32000-1 12.3.2.3); a link runs the name through `/A /GoTo /D (a)`.
    let mut fixture = Fixture::new();
    let (a_page, b, c) = (fixture.pages[0], fixture.pages[1], fixture.pages[2]);
    let names = vec![
        Object::String(b"a".to_vec(), lopdf::StringFormat::Literal),
        dest_to(b),
        Object::String(b"b".to_vec(), lopdf::StringFormat::Literal),
        dest_to(c),
    ];
    let tree = fixture.doc.add_object(dictionary! { "Names" => Object::Array(names) });
    let names_dict = fixture.doc.add_object(dictionary! { "Dests" => Object::Reference(tree) });
    fixture.catalog_set("Names", Object::Reference(names_dict));

    for (page, key) in [(a_page, b"a"), (c, b"b")] {
        let action = fixture.doc.add_object(dictionary! {
            "S" => "GoTo",
            "D" => Object::String(key.to_vec(), lopdf::StringFormat::Literal),
        });
        let annot = fixture.doc.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Link",
            "Rect" => Object::Array(vec![10.into(), 10.into(), 100.into(), 40.into()]),
            "A" => Object::Reference(action),
        });
        let page_dict = fixture.doc.get_object_mut(page).and_then(Object::as_dict_mut).expect("page");
        match page_dict.get_mut(b"Annots") {
            Ok(Object::Array(items)) => items.push(Object::Reference(annot)),
            _ => page_dict.set("Annots", Object::Array(vec![Object::Reference(annot)])),
        }
    }
    let input = fixture.save("named");

    let doc = pruned(&input, "named", &[2]);
    assert_eq!(annotations_of(&doc, 1), 0, "the link through the dead name is removed");
    assert_eq!(annotations_of(&doc, 2), 1, "the link through the live name stays");

    // The dead entry itself is gone from the name tree.
    let catalog = doc.catalog().expect("catalog");
    let names = catalog.get(b"Names").and_then(Object::as_reference).expect("names");
    let dests =
        doc.get_dictionary(names).expect("names dict").get(b"Dests").and_then(Object::as_reference).expect("dests");
    let pairs = doc.get_dictionary(dests).expect("tree").get(b"Names").and_then(Object::as_array).expect("pairs");
    let names: Vec<Vec<u8>> = pairs
        .chunks(2)
        .filter_map(|pair| pair.first().and_then(|name| name.as_str().ok()).map(|name| name.to_vec()))
        .collect();
    assert_eq!(names, vec![b"b".to_vec()], "only the destination that still resolves is kept");
}
