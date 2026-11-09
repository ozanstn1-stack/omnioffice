//! What a protected file says about its own permissions.
//!
//! `protect_pdf` writes `/P`, a 32-bit field whose bits are permissions
//! (ISO 32000-1 7.6.3.2, Table 22). Bits 1-2 are reserved and must be 0, bits 3-12
//! are the permissions, and the rest are set. A wrong bit is silent: the user is
//! told a document cannot be copied and the file still allows it.
//!
//! The expectations here are the integers the table produces, not what this code
//! computes, so a shared mistake between the writer and a reader cannot hide.

use lopdf::Document;
use pdfcore::docutil::OverwritePolicy;
use pdfcore::security::{protect_pdf, ProtectOptions};

fn rect(values: [f64; 4]) -> lopdf::Object {
    lopdf::Object::Array(values.iter().map(|value| lopdf::Object::Real(*value as f32)).collect())
}

/// A one-page document to protect. Each caller gets its own path: the tests run
/// in parallel and a shared fixture would be read while another test rewrites it.
fn source(name: &str) -> std::path::PathBuf {
    use lopdf::{dictionary, Object, Stream};
    let mut doc = Document::with_version("1.7");
    let root = doc.new_object_id();
    let content = doc.add_object(Stream::new(dictionary! {}, b"BT /F1 12 Tf (page) Tj ET".to_vec()));
    let page = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => Object::Reference(root),
        "MediaBox" => rect([0.0, 0.0, 595.0, 842.0]),
        "Contents" => Object::Reference(content),
    });
    doc.objects.insert(
        root,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => Object::Integer(1),
            "Kids" => Object::Array(vec![Object::Reference(page)]),
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(root) });
    doc.trailer.set("Root", Object::Reference(catalog));
    let path = std::env::temp_dir().join(format!("omnioffice-permissions-source-{name}.pdf"));
    doc.save(&path).expect("save the source");
    path
}

fn protect(options: ProtectOptions, name: &str) -> std::path::PathBuf {
    let source = source(name);
    let path = std::env::temp_dir().join(format!("omnioffice-permissions-{name}.pdf"));
    protect_pdf(&source, &path, &options, OverwritePolicy::Replace, None).expect("protect");
    path
}

/// The `/P` value, read straight from the file's bytes: the parsed document
/// decrypts and drops `/Encrypt`, so the field is only in the raw file.
fn permission_bits(path: &std::path::Path) -> i64 {
    let bytes = std::fs::read(path).expect("read the protected file");
    let tail = &bytes[bytes.len().saturating_sub(8192)..];
    let needle = b"/Encrypt";
    let offset = tail
        .windows(needle.len())
        .rposition(|window| window == needle)
        .expect("the trailer names an encryption dictionary");
    let after = String::from_utf8_lossy(&tail[offset + needle.len()..]);
    let number: i64 = after.split_whitespace().find_map(|token| token.parse().ok()).expect("an object number");
    let needle = format!("{number} 0 obj");
    let object =
        bytes.windows(needle.len()).position(|window| window == needle.as_bytes()).expect("the encryption object");
    let window = String::from_utf8_lossy(&bytes[object..bytes.len().min(object + 4096)]).into_owned();
    let at = window.find("/P").expect("the dictionary carries /P");
    window[at + 2..]
        .split(|c: char| !(c.is_ascii_digit() || c == '-'))
        .find(|part| !part.is_empty())
        .expect("a number after /P")
        .parse()
        .expect("an integer")
}

fn options(print: bool, copy: bool, edit: bool, comment: bool) -> ProtectOptions {
    ProtectOptions {
        user_password: String::new(),
        owner_password: "owner".into(),
        allow_printing: print,
        allow_copying: copy,
        allow_editing: edit,
        allow_commenting: comment,
    }
}

#[test]
fn every_permission_granted_sets_only_the_reserved_bits_to_zero() {
    // Bits 1-2 clear, bits 3-12 set, the rest set: 0xFFFFFFFC.
    let path = protect(options(true, true, true, true), "all-on");
    assert_eq!(permission_bits(&path), -4);
}

#[test]
fn one_denied_permission_clears_exactly_its_bits() {
    // Table 22: printing is bits 3 and 12, copying bit 5, editing bits 4 and 11,
    // commenting bits 6 and 9 (filling forms), accessibility bit 10 stays set.
    assert_eq!(permission_bits(&protect(options(true, false, true, true), "no-copy")), -20);
    assert_eq!(permission_bits(&protect(options(false, true, true, true), "no-print")), -2056);
    assert_eq!(permission_bits(&protect(options(true, true, false, true), "no-edit")), -1036);
    // Filling existing form fields (bit 9) is tied to commenting here: the options
    // expose no separate flag, and a form you may not annotate is not one you fill.
    assert_eq!(permission_bits(&protect(options(true, true, true, false), "no-comment")), -292);
}

#[test]
fn every_permission_denied_clears_all_eight_but_keeps_accessibility() {
    // 0xFFFFF2C0: the eight permission bits clear, bit 10 (accessibility) set.
    let path = protect(options(false, false, false, false), "all-off");
    assert_eq!(permission_bits(&path), -3392);
}

#[test]
fn a_protected_file_opens_with_the_empty_user_password() {
    let path = protect(options(false, false, false, false), "opens");
    // An empty user password means the file opens without a prompt - that is what
    // makes restrictions declarative: nothing stops the reader from ignoring them.
    let doc = pdfcore::docutil::load_document(&path, Some("")).expect("the empty user password opens it");
    assert_eq!(doc.get_pages().len(), 1);
    assert!(
        pdfcore::docutil::load_document(&path, None).is_ok(),
        "a file with an empty user password needs no password to open"
    );
}

/// Known defect, found by this probe and not fixed yet: see the plan entry.
#[ignore = "BUG: an owner password equal to the user password leaves the restrictions unenforceable"]
#[test]
fn an_owner_password_that_matches_the_user_password_is_refused_when_restrictions_apply() {
    // If the opener is also the owner, every restriction the file declares is
    // meaningless: a reader that verifies the owner password first grants full
    // rights (ISO 32000-1 Algorithm 2.A).
    let source = source("shared-password");
    let path = std::env::temp_dir().join("omnioffice-permissions-shared-password.pdf");
    let shared = ProtectOptions {
        user_password: "same".into(),
        owner_password: "same".into(),
        allow_printing: false,
        allow_copying: false,
        allow_editing: false,
        allow_commenting: false,
    };
    let result = protect_pdf(&source, &path, &shared, OverwritePolicy::Replace, None);
    assert!(result.is_err(), "a file whose user and owner passwords match cannot enforce its restrictions");
}
