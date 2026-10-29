//! Evidence bundles (C3): cover, index, stamped exhibits and an embedded
//! manifest that verifies on reopen.

use lopdf::{dictionary, Document, Object, Stream};
use pdfcore::certificate::digest;
use pdfcore::evidence::{build_evidence_bundle, match_original, verify_evidence_bundle, EvidenceDetails, EvidenceFile};

/// An `pages`-page PDF whose page content carries `tag`, so two inputs differ.
fn pdf_with_pages(pages: u32, tag: &str) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let mut kids = Vec::new();
    for index in 0..pages {
        let content = format!("BT /F1 12 Tf 72 720 Td ({tag} {index}) Tj ET");
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.into_bytes()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Contents" => Object::Reference(content_id),
        });
        kids.push(Object::Reference(page_id));
    }
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Count" => pages as i64, "Kids" => kids }),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages_id) });
    doc.trailer.set("Root", Object::Reference(catalog_id));
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

fn details() -> EvidenceDetails {
    EvidenceDetails {
        title: "Delil Listesi".into(),
        case_reference: "2026/123 E.".into(),
        court: "İstanbul 3. Asliye Ticaret Mahkemesi".into(),
        submitted_by: "Av. Şule Güneş".into(),
        language: "tr".into(),
    }
}

fn files() -> Vec<EvidenceFile> {
    vec![
        EvidenceFile {
            label: "Kira sözleşmesi".into(),
            file_name: "sozlesme.pdf".into(),
            bytes: pdf_with_pages(2, "A"),
        },
        EvidenceFile { label: "İhtarname".into(), file_name: "ihtar.pdf".into(), bytes: pdf_with_pages(3, "B") },
    ]
}

#[test]
fn builds_a_cover_an_index_and_the_exhibits_in_order() {
    let inputs = files();
    let digests: Vec<String> = inputs.iter().map(|file| digest(&file.bytes).sha256).collect();
    let (bundle, manifest) = build_evidence_bundle(inputs, &details()).unwrap();

    let doc = Document::load_mem(&bundle).unwrap();
    // Cover + one index page + 2 + 3 exhibit pages.
    assert_eq!(doc.get_pages().len(), 7);
    assert_eq!(manifest.total_pages, 7);

    assert_eq!(manifest.entries.len(), 2);
    let first = &manifest.entries[0];
    assert_eq!((first.number, first.first_page, first.last_page, first.pages), (1, 3, 4, 2));
    assert_eq!(first.sha256, digests[0]);
    assert_eq!(first.label, "Kira sözleşmesi");
    let second = &manifest.entries[1];
    assert_eq!((second.number, second.first_page, second.last_page, second.pages), (2, 5, 7, 3));
    assert_eq!(second.sha256, digests[1]);
    assert!(manifest.id.starts_with("EVD-"));
    assert_eq!(manifest.details.case_reference, "2026/123 E.");
}

#[test]
fn a_fresh_bundle_verifies_on_reopen() {
    let (bundle, manifest) = build_evidence_bundle(files(), &details()).unwrap();
    let verification = verify_evidence_bundle(&bundle).unwrap();
    assert!(verification.present);
    assert!(verification.content_matches, "a fresh bundle must verify");
    assert_eq!(verification.manifest.unwrap().entries, manifest.entries);
    assert_eq!(verification.actual_sha256, manifest.bundle.sha256);
}

#[test]
fn an_edited_bundle_fails_verification() {
    let (bundle, _) = build_evidence_bundle(files(), &details()).unwrap();
    let mut doc = Document::load_mem(&bundle).unwrap();
    // Drop the last exhibit page: the manifest survives, the content does not match.
    let last = *doc.get_pages().keys().last().unwrap();
    doc.delete_pages(&[last]);
    let mut edited = Vec::new();
    doc.save_to(&mut edited).unwrap();
    let verification = verify_evidence_bundle(&edited).unwrap();
    assert!(verification.present, "the manifest survives the edit");
    assert!(!verification.content_matches, "an edited bundle must not verify");
}

#[test]
fn a_plain_pdf_carries_no_manifest() {
    let verification = verify_evidence_bundle(&pdf_with_pages(1, "plain")).unwrap();
    assert!(!verification.present);
    assert!(verification.manifest.is_none());
    assert!(!verification.content_matches);
}

#[test]
fn an_original_is_matched_to_its_exhibit_by_hash() {
    let inputs = files();
    let second = inputs[1].bytes.clone();
    let (_, manifest) = build_evidence_bundle(inputs, &details()).unwrap();
    assert_eq!(match_original(&manifest, &second), Some(2));
    assert_eq!(match_original(&manifest, &pdf_with_pages(3, "changed")), None);
}

#[test]
fn many_exhibits_spread_the_index_over_several_pages() {
    let inputs: Vec<EvidenceFile> = (0..40)
        .map(|index| EvidenceFile {
            label: format!("Ek {index}"),
            file_name: format!("ek-{index}.pdf"),
            bytes: pdf_with_pages(1, &format!("E{index}")),
        })
        .collect();
    let (bundle, manifest) = build_evidence_bundle(inputs, &details()).unwrap();
    let index_pages = manifest.entries[0].first_page - 2;
    assert!(index_pages >= 2, "40 exhibits need more than one index page, got {index_pages}");
    assert_eq!(manifest.total_pages, 1 + index_pages + 40);
    assert_eq!(Document::load_mem(&bundle).unwrap().get_pages().len() as u32, manifest.total_pages);
    let last = manifest.entries.last().unwrap();
    assert_eq!(last.last_page, manifest.total_pages);
}

#[test]
fn an_empty_list_and_a_broken_file_are_refused() {
    assert!(build_evidence_bundle(Vec::new(), &details()).is_err());
    let broken =
        vec![EvidenceFile { label: "Bozuk".into(), file_name: "bozuk.pdf".into(), bytes: b"not a pdf".to_vec() }];
    let error = build_evidence_bundle(broken, &details()).unwrap_err().to_string();
    assert!(error.contains("bozuk.pdf"), "the error names the file: {error}");
}
