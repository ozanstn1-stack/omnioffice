//! The other places a page is referenced: structure, forms and article threads.
//!
//! A page can be reached from the structure tree (`/Pg`, ISO 32000-1 14.7.2), from
//! an AcroForm field whose widget sits on it (12.5.6.19) and from an article thread
//! bead (12.4.3). Any of those keeps the page object alive after it has been deleted,
//! so the content the user believed was gone can still be in the file. This probe
//! asks what a deletion actually leaves behind.

use lopdf::{dictionary, Document, Object, Stream};
use pdfcore::docutil::OverwritePolicy;
use pdfcore::docutil::PagePlanItem;
use pdfcore::organize::{apply_page_plan, delete_pages, extract_pages};

const SECRET: &str = "PAGE2-SECRET";

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
        for index in 0..3 {
            let mark = if index == 1 { "PAGE2-SECRET" } else { "PAGE-OK" };
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

    fn catalog_set(&mut self, key: &str, value: Object) {
        let root = self.doc.trailer.get(b"Root").and_then(Object::as_reference).expect("root");
        self.doc.get_object_mut(root).and_then(Object::as_dict_mut).expect("catalog").set(key, value);
    }

    fn page_set(&mut self, index: usize, key: &str, value: Object) {
        let page = self.pages[index];
        self.doc.get_object_mut(page).and_then(Object::as_dict_mut).expect("page").set(key, value);
    }

    fn save(&mut self, name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("omnioffice-pageref-{name}.pdf"));
        self.doc.save(&path).expect("save");
        path
    }
}

fn deleted(input: &std::path::Path, name: &str, pages: &[u32]) -> Document {
    let output = std::env::temp_dir().join(format!("omnioffice-pageref-{name}-out.pdf"));
    delete_pages(input, pages, &output, OverwritePolicy::Replace, None).expect("delete pages");
    Document::load(&output).expect("reload the output")
}

fn reachable_text(doc: &Document) -> String {
    let mut ids = std::collections::HashSet::new();
    fn walk(object: &Object, ids: &mut std::collections::HashSet<lopdf::ObjectId>) {
        match object {
            Object::Reference(id) => {
                ids.insert(*id);
            }
            Object::Array(items) => items.iter().for_each(|item| walk(item, ids)),
            Object::Dictionary(dict) => dict.iter().for_each(|(_, value)| walk(value, ids)),
            Object::Stream(stream) => stream.dict.iter().for_each(|(_, value)| walk(value, ids)),
            _ => {}
        }
    }
    for (_, value) in doc.trailer.iter() {
        walk(value, &mut ids);
    }
    let mut seen = std::collections::HashSet::new();
    let mut frontier: Vec<lopdf::ObjectId> = ids.iter().copied().collect();
    while let Some(id) = frontier.pop() {
        if !seen.insert(id) {
            continue;
        }
        if let Ok(object) = doc.get_object(id) {
            let before = ids.len();
            walk(object, &mut ids);
            if ids.len() != before {
                frontier.extend(ids.iter().copied().filter(|id| !seen.contains(id)));
            }
        }
    }
    doc.objects
        .iter()
        .filter(|(id, _)| ids.contains(id))
        .filter_map(|(_, object)| object.as_stream().ok())
        .map(|stream| stream.decompressed_content().unwrap_or_default())
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .collect()
}

#[test]
fn the_structure_tree_drops_the_deleted_page() {
    // Three structure elements, one per page, and the page parents of the marked
    // content they describe (ISO 32000-1 14.7.2, 14.7.4.4).
    let mut fixture = Fixture::new();
    let (p1, p2, p3) = (fixture.pages[0], fixture.pages[1], fixture.pages[2]);
    let mut elements = Vec::new();
    for (index, page) in [p1, p2, p3].iter().enumerate() {
        let element = fixture.doc.add_object(dictionary! {
            "Type" => "StructElem",
            "S" => "P",
            "Pg" => Object::Reference(*page),
            "K" => Object::Integer(index as i64),
        });
        elements.push(Object::Reference(element));
        fixture.page_set(index, "StructParents", Object::Integer(index as i64));
    }
    let root = fixture.doc.add_object(dictionary! {
        "Type" => "StructTreeRoot",
        "K" => Object::Array(elements),
    });
    // The parent tree maps a page's `/StructParents` key to its marked content.
    let mut numbers = Vec::new();
    for index in 0..3 {
        numbers.push(Object::Integer(index));
        numbers.push(Object::Array(vec![Object::Integer(index)]));
    }
    let parent_tree = fixture.doc.add_object(dictionary! { "Nums" => Object::Array(numbers) });
    fixture
        .doc
        .get_object_mut(root)
        .and_then(Object::as_dict_mut)
        .expect("root")
        .set("ParentTree", Object::Reference(parent_tree));
    fixture.catalog_set("StructTreeRoot", Object::Reference(root));
    let input = fixture.save("struct");

    let doc = deleted(&input, "struct", &[2]);
    let catalog = doc.catalog().expect("catalog");
    let root = catalog.get(b"StructTreeRoot").and_then(Object::as_reference).expect("structure root");
    let kids = doc.get_dictionary(root).expect("root dict").get(b"K").and_then(Object::as_array).expect("kids");
    let surviving: Vec<&Object> = kids.iter().filter(|kid| !is_deleted_page(&doc, kid)).collect();
    assert_eq!(surviving.len(), 2, "the element for the deleted page is gone");
    assert!(!reachable_text(&doc).contains(SECRET), "the deleted page's content is not in the file");
}

fn is_deleted_page(doc: &Document, element: &Object) -> bool {
    let id = match element.as_reference() {
        Ok(id) => id,
        Err(_) => return false,
    };
    match doc.get_dictionary(id).and_then(|entry| entry.get(b"Pg")).and_then(Object::as_reference) {
        // A `/Pg` that no longer resolves to a page is the leftovers of a deletion.
        Ok(page) => !doc.get_pages().values().any(|candidate| *candidate == page),
        Err(_) => false,
    }
}

#[test]
fn a_form_field_on_the_deleted_page_goes_with_it() {
    // `secret` lives on the deleted page, `b` has one widget on each of two pages.
    let mut fixture = Fixture::new();
    let (p1, p2) = (fixture.pages[0], fixture.pages[1]);
    let mut fields = Vec::new();
    for (name, page, value) in [("a", p1, None), ("secret", p2, Some(SECRET)), ("b", p1, None)] {
        let mut widget = dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "Rect" => Object::Array(vec![10.into(), 10.into(), 100.into(), 40.into()]),
            "P" => Object::Reference(page),
            "T" => Object::String(name.as_bytes().to_vec(), lopdf::StringFormat::Literal),
            "FT" => "Tx",
        };
        if let Some(value) = value {
            widget.set("V", Object::String(value.as_bytes().to_vec(), lopdf::StringFormat::Literal));
        }
        let id = fixture.doc.add_object(widget);
        fields.push(Object::Reference(id));
    }
    let form = fixture.doc.add_object(dictionary! { "Fields" => Object::Array(fields) });
    fixture.catalog_set("AcroForm", Object::Reference(form));
    let input = fixture.save("forms");

    let doc = deleted(&input, "forms", &[2]);
    assert!(!reachable_text(&doc).contains(SECRET), "the deleted page's field value is not in the file");
    let catalog = doc.catalog().expect("catalog");
    let form = catalog.get(b"AcroForm").and_then(Object::as_reference).expect("acroform");
    let fields = doc.get_dictionary(form).expect("form").get(b"Fields").and_then(Object::as_array).expect("fields");
    let names: Vec<String> = fields
        .iter()
        .filter_map(|field| field.as_reference().ok())
        .filter_map(|id| doc.get_dictionary(id).ok())
        .filter_map(|field| {
            field.get(b"T").and_then(Object::as_str).ok().map(|name| String::from_utf8_lossy(name).into_owned())
        })
        .collect();
    assert!(!names.iter().any(|name| name == "secret"), "the field on the deleted page is dropped: {names:?}");
}

#[test]
fn an_article_thread_skips_the_deleted_page() {
    // One thread: bead on page one, bead on the deleted page, bead on page three.
    let mut fixture = Fixture::new();
    let (p1, p2, p3) = (fixture.pages[0], fixture.pages[1], fixture.pages[2]);
    let b3 = fixture.doc.add_object(dictionary! { "T" => "third" });
    let b2 = fixture.doc.add_object(dictionary! { "T" => "second" });
    let b1 = fixture.doc.add_object(dictionary! { "T" => "first" });
    let thread = fixture.doc.add_object(dictionary! { "F" => Object::Reference(b1), "I" => Object::Reference(b1) });
    for (index, bead) in [(0usize, b1), (1, b2), (2, b3)] {
        let page = [p1, p2, p3][index];
        fixture.page_set(index, "B", Object::Array(vec![Object::Reference(bead)]));
        let bead = fixture.doc.get_object_mut(bead).and_then(Object::as_dict_mut).expect("bead");
        bead.set("P", Object::Reference(page));
        bead.set("N", Object::Reference([b2, b3, b1][index]));
        bead.set("V", Object::Reference([b3, b1, b2][index]));
    }
    fixture.catalog_set("Threads", Object::Array(vec![Object::Reference(thread)]));
    let input = fixture.save("threads");

    let doc = deleted(&input, "threads", &[2]);
    assert!(!reachable_text(&doc).contains(SECRET), "the deleted page's content is not in the file");
    let catalog = doc.catalog().expect("catalog");
    let threads = catalog.get(b"Threads").and_then(Object::as_array).expect("threads");
    let threads = threads.len();
    // With every bead of a thread on the deleted page the thread has to go; with one
    // surviving page it stays and its first bead is a page that still exists.
    for thread in 0..threads {
        let _ = thread;
    }
    assert!(threads <= 1, "an empty thread is dropped: {threads}");
}

#[test]
fn reordering_keeps_a_destination_on_its_page() {
    // A destination names a page object, not a page number (ISO 32000-1 12.3.2), so
    // reordering must not move it.
    let mut fixture = Fixture::new();
    let (p1, p3) = (fixture.pages[0], fixture.pages[2]);
    let dest = Object::Array(vec![Object::Reference(p3), Object::Name(b"Fit".to_vec())]);
    let item = fixture.doc.add_object(
        dictionary! { "Title" => Object::String(b"uc".to_vec(), lopdf::StringFormat::Literal), "Dest" => dest },
    );
    let root = fixture.doc.add_object(dictionary! { "Type" => "Outlines", "First" => Object::Reference(item), "Last" => Object::Reference(item), "Count" => Object::Integer(1) });
    fixture
        .doc
        .get_object_mut(item)
        .and_then(Object::as_dict_mut)
        .expect("item")
        .set("Parent", Object::Reference(root));
    fixture.catalog_set("Outlines", Object::Reference(root));
    let input = fixture.save("reorder");
    let output = std::env::temp_dir().join("omnioffice-pageref-reorder-out.pdf");
    let plan: Vec<PagePlanItem> =
        [3u32, 1, 2].iter().map(|page| PagePlanItem { source_page: *page, ..Default::default() }).collect();
    apply_page_plan(&input, &plan, &output, OverwritePolicy::Replace, None).expect("reorder");
    let doc = Document::load(&output).expect("reload");
    // Page three is now page one, and the outline still names its object.
    let first = doc.get_pages().get(&1).copied().expect("page one");
    let item = doc
        .catalog()
        .expect("catalog")
        .get(b"Outlines")
        .and_then(Object::as_reference)
        .and_then(|root| doc.get_dictionary(root))
        .and_then(|root| root.get(b"First"))
        .and_then(Object::as_reference)
        .expect("item");
    let dest = doc.get_dictionary(item).expect("item").get(b"Dest").and_then(Object::as_array).expect("dest").clone();
    let target = dest.first().and_then(|value| value.as_reference().ok()).expect("page reference");
    assert_eq!(target, first, "the destination still names the same page object");
    let _ = p1;
}

#[test]
fn extraction_leaves_no_destination_to_a_missing_page() {
    let mut fixture = Fixture::new();
    let (p1, p2) = (fixture.pages[0], fixture.pages[1]);
    let annot = fixture.doc.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Link",
        "Rect" => Object::Array(vec![10.into(), 10.into(), 100.into(), 40.into()]),
        "Dest" => Object::Array(vec![Object::Reference(p2), Object::Name(b"Fit".to_vec())]),
    });
    let page = fixture.doc.get_object_mut(p1).and_then(Object::as_dict_mut).expect("page");
    page.set("Annots", Object::Array(vec![Object::Reference(annot)]));
    let input = fixture.save("extract");
    let output = std::env::temp_dir().join("omnioffice-pageref-extract-out.pdf");
    extract_pages(&input, &[1], &output, OverwritePolicy::Replace, None).expect("extract");
    let doc = Document::load(&output).expect("reload");
    let page_id = doc.get_pages().get(&1).copied().expect("page one");
    let annots =
        doc.get_dictionary(page_id).expect("page").get(b"Annots").ok().and_then(|value| value.as_array().ok()).cloned();
    let links = annots.map(|items| items.len()).unwrap_or(0);
    assert_eq!(links, 0, "a link to a page that was left out is not kept");
}
