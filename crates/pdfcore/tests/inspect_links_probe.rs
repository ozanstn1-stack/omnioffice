//! What the health check says about a document's links and its declared limits.
//!
//! A link is a `/Link` annotation with an action (`/A`, ISO 32000-1 12.6.2) or a
//! destination (`/Dest`, 12.3.2.3). An internal destination names a page; after
//! another tool edits the document it can name a page that is gone, and the
//! reader then offers a link that goes nowhere. A trailer `/Encrypt` may also
//! declare `/P` restrictions (7.6.4.2) - the document's own statement, which the
//! report repeats and does not enforce.

use lopdf::{dictionary, Document, Object, Stream};
use pdfcore::docutil::OverwritePolicy;
use pdfcore::inspect::inspect_document;
use pdfcore::security::{protect_pdf, ProtectOptions};

fn rect(values: [f64; 4]) -> Object {
    Object::Array(values.iter().map(|value| Object::Real(*value as f32)).collect())
}

/// Builds a two-page document whose first page carries the given annotations.
struct Builder {
    doc: Document,
    first_page: lopdf::ObjectId,
    second_page: lopdf::ObjectId,
}

impl Builder {
    fn new() -> Self {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let mut kids = Vec::new();
        let mut pages = Vec::new();
        for index in 0..2 {
            let content =
                doc.add_object(Stream::new(dictionary! {}, format!("BT /F1 12 Tf (page {index}) Tj ET").into_bytes()));
            let page = doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => Object::Reference(pages_id),
                "MediaBox" => rect([0.0, 0.0, 595.0, 842.0]),
                "Contents" => Object::Reference(content),
            });
            kids.push(Object::Reference(page));
            pages.push(page);
        }
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Count" => Object::Integer(2),
                "Kids" => Object::Array(kids),
            }),
        );
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages_id) });
        doc.trailer.set("Root", Object::Reference(catalog));
        Self { doc, first_page: pages[0], second_page: pages[1] }
    }

    fn link(&mut self, body: lopdf::Dictionary) {
        let mut annot = body;
        annot.set("Type", Object::Name(b"Annot".to_vec()));
        annot.set("Subtype", Object::Name(b"Link".to_vec()));
        annot.set("Rect", rect([10.0, 10.0, 100.0, 40.0]));
        let id = self.doc.add_object(Object::Dictionary(annot));
        let page = self.doc.get_object_mut(self.first_page).and_then(Object::as_dict_mut).expect("page");
        match page.get_mut(b"Annots") {
            Ok(Object::Array(items)) => items.push(Object::Reference(id)),
            _ => page.set("Annots", Object::Array(vec![Object::Reference(id)])),
        }
    }

    /// A named destination tree, with `names` as (name, destination) pairs.
    fn names(&mut self, pairs: Vec<(&str, Object)>) {
        let mut flat = Vec::new();
        for (name, dest) in pairs {
            flat.push(Object::String(name.as_bytes().to_vec(), lopdf::StringFormat::Literal));
            flat.push(dest);
        }
        let tree = self.doc.add_object(dictionary! { "Names" => Object::Array(flat) });
        let names = self.doc.add_object(dictionary! { "Dests" => Object::Reference(tree) });
        let catalog_id = self.doc.trailer.get(b"Root").and_then(Object::as_reference).expect("root");
        self.doc.get_object_mut(catalog_id).and_then(Object::as_dict_mut).expect("catalog").set("Names", names);
    }

    fn save(&mut self, name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("omnioffice-inspect-links-{name}.pdf"));
        self.doc.save(&path).expect("save");
        path
    }
}

fn codes(path: &std::path::Path) -> Vec<String> {
    inspect_document(path, None).expect("inspect").findings.into_iter().map(|finding| finding.code).collect()
}

#[test]
fn a_document_without_links_gets_no_link_findings() {
    let path = Builder::new().save("plain");
    let codes = codes(&path);
    assert!(!codes.contains(&"link.broken-internal".to_string()), "{codes:?}");
    assert!(!codes.contains(&"link.external".to_string()), "{codes:?}");
}

#[test]
fn a_link_to_a_page_that_is_gone_is_reported() {
    // `999 0 R` is not a page of this document.
    let mut builder = Builder::new();
    let missing = Object::Reference((999, 0));
    builder.link(dictionary! { "Dest" => Object::Array(vec![missing, Object::Name(b"Fit".to_vec())]) });
    let path = builder.save("broken");
    let codes = codes(&path);
    assert!(codes.contains(&"link.broken-internal".to_string()), "{codes:?}");
}

#[test]
fn a_link_to_a_real_page_is_not_reported() {
    let mut builder = Builder::new();
    let second = Object::Reference(builder.second_page);
    builder.link(dictionary! { "Dest" => Object::Array(vec![second, Object::Name(b"Fit".to_vec())]) });
    let path = builder.save("internal-ok");
    let codes = codes(&path);
    assert!(!codes.contains(&"link.broken-internal".to_string()), "{codes:?}");
}

#[test]
fn a_named_destination_is_resolved_through_the_name_tree() {
    let mut builder = Builder::new();
    let second = Object::Reference(builder.second_page);
    builder.names(vec![("chapter2", Object::Array(vec![second, Object::Name(b"Fit".to_vec())]))]);
    builder.link(dictionary! { "Dest" => Object::String(b"chapter2".to_vec(), lopdf::StringFormat::Literal) });
    let path = builder.save("named-ok");
    let resolved = codes(&path);
    assert!(!resolved.contains(&"link.broken-internal".to_string()), "a resolvable name is not broken: {resolved:?}");

    // The same tree with a name that is not in it.
    let mut builder = Builder::new();
    let second = Object::Reference(builder.second_page);
    builder.names(vec![("chapter2", Object::Array(vec![second, Object::Name(b"Fit".to_vec())]))]);
    builder.link(dictionary! { "Dest" => Object::String(b"chapter9".to_vec(), lopdf::StringFormat::Literal) });
    let path = builder.save("named-missing");
    let unresolved = codes(&path);
    assert!(unresolved.contains(&"link.broken-internal".to_string()), "an unknown name is broken: {unresolved:?}");
}

#[test]
fn an_action_wins_over_a_destination_and_a_web_link_is_counted() {
    // Both are written; the action is what a reader runs (ISO 32000-1 12.6.2).
    let mut builder = Builder::new();
    let missing = Object::Reference((999, 0));
    let uri = builder.doc.add_object(dictionary! {
        "S" => "URI",
        "URI" => Object::String(b"https://example.org/report".to_vec(), lopdf::StringFormat::Literal),
    });
    builder.link(dictionary! {
        "A" => Object::Reference(uri),
        "Dest" => Object::Array(vec![missing, Object::Name(b"Fit".to_vec())]),
    });
    let path = builder.save("action-wins");
    let codes = codes(&path);
    assert!(codes.contains(&"link.external".to_string()), "{codes:?}");
    assert!(
        !codes.contains(&"link.broken-internal".to_string()),
        "the action is what runs, so the dead destination is not counted: {codes:?}"
    );
}

#[test]
fn a_declared_restriction_is_reported_and_a_fully_open_document_is_not() {
    // An owner password with an empty user password: the file opens without a
    // prompt but still declares `/P` restrictions (ISO 32000-1 7.6.4.2).
    let source = Builder::new().save("protect-source");
    let restricted = std::env::temp_dir().join("omnioffice-inspect-links-restricted.pdf");
    protect_pdf(
        &source,
        &restricted,
        &ProtectOptions {
            user_password: String::new(),
            owner_password: "owner".into(),
            allow_printing: false,
            allow_copying: false,
            allow_editing: false,
            allow_commenting: false,
        },
        OverwritePolicy::Replace,
        None,
    )
    .expect("protect");
    let report = inspect_document(&restricted, Some("")).expect("inspect");
    let finding =
        report.findings.iter().find(|finding| finding.code == "security.permissions").expect("a permissions finding");
    assert!(finding.detail.contains("printing"), "the detail names what is denied: {}", finding.detail);
    assert!(finding.detail.contains("copying"), "{}", finding.detail);

    // A file this app protects with every permission granted declares only the
    // reserved bits as clear, so there is nothing to report.
    let open = std::env::temp_dir().join("omnioffice-inspect-links-open.pdf");
    protect_pdf(
        &source,
        &open,
        &ProtectOptions {
            user_password: String::new(),
            owner_password: "owner".into(),
            allow_printing: true,
            allow_copying: true,
            allow_editing: true,
            allow_commenting: true,
        },
        OverwritePolicy::Replace,
        None,
    )
    .expect("protect");
    let report = inspect_document(&open, Some("")).expect("inspect");
    assert!(
        !report.findings.iter().any(|finding| finding.code == "security.permissions"),
        "nothing is denied, so nothing is reported"
    );
}
