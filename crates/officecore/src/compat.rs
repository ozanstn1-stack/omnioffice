//! Format capability matrix and compatibility reports.
//!
//! This module answers two questions for the UI:
//!
//! 1. "What can this build do with format X?" - [`format_capabilities`], used
//!    by theImport/Export dialogs and the Compatibility Center.
//! 2. "What will this specific document lose if it is saved as X?" -
//!    [`document_feature_report`] and friends, used to warn *before* a save
//!    instead of silently dropping data.
//!
//! The reports are deterministic and testable; the string status values are
//! part of the UI contract and must stay stable.

use crate::model::{Deck, TextDocument, Workbook};
use serde::{Deserialize, Serialize};

/// How well a format supports one feature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportLevel {
    Full,
    Partial,
    Unsupported,
}

impl SupportLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            SupportLevel::Full => "full",
            SupportLevel::Partial => "partial",
            SupportLevel::Unsupported => "unsupported",
        }
    }
}

/// One feature support entry for a format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureSupport {
    pub feature: String,
    pub level: SupportLevel,
    pub note: String,
}

/// What a format can do in this build.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatCapabilities {
    pub extension: String,
    pub open: bool,
    pub edit: bool,
    pub save: bool,
    pub pdf_export: bool,
    pub lossless_native: bool,
    pub features: Vec<FeatureSupport>,
}

/// One lossy item in a compatibility report.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureLoss {
    pub feature: String,
    /// `lost`, `transformed` or `unchanged`.
    pub status: String,
    pub message: String,
}

/// The report produced before saving a document into a target format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatibilityReport {
    pub target: String,
    pub items: Vec<FeatureLoss>,
}

impl CompatibilityReport {
    pub fn lossy(&self) -> bool {
        self.items.iter().any(|item| item.status != "unchanged")
    }

    pub fn summary(&self) -> String {
        let lost = self.items.iter().filter(|item| item.status == "lost").count();
        let transformed = self.items.iter().filter(|item| item.status == "transformed").count();
        match (lost, transformed) {
            (0, 0) => "Everything in this document is supported by the target format.".into(),
            (0, transformed) => format!("{transformed} feature(s) are converted rather than kept as-is."),
            (lost, 0) => format!("{lost} feature(s) cannot be represented and will be lost."),
            (lost, transformed) => format!("{lost} feature(s) will be lost and {transformed} converted."),
        }
    }
}

fn feature(feature: &str, level: SupportLevel, note: &str) -> FeatureSupport {
    FeatureSupport { feature: feature.to_string(), level, note: note.to_string() }
}

/// The capability matrix for a file extension (without the dot).
pub fn format_capabilities(extension: &str) -> FormatCapabilities {
    let extension = extension.trim_start_matches('.').to_ascii_lowercase();
    let (open, edit, save, pdf_export, features): (bool, bool, bool, bool, Vec<FeatureSupport>) = match extension.as_str() {
        "docx" | "docm" | "dotx" => (
            true,
            true,
            true,
            true,
            vec![
                feature("sections", SupportLevel::Full, "Section breaks with per-section page setup, headers and footers."),
                feature("footnotes", SupportLevel::Full, "Footnotes and endnotes round-trip."),
                feature("trackChanges", SupportLevel::Partial, "Insertions, deletions and formatting revisions round-trip; paragraph-level move revisions are simplified."),
                feature("comments", SupportLevel::Partial, "Comments round-trip; DOCX has no native reply threading, so replies are stored as reply paragraphs."),
                feature("fields", SupportLevel::Partial, "PAGE, NUMPAGES, DATE, TIME, TITLE, AUTHOR, REF and PAGEREF are written as real fields."),
                feature("styles", SupportLevel::Full, "Based-on inheritance and next styles."),
                feature("charts", SupportLevel::Unsupported, "Writer charts are not implemented."),
                feature("digitalSignature", SupportLevel::Partial, "Existing signatures are preserved but the writer does not create new ones."),
                feature("macros", SupportLevel::Unsupported, "Macros are detected and never executed; saving writes the macro-free document, so the VBA project is dropped."),
            ],
        ),
        "odt" => (
            true,
            true,
            true,
            true,
            vec![
                feature("sections", SupportLevel::Partial, "Section breaks are written as page breaks; per-section page setup is not exported to ODT."),
                feature("footnotes", SupportLevel::Full, "Footnotes and endnotes round-trip as text:note elements with matching citation numbers."),
                feature("trackChanges", SupportLevel::Unsupported, "Tracked changes are kept in .oswk only; a warning is reported when exporting to ODT."),
                feature("comments", SupportLevel::Unsupported, "Comments are kept in .oswk only."),
                feature("fields", SupportLevel::Unsupported, "Fields export as their cached text."),
            ],
        ),
        "rtf" => (
            true,
            true,
            true,
            true,
            vec![
                feature("sections", SupportLevel::Partial, "Sections become page breaks."),
                feature("footnotes", SupportLevel::Partial, "Footnotes are written as real \\footnote destinations; the endnote class survives through an ignorable \\* marker because RTF has no per-note endnote class."),
                feature("trackChanges", SupportLevel::Partial, "Insertions and deletions are written as \\revised/\\deleted marks with a \\revtbl author table; formatting revisions are not representable."),
                feature("comments", SupportLevel::Unsupported, "Comments are kept in .oswk only."),
            ],
        ),
        "txt" | "md" | "markdown" | "html" | "htm" => (
            true,
            true,
            true,
            true,
            vec![
                feature("sections", SupportLevel::Partial, "Sections become page breaks."),
                feature("footnotes", SupportLevel::Partial, "Note text is appended at the end of the document."),
                feature("trackChanges", SupportLevel::Unsupported, "Tracked changes are applied visually only."),
            ],
        ),
        "xlsx" | "xlsm" => (
            true,
            true,
            true,
            true,
            vec![
                feature("tables", SupportLevel::Full, "Structured tables with headers, totals and filters round-trip."),
                feature("pivotTables", SupportLevel::Partial, "Imported pivot caches and tables are preserved and re-exported from their raw parts; the grid is not recomputed, and editor pivots export as computed values."),
                feature("charts", SupportLevel::Full, "Column, bar, line, pie and area charts round-trip, including titles, series colours, caches and cell anchors."),
                feature("images", SupportLevel::Full, "Pictures are imported and exported with their anchor, size and rotation."),
                feature("printSettings", SupportLevel::Full, "Print area, repeating titles, margins, headers/footers and manual page breaks round-trip."),
                feature("protection", SupportLevel::Full, "Password verifiers and locked-action flags are preserved exactly and never cracked."),
                feature("conditionalFormatting", SupportLevel::Full, "All editor rule kinds."),
                feature("dataValidation", SupportLevel::Full, "List and numeric ranges."),
                feature("macros", SupportLevel::Unsupported, "Macros are detected and never executed; saving writes the macro-free workbook, so the VBA project is dropped."),
            ],
        ),
        "xls" => (true, false, false, false, vec![]),
        "doc" | "dot" => (
            true,
            true,
            false,
            true,
            vec![feature(
                "textOnly",
                SupportLevel::Partial,
                "Text and paragraph breaks are imported (a local LibreOffice is used when installed); character formatting, tables, headers, footnotes and images are not preserved. Save as .docx or .oswk to keep edits.",
            )],
        ),
        "ods" => (
            true,
            true,
            true,
            true,
            vec![
                feature("tables", SupportLevel::Partial, "Tables export as plain cell ranges."),
                feature("pivotTables", SupportLevel::Partial, "Editor pivot tables are written as their computed values; the live definition stays in .oswk."),
                feature("charts", SupportLevel::Full, "Column, bar, line, pie and area charts are written as embedded chart objects with their ranges, titles, legend, series colours and cached values."),
            ],
        ),
        "csv" | "tsv" => (true, true, true, false, vec![]),
        "pptx" | "pptm" => (
            true,
            true,
            true,
            true,
            vec![
                feature("masters", SupportLevel::Full, "Slide masters, layouts and placeholder inheritance."),
                feature("groups", SupportLevel::Full, "Nested shape groups round-trip."),
                feature("charts", SupportLevel::Full, "Column, bar, line, pie and area charts with cached values and an embedded workbook."),
                feature("animations", SupportLevel::Partial, "Entrance/emphasis/exit effects; PowerPoint-only effects are simplified."),
                feature("smartArt", SupportLevel::Partial, "SmartArt is imported as its rendered shapes when available."),
                feature("macros", SupportLevel::Unsupported, "Macros are detected and never executed; saving writes the macro-free presentation, so the VBA project is dropped."),
            ],
        ),
        "ppt" => (
            true,
            true,
            false,
            true,
            vec![feature(
                "textOnly",
                SupportLevel::Partial,
                "Slide text is imported (a local LibreOffice is used when installed); shapes, images, animations, themes and formatting are not preserved. Save as .pptx or .oswk to keep edits.",
            )],
        ),
        "odp" => (
            true,
            true,
            true,
            true,
            vec![
                feature("masters", SupportLevel::Partial, "A single default master page."),
                feature("groups", SupportLevel::Full, "Nested shape groups round-trip as draw:g elements."),
                feature("charts", SupportLevel::Partial, "Charts export as drawn shapes."),
                feature("animations", SupportLevel::Partial, "Entrance, emphasis and exit effects are written as SMIL timing with LibreOffice presets; LibreOffice effects without an editor equivalent import as the closest one."),
            ],
        ),
        "pdf" => (
            true,
            true,
            true,
            false,
            vec![
                feature("editing", SupportLevel::Partial, "Page tools, annotations, forms and object editing where the PDF allows it."),
                feature("pdfa", SupportLevel::Partial, "Validation and best-effort conversion: simple fonts are embedded as FontFile2 and the output intent carries an sRGB profile; CID/Type0, symbolic and custom-encoded fonts are reported instead of embedded, and there is no subsetting."),
                feature("signatures", SupportLevel::Partial, "Detached CMS/PKCS#7 signatures are created and validated (digest, coverage, signer, chain); archived validation data (DSS) is written for offline PAdES B-LT, RFC 3161 timestamps are not requested yet."),
            ],
        ),
        "oswk" => (
            true,
            true,
            true,
            true,
            vec![feature("everything", SupportLevel::Full, "The native unit format keeps every feature this build understands.")],
        ),
        _ => (false, false, false, false, vec![]),
    };
    FormatCapabilities { extension, open, edit, save, pdf_export, lossless_native: true, features }
}

fn item(feature: &str, status: &str, message: &str) -> FeatureLoss {
    FeatureLoss { feature: feature.to_string(), status: status.to_string(), message: message.to_string() }
}

fn has_revisions(document: &TextDocument) -> bool {
    crate::revisions::revision_count(document) > 0
}

fn has_notes(document: &TextDocument, endnotes: bool) -> bool {
    if endnotes {
        !document.endnotes.is_empty()
    } else {
        !document.footnotes.is_empty()
    }
}

fn table_comment(document: &TextDocument) -> bool {
    document.comments.iter().any(|comment| !comment.replies.is_empty())
}

/// What a Writer document loses when saved as `format`.
pub fn document_feature_report(document: &TextDocument, format: &str) -> CompatibilityReport {
    let format = format.trim_start_matches('.').to_ascii_lowercase();
    let sections = document.blocks.iter().filter(|block| block.is_section_break()).count();
    let mut items = Vec::new();
    match format.as_str() {
        "oswk" => {}
        "docx" | "docm" | "dotx" => {
            if sections > 0 {
                items.push(item(
                    "sections",
                    "unchanged",
                    "Section breaks, per-section page setup and first/even headers are written as real sectPr parts.",
                ));
            }
            if has_revisions(document) {
                items.push(item(
                    "trackChanges",
                    "transformed",
                    "Insertions, deletions and formatting changes are written as w:ins/w:del/rPrChange; paragraph moves are simplified.",
                ));
            }
            if table_comment(document) {
                items.push(item(
                    "commentReplies",
                    "transformed",
                    "Comment replies are written as extra reply paragraphs because DOCX has no portable reply threading.",
                ));
            }
        }
        "odt" => {
            if sections > 0 {
                items.push(item(
                    "sections",
                    "transformed",
                    "Sections are written as page breaks; per-section page setup is lost.",
                ));
            }
            if has_revisions(document) {
                items.push(item(
                    "trackChanges",
                    "lost",
                    "Pending tracked changes are not written to ODT; keep the .oswk copy.",
                ));
            }
            if document.track_changes || !document.comments.is_empty() {
                items.push(item("comments", "lost", "Comments are not written to ODT; keep the .oswk copy."));
            }
        }
        "rtf" => {
            if sections > 0 {
                items.push(item("sections", "transformed", "Sections become page breaks."));
            }
            if has_revisions(document) {
                items.push(item("trackChanges", "transformed", "Insertions and deletions are written as \\revised/\\deleted marks with a \\revtbl author table and \\revdttm timestamps; formatting revisions are simplified."));
            }
            if !document.comments.is_empty() {
                items.push(item("comments", "lost", "Comments are not written to RTF; keep the .oswk copy."));
            }
            if has_notes(document, false) || has_notes(document, true) {
                items.push(item("footnotes", "transformed", "Notes are written as RTF \\footnote destinations; endnote classes are preserved with an ignorable marker that Word ignores."));
            }
        }
        "txt" | "md" | "markdown" | "html" | "htm" => {
            if sections > 0 {
                items.push(item("sections", "transformed", "Sections become page breaks."));
            }
            if has_notes(document, false) || has_notes(document, true) {
                items.push(item(
                    "footnotes",
                    "transformed",
                    "Note references become plain markers and the note text is appended at the end.",
                ));
            }
            if has_revisions(document) || !document.comments.is_empty() {
                items.push(item("review", "lost", "Tracked changes and comments are not written to this format."));
            }
        }
        "pdf" => {
            if document.track_changes || has_revisions(document) {
                items.push(item("trackChanges", "transformed", "Revisions are rendered (insertions underlined, deletions struck through) rather than exported as revisions."));
            }
        }
        _ => items.push(item("format", "lost", "This format is not a Writer target.")),
    }
    CompatibilityReport { target: format, items }
}

/// What a workbook loses when saved as `format`.
pub fn workbook_feature_report(workbook: &Workbook, format: &str) -> CompatibilityReport {
    let format = format.trim_start_matches('.').to_ascii_lowercase();
    let mut items = Vec::new();
    let tables: usize = workbook.sheets.iter().map(|sheet| sheet.tables.len()).sum();
    let pivots: usize = workbook.sheets.iter().map(|sheet| sheet.pivot_tables.len()).sum();
    let charts: usize = workbook.sheets.iter().map(|sheet| sheet.charts.len()).sum();
    match format.as_str() {
        "oswk" | "xlsx" | "xlsm" => {
            if pivots > 0 {
                items.push(item(
                    "pivotTables",
                    "transformed",
                    "Editor pivot definitions are written as computed values; pivot caches imported from a package are re-exported from their preserved raw parts.",
                ));
            }
            if format != "oswk" && format != "xlsx" && format != "xlsm" {
                items.push(item("format", "lost", "Unsupported target."));
            }
        }
        "ods" => {
            if tables > 0 {
                items.push(item("tables", "transformed", "Structured tables export as plain cell ranges."));
            }
            if pivots > 0 {
                items.push(item(
                    "pivotTables",
                    "transformed",
                    "Pivot tables are written as their computed values; the live pivot definition stays in the .oswk file.",
                ));
            }
            if charts > 0 {
                let problems: Vec<String> = workbook
                    .sheets
                    .iter()
                    .flat_map(|sheet| sheet.charts.iter())
                    .filter_map(|placement| {
                        crate::odf::ods_chart_problem(&placement.chart)
                            .map(|reason| format!("\"{}\": {reason}", placement.chart.title))
                    })
                    .collect();
                if problems.is_empty() {
                    items.push(item(
                        "charts",
                        "unchanged",
                        "Charts are written as embedded chart objects with their ranges, titles, legend, series colours and cached values.",
                    ));
                } else {
                    items.push(item(
                        "charts",
                        "lost",
                        &format!(
                            "{} of {charts} chart(s) cannot be written to ODS and stay in .oswk only ({}); the others are written as embedded chart objects.",
                            problems.len(),
                            problems.join("; ")
                        ),
                    ));
                }
            }
        }
        "csv" | "tsv" => {
            if workbook.sheets.len() > 1 {
                items.push(item("sheets", "lost", "Only the active sheet is exported."));
            }
            items.push(item("formatting", "lost", "Formatting, formulas and structure are not kept in CSV."));
        }
        "pdf" => {
            if pivots > 0 || charts > 0 {
                items.push(item("objects", "transformed", "Pivots and charts are rendered as static output."));
            }
        }
        _ => items.push(item("format", "lost", "This format is not a Calc target.")),
    }
    CompatibilityReport { target: format, items }
}

/// Charts on a slide, including those inside groups.
fn count_charts(objects: &[crate::model::SlideObject]) -> usize {
    objects.iter().map(|object| usize::from(object.chart.is_some()) + count_charts(&object.children)).sum()
}

/// What a deck loses when saved as `format`.
pub fn deck_feature_report(deck: &Deck, format: &str) -> CompatibilityReport {
    let format = format.trim_start_matches('.').to_ascii_lowercase();
    let mut items = Vec::new();
    let groups: usize =
        deck.slides.iter().map(|slide| slide.objects.iter().filter(|object| object.kind == "group").count()).sum();
    let charts: usize = deck.slides.iter().map(|slide| count_charts(&slide.objects)).sum();
    let animations: usize = deck.slides.iter().map(|slide| slide.animations.len()).sum();
    match format.as_str() {
        "oswk" => {}
        "pptx" | "pptm" => {
            if animations > 0 {
                items.push(item(
                    "animations",
                    "partial",
                    "The effects the built-in slideshow can run round-trip; other effects are simplified.",
                ));
            }
        }
        "odp" => {
            if groups > 0 {
                items.push(item(
                    "groups",
                    "unchanged",
                    "Groups are written as draw:g elements with their child shapes.",
                ));
            }
            if charts > 0 {
                items.push(item("charts", "transformed", "Charts are exported as drawn shapes."));
            }
            if animations > 0 {
                items.push(item(
                    "animations",
                    "partial",
                    "Entrance, emphasis and exit effects are written as SMIL timing with LibreOffice presets and keep their trigger, duration and delay; effects without a LibreOffice preset are written as the closest one.",
                ));
            }
        }
        "pdf" => {
            if animations > 0 {
                items.push(item("animations", "lost", "Animations do not apply to PDF output."));
            }
        }
        _ => items.push(item("format", "lost", "This format is not an Impress target.")),
    }
    CompatibilityReport { target: format, items }
}

/// Every extension this build can open.
pub fn supported_open_extensions() -> Vec<&'static str> {
    vec![
        "docx", "docm", "dotx", "odt", "rtf", "txt", "md", "markdown", "html", "htm", "xlsx", "xlsm", "xls", "ods",
        "csv", "tsv", "pptx", "pptm", "odp", "oswk",
    ]
}

/// The capabilities of a document model (what the *model* supports, regardless
/// of the current file format).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentCapabilities {
    pub kind: String,
    pub supports_sections: bool,
    pub supports_track_changes: bool,
    pub supports_footnotes: bool,
    pub supports_endnotes: bool,
    pub supports_comments: bool,
    pub supports_fields: bool,
    pub supports_bookmarks: bool,
    pub supports_structured_references: bool,
    pub supports_charts: bool,
    pub supports_animations: bool,
    pub supports_masters: bool,
    pub supports_groups: bool,
}

/// Model capabilities for `writer`, `calc` or `impress`.
pub fn model_capabilities(kind: &str) -> DocumentCapabilities {
    match kind {
        "writer" => DocumentCapabilities {
            kind: "writer".into(),
            supports_sections: true,
            supports_track_changes: true,
            supports_footnotes: true,
            supports_endnotes: true,
            supports_comments: true,
            supports_fields: true,
            supports_bookmarks: true,
            supports_structured_references: false,
            supports_charts: false,
            supports_animations: false,
            supports_masters: false,
            supports_groups: false,
        },
        "calc" => DocumentCapabilities {
            kind: "calc".into(),
            supports_sections: false,
            supports_track_changes: false,
            supports_footnotes: false,
            supports_endnotes: false,
            supports_comments: true,
            supports_fields: false,
            supports_bookmarks: false,
            supports_structured_references: true,
            supports_charts: true,
            supports_animations: false,
            supports_masters: false,
            supports_groups: false,
        },
        _ => DocumentCapabilities {
            kind: "impress".into(),
            supports_sections: false,
            supports_track_changes: false,
            supports_footnotes: false,
            supports_endnotes: false,
            supports_comments: false,
            supports_fields: false,
            supports_bookmarks: false,
            supports_structured_references: false,
            supports_charts: true,
            supports_animations: true,
            supports_masters: true,
            supports_groups: true,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        Block, ChartData, ChartPlacement, ChartSeries, Footnote, PivotTable, RevisionMark, Run, SectionProps,
        SpreadsheetTable,
    };

    #[test]
    fn capability_matrix_is_populated_for_real_formats() {
        let docx = format_capabilities("docx");
        assert!(docx.open && docx.save && docx.pdf_export);
        assert!(docx.features.iter().any(|feature| feature.feature == "sections"));
        let xls = format_capabilities("xls");
        assert!(xls.open && !xls.save);
        let unknown = format_capabilities("zip");
        assert!(!unknown.open);
        assert!(supported_open_extensions().contains(&"oswk"));
    }

    #[test]
    fn writer_report_flags_revisions_for_odt_and_not_for_native() {
        let mut document = TextDocument::new_blank("Report");
        document.footnotes = vec![Footnote { id: "fn1".into(), runs: vec![Run::default()], marker: String::new() }];
        document.blocks.push(Block::SectionBreak { section: SectionProps::default() });
        document.blocks.push(Block::Paragraph {
            props: Default::default(),
            runs: vec![Run {
                text: "changed".into(),
                revision: Some(RevisionMark {
                    id: "r1".into(),
                    kind: "insert".into(),
                    author: "A".into(),
                    date: String::new(),
                    original: None,
                }),
                ..Default::default()
            }],
        });
        let native = document_feature_report(&document, "oswk");
        assert!(!native.lossy());
        let odt = document_feature_report(&document, "odt");
        assert!(odt.lossy());
        assert!(odt.items.iter().any(|item| item.feature == "trackChanges" && item.status == "lost"));
        assert!(odt.items.iter().any(|item| item.feature == "sections"));
        let docx = document_feature_report(&document, "docx");
        assert!(docx.items.iter().any(|item| item.feature == "trackChanges" && item.status == "transformed"));
    }

    #[test]
    fn calc_report_mentions_pivot_and_table_limits() {
        let mut workbook = Workbook::new_blank("Report");
        workbook.sheets[0].tables.push(SpreadsheetTable::new(
            "Sales",
            "A1:C4",
            vec!["A".into(), "B".into(), "C".into()],
        ));
        let xlsx = workbook_feature_report(&workbook, "xlsx");
        assert!(!xlsx.lossy(), "xlsx should keep structured tables");
        let ods = workbook_feature_report(&workbook, "ods");
        assert!(ods.items.iter().any(|item| item.feature == "tables" && item.status == "transformed"));

        workbook.sheets[0].pivot_tables.push(PivotTable { id: "p1".into(), ..Default::default() });
        workbook.sheets[0].charts.push(ChartPlacement {
            chart: ChartData {
                kind: "column".into(),
                series: vec![ChartSeries { name: "S".into(), range: "B2:B4".into(), color: None }],
                ..Default::default()
            },
            ..Default::default()
        });
        let ods = workbook_feature_report(&workbook, "ods");
        assert!(ods.items.iter().any(|item| item.feature == "pivotTables" && item.status == "transformed"));
        assert!(ods.items.iter().any(|item| item.feature == "charts" && item.status == "unchanged"));
        workbook.sheets[0].charts[0].chart.kind = "radar".into();
        let ods = workbook_feature_report(&workbook, "ods");
        assert!(ods.items.iter().any(|item| item.feature == "charts" && item.status == "lost"));
    }

    #[test]
    fn impress_report_keeps_groups_and_simplifies_animations_in_odp() {
        let mut deck = Deck::new_blank("Report");
        let mut group = crate::model::SlideObject::new("group", 0.0, 0.0, 100.0, 100.0);
        group.children.push(crate::model::SlideObject::new("rect", 0.0, 0.0, 10.0, 10.0));
        deck.slides[0].objects.push(group);
        let pptx = deck_feature_report(&deck, "pptx");
        assert!(!pptx.lossy());
        let odp = deck_feature_report(&deck, "odp");
        assert!(!odp.lossy(), "groups are written as draw:g");
        assert!(odp.items.iter().any(|item| item.feature == "groups" && item.status == "unchanged"));
        deck.slides[0].animations.push(crate::model::Animation { kind: "entrance".into(), ..Default::default() });
        let odp = deck_feature_report(&deck, "odp");
        assert!(odp.items.iter().any(|item| item.feature == "animations" && item.status == "partial"));
        assert!(!odp.items.iter().any(|item| item.status == "lost"));
    }
}
