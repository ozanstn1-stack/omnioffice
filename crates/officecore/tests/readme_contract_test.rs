//! The "Supported formats" table in README.md and `officecore::compat` answer
//! the same question for two audiences, and they had drifted: the matrix still
//! reported that PDF/A font embedding and signature validation were not
//! implemented after both shipped in 3.1.0, and the Compatibility Center shows
//! those notes verbatim (src-tauri/src/office.rs -> format_capabilities).
//!
//! The table is the user-facing contract, so this test fails when a format the
//! engine can open is missing from it, or when a documented yes/no disagrees
//! with `format_capabilities`.

use officecore::compat::{format_capabilities, supported_open_extensions};
use std::collections::BTreeSet;
use std::path::PathBuf;

/// Formats the matrix has an arm for that this build deliberately cannot open.
/// They are listed here so that a table cell claiming otherwise still fails.
/// (`.doc`/`.ppt` moved out of this list in v3.5.7: they are imported now.)
const KNOWN_UNSUPPORTED: &[&str] = &[];

struct Row {
    label: String,
    /// open, edit, save, pdf export - in table order.
    flags: [bool; 4],
}

fn readme() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("README.md");
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

/// A cell is "yes" when it starts with the check mark; the dash and the cross
/// (SVG PDF export) are both "no". Prose after the mark is allowed, which is
/// what the table uses for the caveats.
fn cell_is_true(cell: &str) -> bool {
    cell.starts_with('✓')
}

fn rows() -> Vec<Row> {
    let text = readme();
    let mut rows = Vec::new();
    let mut in_table = false;
    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with('|') {
            if in_table {
                break;
            }
            continue;
        }
        let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
        if cells.len() < 5 {
            continue;
        }
        if cells[0] == "Format" {
            in_table = true;
            continue;
        }
        if !in_table {
            continue;
        }
        // The |---|---| separator row.
        if cells[1..5].iter().all(|cell| cell.chars().all(|ch| ch == '-' || ch == ':')) {
            continue;
        }
        rows.push(Row {
            label: cells[0].to_string(),
            flags: [cell_is_true(cells[1]), cell_is_true(cells[2]), cell_is_true(cells[3]), cell_is_true(cells[4])],
        });
    }
    rows
}

/// "TXT / Markdown / HTML" -> txt, markdown, md, html, htm; "`.oswk` unit" -> oswk.
///
/// A documented name also covers the short aliases the engine accepts, so the
/// table does not have to list "md" and "htm" separately.
fn extensions(label: &str) -> Vec<String> {
    label
        .replace('`', "")
        .trim_end_matches(" unit")
        .split('/')
        .map(|part| part.trim().trim_start_matches('.').to_ascii_lowercase())
        .filter(|part| !part.is_empty())
        .flat_map(|part| match part.as_str() {
            "markdown" => vec!["markdown".to_string(), "md".to_string()],
            "html" => vec!["html".to_string(), "htm".to_string()],
            _ => vec![part],
        })
        .collect()
}

#[test]
fn the_supported_formats_table_parses() {
    let rows = rows();
    assert!(rows.len() >= 15, "expected the README table, parsed {} rows", rows.len());
    assert!(rows.iter().any(|row| extensions(&row.label).contains(&"docx".to_string())));
}

#[test]
fn every_openable_format_is_documented() {
    let documented: BTreeSet<String> = rows().iter().flat_map(|row| extensions(&row.label)).collect();
    let missing: Vec<&str> =
        supported_open_extensions().into_iter().filter(|extension| !documented.contains(*extension)).collect();
    assert!(missing.is_empty(), "README.md does not document these openable formats: {missing:?}");
}

#[test]
fn documented_flags_match_the_capability_matrix() {
    let mut mismatches = Vec::new();
    for row in rows() {
        for extension in extensions(&row.label) {
            let capabilities = format_capabilities(&extension);
            let actual = [capabilities.open, capabilities.edit, capabilities.save, capabilities.pdf_export];
            let office_format = capabilities.open || KNOWN_UNSUPPORTED.contains(&extension.as_str());
            // Images and SVG are handled by other crates and have no arm here.
            if !office_format {
                continue;
            }
            if actual != row.flags {
                mismatches.push(format!("{extension}: README {:?} vs matrix {:?}", row.flags, actual));
            }
        }
    }
    assert!(mismatches.is_empty(), "README table and the capability matrix disagree:\n{}", mismatches.join("\n"));
}

#[test]
fn the_matrix_describes_the_features_it_claims() {
    // The two notes that had gone stale - keep them honest.
    let pdf = format_capabilities("pdf");
    let pdfa = pdf.features.iter().find(|feature| feature.feature == "pdfa").expect("pdfa entry");
    assert!(!pdfa.note.contains("not implemented"), "the PDF/A note no longer matches the engine: {}", pdfa.note);
    let signatures = pdf.features.iter().find(|feature| feature.feature == "signatures").expect("signatures entry");
    assert!(
        !signatures.note.contains("not implemented"),
        "the signature note no longer matches the engine: {}",
        signatures.note
    );
    // DSS archiving shipped (pdfcore::ltv); the note may only claim the
    // timestamp half is missing, never that the whole validation-data path is.
    assert!(
        signatures.note.contains("DSS") && !signatures.note.contains("DSS, RFC 3161 timestamps"),
        "the signature note must describe DSS as written and timestamps as missing: {}",
        signatures.note
    );
    // RFC 3161 timestamps shipped in 3.6.0 (optional TSA URL) and the optional
    // OCSP/CRL check in 4.1.0; neither may be described as missing.
    assert!(
        signatures.note.contains("RFC 3161 timestamp") && !signatures.note.contains("not requested"),
        "the signature note must describe RFC 3161 timestamps as supported: {}",
        signatures.note
    );
    // PDF/A font embedding subsets the program since 4.1.0.
    assert!(
        pdfa.note.contains("subset") && !pdfa.note.contains("no subsetting"),
        "the PDF/A note must say fonts are subset: {}",
        pdfa.note
    );
    // Macro-enabled formats must say what happens to the macros.
    for extension in ["docm", "xlsm", "pptm"] {
        let capabilities = format_capabilities(extension);
        let macros = capabilities
            .features
            .iter()
            .find(|feature| feature.feature == "macros")
            .unwrap_or_else(|| panic!("{extension} has no macros entry"));
        assert!(macros.note.contains("dropped"), "{extension}: {}", macros.note);
    }
}
