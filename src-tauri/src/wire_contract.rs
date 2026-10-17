//! The IPC wire contract.
//!
//! One place that pins the exact JSON shape of the DTOs the screens receive,
//! checked with the **real serde** implementation. The screen tests mock
//! `invoke`, so a renamed field there only shows up as `undefined` at runtime
//! (the v5.0.0 audit's `headerFooter` / `taxIdValid` failures); this module
//! fails at `cargo test` time instead. A deliberate wire change updates the
//! expected list in the same commit.
//!
//! `CompressOptions` and `OcrOptions` are the two DTOs whose multi-word fields
//! stay **snake_case** on the wire (their structs carry no `rename_all`); the
//! lists below pin that too, so a future change to camelCase is deliberate.

use serde_json::Value;

/// The sorted top-level object keys of a serialized DTO.
fn keys(value: &Value) -> Vec<String> {
    match value {
        Value::Object(map) => {
            let mut names: Vec<String> = map.keys().cloned().collect();
            names.sort();
            names
        }
        other => panic!("expected a JSON object, got {other:?}"),
    }
}

/// Compares one DTO's keys with the expected list, returning the mismatch text.
fn check(name: &str, value: Value, expected: &[&str]) -> Option<String> {
    let mut expected: Vec<String> = expected.iter().map(|entry| (*entry).to_string()).collect();
    expected.sort();
    let actual = keys(&value);
    if actual == expected {
        return None;
    }
    Some(format!("{name}\n  expected: {expected:?}\n  got:      {actual:?}"))
}

fn report(failures: Vec<Option<String>>, subject: &str) {
    let failures: Vec<String> = failures.into_iter().flatten().collect();
    assert!(failures.is_empty(), "{subject} wire shapes changed:\n{}", failures.join("\n"));
}

#[test]
fn pdf_dtos_keep_their_wire_shape() {
    let failures = vec![
        check(
            "GibQr",
            serde_json::to_value(pdfcore::gibqr::GibQr::default()).unwrap(),
            &[
                "avknTckn",
                "ettn",
                "hesaplananKdv",
                "kdvMatrah",
                "malHizmetToplam",
                "no",
                "odenecek",
                "paraBirimi",
                "senaryo",
                "tarih",
                "tip",
                "vergiDahil",
                "vknTckn",
            ],
        ),
        check(
            "HeaderFooterOptions",
            serde_json::to_value(pdfcore::stamp::HeaderFooterOptions::default()).unwrap(),
            &[
                "color",
                "countFromStart",
                "fontSizePt",
                "footerCenter",
                "footerLeft",
                "footerRight",
                "headerCenter",
                "headerLeft",
                "headerRight",
                "marginPt",
                "pages",
                "startNumber",
            ],
        ),
        check(
            "BatesOptions",
            serde_json::to_value(pdfcore::stamp::BatesOptions::default()).unwrap(),
            &["color", "digits", "fontSizePt", "marginPt", "pages", "position", "prefix", "start", "suffix"],
        ),
        check(
            "CompressOptions",
            serde_json::to_value(pdfcore::compress::CompressOptions::default()).unwrap(),
            &["dpi", "grayscale", "jpeg_quality", "preset", "remove_metadata", "strategy"],
        ),
        check(
            "OcrOptions",
            serde_json::to_value(pdfcore::ocr::OcrOptions::default()).unwrap(),
            &["dpi", "languages", "output_mode", "pages", "preprocess", "psm", "skip_text_pages"],
        ),
        check(
            "OcrPreprocess",
            serde_json::to_value(pdfcore::ocr::OcrPreprocess {
                auto_rotate: false,
                deskew: false,
                contrast: false,
                denoise: false,
                binarize: false,
                grayscale: false,
            })
            .unwrap(),
            &["auto_rotate", "binarize", "contrast", "denoise", "deskew", "grayscale"],
        ),
        check(
            "RedactionOptions",
            serde_json::to_value(pdfcore::redact::RedactionOptions::default()).unwrap(),
            &["fill", "images", "paddingPt", "removeMetadata"],
        ),
        check(
            "RedactionArea",
            serde_json::to_value(pdfcore::redact::RedactionArea {
                page: 1,
                left: 0.0,
                bottom: 0.0,
                right: 1.0,
                top: 1.0,
            })
            .unwrap(),
            &["bottom", "left", "page", "right", "top"],
        ),
    ];
    report(failures, "PDF");
}

#[test]
fn office_and_recipe_dtos_keep_their_wire_shape() {
    let failures = vec![
        check(
            "InvoiceQr",
            serde_json::to_value(crate::office::InvoiceQr {
                payload: pdfcore::gibqr::GibQr::default(),
                raw: String::new(),
                page: 1,
                tax_id_valid: true,
                warnings: Vec::new(),
            })
            .unwrap(),
            &["page", "payload", "raw", "taxIdValid", "warnings"],
        ),
        check(
            "InvoiceDocument",
            serde_json::to_value(crate::office::InvoiceDocument {
                kind: "writer".into(),
                title: String::new(),
                model: Value::Null,
                invoice: officecore::ubl::UblInvoice::default(),
                warnings: Vec::new(),
                issues: Vec::new(),
            })
            .unwrap(),
            &["invoice", "issues", "kind", "model", "title", "warnings"],
        ),
        check(
            "Recipe",
            serde_json::to_value(crate::recipes::Recipe {
                id: "r".into(),
                name: "n".into(),
                steps: vec![crate::recipes::RecipeStep::Compress {
                    options: pdfcore::compress::CompressOptions::default(),
                }],
            })
            .unwrap(),
            &["id", "name", "steps"],
        ),
        // The optional halves are skipped when absent, matching the generated
        // binding's `headerFooter?: ...` fields.
        check(
            "RecipeStep::Stamp",
            serde_json::to_value(crate::recipes::RecipeStep::Stamp { header_footer: None, bates: None }).unwrap(),
            &["kind"],
        ),
        check(
            "RecipeStep::Redact",
            serde_json::to_value(crate::recipes::RecipeStep::Redact {
                areas: Vec::new(),
                options: pdfcore::redact::RedactionOptions::default(),
            })
            .unwrap(),
            &["areas", "kind", "options"],
        ),
        check(
            "WatchedFolder",
            serde_json::to_value(crate::recipes::WatchedFolder::default()).unwrap(),
            &["folder", "outputDir", "password", "recipeId"],
        ),
        check(
            "RecipeRunResult",
            serde_json::to_value(crate::recipes::RecipeRunResult { output: String::new(), applied: Vec::new() })
                .unwrap(),
            &["applied", "output"],
        ),
    ];
    report(failures, "office/recipe");
}

#[test]
fn ubl_and_ai_dtos_keep_their_wire_shape() {
    let failures = vec![
        check(
            "UblInvoice",
            serde_json::to_value(officecore::ubl::UblInvoice::default()).unwrap(),
            &[
                "allowanceTotal",
                "currency",
                "customer",
                "ettn",
                "issueDate",
                "issueTime",
                "lineExtensionAmount",
                "lines",
                "notes",
                "number",
                "payableAmount",
                "profile",
                "supplier",
                "taxAmount",
                "taxExclusiveAmount",
                "taxInclusiveAmount",
                "taxSubtotals",
                "typeCode",
                "withholdingAmount",
                "withholdingSubtotals",
            ],
        ),
        check(
            "UblLine",
            serde_json::to_value(officecore::ubl::UblLine::default()).unwrap(),
            &[
                "allowance",
                "id",
                "lineTotal",
                "name",
                "quantity",
                "unit",
                "unitPrice",
                "vatAmount",
                "vatRate",
                "withholdingAmount",
                "withholdingRate",
            ],
        ),
        check(
            "UblTaxSubtotal",
            serde_json::to_value(officecore::ubl::UblTaxSubtotal::default()).unwrap(),
            &["exemptionReason", "name", "rate", "tax", "taxable"],
        ),
        check(
            "UblIssue",
            serde_json::to_value(officecore::ubl::UblIssue::default()).unwrap(),
            &["expected", "field", "found", "message"],
        ),
        check(
            "AiPreview",
            serde_json::to_value(crate::ai::AiPreview {
                pages: 0,
                characters: 0,
                sample: String::new(),
                estimated_words: 0,
            })
            .unwrap(),
            &["characters", "estimatedWords", "pages", "sample"],
        ),
        check(
            "AiTextResult",
            serde_json::to_value(crate::ai::AiTextResult {
                text: String::new(),
                pages: 0,
                characters: 0,
                model: String::new(),
                elapsed_ms: 0,
            })
            .unwrap(),
            &["characters", "elapsedMs", "model", "pages", "text"],
        ),
    ];
    report(failures, "UBL/AI");
}
