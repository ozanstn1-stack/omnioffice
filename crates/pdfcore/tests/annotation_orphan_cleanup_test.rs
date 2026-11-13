//! Orphan annotation cleanup on full-rewrite saves (E6.7).

mod common;

use common::*;
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};
use pdfcore::annotate::prune_orphan_annotations;
use pdfcore::compress::{estimate_compression, CompressOptions};
use pdfcore::docutil::{load_document, save_document};
use pdfcore::sign::{sign_pdf, verify_signatures, SignOptions};
use rsa::pkcs8::{EncodePrivateKey, LineEnding};
use rsa::RsaPrivateKey;

fn test_identity() -> (Vec<u8>, Vec<u8>) {
    let key = RsaPrivateKey::new(&mut rand::rngs::OsRng, 2048).expect("rsa key");
    let pem = key.to_pkcs8_pem(LineEnding::LF).expect("pkcs8 pem");
    let key_pair = rcgen::KeyPair::from_pkcs8_pem_and_sign_algo(&pem, &rcgen::PKCS_RSA_SHA256).expect("keypair");
    let mut params = rcgen::CertificateParams::new(Vec::<String>::new()).expect("params");
    params.distinguished_name.push(rcgen::DnType::CommonName, "Orphan Cleanup Test Signer");
    let cert = params.self_signed(&key_pair).expect("self signed");
    (cert.der().to_vec(), key.to_pkcs8_der().expect("pkcs8 der").as_bytes().to_vec())
}

fn reals(values: &[f64]) -> Object {
    Object::Array(values.iter().map(|value| Object::Real(*value as f32)).collect())
}

fn note(contents: &str) -> Dictionary {
    dictionary! {
        "Type" => "Annot", "Subtype" => "Text", "F" => 4,
        "Rect" => reals(&[300.0, 600.0, 320.0, 620.0]),
        "Contents" => Object::string_literal(contents),
    }
}

fn ap_stream(doc: &mut Document, payload_len: usize) -> ObjectId {
    let content = vec![b' '; payload_len];
    doc.add_object(Object::Stream(Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => reals(&[0.0, 0.0, 20.0, 20.0]) },
        content,
    )))
}

#[test]
fn prunes_orphan_popups_replies_and_unreferenced_annots_on_save() {
    let dir = TestDir::new();
    let mut doc = build_text_doc(1, "Orphan", "Orphan Cleanup");
    let page_id = doc.get_pages().get(&1).copied().expect("page");

    let live_comment = doc.add_object(note("Kept comment"));
    let mut live_with_stale_popup = note("Surviving with dead popup");
    let dead_comment = doc.add_object(note("Removed elsewhere"));

    let orphan_popup = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Popup",
        "Rect" => reals(&[320.0, 500.0, 480.0, 600.0]),
        "Parent" => Object::Reference((9999, 0)),
    });
    let mut orphan_reply = note("Reply to missing parent");
    orphan_reply.set("IRT", Object::Reference(dead_comment));
    let orphan_reply_ap = ap_stream(&mut doc, 4096);
    orphan_reply.set("AP", dictionary! { "N" => Object::Reference(orphan_reply_ap) });
    let orphan_reply_id = doc.add_object(orphan_reply);

    let mut chained_reply = note("Reply to reply");
    chained_reply.set("IRT", Object::Reference(orphan_reply_id));
    let chained_reply_id = doc.add_object(chained_reply);

    live_with_stale_popup.set("Popup", Object::Reference(orphan_popup));
    let survivor_with_stale_id = doc.add_object(live_with_stale_popup);

    // Unattached /Type /Annot object sitting directly in doc.objects with a heavy stream.
    let dead_ap = ap_stream(&mut doc, 8192);
    doc.objects
        .get_mut(&dead_comment)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("AP", dictionary! { "N" => Object::Reference(dead_ap) });

    // Hierarchical AcroForm widget whose /Parent is a field dict (must survive).
    let field_id = doc.new_object_id();
    let widget_id = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Widget", "FT" => "Tx",
        "T" => Object::string_literal("applicant"),
        "Parent" => Object::Reference(field_id),
        "Rect" => reals(&[72.0, 500.0, 220.0, 520.0]),
    });
    doc.objects.insert(
        field_id,
        Object::Dictionary(dictionary! {
            "T" => Object::string_literal("applicant"),
            "FT" => "Tx",
            "Kids" => vec![Object::Reference(widget_id)],
        }),
    );
    let acroform_id = doc.add_object(dictionary! { "Fields" => vec![Object::Reference(field_id)] });
    let catalog_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
    doc.get_dictionary_mut(catalog_id).unwrap().set("AcroForm", Object::Reference(acroform_id));

    doc.get_dictionary_mut(page_id).unwrap().set(
        "Annots",
        Object::Array(vec![
            Object::Reference(live_comment),
            Object::Reference(survivor_with_stale_id),
            Object::Reference(orphan_popup),
            Object::Reference(orphan_reply_id),
            Object::Reference(chained_reply_id),
            Object::Reference(widget_id),
        ]),
    );

    let removed = prune_orphan_annotations(&mut doc);
    assert_eq!(removed, 4, "orphan popup, reply, chained reply, and unattached annot are removed");

    let out = dir.path("cleaned.pdf");
    save_document(&mut doc, &out, false).expect("save cleaned");
    let reloaded = load_document(&out, None).expect("reload");
    let page = reloaded.get_pages().get(&1).copied().unwrap();
    let annots = reloaded.get_dictionary(page).unwrap().get(b"Annots").unwrap().as_array().unwrap();
    assert_eq!(annots.len(), 3, "only the two live comments and form widget stay on page");

    for entry in annots {
        let id = entry.as_reference().unwrap();
        let dict = reloaded.get_dictionary(id).unwrap();
        assert!(dict.get(b"Popup").is_err(), "stale /Popup link was cleared");
    }

    // No 8KiB/4KiB orphan appearance payload streams remain in the saved file.
    assert!(!reloaded.objects.values().any(|obj| { obj.as_stream().is_ok_and(|stream| stream.content.len() >= 4096) }));

    // Lossless compression estimate also reflects orphan cleanup cleanly.
    let estimate = estimate_compression(
        &out,
        &CompressOptions { strategy: "lossless".into(), ..CompressOptions::default() },
        None,
    )
    .expect("estimate");
    assert!(estimate.estimated_bytes > 0);
}

#[test]
fn signed_document_with_incremental_update_stays_byte_valid() {
    let dir = TestDir::new();
    let base_doc = build_text_doc(1, "Signed", "Signed Orphans");
    let input = dir.path("signed-in.pdf");
    let mut mutable = base_doc;
    // Inject an unattached annotation before saving clean bytes for signing.
    let page_id = mutable.get_pages().get(&1).copied().unwrap();
    let active = mutable.add_object(note("Signed note"));
    mutable.get_dictionary_mut(page_id).unwrap().set("Annots", vec![Object::Reference(active)]);
    save_document(&mut mutable, &input, false).expect("save");

    let mut input_bytes = std::fs::read(&input).unwrap();
    // Append an incremental modification using the real signer on a clean copy.
    let (cert_der, key_der) = test_identity();
    let options = SignOptions { page: 1, ..SignOptions::default() };
    let mut signed = input.clone();
    signed.set_file_name("signed.pdf");
    sign_pdf(&input_bytes, &cert_der, &key_der, &[], &options)
        .map(|bytes| {
            std::fs::write(&signed, &bytes).unwrap();
            input_bytes = bytes;
        })
        .expect("sign");

    // Appending an editable annotation with incremental mode preserves the signed prefix.
    let added = pdfcore::annotate::Annotation {
        kind: "note".into(),
        page: 1,
        x: 72.0,
        y: 72.0,
        w: 120.0,
        h: 40.0,
        text: "After signing".into(),
        font_size_pt: 12.0,
        bold: false,
        color: "#dc2626".into(),
        opacity: 1.0,
        image_path: None,
        line_width_pt: 1.0,
        x2: None,
        y2: None,
        strokes: vec![],
        image_base64: None,
    };
    let appended =
        pdfcore::annotate::annotate_editable_pdf_incremental(&input_bytes, &[added]).expect("incremental annotate");
    assert!(appended.starts_with(&input_bytes), "signed byte prefix stays untouched");
    let report = verify_signatures(&appended);
    assert_eq!(report.signatures.len(), 1);
    assert!(report.signatures[0].signature_valid);
    assert!(report.signatures[0].digest_matches);
}
