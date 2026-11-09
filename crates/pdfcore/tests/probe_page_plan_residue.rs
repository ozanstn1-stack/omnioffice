//! What a page plan leaves behind: actions, structure references and page labels.
//!
//! Deleting a page does not only break `/Dest` and `/Pg`; an action's `/Next` chain
//! (ISO 32000-1 12.6.2), an additional-action dictionary (12.6.3), an outline item's
//! `/SE` (12.3.3) and an OBJR inside a structure element (14.7.4.3) can all still
//! name the page - and every one of them keeps its object, and the content on it,
//! alive. `/PageLabels` (12.4.2) is a different failure: it does not keep anything
//! alive, but the numbers a reader shows shift when an index is removed.

use lopdf::{dictionary, Document, Object, Stream};
use pdfcore::docutil::OverwritePolicy;
use pdfcore::docutil::PagePlanItem;
use pdfcore::organize::apply_page_plan;

struct Fixture {
    doc: Document,
    pages: Vec<lopdf::ObjectId>,
}

impl Fixture {
    fn new(pages: usize) -> Self {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let mut kids = Vec::new();
        let mut ids = Vec::new();
        for index in 0..pages {
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
            ids.push(page);
        }
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Count" => Object::Integer(pages as i64),
                "Kids" => Object::Array(kids),
            }),
        );
        let catalog = doc.new_object_id();
        doc.objects.insert(
            catalog,
            Object::Dictionary(dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages_id) }),
        );
        doc.trailer.set("Root", Object::Reference(catalog));
        Self { doc, pages: ids }
    }

    fn catalog_id(&self) -> lopdf::ObjectId {
        self.doc.trailer.get(b"Root").and_then(Object::as_reference).expect("root")
    }

    fn catalog_set(&mut self, key: &str, value: Object) {
        let id = self.catalog_id();
        self.doc.get_object_mut(id).and_then(Object::as_dict_mut).expect("catalog").set(key, value);
    }

    fn page_set(&mut self, index: usize, key: &str, value: Object) {
        let page = self.pages[index];
        self.doc.get_object_mut(page).and_then(Object::as_dict_mut).expect("page").set(key, value);
    }

    fn save(&mut self, name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("omnioffice-residue-{name}.pdf"));
        self.doc.save(&path).expect("save");
        path
    }
}

/// Keeps every page except the second one (1-based page 2).
fn drop_second(input: &std::path::Path, name: &str, total: usize) -> Document {
    let plan: Vec<PagePlanItem> = (1..=total as u32)
        .filter(|page| *page != 2)
        .map(|page| PagePlanItem { source_page: page, ..Default::default() })
        .collect();
    let output = std::env::temp_dir().join(format!("omnioffice-residue-{name}-out.pdf"));
    apply_page_plan(input, &plan, &output, OverwritePolicy::Replace, None).expect("apply the plan");
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

fn goto(page: lopdf::ObjectId) -> lopdf::Dictionary {
    dictionary! {
        "S" => "GoTo",
        "D" => Object::Array(vec![Object::Reference(page), Object::Name(b"Fit".to_vec())]),
    }
}

#[test]
fn an_additional_action_on_a_kept_widget_loses_the_dead_page() {
    // A widget on page one whose "on release" action jumps to the deleted page
    // (ISO 32000-1 12.6.3, Table 194).
    let mut fixture = Fixture::new(3);
    let p2 = fixture.pages[1];
    let action = fixture.doc.add_object(goto(p2));
    let aa = fixture.doc.add_object(dictionary! { "U" => Object::Reference(action) });
    let widget = fixture.doc.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "Rect" => Object::Array(vec![10.into(), 10.into(), 100.into(), 40.into()]),
        "FT" => "Btn",
        "T" => Object::String(b"kutu".to_vec(), lopdf::StringFormat::Literal),
        "V" => "On",
        "AS" => "On",
        "AA" => Object::Reference(aa),
    });
    fixture.page_set(0, "Annots", Object::Array(vec![Object::Reference(widget)]));
    let input = fixture.save("aa");

    let doc = drop_second(&input, "aa", 3);
    assert!(!reachable_text(&doc).contains("PAGE2-SECRET"), "the deleted page's content is not in the file");
    // Object numbers change when the file is rewritten; find the widget through
    // the page that owns it.
    let page_one = *doc.get_pages().get(&1).expect("page one");
    let widget = doc
        .get_dictionary(page_one)
        .expect("page one")
        .get(b"Annots")
        .ok()
        .and_then(|value| value.as_array().ok())
        .and_then(|items| items.first())
        .and_then(|value| value.as_reference().ok())
        .expect("the widget survives");
    let widget = doc.get_dictionary(widget).expect("the widget survives");
    assert!(widget.get(b"V").is_ok(), "the field value is untouched");
    match widget.get(b"AA").ok() {
        None => {}
        Some(Object::Reference(id)) => {
            let aa = doc.get_dictionary(*id).expect("the action dictionary");
            assert!(aa.get(b"U").is_err(), "the action on the deleted page is gone");
        }
        Some(Object::Dictionary(aa)) => assert!(aa.get(b"U").is_err(), "the action on the deleted page is gone"),
        Some(other) => panic!("unexpected /AA: {other:?}"),
    }
}

#[test]
fn a_next_chain_drops_only_the_dead_step() {
    // `/OpenAction` opens page one and chains to the deleted page (12.6.2, 193).
    let mut fixture = Fixture::new(3);
    let (p1, p2) = (fixture.pages[0], fixture.pages[1]);
    let mut first = goto(p1);
    first.set("Next", Object::Dictionary(goto(p2)));
    fixture.catalog_set("OpenAction", Object::Dictionary(first));
    let input = fixture.save("next");

    let doc = drop_second(&input, "next", 3);
    assert!(!reachable_text(&doc).contains("PAGE2-SECRET"), "the deleted page's content is not in the file");
    let action = doc.catalog().expect("catalog").get(b"OpenAction").ok().cloned().expect("the action stays");
    let action = match action {
        Object::Reference(id) => doc.get_dictionary(id).expect("action").clone(),
        Object::Dictionary(dict) => dict,
        other => panic!("unexpected /OpenAction: {other:?}"),
    };
    assert!(action.get(b"D").is_ok(), "the step on a kept page stays");
    assert!(action.get(b"Next").is_err(), "the step on the deleted page is gone");
}

#[test]
fn an_outline_item_keeps_its_destination_and_loses_a_dead_structure_reference() {
    // `/SE` names the structure element the item stands for (12.3.3, Table 153).
    let mut fixture = Fixture::new(3);
    let (p1, p2) = (fixture.pages[0], fixture.pages[1]);
    let element = fixture.doc.add_object(dictionary! {
        "Type" => "StructElem",
        "S" => "P",
        "Pg" => Object::Reference(p2),
    });
    let item = fixture.doc.add_object(dictionary! {
        "Title" => Object::String(b"baslik".to_vec(), lopdf::StringFormat::Literal),
        "Dest" => Object::Array(vec![Object::Reference(p1), Object::Name(b"Fit".to_vec())]),
        "SE" => Object::Reference(element),
    });
    let root = fixture.doc.add_object(dictionary! {
        "Type" => "Outlines",
        "First" => Object::Reference(item),
        "Last" => Object::Reference(item),
        "Count" => Object::Integer(1),
    });
    fixture
        .doc
        .get_object_mut(item)
        .and_then(Object::as_dict_mut)
        .expect("item")
        .set("Parent", Object::Reference(root));
    fixture.catalog_set("Outlines", Object::Reference(root));
    let structure = fixture.doc.add_object(dictionary! {
        "Type" => "StructTreeRoot",
        "K" => Object::Array(vec![Object::Reference(element)]),
    });
    fixture.catalog_set("StructTreeRoot", Object::Reference(structure));
    let input = fixture.save("se");

    let doc = drop_second(&input, "se", 3);
    let item = doc
        .catalog()
        .expect("catalog")
        .get(b"Outlines")
        .and_then(Object::as_reference)
        .and_then(|root| doc.get_dictionary(root))
        .and_then(|root| root.get(b"First"))
        .and_then(Object::as_reference)
        .expect("the item survives");
    let item = doc.get_dictionary(item).expect("item");
    assert!(item.get(b"Dest").is_ok(), "the destination on a kept page stays");
    assert!(item.get(b"SE").is_err(), "the reference to a dead structure element is gone");
}

#[test]
fn an_object_reference_to_a_deleted_page_annotation_leaves_the_structure() {
    // A structure element on page one whose `/K` holds an OBJR that points at an
    // annotation of the deleted page (14.7.4.3, Table 325).
    let mut fixture = Fixture::new(3);
    let (p1, p2) = (fixture.pages[0], fixture.pages[1]);
    let annot = fixture.doc.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Link",
        "Rect" => Object::Array(vec![10.into(), 10.into(), 100.into(), 40.into()]),
    });
    fixture.page_set(1, "Annots", Object::Array(vec![Object::Reference(annot)]));
    let element = fixture.doc.add_object(dictionary! {
        "Type" => "StructElem",
        "S" => "P",
        "Pg" => Object::Reference(p1),
        "K" => Object::Array(vec![
            Object::Integer(0),
            Object::Dictionary(dictionary! { "Type" => "OBJR", "Obj" => Object::Reference(annot), "Pg" => Object::Reference(p2) }),
        ]),
    });
    let structure = fixture.doc.add_object(dictionary! {
        "Type" => "StructTreeRoot",
        "K" => Object::Array(vec![Object::Reference(element)]),
    });
    fixture.catalog_set("StructTreeRoot", Object::Reference(structure));
    let input = fixture.save("objr");

    let doc = drop_second(&input, "objr", 3);
    assert!(!reachable_text(&doc).contains("PAGE2-SECRET"), "the deleted page's content is not in the file");
    // The element is found through the structure tree, not by the id it had before
    // the file was rewritten.
    let element = doc
        .catalog()
        .expect("catalog")
        .get(b"StructTreeRoot")
        .ok()
        .and_then(|value| value.as_reference().ok())
        .and_then(|root| doc.get_dictionary(root).ok())
        .and_then(|root| root.get(b"K").ok())
        .and_then(|value| value.as_array().ok())
        .and_then(|items| items.first())
        .and_then(|value| value.as_reference().ok())
        .expect("the element survives");
    let element = doc.get_dictionary(element).expect("the element survives");
    let kids = element.get(b"K").expect("kids");
    let kids = match kids {
        Object::Array(items) => items.clone(),
        other => vec![other.clone()],
    };
    assert_eq!(kids.len(), 1, "the object reference to the deleted page is gone: {kids:?}");
    assert!(matches!(kids[0], Object::Integer(_)), "what is left is the marked-content index");
}

#[test]
fn page_labels_still_name_the_pages_they_belong_to() {
    // i, ii, 1, 2, 3 for five pages (ISO 32000-1 12.4.2, Table 159).
    let mut fixture = Fixture::new(5);
    fixture.catalog_set(
        "PageLabels",
        Object::Dictionary(dictionary! {
            "Nums" => Object::Array(vec![
                Object::Integer(0),
                Object::Dictionary(dictionary! { "S" => "r" }),
                Object::Integer(2),
                Object::Dictionary(dictionary! { "S" => "D" }),
            ]),
        }),
    );
    let input = fixture.save("labels");

    // Drop page two (index 1): [0 r, 1 D] - i, then 1, 2, 3.
    let doc = drop_second(&input, "labels", 5);
    let labels = doc
        .catalog()
        .expect("catalog")
        .get(b"PageLabels")
        .ok()
        .and_then(|value| value.as_dict().ok())
        .cloned()
        .expect("page labels");
    let nums = labels.get(b"Nums").and_then(Object::as_array).expect("nums").clone();
    let starts: Vec<i64> =
        nums.chunks(2).filter_map(|pair| pair.first().and_then(|value| value.as_i64().ok())).collect();
    assert_eq!(starts, vec![0, 1], "the second range starts one page earlier now");
}
