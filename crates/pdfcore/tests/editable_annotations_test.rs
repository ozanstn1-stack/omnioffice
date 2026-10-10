//! Real (editable) annotations: writing into `/Annots`, listing back into
//! display space, and moving/resizing/updating/deleting them as appended
//! revisions.

mod common;

use common::{build_text_doc, no_progress, write_doc, TestDir};
use lopdf::{dictionary, Document, Object};
use pdfcore::annotate::{
    annotate_editable_pdf, annotate_editable_pdf_incremental, edit_annotations, list_annotations,
    list_annotations_in_file, Annotation, AnnotationAction, AnnotationEditItem, AnnotationEditReport,
    EditableAnnotation,
};
use pdfcore::docutil::OverwritePolicy;
use pdfcore::progress::CancelToken;
use pdfcore::sign::{self, SignOptions};
use rsa::pkcs8::{EncodePrivateKey, LineEnding};
use rsa::RsaPrivateKey;
use std::sync::OnceLock;

fn png_base64() -> String {
    let image = image::RgbaImage::from_fn(8, 8, |x, y| image::Rgba([200, (x * 24) as u8, (y * 24) as u8, 255]));
    let mut bytes = Vec::new();
    image.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).expect("png");
    pdfcore::images::base64_encode(&bytes)
}

fn base(kind: &str, x: f64, y: f64, w: f64, h: f64) -> Annotation {
    Annotation {
        kind: kind.into(),
        page: 1,
        x,
        y,
        w,
        h,
        text: String::new(),
        font_size_pt: 12.0,
        bold: false,
        color: "#e11d48".into(),
        opacity: 1.0,
        image_path: None,
        line_width_pt: 2.0,
        x2: None,
        y2: None,
        strokes: Vec::new(),
        image_base64: None,
    }
}

fn editable_annotations() -> Vec<Annotation> {
    vec![
        Annotation { text: "Kontrol edilecek".into(), color: "#f59e0b".into(), ..base("note", 60.0, 60.0, 20.0, 20.0) },
        Annotation { color: "#facc15".into(), opacity: 0.35, ..base("highlight", 60.0, 120.0, 200.0, 24.0) },
        Annotation { color: "#e11d48".into(), ..base("underline", 60.0, 160.0, 200.0, 24.0) },
        Annotation { color: "#e11d48".into(), ..base("strike", 60.0, 200.0, 200.0, 24.0) },
        Annotation {
            color: "#0ea5e9".into(),
            line_width_pt: 3.0,
            strokes: vec![vec![[60.0, 260.0], [120.0, 300.0], [180.0, 260.0]]],
            ..base("ink", 60.0, 260.0, 120.0, 40.0)
        },
        Annotation {
            text: "Onaylandı şğüöç".into(),
            color: "#0f172a".into(),
            font_size_pt: 16.0,
            ..base("textbox", 60.0, 340.0, 220.0, 40.0)
        },
        Annotation { image_base64: Some(png_base64()), ..base("signature", 300.0, 340.0, 120.0, 60.0) },
    ]
}

fn source_doc(dir: &TestDir, name: &str) -> std::path::PathBuf {
    let input = dir.path(&format!("{name}.pdf"));
    write_doc(&mut build_text_doc(1, name, name), &input);
    input
}

#[test]
fn editable_annotations_roundtrip_every_kind() {
    let dir = TestDir::new();
    let input = source_doc(&dir, "roundtrip");
    let output = dir.path("roundtrip-out.pdf");
    annotate_editable_pdf(
        &input,
        &output,
        &editable_annotations(),
        OverwritePolicy::Replace,
        None,
        &no_progress,
        &CancelToken::new(),
    )
    .expect("write editable annotations");

    let listed = list_annotations_in_file(&output, None).expect("list");
    let kinds: Vec<&str> = listed.iter().map(|annotation| annotation.kind.as_str()).collect();
    assert_eq!(kinds, ["note", "highlight", "underline", "strike", "ink", "textbox", "signature"]);

    let note = &listed[0];
    assert!((note.x - 60.0).abs() < 0.01 && (note.y - 60.0).abs() < 0.01);
    assert!((note.w - 20.0).abs() < 0.01 && (note.h - 20.0).abs() < 0.01);
    assert_eq!(note.text, "Kontrol edilecek");
    assert_eq!(note.color, "#f59e0b");

    let highlight = &listed[1];
    assert!((highlight.x - 60.0).abs() < 0.01 && (highlight.y - 120.0).abs() < 0.01);
    assert!((highlight.w - 200.0).abs() < 0.5 && (highlight.h - 24.0).abs() < 0.5);
    assert!((highlight.opacity - 0.35).abs() < 1e-6);
    assert_eq!(highlight.color, "#facc15");

    let ink = &listed[4];
    assert_eq!(ink.strokes.len(), 1);
    assert_eq!(ink.strokes[0].len(), 3);
    assert!((ink.strokes[0][0][0] - 60.0).abs() < 0.5);
    assert!((ink.strokes[0][0][1] - 260.0).abs() < 0.5);
    assert!((ink.strokes[0][1][1] - 300.0).abs() < 0.5);
    assert!((ink.line_width_pt - 3.0).abs() < 0.01);

    let textbox = &listed[5];
    assert_eq!(textbox.text, "Onaylandı şğüöç");
    assert!((textbox.font_size_pt - 16.0).abs() < 0.01);
    assert!((textbox.w - 220.0).abs() < 0.5 && (textbox.h - 40.0).abs() < 0.5);

    // The page dictionary really carries annotation dictionaries, with the
    // expected /Subtype names and appearances where viewers need them.
    let doc = Document::load(&output).expect("reload");
    let page_id = *doc.get_pages().values().next().expect("page");
    let annots = doc.get_dictionary(page_id).unwrap().get(b"Annots").unwrap().as_array().unwrap().clone();
    assert_eq!(annots.len(), 7);
    let mut subtypes: Vec<String> = Vec::new();
    let mut stamp_name = String::new();
    for entry in &annots {
        let id = entry.as_reference().expect("editable annotations are indirect objects");
        let dict = doc.get_dictionary(id).expect("annotation dict");
        let subtype = dict.get(b"Subtype").and_then(Object::as_name).map(|n| String::from_utf8_lossy(n).to_string());
        let subtype = subtype.expect("subtype");
        if subtype == "Stamp" {
            stamp_name = dict
                .get(b"Name")
                .and_then(Object::as_name)
                .map(|name| String::from_utf8_lossy(name).to_string())
                .unwrap_or_default();
        }
        if subtype == "FreeText" || subtype == "Stamp" {
            assert!(dict.get(b"AP").is_ok(), "{subtype} must carry an appearance");
        }
        subtypes.push(subtype);
    }
    assert_eq!(subtypes, ["Text", "Highlight", "Underline", "StrikeOut", "Ink", "FreeText", "Stamp"]);
    assert_eq!(stamp_name, "OsakSignature");
}

#[test]
fn editable_annotations_can_be_moved_resized_updated_and_deleted() {
    let dir = TestDir::new();
    let input = source_doc(&dir, "edit");
    let full = dir.path("edit-full.pdf");
    annotate_editable_pdf(
        &input,
        &full,
        &editable_annotations(),
        OverwritePolicy::Replace,
        None,
        &no_progress,
        &CancelToken::new(),
    )
    .expect("write");
    let bytes = std::fs::read(&full).expect("read");

    let items = vec![
        AnnotationEditItem { page: 1, index: 0, action: AnnotationAction::Move { dx: 25.0, dy: -10.0 } },
        AnnotationEditItem {
            page: 1,
            index: 1,
            action: AnnotationAction::Resize { x: 100.0, y: 130.0, w: 150.0, h: 30.0 },
        },
        AnnotationEditItem {
            page: 1,
            index: 5,
            action: AnnotationAction::Update {
                text: Some("Düzeltildi".into()),
                color: Some("#2563eb".into()),
                opacity: None,
                line_width_pt: None,
            },
        },
        AnnotationEditItem {
            page: 1,
            index: 6,
            action: AnnotationAction::Resize { x: 320.0, y: 350.0, w: 100.0, h: 50.0 },
        },
        // The delete shifts indexes after it, so it runs last.
        AnnotationEditItem { page: 1, index: 3, action: AnnotationAction::Delete },
    ];
    let (edited, report) = edit_annotations(&bytes, &items).expect("edit");
    assert!(edited.starts_with(&bytes), "edits append a revision, they do not rewrite the file");
    assert_eq!(report.edited, 4);
    assert_eq!(report.deleted, 1);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);

    let reloaded = Document::load_mem(&edited).expect("reload");
    let listed = list_annotations(&reloaded);
    let kinds: Vec<&str> = listed.iter().map(|annotation| annotation.kind.as_str()).collect();
    assert_eq!(kinds, ["note", "highlight", "underline", "ink", "textbox", "signature"]);

    let note = &listed[0];
    assert!((note.x - 85.0).abs() < 0.5 && (note.y - 50.0).abs() < 0.5, "note rect should move: {note:?}");
    let highlight = &listed[1];
    assert!((highlight.x - 100.0).abs() < 0.5 && (highlight.y - 130.0).abs() < 0.5);
    assert!((highlight.w - 150.0).abs() < 0.5 && (highlight.h - 30.0).abs() < 0.5);
    let textbox = &listed[4];
    assert_eq!(textbox.text, "Düzeltildi");
    assert_eq!(textbox.color, "#2563eb");
    let signature = &listed[5];
    assert!((signature.x - 320.0).abs() < 0.5 && (signature.y - 350.0).abs() < 0.5);
    assert!((signature.w - 100.0).abs() < 0.5 && (signature.h - 50.0).abs() < 0.5);

    // Resizing a stamp wraps the old appearance in a rescaled form.
    let page_id = *reloaded.get_pages().values().next().unwrap();
    let annots = reloaded.get_dictionary(page_id).unwrap().get(b"Annots").unwrap().as_array().unwrap().clone();
    let signature_id = annots.last().unwrap().as_reference().unwrap();
    let stamp = reloaded.get_dictionary(signature_id).unwrap();
    let appearance = stamp.get(b"AP").unwrap().as_dict().unwrap().get(b"N").unwrap().as_reference().unwrap();
    let form = reloaded.get_object(appearance).unwrap().as_stream().unwrap();
    let bbox = form.dict.get(b"BBox").unwrap().as_array().unwrap();
    let bbox_w = bbox[2].as_float().unwrap();
    let bbox_h = bbox[3].as_float().unwrap();
    assert!((bbox_w - 100.0).abs() < 0.5 && (bbox_h - 50.0).abs() < 0.5, "appearance must be rescaled: {bbox:?}");

    // A stale handle is a warning, not a failed batch.
    let (_, report) =
        edit_annotations(&edited, &[AnnotationEditItem { page: 1, index: 99, action: AnnotationAction::Delete }])
            .expect("warning batch");
    assert_eq!(report.warnings.len(), 1);
    assert_eq!(report.deleted, 0);
}

#[test]
fn inline_annotations_can_be_edited_in_place() {
    let mut doc = build_text_doc(1, "Inline", "Inline");
    let page_id = *doc.get_pages().values().next().expect("page");
    let inline = Object::Dictionary(lopdf::dictionary! {
        "Type" => "Annot",
        "Subtype" => "Highlight",
        "Rect" => vec![
            Object::Real(100.0),
            Object::Real(100.0),
            Object::Real(200.0),
            Object::Real(120.0),
        ],
        "QuadPoints" => vec![
            Object::Real(100.0),
            Object::Real(120.0),
            Object::Real(200.0),
            Object::Real(120.0),
            Object::Real(100.0),
            Object::Real(100.0),
            Object::Real(200.0),
            Object::Real(100.0),
        ],
    });
    doc.get_object_mut(page_id).unwrap().as_dict_mut().unwrap().set("Annots", Object::Array(vec![inline]));
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save");

    let (edited, report) = edit_annotations(
        &bytes,
        &[AnnotationEditItem { page: 1, index: 0, action: AnnotationAction::Move { dx: 10.0, dy: 20.0 } }],
    )
    .expect("move inline annotation");
    assert_eq!(report.edited, 1);
    assert!(edited.starts_with(&bytes));

    let reloaded = Document::load_mem(&edited).expect("reload");
    let listed = list_annotations(&reloaded);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].kind, "highlight");
    assert!((listed[0].x - 110.0).abs() < 0.01, "{:?}", listed[0]);
    assert!((listed[0].y - 741.89).abs() < 0.5, "{:?}", listed[0]);
}

#[test]
fn foreign_freetext_and_ink_list_parse_without_panic() {
    let mut doc = build_text_doc(1, "Foreign", "Foreign");
    let page_id = *doc.get_pages().values().next().expect("page");
    let free_text = doc.add_object(Object::Dictionary(lopdf::dictionary! {
        "Type" => "Annot",
        "Subtype" => "FreeText",
        "Rect" => vec![
            Object::Real(100.0),
            Object::Real(200.0),
            Object::Real(300.0),
            Object::Real(240.0),
        ],
        "Contents" => Object::string_literal("Hand written"),
        "DA" => Object::string_literal("/Helvetica 14 Tf 0 0 0 rg"),
        "C" => vec![Object::Real(0.0), Object::Real(0.5), Object::Real(1.0)],
    }));
    // The second stroke is malformed on purpose: listing must skip it, not panic.
    let ink = doc.add_object(Object::Dictionary(lopdf::dictionary! {
        "Type" => "Annot",
        "Subtype" => "Ink",
        "Rect" => vec![
            Object::Real(50.0),
            Object::Real(50.0),
            Object::Real(150.0),
            Object::Real(100.0),
        ],
        "InkList" => vec![
            Object::Array(vec![
                Object::Real(60.0),
                Object::Real(60.0),
                Object::Real(120.0),
                Object::Real(90.0),
            ]),
            Object::Array(vec![Object::Integer(1)]),
        ],
    }));
    doc.get_object_mut(page_id)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("Annots", Object::Array(vec![Object::Reference(free_text), Object::Reference(ink)]));

    let listed = list_annotations(&doc);
    assert_eq!(listed.len(), 2);
    let textbox = &listed[0];
    assert_eq!(textbox.kind, "textbox");
    assert_eq!(textbox.text, "Hand written");
    assert!((textbox.font_size_pt - 14.0).abs() < 1e-9);
    assert_eq!(textbox.color, "#0080ff");
    assert!((textbox.x - 100.0).abs() < 0.01);
    assert!((textbox.y - 601.89).abs() < 0.5);
    assert!((textbox.w - 200.0).abs() < 0.01 && (textbox.h - 40.0).abs() < 0.01);

    let ink = &listed[1];
    assert_eq!(ink.kind, "ink");
    assert_eq!(ink.strokes.len(), 1, "the malformed stroke is skipped");
    assert_eq!(ink.strokes[0].len(), 2);
    assert_eq!(ink.color, "#e11d48", "ink falls back to its default colour");
    assert!((ink.font_size_pt - 12.0).abs() < 1e-9);
}

#[test]
fn rotated_pages_roundtrip_display_coordinates() {
    let dir = TestDir::new();
    let mut doc = build_text_doc(1, "Rotated", "Rotated");
    let page_id = *doc.get_pages().values().next().expect("page");
    doc.get_object_mut(page_id).unwrap().as_dict_mut().unwrap().set("Rotate", 90i64);
    let input = dir.path("rotated.pdf");
    write_doc(&mut doc, &input);

    let annotations = vec![Annotation { text: "Döndürülmüş".into(), ..base("note", 50.0, 60.0, 30.0, 30.0) }];
    let output = dir.path("rotated-out.pdf");
    annotate_editable_pdf(
        &input,
        &output,
        &annotations,
        OverwritePolicy::Replace,
        None,
        &no_progress,
        &CancelToken::new(),
    )
    .expect("write on rotated page");

    let listed = list_annotations_in_file(&output, None).expect("list");
    assert_eq!(listed.len(), 1);
    let note = &listed[0];
    assert!((note.x - 50.0).abs() < 0.5 && (note.y - 60.0).abs() < 0.5, "{note:?}");
    assert!((note.w - 30.0).abs() < 0.5 && (note.h - 30.0).abs() < 0.5, "{note:?}");

    // The raw /Rect lives in page space: on this 90° rotated page the display
    // rect (50, 60, 30, 30) maps to the page rect [60 50 90 80].
    let doc = Document::load(&output).expect("reload");
    let page_id = *doc.get_pages().values().next().expect("page");
    let annots = doc.get_dictionary(page_id).unwrap().get(b"Annots").unwrap().as_array().unwrap().clone();
    let dict = doc.get_dictionary(annots[0].as_reference().unwrap()).unwrap();
    let rect: Vec<f64> =
        dict.get(b"Rect").unwrap().as_array().unwrap().iter().map(|value| value.as_float().unwrap() as f64).collect();
    assert!((rect[0] - 60.0).abs() < 0.5 && (rect[1] - 50.0).abs() < 0.5, "{rect:?}");
    assert!((rect[2] - 90.0).abs() < 0.5 && (rect[3] - 80.0).abs() < 0.5, "{rect:?}");
}

#[test]
fn flatten_path_handles_the_new_kinds() {
    let dir = TestDir::new();
    let input = source_doc(&dir, "flatten");
    let output = dir.path("flatten-out.pdf");
    let annotations = vec![
        Annotation { ..base("note", 60.0, 60.0, 20.0, 20.0) },
        Annotation { ..base("underline", 60.0, 100.0, 200.0, 20.0) },
        Annotation { ..base("strike", 60.0, 140.0, 200.0, 20.0) },
        Annotation {
            line_width_pt: 2.0,
            strokes: vec![vec![[60.0, 200.0], [120.0, 220.0]]],
            ..base("ink", 60.0, 200.0, 60.0, 20.0)
        },
        Annotation { image_base64: Some(png_base64()), ..base("signature", 300.0, 200.0, 80.0, 40.0) },
    ];
    pdfcore::annotate::annotate_pdf(
        &input,
        &output,
        &annotations,
        OverwritePolicy::Replace,
        None,
        &no_progress,
        &CancelToken::new(),
    )
    .expect("flatten new kinds");

    let doc = Document::load(&output).expect("reload");
    let page_id = *doc.get_pages().values().next().expect("page");
    let content = doc.get_page_content(page_id);
    assert!(
        content.windows(6).any(|window| window == b" re\nf\n"),
        "the note marker must be filled: {}",
        String::from_utf8_lossy(&content)
    );
    assert!(
        content.windows(3).any(|window| window == b" m\n") && content.windows(3).any(|window| window == b" l\n"),
        "underline/strike/ink strokes must be drawn"
    );
    assert!(content.windows(3).any(|window| window == b"/AN"), "the signature image must be drawn");
}

fn identity() -> &'static (Vec<u8>, Vec<u8>) {
    static IDENTITY: OnceLock<(Vec<u8>, Vec<u8>)> = OnceLock::new();
    IDENTITY.get_or_init(|| {
        let key = RsaPrivateKey::new(&mut rand::rngs::OsRng, 2048).expect("rsa key");
        let pem = key.to_pkcs8_pem(LineEnding::LF).expect("pkcs8 pem");
        let key_pair = rcgen::KeyPair::from_pkcs8_pem_and_sign_algo(&pem, &rcgen::PKCS_RSA_SHA256)
            .expect("ring accepts the RSA key");
        let mut params = rcgen::CertificateParams::new(Vec::<String>::new()).expect("params");
        params.distinguished_name.push(rcgen::DnType::CommonName, "PDF SAK Editable Signer");
        let cert = params.self_signed(&key_pair).expect("self signed");
        (cert.der().to_vec(), key.to_pkcs8_der().expect("pkcs8 der").as_bytes().to_vec())
    })
}

#[test]
fn incremental_editable_annotations_keep_the_signature_valid() {
    let mut doc = build_text_doc(1, "Signed", "Signed");
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save");
    let (cert_der, key_der) = identity();
    let options = SignOptions {
        page: 1,
        rect: None,
        reason: "Approval".to_string(),
        location: "Istanbul".to_string(),
        contact: String::new(),
        appearance: true,
        signer_name: Some("Editable Signer".to_string()),
    };
    let signed = sign::sign_pdf(&bytes, cert_der, key_der, &[], &options).expect("sign");

    let updated = annotate_editable_pdf_incremental(&signed, &editable_annotations()).expect("incremental annotations");
    assert!(updated.starts_with(&signed), "the signed revision must stay byte-identical");
    assert_eq!(pdfcore::incremental::signature_count(&updated), 1);
    let verification = sign::verify_signatures(&updated);
    assert_eq!(verification.signatures.len(), 1);
    assert!(verification.signatures[0].digest_matches, "{:?}", verification.signatures[0].notes);
    assert!(verification.signatures[0].signature_valid, "{:?}", verification.signatures[0].notes);

    let reloaded = Document::load_mem(&updated).expect("reload");
    let listed = list_annotations(&reloaded);
    let editable: Vec<&EditableAnnotation> = listed.iter().filter(|annotation| annotation.kind != "widget").collect();
    assert_eq!(editable.len(), 7);
    assert!(listed.iter().any(|annotation| annotation.kind == "signature"));
}

#[test]
fn an_empty_annotation_list_is_rejected() {
    let dir = TestDir::new();
    let input = source_doc(&dir, "empty");
    let output = dir.path("empty-out.pdf");
    let error =
        annotate_editable_pdf(&input, &output, &[], OverwritePolicy::Replace, None, &no_progress, &CancelToken::new())
            .expect_err("empty list must be rejected");
    assert!(matches!(error, pdfcore::PdfError::InvalidInput(_)));

    let bytes = std::fs::read(&input).expect("read");
    assert!(annotate_editable_pdf_incremental(&bytes, &[]).is_err());
}

#[test]
fn editable_wire_shapes_match_the_typescript_contract() {
    let annotation = EditableAnnotation {
        page: 1,
        index: 0,
        kind: "ink".into(),
        x: 1.0,
        y: 2.0,
        w: 3.0,
        h: 4.0,
        text: "t".into(),
        color: "#000000".into(),
        opacity: 1.0,
        line_width_pt: 2.0,
        font_size_pt: 12.0,
        bold: false,
        strokes: vec![vec![[1.0, 2.0]]],
    };
    let json = serde_json::to_value(&annotation).unwrap();
    for key in ["page", "index", "kind", "lineWidthPt", "fontSizePt", "strokes"] {
        assert!(json.get(key).is_some(), "missing key {key} in {json}");
    }

    let item = AnnotationEditItem {
        page: 1,
        index: 2,
        action: AnnotationAction::Update { text: None, color: None, opacity: Some(0.5), line_width_pt: Some(3.0) },
    };
    let json = serde_json::to_value(&item).unwrap();
    assert_eq!(json["action"], "update");
    assert_eq!(json["opacity"], 0.5);
    assert_eq!(json["lineWidthPt"], 3.0);

    // The payload the webview sends must deserialize into the same action.
    let parsed: AnnotationEditItem = serde_json::from_value(
        serde_json::json!({ "page": 3, "index": 7, "action": "resize", "x": 1, "y": 2, "w": 3, "h": 4 }),
    )
    .expect("resize payload");
    assert_eq!(parsed.page, 3);
    assert_eq!(parsed.index, 7);
    assert!(matches!(parsed.action, AnnotationAction::Resize { w, .. } if w == 3.0));
    let parsed: AnnotationEditItem =
        serde_json::from_value(serde_json::json!({ "page": 1, "index": 0, "action": "update", "lineWidthPt": 1.5 }))
            .expect("update payload");
    assert!(matches!(parsed.action, AnnotationAction::Update { line_width_pt: Some(value), .. } if value == 1.5));

    let json = serde_json::to_value(AnnotationEditReport::default()).unwrap();
    assert!(json.get("edited").is_some() && json.get("deleted").is_some() && json.get("warnings").is_some());
}
