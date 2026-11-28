//! Writer format capability definitions extracted from `compat`.

use crate::compat::{feature, FeatureSupport, SupportLevel};

pub(crate) fn writer_format_capabilities(ext: &str) -> Vec<FeatureSupport> {
    match ext {
        "docx" | "docm" | "dotx" => vec![
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
            feature("charts", SupportLevel::Full, "Column, bar, line, area, pie, doughnut and scatter charts round-trip as ChartML parts with cached values and an embedded workbook."),
            feature("digitalSignature", SupportLevel::Partial, "Existing signatures are preserved but the writer does not create new ones."),
            feature("macros", SupportLevel::Unsupported, "Macros are detected and never executed; saving writes the macro-free document, so the VBA project is dropped."),
        ],
        "odt" => vec![
            feature("sections", SupportLevel::Partial, "Section breaks are written as page breaks; per-section page setup is not exported to ODT."),
            feature("footnotes", SupportLevel::Full, "Footnotes and endnotes round-trip as text:note elements with matching citation numbers."),
            feature("trackChanges", SupportLevel::Partial, "Insertions and deletions round-trip via text:tracked-changes; formatting-only revisions are simplified."),
            feature("comments", SupportLevel::Partial, "Comments round-trip as office:annotation ranges with LibreOffice's resolved flag; ODF has no portable reply threading, so replies are stored as reply paragraphs."),
            feature("fields", SupportLevel::Unsupported, "Fields export as their cached text."),
            feature("tables", SupportLevel::Full, "Merged cells round-trip as table:number-columns-spanned / table:number-rows-spanned with covered-table-cell placeholders."),
            feature("images", SupportLevel::Partial, "Anchored images keep their wrap style (parallel/none); a wrap the model cannot name imports as top-and-bottom."),
            feature("tabStops", SupportLevel::Full, "Custom tab stops round-trip as style:tab-stops."),
            feature("watermark", SupportLevel::Full, "The watermark is written as a run-through frame and kept losslessly in meta:user-defined."),
            feature("charts", SupportLevel::Full, "Column, bar, line, area, pie, doughnut and scatter charts round-trip as embedded chart objects with cached values."),
        ],
        "udf" => vec![
            feature("paragraphs", SupportLevel::Full, "Paragraphs, headings and lists round-trip as UDF paragraphs."),
            feature("characterFormatting", SupportLevel::Full, "Bold, italic, underline, strikethrough, font, size, colour and super/subscript round-trip per segment."),
            feature("alignment", SupportLevel::Full, "Left, centre, right and justified alignment, indents, spacing and line height round-trip."),
            feature("tables", SupportLevel::Partial, "Tables, header rows, spans and cell backgrounds round-trip; nested tables are not written."),
            feature("images", SupportLevel::Partial, "Embedded base64 images (<image imageData=\"...\"/>) round-trip and display in the editor; other picture/object tags are preserved verbatim."),
            feature("charts", SupportLevel::Unsupported, "Charts export as text summaries in UDF; DOCX, ODT, PDF and .oswk keep full charts."),
            feature("rtfFragments", SupportLevel::Partial, "An embedded base64 RTF fragment is imported as plain text and reported."),
            feature("unknownElements", SupportLevel::Partial, "Elements this build does not model are preserved verbatim and reported, so a round trip does not drop them."),
            feature("pageSetup", SupportLevel::Partial, "Page size, margins and orientation round-trip; columns and header/footer distance do not."),
            feature("signature", SupportLevel::Unsupported, "A PKCS#7 signature block in the package is left untouched but not verified; editing the document invalidates it."),
        ],
        "rtf" => vec![
            feature("sections", SupportLevel::Partial, "Sections become page breaks."),
            feature("footnotes", SupportLevel::Partial, "Footnotes are written as real \\footnote destinations; the endnote class survives through an ignorable \\* marker because RTF has no per-note endnote class."),
            feature("trackChanges", SupportLevel::Partial, "Insertions and deletions are written as \\revised/\\deleted marks with a \\revtbl author table; formatting revisions are not representable."),
            feature("comments", SupportLevel::Partial, "Comments are written as Word annotations (\\atrfstart/\\atrfend ranges and \\annotation groups); replies are stored as reply paragraphs, timestamps keep minute precision and the resolved state survives only through an ignorable \\* marker."),
            feature("tables", SupportLevel::Partial, "Rows and cells are written, but colspan/rowspan merging is not representable."),
            feature("images", SupportLevel::Partial, "Images are embedded inline; wrapping and cropping are not representable."),
            feature("tabStops", SupportLevel::Unsupported, "Custom tab stops are not written; tab characters stay in the text."),
            feature("watermark", SupportLevel::Unsupported, "The watermark is not written to RTF."),
            feature("charts", SupportLevel::Unsupported, "Charts export as text summaries in RTF; DOCX, ODT, PDF and .oswk keep full charts."),
        ],
        _ => vec![
            feature("sections", SupportLevel::Partial, "Sections become page breaks."),
            feature("footnotes", SupportLevel::Partial, "Note text is appended at the end of the document."),
            feature("trackChanges", SupportLevel::Unsupported, "Tracked changes are applied visually only."),
            feature("tables", SupportLevel::Partial, "Tables export as tab-separated text (TXT), pipe tables (Markdown) or HTML; merged cells are flattened."),
            feature("images", SupportLevel::Partial, "HTML embeds image data; Markdown writes a reference without the data and TXT drops images."),
            feature("watermark", SupportLevel::Unsupported, "The watermark is not written to text formats."),
            feature("charts", SupportLevel::Unsupported, "Charts export as text summaries; DOCX, ODT, PDF and .oswk keep full charts."),
        ],
    }
}
