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

use crate::model::{Block, Deck, TextDocument, Workbook};
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
                feature("tables", SupportLevel::Full, "Merged cells round-trip: colspans as w:gridSpan and rowspans as w:vMerge with continuation cells."),
                feature("images", SupportLevel::Full, "Anchored images with square or top-and-bottom wrap round-trip."),
                feature("tabStops", SupportLevel::Full, "Custom tab stops round-trip as w:tabs."),
                feature("watermark", SupportLevel::Full, "Text watermarks round-trip through the default header's VML shape."),
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
                feature("comments", SupportLevel::Partial, "Comments round-trip as office:annotation ranges with LibreOffice's resolved flag; ODF has no portable reply threading, so replies are stored as reply paragraphs."),
                feature("fields", SupportLevel::Unsupported, "Fields export as their cached text."),
                feature("tables", SupportLevel::Full, "Merged cells round-trip as table:number-columns-spanned / table:number-rows-spanned with covered-table-cell placeholders."),
                feature("images", SupportLevel::Partial, "Anchored images keep their wrap style (parallel/none); a wrap the model cannot name imports as top-and-bottom."),
                feature("tabStops", SupportLevel::Full, "Custom tab stops round-trip as style:tab-stops."),
                feature("watermark", SupportLevel::Full, "The watermark is written as a run-through frame and kept losslessly in meta:user-defined."),
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
                feature("comments", SupportLevel::Partial, "Comments are written as Word annotations (\\atrfstart/\\atrfend ranges and \\annotation groups); replies are stored as reply paragraphs, timestamps keep minute precision and the resolved state survives only through an ignorable \\* marker."),
                feature("tables", SupportLevel::Partial, "Rows and cells are written, but colspan/rowspan merging is not representable."),
                feature("images", SupportLevel::Partial, "Images are embedded inline; wrapping and cropping are not representable."),
                feature("tabStops", SupportLevel::Unsupported, "Custom tab stops are not written; tab characters stay in the text."),
                feature("watermark", SupportLevel::Unsupported, "The watermark is not written to RTF."),
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
                feature("tables", SupportLevel::Partial, "Tables export as tab-separated text (TXT), pipe tables (Markdown) or HTML; merged cells are flattened."),
                feature("images", SupportLevel::Partial, "HTML embeds image data; Markdown writes a reference without the data and TXT drops images."),
                feature("watermark", SupportLevel::Unsupported, "The watermark is not written to text formats."),
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
                feature("charts", SupportLevel::Full, "Column, bar, line, pie, area, scatter and doughnut charts round-trip, including titles, series colours, caches, doughnut hole size and cell anchors; scatter series share one X range."),
                feature("images", SupportLevel::Full, "Pictures are imported and exported with their anchor, size and rotation."),
                feature("printSettings", SupportLevel::Full, "Print area, repeating titles, margins, headers/footers and manual page breaks round-trip."),
                feature("protection", SupportLevel::Full, "Password verifiers and locked-action flags are preserved exactly and never cracked."),
                feature("conditionalFormatting", SupportLevel::Full, "Cell-value, text, duplicate and top/bottom rules, formula rules, color scales, data bars and icon sets round-trip with their own colours and rule order. Excel rule types the editor has no kind for (begins with, blanks, above average, ...) import as formula rules; Excel 2010 data bar extras (negative colours, custom icons) are not imported."),
                feature("comments", SupportLevel::Full, "Cell notes keep their text, author and whether they stay visible (legacy VML shape included); a threaded comment imports as its note text under no author."),
                feature("hyperlinks", SupportLevel::Full, "External https, http and mailto links and internal sheet references keep their display text and screen tip; other targets (file:, javascript:, network paths, relative files) are never written or read, and are reported on import."),
                feature("dataValidation", SupportLevel::Full, "List and numeric ranges."),
                feature("hiddenRowsColumns", SupportLevel::Partial, "Hidden rows and columns (also the rows an AutoFilter hides) round-trip, including hidden ones without cells; the editor keeps no size for a hidden row or column, so showing it again gives the default size. A sheet that hides all its unused rows with zeroHeight opens with those rows visible."),
                feature("formulas", SupportLevel::Partial, "Functions added after Excel 2007 are written with the _xlfn. prefix Excel requires (_xlfn._xlws. for SORT and FILTER, _xlpm. for LET names) and read back without it; #CALC! and #SPILL! are kept as error cells. Dynamic-array formulas are written as ordinary formulas without Excel's spill metadata, so Excel does not spill them."),
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
                feature("charts", SupportLevel::Full, "Column, bar, line, pie, area, scatter and doughnut charts are written as embedded chart objects with their ranges, titles, legend, series colours and cached values; ODF has no doughnut hole size, so a custom one stays in .oswk and XLSX."),
                feature("conditionalFormatting", SupportLevel::Partial, "Cell-value, text, duplicate and top/bottom rules, formula rules, color scales, data bars and icon sets are written as LibreOffice conditional formats (calcext) and read back, including LibreOffice's own files; stop-if-true and reversed icon sets are not part of ODF, so they stay in .oswk and XLSX."),
                feature("comments", SupportLevel::Full, "Cell notes round-trip as office:annotation with their text, author and visibility; the note date and box size are not kept."),
                feature("hyperlinks", SupportLevel::Partial, "External https, http and mailto links and internal sheet references round-trip with their screen tip; a link on an empty cell shows its target as text, and other targets (file:, javascript:, macro links, network paths, relative files) are never written or read."),
                feature("hiddenRowsColumns", SupportLevel::Partial, "Hidden rows and columns round-trip as collapsed rows and columns, and LibreOffice's filtered rows and folded groups are read as hidden; the AutoFilter itself is not exported, so filtered rows are written as collapsed, and the editor keeps no size for a hidden row or column."),
                feature("formulas", SupportLevel::Partial, "Formulas are written as OpenFormula (semicolon separators, bracketed references, COM.MICROSOFT. names for Excel functions) and read back, including LibreOffice's own; the newer array functions (SORT, FILTER, HSTACK, ...) need a LibreOffice version that has them. Error cells keep their code, #CALC! and #SPILL! included."),
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
                feature("richText", SupportLevel::Full, "Per-run bold/italic/underline, colour, size and language round-trip."),
                feature("slideNumbers", SupportLevel::Full, "Footer, date and slide-number placeholders round-trip; an enabled deck footer is written as real fields on the master."),
                feature("hiddenSlides", SupportLevel::Full, "Hidden slides keep their flag (p:sldId show=\"0\") and are skipped by the slideshow and the PDF export."),
                feature("connectors", SupportLevel::Full, "Connectors keep their glue points (stCxn/endCxn) and connection sites."),
                feature("imageCrop", SupportLevel::Full, "Image crops round-trip as a:srcRect source rectangles."),
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
                feature("masters", SupportLevel::Partial, "A single default master page (now carrying the footer, date and slide-number frames)."),
                feature("groups", SupportLevel::Full, "Nested shape groups round-trip as draw:g elements."),
                feature("charts", SupportLevel::Full, "Column, bar, line, pie, area, scatter and doughnut charts are written as embedded chart objects (Object N sub-documents) with their ranges, title, legend and cached values, and read back; a chart kind ODF cannot name is reported and keeps a placeholder."),
                feature("richText", SupportLevel::Full, "Paragraph runs round-trip as text:span with named character styles."),
                feature("slideNumbers", SupportLevel::Full, "The footer, date and slide-number master frames and their display flags round-trip."),
                feature("hiddenSlides", SupportLevel::Full, "Hidden slides are written with presentation:visibility=\"hidden\" and read back."),
                feature("connectors", SupportLevel::Full, "draw:connector keeps its glue points (draw:start-shape/end-shape) and connection sites."),
                feature("imageCrop", SupportLevel::Full, "Image crops round-trip as fo:clip rectangles."),
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
                feature("officeExport", SupportLevel::Partial, "Writer and Impress export to PDF: paragraph/run formatting, tab stops, merged table cells and text watermarks are rendered; hidden slides are skipped; footer/date/slide-number placeholders and image crops are resolved; charts are drawn as labelled data-range boxes and image wrap is block-level."),
                feature("pdfa", SupportLevel::Partial, "Validation and best-effort conversion: non-embedded simple fonts and Identity Type0 fonts with a ToUnicode map are embedded as substitute programs subset to the characters the document uses, and the output intent carries an sRGB profile; Type0/CID fonts without ToUnicode, symbolic and custom-encoded fonts are reported instead of embedded."),
                feature("signatures", SupportLevel::Partial, "Detached CMS/PKCS#7 signatures are created and validated (digest, coverage, signer, chain); archived validation data (DSS) is written for offline PAdES B-LT; an optional TSA URL adds an RFC 3161 timestamp (its time is reported, the TSA chain is not validated) and an optional OCSP/CRL check reports revocation, while trust stays unknown without a trust store."),
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

/// Whether any table cell starts a colspan / rowspan merge.
fn table_spans(document: &TextDocument) -> (bool, bool) {
    let mut colspan = false;
    let mut rowspan = false;
    for block in &document.blocks {
        if let Block::Table { table } = block {
            for cell in table.rows.iter().flat_map(|row| row.cells.iter()) {
                colspan |= cell.colspan > 1;
                rowspan |= cell.rowspan > 1;
            }
        }
    }
    (colspan, rowspan)
}

/// Whether any paragraph carries custom tab stops.
fn has_tab_stops(document: &TextDocument) -> bool {
    document.blocks.iter().any(|block| match block {
        Block::Paragraph { props, .. } => !props.tabs.is_empty(),
        _ => false,
    })
}

/// Whether the document contains a table / image block.
fn has_block(document: &TextDocument, kind: &str) -> bool {
    document
        .blocks
        .iter()
        .any(|block| matches!((kind, block), ("table", Block::Table { .. }) | ("image", Block::Image { .. })))
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
            if table_comment(document) {
                items.push(item(
                    "comments",
                    "transformed",
                    "Comments are written as office:annotation ranges; replies become extra reply paragraphs because ODF has no portable reply threading, and reply timestamps are not kept.",
                ));
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
                items.push(item("comments", "transformed", "Comments are written as Word annotations with minute-precision \\atndate timestamps; replies become extra reply paragraphs and the resolved state is kept with an ignorable marker that Word ignores."));
            }
            if has_notes(document, false) || has_notes(document, true) {
                items.push(item("footnotes", "transformed", "Notes are written as RTF \\footnote destinations; endnote classes are preserved with an ignorable marker that Word ignores."));
            }
            let (colspan, rowspan) = table_spans(document);
            if colspan || rowspan {
                items.push(item(
                    "tables",
                    "transformed",
                    "RTF has no cell merging, so colspan/rowspan cells are written as separate cells.",
                ));
            }
            if has_tab_stops(document) {
                items.push(item(
                    "tabStops",
                    "lost",
                    "Custom tab stops are not written to RTF; the tab characters stay in the text.",
                ));
            }
            if document.watermark.is_some() {
                items.push(item(
                    "watermark",
                    "lost",
                    "The watermark is not written to RTF; DOCX, ODT and PDF keep it.",
                ));
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
            let (colspan, rowspan) = table_spans(document);
            if has_block(document, "table") {
                let message = match format.as_str() {
                    "html" | "htm" if colspan || rowspan => {
                        "Tables export as HTML; colspan is kept but rowspan is flattened."
                    }
                    "html" | "htm" => "Tables export as HTML with their cell backgrounds and alignment.",
                    "md" | "markdown" => "Tables export as pipe tables; merged cells are flattened.",
                    _ => "Tables export as tab-separated text.",
                };
                let status =
                    if matches!(format.as_str(), "html" | "htm") && !rowspan { "unchanged" } else { "transformed" };
                items.push(item("tables", status, message));
            }
            if has_block(document, "image") {
                let (status, message) = match format.as_str() {
                    "html" | "htm" => ("unchanged", "Image data is embedded as data URIs with its caption."),
                    "md" | "markdown" => {
                        ("lost", "Images are written as a link reference; the image data is not kept.")
                    }
                    _ => ("lost", "Images are not written to plain text."),
                };
                items.push(item("images", status, message));
            }
            if document.watermark.is_some() {
                items.push(item("watermark", "lost", "The watermark is not written to text formats."));
            }
        }
        "pdf" => {
            if document.track_changes || has_revisions(document) {
                items.push(item("trackChanges", "transformed", "Revisions are rendered (insertions underlined, deletions struck through) rather than exported as revisions."));
            }
            if document.watermark.is_some() {
                items.push(item(
                    "watermark",
                    "unchanged",
                    "The watermark is drawn on every page as an incremental PDF revision.",
                ));
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
            let unwritable: Vec<String> = workbook
                .sheets
                .iter()
                .flat_map(|sheet| sheet.conditional.iter())
                .filter_map(crate::odf::ods_conditional_problem)
                .collect();
            if !unwritable.is_empty() {
                items.push(item(
                    "conditionalFormatting",
                    "lost",
                    &format!(
                        "{} conditional formatting rule(s) cannot be written to ODS and stay in .oswk only ({}).",
                        unwritable.len(),
                        unwritable.join("; ")
                    ),
                ));
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
                items.push(item(
                    "charts",
                    "unchanged",
                    "Charts are written as embedded chart objects (Object N sub-documents) with their ranges, title, legend and cached values; a chart kind ODF cannot name is reported and keeps a placeholder.",
                ));
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
    pub supports_watermark: bool,
    pub supports_hidden_slides: bool,
    pub supports_footer: bool,
    pub supports_connectors: bool,
    pub supports_image_crop: bool,
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
            supports_watermark: true,
            supports_hidden_slides: false,
            supports_footer: false,
            supports_connectors: false,
            supports_image_crop: false,
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
            supports_watermark: false,
            supports_hidden_slides: false,
            supports_footer: false,
            supports_connectors: false,
            supports_image_crop: false,
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
            supports_watermark: false,
            supports_hidden_slides: true,
            supports_footer: true,
            supports_connectors: true,
            supports_image_crop: true,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        Block, ChartData, ChartPlacement, ChartSeries, Footnote, ParaProps, PivotTable, RevisionMark, Run,
        SectionProps, SpreadsheetTable, TabStop, TableData, Watermark,
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

    #[test]
    fn odp_capabilities_report_real_charts_and_new_impress_features() {
        let odp = format_capabilities("odp");
        let charts = odp.features.iter().find(|feature| feature.feature == "charts").expect("charts row");
        assert_eq!(charts.level, SupportLevel::Full);
        assert!(!charts.note.contains("drawn shapes"), "the ODP chart note is stale: {}", charts.note);
        for name in ["richText", "slideNumbers", "hiddenSlides", "connectors", "imageCrop"] {
            assert!(odp.features.iter().any(|feature| feature.feature == name), "odp is missing {name}");
        }

        let pptx = format_capabilities("pptx");
        for name in ["richText", "slideNumbers", "hiddenSlides", "connectors", "imageCrop"] {
            assert!(pptx.features.iter().any(|feature| feature.feature == name), "pptx is missing {name}");
        }

        let docx = format_capabilities("docx");
        for name in ["tables", "images", "tabStops", "watermark"] {
            assert!(docx.features.iter().any(|feature| feature.feature == name), "docx is missing {name}");
        }
        let odt = format_capabilities("odt");
        for name in ["tables", "images", "tabStops", "watermark"] {
            assert!(odt.features.iter().any(|feature| feature.feature == name), "odt is missing {name}");
        }
        let rtf = format_capabilities("rtf");
        let tables = rtf.features.iter().find(|feature| feature.feature == "tables").expect("rtf tables row");
        assert_eq!(tables.level, SupportLevel::Partial);
        let watermark = rtf.features.iter().find(|feature| feature.feature == "watermark").expect("rtf watermark");
        assert_eq!(watermark.level, SupportLevel::Unsupported);
        let pdf = format_capabilities("pdf");
        assert!(pdf.features.iter().any(|feature| feature.feature == "officeExport"));
    }

    #[test]
    fn writer_report_names_rtf_and_text_format_limits() {
        let mut document = TextDocument::new_blank("Losses");
        let mut table = TableData::simple(2, 2, 200.0);
        table.rows[0].cells[0].colspan = 2;
        document.blocks.push(Block::Table { table });
        document.blocks.push(Block::Paragraph {
            props: ParaProps { tabs: vec![TabStop { pos_pt: 72.0, align: "left".into() }], ..Default::default() },
            runs: vec![Run { text: "a\tb".into(), ..Default::default() }],
        });
        document.watermark = Some(Watermark::default());

        let rtf = document_feature_report(&document, "rtf");
        assert!(rtf.items.iter().any(|item| item.feature == "tables" && item.status == "transformed"));
        assert!(rtf.items.iter().any(|item| item.feature == "tabStops" && item.status == "lost"));
        assert!(rtf.items.iter().any(|item| item.feature == "watermark" && item.status == "lost"));

        let txt = document_feature_report(&document, "txt");
        assert!(txt.items.iter().any(|item| item.feature == "tables" && item.status == "transformed"));
        assert!(txt.items.iter().any(|item| item.feature == "watermark" && item.status == "lost"));

        // HTML keeps colspan (only a rowspan would be flattened).
        let html = document_feature_report(&document, "html");
        assert!(html.items.iter().any(|item| item.feature == "tables" && item.status == "unchanged"));

        let pdf = document_feature_report(&document, "pdf");
        assert!(pdf.items.iter().any(|item| item.feature == "watermark" && item.status == "unchanged"));
    }

    #[test]
    fn impress_report_writes_real_charts_to_odp() {
        let mut deck = Deck::new_blank("Charts");
        let mut object = crate::model::SlideObject::new("chart", 0.0, 0.0, 100.0, 100.0);
        object.chart = Some(ChartData { kind: "column".into(), ..Default::default() });
        deck.slides[0].objects.push(object);
        let odp = deck_feature_report(&deck, "odp");
        assert!(odp.items.iter().any(|item| item.feature == "charts" && item.status == "unchanged"));
    }
}
