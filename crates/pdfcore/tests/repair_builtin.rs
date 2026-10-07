//! The built-in repair (no qpdf): a valid multi-page PDF is damaged in several
//! typical ways and must come back with its pages and content streams, and
//! hostile or random input must neither panic nor take long.

mod common;

use common::*;
use lopdf::xref::XrefType;
use lopdf::{Document, Object};
use pdfcore::docutil::{load_document, OverwritePolicy};
use pdfcore::progress::CancelToken;
use pdfcore::rebuild::{rebuild_pdf, RebuiltPdf};
use pdfcore::repair::{repair_pdf_builtin, RepairMethod};
use pdfcore::security::{protect_pdf, ProtectOptions};
use std::time::{Duration, Instant};

const PAGES: u32 = 5;

/// A valid file with a classic cross-reference table (lopdf writes a
/// cross-reference stream by default).
fn valid_pdf() -> Vec<u8> {
    let mut doc = build_text_doc(PAGES, "Sample", "Repair sample");
    doc.reference_table.cross_reference_type = XrefType::CrossReferenceTable;
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("serialize");
    bytes
}

fn find(data: &[u8], needle: &[u8]) -> Option<usize> {
    data.windows(needle.len()).position(|window| window == needle)
}

fn rfind(data: &[u8], needle: &[u8]) -> Option<usize> {
    data.windows(needle.len()).rposition(|window| window == needle)
}

/// Offset of the cross-reference section (not `startxref`).
fn xref_start(data: &[u8]) -> usize {
    rfind(data, b"\nxref").expect("xref table") + 1
}

fn replace_all(data: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut pos = 0;
    while let Some(offset) = find(&data[pos..], from) {
        out.extend_from_slice(&data[pos..pos + offset]);
        out.extend_from_slice(to);
        pos += offset + from.len();
    }
    out.extend_from_slice(&data[pos..]);
    out
}

fn rebuild(data: &[u8]) -> RebuiltPdf {
    rebuild_pdf(data, &CancelToken::new()).expect("rebuild")
}

/// The content stream text of every page, in page order.
fn page_contents(doc: &Document) -> Vec<String> {
    doc.get_pages().values().map(|&page| String::from_utf8_lossy(&doc.get_page_content(page)).into_owned()).collect()
}

/// Rebuilds `damaged` and checks the result has `PAGES` pages whose content
/// streams carry their page label.
fn assert_repaired(damaged: &[u8]) -> Document {
    let rebuilt = rebuild(damaged);
    assert_eq!(rebuilt.pages, PAGES);
    let doc = Document::load_mem(&rebuilt.bytes).expect("rebuilt file loads");
    let contents = page_contents(&doc);
    assert_eq!(contents.len(), PAGES as usize);
    for (index, content) in contents.iter().enumerate() {
        assert!(content.contains(&format!("(Sample page {}) Tj", index + 1)), "page {}: {content}", index + 1);
    }
    doc
}

#[test]
fn undamaged_file_round_trips() {
    assert_repaired(&valid_pdf());
}

#[test]
fn zeroed_xref_offsets_are_ignored() {
    let data = valid_pdf();
    let xref = xref_start(&data);
    let mut damaged = data.clone();
    let mut pos = xref;
    // Entries look like `0000000015 00000 n`.
    while pos + 18 <= damaged.len() {
        let entry = &damaged[pos..pos + 18];
        if entry[..10].iter().all(u8::is_ascii_digit) && entry[10] == b' ' && entry[16] == b' ' {
            damaged[pos..pos + 10].fill(b'0');
            pos += 18;
        } else {
            pos += 1;
        }
    }
    assert!(damaged != data, "no xref entry was changed");
    assert_repaired(&damaged);
}

#[test]
fn missing_xref_and_trailer() {
    let data = valid_pdf();
    let xref = xref_start(&data);
    assert_repaired(&data[..xref]);
}

#[test]
fn cross_reference_stream_files_are_repaired() {
    // lopdf's default output: objects plus a cross-reference stream.
    let mut doc = build_text_doc(PAGES, "Sample", "Repair sample");
    let mut data = Vec::new();
    doc.save_to(&mut data).expect("serialize");
    assert!(find(&data, b"/XRef").is_some());
    assert_repaired(&data);
    // Cut inside the cross-reference stream.
    assert_repaired(&data[..data.len() - 40]);
}

#[test]
fn trailer_without_xref_table() {
    let data = valid_pdf();
    let xref = xref_start(&data);
    let trailer = rfind(&data, b"trailer").expect("trailer");
    let mut damaged = data[..xref].to_vec();
    damaged.extend_from_slice(&data[trailer..]);
    assert_repaired(&damaged);
}

#[test]
fn truncated_tail_keeps_the_complete_objects() {
    let data = valid_pdf();
    let xref = xref_start(&data);
    // Cut the cross-reference data and the last bytes of the final object.
    let rebuilt = rebuild(&data[..xref - 20]);
    let doc = Document::load_mem(&rebuilt.bytes).expect("rebuilt file loads");
    assert!(rebuilt.pages >= PAGES - 1, "pages: {}", rebuilt.pages);
    assert_eq!(doc.get_pages().len() as u32, rebuilt.pages);
    assert!(page_contents(&doc)[0].contains("(Sample page 1) Tj"));
}

#[test]
fn truncated_mid_file_keeps_the_first_pages() {
    let data = valid_pdf();
    let third_page_content = find(&data, b"(Sample page 3)").expect("page 3");
    let rebuilt = rebuild(&data[..third_page_content]);
    let doc = Document::load_mem(&rebuilt.bytes).expect("rebuilt file loads");
    assert_eq!(doc.get_pages().len() as u32, rebuilt.pages);
    assert!(rebuilt.pages >= 2, "pages: {}", rebuilt.pages);
    let contents = page_contents(&doc);
    assert!(contents[0].contains("(Sample page 1) Tj"));
    assert!(contents[1].contains("(Sample page 2) Tj"));
}

#[test]
fn one_unparsable_object_is_skipped() {
    let data = valid_pdf();
    let marker = b"/Subtype/Type1";
    let at = find(&data, marker).expect("font object");
    let mut damaged = data[..at].to_vec();
    damaged.extend_from_slice(b"/Subtype (((<<[[ ]]>>>>/BaseFont");
    damaged.extend_from_slice(&data[at + marker.len()..]);
    let doc = assert_repaired(&damaged);
    // The page tree and the other fonts survive.
    assert_eq!(doc.get_pages().len() as u32, PAGES);
}

#[test]
fn wrong_stream_length_is_recovered_from_endstream() {
    let data = valid_pdf();
    // Too short, and too long.
    for length in [&b"/Length 3"[..], &b"/Length 99999"[..]] {
        let mut damaged = data.clone();
        let mut pos = 0;
        let mut changed = 0;
        while let Some(offset) = find(&damaged[pos..], b"/Length ") {
            let start = pos + offset;
            let end = start + damaged[start + 8..].iter().take_while(|byte| byte.is_ascii_digit()).count() + 8;
            damaged.splice(start..end, length.iter().copied());
            pos = start + length.len();
            changed += 1;
        }
        assert!(changed >= PAGES, "only {changed} /Length entries");
        assert_repaired(&damaged);
    }
}

#[test]
fn missing_endobj_and_garbage_between_objects() {
    let data = valid_pdf();
    let without_endobj = replace_all(&data, b"endobj", b"");
    assert_repaired(&without_endobj);
    let noisy = replace_all(&data, b"endobj", b"endobj\n\x00\xff garbage 12 0 R << ( \n");
    assert_repaired(&noisy);
}

#[test]
fn appended_incremental_update_wins() {
    let data = valid_pdf();
    let doc = Document::load_mem(&data).expect("load");
    let page = *doc.get_pages().get(&1).expect("page 1");
    let content_id = doc.get_page_contents(page)[0];
    let root = doc.trailer.get(b"Root").and_then(Object::as_reference).expect("root");
    let content = b"BT /F1 24 Tf 72 700 Td (Sample page 1 updated) Tj ET";
    let mut damaged = data.clone();
    damaged.extend_from_slice(
        format!("\n{} {} obj\n<< /Length {} >>\nstream\n", content_id.0, content_id.1, content.len()).as_bytes(),
    );
    damaged.extend_from_slice(content);
    damaged.extend_from_slice(b"\nendstream\nendobj\n");
    // A real update would carry an xref section; its offsets are wrong here.
    damaged.extend_from_slice(
        format!(
            "xref\n0 1\n0000000000 65535 f \n{} 1\n0000000009 00000 n \ntrailer\n<< /Size {} /Root {} 0 R /Prev 9 >>\nstartxref\n{}\n%%EOF\n",
            content_id.0,
            doc.max_id + 1,
            root.0,
            data.len() + 5,
        )
        .as_bytes(),
    );
    let rebuilt = rebuild(&damaged);
    assert_eq!(rebuilt.pages, PAGES);
    let repaired = Document::load_mem(&rebuilt.bytes).expect("rebuilt file loads");
    let contents = page_contents(&repaired);
    assert!(contents[0].contains("(Sample page 1 updated) Tj"), "{}", contents[0]);
    assert!(!contents[0].contains("(Sample page 1) Tj"));
    assert!(contents[1].contains("(Sample page 2) Tj"));
}

#[test]
fn lost_catalog_and_page_tree_are_rebuilt_in_file_order() {
    let data = valid_pdf();
    let mut damaged = replace_all(&data, b"/Type/Catalog", b"/Type/Gone");
    damaged = replace_all(&damaged, b"/Type/Pages", b"/Type/Gone");
    let xref = xref_start(&damaged);
    damaged.truncate(xref);
    let rebuilt = rebuild(&damaged);
    assert_eq!(rebuilt.pages, PAGES);
    assert!(rebuilt.warnings.iter().any(|warning| warning.contains("catalog") || warning.contains("page tree")));
    let doc = Document::load_mem(&rebuilt.bytes).expect("rebuilt file loads");
    let contents = page_contents(&doc);
    for (index, content) in contents.iter().enumerate() {
        assert!(content.contains(&format!("(Sample page {}) Tj", index + 1)));
    }
}

#[test]
fn repair_writes_a_file_and_reports_the_method() {
    let dir = TestDir::new();
    let input = dir.path("broken.pdf");
    let output = dir.path("fixed.pdf");
    let data = valid_pdf();
    let xref = xref_start(&data);
    std::fs::write(&input, &data[..xref]).unwrap();
    let report = repair_pdf_builtin(&input, &output, &no_progress, &CancelToken::new()).expect("repair");
    assert_eq!(report.method, Some(RepairMethod::Builtin));
    assert_eq!(report.pages, PAGES);
    assert_eq!(page_count(&output), PAGES);
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["method"], "builtin");
}

#[test]
fn repair_pdf_without_qpdf_falls_back_to_the_builtin_engine() {
    if pdfcore::engines::qpdf_path().is_some() {
        return;
    }
    let dir = TestDir::new();
    let input = dir.path("broken.pdf");
    let output = dir.path("fixed.pdf");
    let data = valid_pdf();
    let xref = xref_start(&data);
    std::fs::write(&input, &data[..xref]).unwrap();
    let report = pdfcore::repair::repair_pdf(&input, &output, &no_progress, &CancelToken::new()).expect("repair");
    assert_eq!(report.method, Some(RepairMethod::Builtin));
    assert_eq!(page_count(&output), PAGES);
}

#[test]
fn encryption_is_kept() {
    let dir = TestDir::new();
    let plain = dir.path("plain.pdf");
    write_doc(&mut build_text_doc(PAGES, "Sample", "Secret"), &plain);
    let protected = dir.path("protected.pdf");
    protect_pdf(
        &plain,
        &protected,
        &ProtectOptions {
            user_password: "open".into(),
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
    let data = std::fs::read(&protected).unwrap();
    // The cross-reference stream is cut off; its dictionary still names the
    // catalog, the encryption dictionary and the file ID.
    let broken = dir.path("broken.pdf");
    std::fs::write(&broken, &data[..data.len() - 40]).unwrap();
    let fixed = dir.path("fixed.pdf");
    let report = repair_pdf_builtin(&broken, &fixed, &no_progress, &CancelToken::new()).expect("repair");
    assert_eq!(report.pages, PAGES);
    assert!(matches!(load_document(&fixed, None), Err(pdfcore::PdfError::PasswordRequired)));
    let doc = load_document(&fixed, Some("open")).expect("opens with the original password");
    assert_eq!(doc.get_pages().len() as u32, PAGES);
    let first = *doc.get_pages().get(&1).unwrap();
    let content = String::from_utf8_lossy(&doc.get_page_content(first)).into_owned();
    assert!(content.contains("(Sample page 1) Tj"), "{content}");
}

#[test]
fn nothing_to_recover_is_an_error() {
    let cancel = CancelToken::new();
    for data in [&b""[..], b"hello", b"%PDF-1.7\nnot a pdf\n%%EOF", b"1 0 obj\n<< /A 1 >>\nendobj\n"] {
        assert!(rebuild_pdf(data, &cancel).is_err());
    }
}

#[test]
fn cancellation_is_honoured() {
    let cancel = CancelToken::new();
    cancel.cancel();
    assert!(matches!(rebuild_pdf(&valid_pdf(), &cancel), Err(pdfcore::PdfError::Cancelled)));
}

// ---------------------------------------------------------------------------
// Hostile input
// ---------------------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, bound: usize) -> usize {
        usize::try_from(self.next() % bound.max(1) as u64).unwrap_or(0)
    }
}

/// Rebuilds `data`; a rebuilt file must always load again.
fn rebuild_must_not_panic(data: &[u8]) {
    let started = Instant::now();
    if let Ok(rebuilt) = rebuild_pdf(data, &CancelToken::new()) {
        let doc = Document::load_mem(&rebuilt.bytes).expect("an Ok rebuild loads");
        assert_eq!(doc.get_pages().len() as u32, rebuilt.pages);
    }
    assert!(started.elapsed() < Duration::from_secs(20), "rebuild took {:?}", started.elapsed());
}

#[test]
fn fuzzed_input_does_not_panic_and_finishes_quickly() {
    let started = Instant::now();
    let base = valid_pdf();
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    for round in 0..300 {
        let mut data = base.clone();
        match round % 5 {
            // Truncation at a random point.
            0 => data.truncate(rng.below(data.len())),
            // Random byte flips.
            1 => {
                for _ in 0..1 + rng.below(40) {
                    let at = rng.below(data.len());
                    data[at] = rng.next() as u8;
                }
            }
            // Deleted chunks.
            2 => {
                for _ in 0..1 + rng.below(4) {
                    let at = rng.below(data.len());
                    let end = (at + rng.below(200)).min(data.len());
                    data.drain(at..end);
                }
            }
            // Duplicated and shuffled chunks.
            3 => {
                let at = rng.below(data.len());
                let end = (at + rng.below(600)).min(data.len());
                let chunk = data[at..end].to_vec();
                let to = rng.below(data.len());
                data.splice(to..to, chunk);
            }
            // Flips and truncation together.
            _ => {
                for _ in 0..1 + rng.below(10) {
                    let at = rng.below(data.len());
                    data[at] = rng.next() as u8;
                }
                data.truncate(1 + rng.below(data.len()));
            }
        }
        rebuild_must_not_panic(&data);
    }
    for _ in 0..200 {
        let mut data = vec![0u8; rng.below(4096)];
        for byte in &mut data {
            *byte = rng.next() as u8;
        }
        rebuild_must_not_panic(&data);
        // Random bytes around PDF tokens.
        let tokens: [&[u8]; 9] =
            [b" 1 0 obj ", b"<<", b">>", b"stream\n", b"endstream", b"endobj", b"trailer", b"/Type /Page ", b"[ ( "];
        let mut tokens_data = Vec::new();
        for _ in 0..rng.below(200) {
            tokens_data.extend_from_slice(tokens[rng.below(tokens.len())]);
            tokens_data.push(rng.next() as u8);
        }
        rebuild_must_not_panic(&tokens_data);
    }
    assert!(started.elapsed() < Duration::from_secs(120), "fuzzing took {:?}", started.elapsed());
}

#[test]
fn pathological_structures_finish_quickly() {
    let started = Instant::now();
    // Deep nesting, unterminated.
    let mut cases: Vec<Vec<u8>> = vec![[b"1 0 obj\n".to_vec(), b"[".repeat(200_000)].concat()];
    cases.push([b"1 0 obj\n".to_vec(), b"<<".repeat(100_000)].concat());
    // Many headers with unterminated values.
    cases.push(b"1 0 obj << /A (".repeat(50_000));
    // Many streams without endstream.
    cases.push(b"1 0 obj\n<< /Length 99999999 >>\nstream\n".repeat(20_000));
    // Many tiny valid objects.
    let mut many = Vec::new();
    for number in 1..=50_000u32 {
        many.extend_from_slice(format!("{number} 0 obj\n<< /Type /Page >>\nendobj\n").as_bytes());
    }
    cases.push(many);
    // Escape-heavy strings.
    let mut escapes = b"1 0 obj\n<< /Type /Page /T (".to_vec();
    escapes.extend(std::iter::repeat_n(b"\\(".iter().copied(), 200_000).flatten());
    escapes.extend_from_slice(b") >>\nendobj\n");
    cases.push(escapes);
    // A huge object number and a huge generation.
    cases.push(b"99999999999 0 obj << >> endobj 1 99999999 obj << >> endobj".to_vec());
    // Self-referencing page tree.
    cases.push(
        b"1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj 2 0 obj << /Type /Pages /Kids [2 0 R 1 0 R] /Parent 2 0 R >> endobj trailer << /Root 1 0 R >>"
            .to_vec(),
    );
    for case in &cases {
        rebuild_must_not_panic(case);
    }
    assert!(started.elapsed() < Duration::from_secs(120), "took {:?}", started.elapsed());
}
