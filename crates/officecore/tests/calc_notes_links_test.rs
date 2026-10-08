//! Calc cell notes (comments) and hyperlinks as editable data: author,
//! visibility, link display text, screen tip and internal references, in XLSX
//! and ODS, plus the rule that only http, https, mailto and internal targets are
//! ever written or read.
//!
//! The hand-written parts are the shapes Excel 365 and LibreOffice 7 write.

use officecore::compat;
use officecore::model::*;
use officecore::odf;
use officecore::xlsx;
use officecore::zip::{ZipLimits, ZipReader, ZipWriter};

fn text(value: &str) -> CellValue {
    CellValue::Text(value.to_string())
}

fn note(value: &str, comment: &str, author: Option<&str>, visible: bool) -> Cell {
    Cell {
        value: text(value),
        comment: Some(comment.into()),
        comment_author: author.map(str::to_string),
        comment_visible: visible,
        ..Default::default()
    }
}

fn link(value: &str, target: &str) -> Cell {
    Cell { value: text(value), link: Some(target.into()), ..Default::default() }
}

fn sheet_of(workbook: &Workbook) -> &Sheet {
    &workbook.sheets[0]
}

fn workbook_with(cells: Vec<(&str, Cell)>) -> Workbook {
    let mut workbook = Workbook::new_blank("Notes");
    workbook.sheets[0].name = "Data".into();
    workbook.sheets.push(Sheet::new("My Sheet"));
    for (address, cell) in cells {
        workbook.sheets[0].set(address, cell);
    }
    workbook
}

/// Rebuilds a package with some parts replaced.
fn rebuild(bytes: &[u8], replacements: &[(&str, &str)]) -> Vec<u8> {
    let reader = ZipReader::open(bytes.to_vec()).unwrap();
    let mut writer = ZipWriter::new();
    for (name, data) in reader.read_all(ZipLimits::default()).unwrap() {
        match replacements.iter().find(|(part, _)| *part == name) {
            Some((_, replacement)) => writer.add_text(&name, replacement),
            None => writer.add(&name, &data),
        }
    }
    writer.finish()
}

// ---------------------------------------------------------------------------
// The link target rule
// ---------------------------------------------------------------------------

#[test]
fn only_http_https_mailto_and_internal_targets_are_safe() {
    for allowed in [
        "https://example.org/report?a=1&b=2#top",
        "HTTP://EXAMPLE.ORG",
        "  https://example.org/padded  ",
        "mailto:ada@example.org?subject=Hi",
        "MAILTO:ada@example.org",
        "#Sheet2!A1",
        "#'My Sheet'!A1:B2",
        "#TaxRate",
    ] {
        assert_eq!(safe_link_target(allowed).as_deref(), Some(allowed.trim()), "{allowed}");
    }
    for denied in [
        "",
        "   ",
        "#",
        "http://",
        "https:///",
        "mailto:",
        "file:///etc/passwd",
        "FILE:///C:/Windows/System32/calc.exe",
        "javascript:alert(1)",
        " javascript:alert(1)",
        "JaVaScRiPt:alert(1)",
        "vbscript:msgbox(1)",
        "data:text/html;base64,PHNjcmlwdD4=",
        "vnd.sun.star.script:Standard.Module1.Main?language=Basic&location=document",
        "ftp://example.org/file",
        "tel:+15551234567",
        "\\\\server\\share\\file.xlsx",
        "//server/share/file.xlsx",
        "C:\\Windows\\System32\\cmd.exe",
        "report.xlsx",
        "../secret.xlsx",
        "https://example.org/\r\nHost: evil",
        "https://example.org/\u{0}",
    ] {
        assert_eq!(safe_link_target(denied), None, "{denied:?} must be refused");
    }
    assert_eq!(safe_link_target(&format!("https://example.org/{}", "a".repeat(9_000))), None, "absurd length");
}

// ---------------------------------------------------------------------------
// XLSX notes
// ---------------------------------------------------------------------------

#[test]
fn xlsx_notes_round_trip_author_visibility_and_multiple_lines() {
    let workbook = workbook_with(vec![
        ("A1", note("Total", "Check the VAT\nbefore sending", Some("Ada Lovelace"), true)),
        ("B2", note("Net", "Rounded", Some("Bob"), false)),
        ("C3", note("Gross", "Default author", None, false)),
        // A note on a cell with no value at all.
        (
            "D4",
            Cell {
                comment: Some("standalone".into()),
                comment_author: Some("Ada Lovelace".into()),
                ..Default::default()
            },
        ),
    ]);
    let bytes = xlsx::write_xlsx(&workbook).unwrap();
    let reader = ZipReader::open(bytes.clone()).unwrap();

    let comments = reader.read_text("xl/comments1.xml").unwrap();
    assert!(
        comments.contains(
            "<authors><author>Ada Lovelace</author><author>Bob</author><author>OmniOffice</author></authors>"
        ),
        "{comments}"
    );
    assert!(comments.contains("<comment ref=\"A1\" authorId=\"0\">"));
    assert!(comments.contains("<comment ref=\"B2\" authorId=\"1\">"));
    assert!(comments.contains("<comment ref=\"C3\" authorId=\"2\">"));
    assert!(comments.contains("Check the VAT\nbefore sending"));

    let vml = reader.read_text("xl/drawings/vmlDrawing1.vml").unwrap();
    assert_eq!(vml.matches("visibility:visible").count(), 1, "{vml}");
    assert_eq!(vml.matches("<x:Visible/>").count(), 1);
    assert_eq!(vml.matches("visibility:hidden").count(), 3);

    let read = xlsx::read_workbook_bytes(&bytes).unwrap();
    let sheet = sheet_of(&read.workbook);
    for (address, source) in &sheet_of(&workbook).cells {
        let back = sheet.get(address).unwrap_or_else(|| panic!("{address} disappeared"));
        assert_eq!(back.comment, source.comment, "{address}");
        assert_eq!(back.comment_author, source.comment_author, "{address}");
        assert_eq!(back.comment_visible, source.comment_visible, "{address}");
    }
    assert_eq!(sheet.get("D4").map(|cell| cell.value.clone()), Some(CellValue::Empty));
    assert!(read.workbook.sheets[1].cells.is_empty());
}

/// What Excel 365 writes for a note: the author in a bold first run, the text in
/// a second; plus a threaded comment's legacy copy whose author is `tc={guid}`.
const EXCEL_COMMENTS: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n",
    "<comments xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">",
    "<authors><author>Ada Lovelace</author><author>tc={7A2E1C22-1111-4B55-9A2F-3D5E8C9B0A11}</author></authors><commentList>",
    "<comment ref=\"A1\" authorId=\"0\" shapeId=\"0\"><text>",
    "<r><rPr><b/><sz val=\"9\"/><color indexed=\"81\"/><rFont val=\"Tahoma\"/><family val=\"2\"/></rPr><t>Ada Lovelace:</t></r>",
    "<r><rPr><sz val=\"9\"/><color indexed=\"81\"/><rFont val=\"Tahoma\"/><family val=\"2\"/></rPr><t xml:space=\"preserve\">\nCheck the VAT</t></r>",
    "</text></comment>",
    "<comment ref=\"B2\" authorId=\"1\" shapeId=\"0\"><text><t>[Threaded comment]\n\nYour version of Excel allows you to read this threaded comment; however, any edits to it will get removed if the file is opened in a newer version of Excel. Learn more: https://go.microsoft.com/fwlink/?linkid=870924\n\nComment:\n    Please confirm\n    both figures</t></text></comment>",
    "<comment ref=\"C3\" authorId=\"0\" shapeId=\"0\"><text><t>Plain, no author run</t></text></comment>",
    "</commentList></comments>"
);

const EXCEL_VML: &str = concat!(
    "<xml xmlns:v=\"urn:schemas-microsoft-com:vml\" xmlns:o=\"urn:schemas-microsoft-com:office:office\" xmlns:x=\"urn:schemas-microsoft-com:office:excel\">",
    "<o:shapelayout v:ext=\"edit\"><o:idmap v:ext=\"edit\" data=\"1\"/></o:shapelayout>",
    "<v:shapetype id=\"_x0000_t202\" coordsize=\"21600,21600\" o:spt=\"202\" path=\"m,l,21600r21600,l21600,xe\"><v:stroke joinstyle=\"miter\"/><v:path gradientshapeok=\"t\" o:connecttype=\"rect\"/></v:shapetype>",
    "<v:shape id=\"_x0000_s1025\" type=\"#_x0000_t202\" style=\"position:absolute;margin-left:59.25pt;margin-top:1.5pt;width:108pt;height:59.25pt;z-index:1;visibility:visible\" fillcolor=\"#ffffe1\" o:insetmode=\"auto\">",
    "<v:fill color2=\"#ffffe1\"/><v:shadow on=\"t\" color=\"black\" obscured=\"t\"/><v:path o:connecttype=\"none\"/><v:textbox style=\"mso-direction-alt:auto\"><div style=\"text-align:left\"></div></v:textbox>",
    "<x:ClientData ObjectType=\"Note\"><x:MoveWithCells/><x:SizeWithCells/><x:Anchor>1, 15, 0, 2, 3, 15, 4, 16</x:Anchor><x:AutoFill>False</x:AutoFill><x:Row>0</x:Row><x:Column>0</x:Column><x:Visible/></x:ClientData></v:shape>",
    "<v:shape id=\"_x0000_s1026\" type=\"#_x0000_t202\" style=\"position:absolute;visibility:hidden\" fillcolor=\"#ffffe1\">",
    "<x:ClientData ObjectType=\"Note\"><x:Row>1</x:Row><x:Column>1</x:Column></x:ClientData></v:shape>",
    "<v:shape id=\"_x0000_s1027\" type=\"#_x0000_t202\" style=\"position:absolute; visibility: visible\" fillcolor=\"#ffffe1\">",
    "<x:ClientData ObjectType=\"Note\"><x:Row>2</x:Row><x:Column>2</x:Column></x:ClientData></v:shape>",
    "</xml>"
);

#[test]
fn imports_the_notes_excel_writes() {
    let workbook = workbook_with(vec![("A1", note("a", "x", None, false))]);
    let bytes = xlsx::write_xlsx(&workbook).unwrap();
    let patched = rebuild(&bytes, &[("xl/comments1.xml", EXCEL_COMMENTS), ("xl/drawings/vmlDrawing1.vml", EXCEL_VML)]);
    let read = xlsx::read_workbook_bytes(&patched).unwrap();
    let sheet = sheet_of(&read.workbook);

    let first = sheet.get("A1").unwrap();
    assert_eq!(first.comment.as_deref(), Some("Check the VAT"), "the bold author run is the author, not the text");
    assert_eq!(first.comment_author.as_deref(), Some("Ada Lovelace"));
    assert!(first.comment_visible, "x:Visible keeps the note open");

    let threaded = sheet.get("B2").unwrap();
    assert_eq!(threaded.comment.as_deref(), Some("Please confirm\nboth figures"), "the legacy notice is dropped");
    assert_eq!(threaded.comment_author, None, "tc={{guid}} names nobody");
    assert!(!threaded.comment_visible);

    let plain = sheet.get("C3").unwrap();
    assert_eq!(plain.comment.as_deref(), Some("Plain, no author run"));
    assert!(plain.comment_visible, "visibility:visible with spaces still counts");
}

#[test]
fn a_broken_vml_part_only_costs_the_visibility() {
    let workbook = workbook_with(vec![("A1", note("a", "kept", Some("Ada"), true))]);
    let bytes = xlsx::write_xlsx(&workbook).unwrap();
    let read =
        xlsx::read_workbook_bytes(&rebuild(&bytes, &[("xl/drawings/vmlDrawing1.vml", "<xml><v:shape")])).unwrap();
    let cell = sheet_of(&read.workbook).get("A1").unwrap();
    assert_eq!(cell.comment.as_deref(), Some("kept"));
    assert_eq!(cell.comment_author.as_deref(), Some("Ada"));
    assert!(!cell.comment_visible);
}

// ---------------------------------------------------------------------------
// XLSX hyperlinks
// ---------------------------------------------------------------------------

fn linked_cells() -> Vec<(&'static str, Cell)> {
    vec![
        (
            "A1",
            Cell {
                link_display: Some("Open the report".into()),
                link_tooltip: Some("Quarterly report".into()),
                ..link("Report", "https://example.org/report?a=1&b=2")
            },
        ),
        ("A2", link("Write to Ada", "mailto:ada@example.org?subject=Hello")),
        ("A3", Cell { link_tooltip: Some("Jump".into()), ..link("Go to totals", "#Totals!B2") }),
        ("A4", link("Spaced", "#'My Sheet'!A1")),
        // A link on a cell with no value, which is what the importer creates.
        ("A5", Cell { link: Some("https://example.org/empty".into()), ..Default::default() }),
        (
            "A6",
            Cell { value: CellValue::Number(42.0), link: Some("https://example.org/n".into()), ..Default::default() },
        ),
    ]
}

#[test]
fn xlsx_links_round_trip_with_display_tooltip_and_internal_references() {
    let workbook = workbook_with(linked_cells());
    let bytes = xlsx::write_xlsx(&workbook).unwrap();
    let reader = ZipReader::open(bytes.clone()).unwrap();
    let sheet_xml = reader.read_text("xl/worksheets/sheet1.xml").unwrap();
    assert!(sheet_xml.contains("<hyperlinks count=\"6\">"), "{sheet_xml}");
    assert!(
        sheet_xml
            .contains("<hyperlink ref=\"A1\" r:id=\"rId1\" display=\"Open the report\" tooltip=\"Quarterly report\"/>"),
        "{sheet_xml}"
    );
    assert!(
        sheet_xml.contains("<hyperlink ref=\"A3\" location=\"Totals!B2\" tooltip=\"Jump\"/>"),
        "internal link: a location, no relationship: {sheet_xml}"
    );
    assert!(sheet_xml.contains("location=\"&apos;My Sheet&apos;!A1\""));
    let rels = reader.read_text("xl/worksheets/_rels/sheet1.xml.rels").unwrap();
    assert_eq!(rels.matches("TargetMode=\"External\"").count(), 4, "{rels}");
    assert!(rels.contains("Target=\"mailto:ada@example.org?subject=Hello\""));
    assert!(!rels.contains("Totals"));

    let read = xlsx::read_workbook_bytes(&bytes).unwrap();
    assert!(read.warnings.iter().all(|warning| !warning.contains("hyperlink")), "{:?}", read.warnings);
    let sheet = sheet_of(&read.workbook);
    for (address, source) in &sheet_of(&workbook).cells {
        let back = sheet.get(address).unwrap_or_else(|| panic!("{address} disappeared"));
        assert_eq!(back.link, source.link, "{address}");
        assert_eq!(back.link_display, source.link_display, "{address}");
        assert_eq!(back.link_tooltip, source.link_tooltip, "{address}");
    }
    assert_eq!(sheet.get("A5").map(|cell| cell.value.clone()), Some(CellValue::Empty));
    assert_eq!(sheet.get("A6").map(|cell| cell.value.clone()), Some(CellValue::Number(42.0)));
}

/// The links of an Excel sheet: a ranged link, a mailto with a subject, an
/// internal location, and the kinds that must never survive.
const EXCEL_LINKS: &str = concat!(
    "<hyperlinks>",
    "<hyperlink ref=\"A1:A3\" r:id=\"rId10\" display=\"Same text\"/>",
    "<hyperlink ref=\"B1\" r:id=\"rId11\" tooltip=\"Mail Ada\"/>",
    "<hyperlink ref=\"B2\" location=\"Data!D5\" display=\"Jump\"/>",
    "<hyperlink ref=\"C1\" r:id=\"rId12\"/>",
    "<hyperlink ref=\"C2\" r:id=\"rId13\"/>",
    "<hyperlink ref=\"C3\" r:id=\"rId14\"/>",
    "<hyperlink ref=\"C4\" r:id=\"rId15\" location=\"Sheet1!A1\"/>",
    "<hyperlink ref=\"C5\" r:id=\"rId16\"/>",
    "<hyperlink ref=\"C6\" r:id=\"rId17\"/>",
    "<hyperlink ref=\"C7\" r:id=\"rId18\"/>",
    "<hyperlink ref=\"C8\" r:id=\"rId99\"/>",
    "</hyperlinks>"
);

fn relationship(id: &str, target: &str) -> String {
    format!(
        "<Relationship Id=\"{id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink\" Target=\"{target}\" TargetMode=\"External\"/>"
    )
}

fn package_with_foreign_links() -> Vec<u8> {
    let mut workbook = workbook_with(vec![
        ("A1", link("Same text", "https://example.org/x")),
        ("A2", Cell { value: text("Other text"), ..Default::default() }),
        ("A3", Cell { value: CellValue::Empty, ..Default::default() }),
        ("B1", Cell { value: text("Mail"), ..Default::default() }),
        ("B2", Cell { value: text("Jump"), ..Default::default() }),
    ]);
    workbook.sheets[0].set("C1", Cell { value: text("c"), ..Default::default() });
    let bytes = xlsx::write_xlsx(&workbook).unwrap();
    let reader = ZipReader::open(bytes.clone()).unwrap();
    let sheet = reader.read_text("xl/worksheets/sheet1.xml").unwrap();
    let start = sheet.find("<hyperlinks").unwrap();
    let end = sheet.find("</hyperlinks>").unwrap() + "</hyperlinks>".len();
    let patched_sheet = format!("{}{}{}", &sheet[..start], EXCEL_LINKS, &sheet[end..]);
    let rels = reader.read_text("xl/worksheets/_rels/sheet1.xml.rels").unwrap();
    let extra = [
        relationship("rId10", "https://example.org/range?x=1&amp;y=2"),
        relationship("rId11", "mailto:ada@example.org?subject=Hi"),
        relationship("rId12", "file:///C:/Windows/System32/calc.exe"),
        relationship("rId13", "javascript:alert(document.domain)"),
        relationship("rId14", "\\\\attacker\\share\\payload.xlsx"),
        relationship("rId15", "report.xlsx"),
        relationship("rId16", "vnd.sun.star.script:Standard.Module1.Main?language=Basic"),
        relationship("rId17", "https://example.org/ok#frag"),
        relationship("rId18", "ftp://example.org/file"),
    ]
    .concat();
    let patched_rels = rels.replace("</Relationships>", &format!("{extra}</Relationships>"));
    rebuild(
        &bytes,
        &[("xl/worksheets/sheet1.xml", &patched_sheet), ("xl/worksheets/_rels/sheet1.xml.rels", &patched_rels)],
    )
}

#[test]
fn imports_the_links_excel_writes_and_drops_the_unsafe_ones() {
    let read = xlsx::read_workbook_bytes(&package_with_foreign_links()).unwrap();
    let sheet = sheet_of(&read.workbook);
    let target = |address: &str| sheet.get(address).and_then(|cell| cell.link.clone());

    // A range link covers every cell of the range.
    for address in ["A1", "A2", "A3"] {
        assert_eq!(target(address).as_deref(), Some("https://example.org/range?x=1&y=2"), "{address}");
    }
    // `display` equal to the cell text is redundant; otherwise it is kept.
    assert_eq!(sheet.get("A1").unwrap().link_display, None);
    assert_eq!(sheet.get("A2").unwrap().link_display.as_deref(), Some("Same text"));
    assert_eq!(sheet.get("A3").unwrap().link_display.as_deref(), Some("Same text"));

    assert_eq!(target("B1").as_deref(), Some("mailto:ada@example.org?subject=Hi"));
    assert_eq!(sheet.get("B1").unwrap().link_tooltip.as_deref(), Some("Mail Ada"));
    assert_eq!(target("B2").as_deref(), Some("#Data!D5"));

    // file:, javascript:, UNC, a bare relative path, a macro link, a web URL
    // with a location (kept) and ftp: (dropped), and a dangling relationship.
    for address in ["C1", "C2", "C3", "C5", "C7", "C8"] {
        assert!(target(address).is_none(), "{address} must not carry a link: {:?}", target(address));
    }
    assert!(sheet.get("C4").is_none() || target("C4").is_none(), "a relative file with a location is dropped");
    assert_eq!(target("C6").as_deref(), Some("https://example.org/ok#frag"));
    let warning = read.warnings.iter().find(|warning| warning.contains("hyperlink")).expect("a warning");
    assert!(warning.starts_with("6 hyperlink(s) on sheet \"Data\" were dropped"), "{warning}");
    assert!(warning.contains("http, https, mailto and internal references"));
}

#[test]
fn xlsx_writer_never_writes_unsafe_targets() {
    let workbook = workbook_with(vec![
        ("A1", link("file", "file:///etc/passwd")),
        ("A2", link("script", "javascript:alert(1)")),
        ("A3", link("unc", "\\\\server\\share")),
        ("A4", link("ok", "https://example.org/")),
    ]);
    let result = xlsx::write_xlsx_package(&workbook).unwrap();
    assert!(
        result.warnings.iter().any(|warning| warning.starts_with("3 hyperlink(s) on sheet \"Data\" were not written")),
        "{:?}",
        result.warnings
    );
    let reader = ZipReader::open(result.bytes.clone()).unwrap();
    let sheet_xml = reader.read_text("xl/worksheets/sheet1.xml").unwrap();
    assert!(sheet_xml.contains("<hyperlinks count=\"1\">"));
    let rels = reader.read_text("xl/worksheets/_rels/sheet1.xml.rels").unwrap();
    for forbidden in ["file:", "javascript:", "server"] {
        assert!(!rels.contains(forbidden) && !sheet_xml.contains(forbidden), "{forbidden}");
    }
    // The cell values survive; only the links are gone.
    let read = xlsx::read_workbook_bytes(&result.bytes).unwrap();
    assert_eq!(sheet_of(&read.workbook).get("A1").map(|cell| cell.value.clone()), Some(text("file")));
    assert_eq!(sheet_of(&read.workbook).get("A1").and_then(|cell| cell.link.clone()), None);
    assert_eq!(
        sheet_of(&read.workbook).get("A4").and_then(|cell| cell.link.clone()).as_deref(),
        Some("https://example.org/")
    );
}

// ---------------------------------------------------------------------------
// ODS
// ---------------------------------------------------------------------------

#[test]
fn ods_notes_and_links_round_trip() {
    let mut cells = vec![
        ("A1", note("Total", "Check the VAT\nbefore sending", Some("Ada Lovelace"), true)),
        ("B2", note("Net", "Rounded", None, false)),
        ("D4", Cell { comment: Some("standalone".into()), ..Default::default() }),
    ];
    cells.extend(linked_cells().into_iter().filter(|(address, _)| *address != "A5").map(|(address, cell)| {
        // Rows 10+ keep the links clear of the notes above.
        let moved = match address {
            "A1" => "F1",
            "A2" => "F2",
            "A3" => "F3",
            "A4" => "F4",
            _ => "F6",
        };
        (moved, cell)
    }));
    let workbook = workbook_with(cells);
    let bytes = odf::write_ods(&workbook).unwrap();
    let content = ZipReader::open(bytes.clone()).unwrap().read_text("content.xml").unwrap();
    assert!(content.contains(
        "<office:annotation office:display=\"true\"><dc:creator>Ada Lovelace</dc:creator><text:p>Check the VAT</text:p><text:p>before sending</text:p></office:annotation><text:p>Total</text:p>"
    ), "{content}");
    assert!(
        content.contains("<office:annotation office:display=\"false\"><text:p>Rounded</text:p></office:annotation>")
    );
    assert!(content.contains(
        "<text:a xlink:type=\"simple\" xlink:href=\"https://example.org/report?a=1&amp;b=2\" office:title=\"Quarterly report\">Report</text:a>"
    ), "{content}");
    // LibreOffice ignores the paragraph of a cell that also has a string value, so
    // a linked cell carries its text in the paragraph only.
    assert!(!content.contains("office:string-value=\"Report\""), "{content}");
    assert!(content.contains("office:string-value=\"Net\""), "unlinked cells keep it");
    assert!(content.contains("xlink:href=\"#Totals.B2\""), "internal links use ODF's sheet separator: {content}");
    assert!(content.contains("xlink:href=\"#&apos;My Sheet&apos;.A1\""));

    let read = odf::read_ods(&bytes).unwrap();
    assert!(read.warnings.iter().all(|warning| !warning.contains("hyperlink")), "{:?}", read.warnings);
    let sheet = sheet_of(&read.workbook);
    for (address, source) in &sheet_of(&workbook).cells {
        let back = sheet.get(address).unwrap_or_else(|| panic!("{address} disappeared"));
        assert_eq!(back.value, source.value, "{address}: the note text is not part of the value");
        assert_eq!(back.comment, source.comment, "{address}");
        assert_eq!(back.comment_author, source.comment_author, "{address}");
        assert_eq!(back.comment_visible, source.comment_visible, "{address}");
        assert_eq!(back.link, source.link, "{address}");
        assert_eq!(back.link_tooltip, source.link_tooltip, "{address}");
    }
    assert_eq!(sheet.get("D4").map(|cell| cell.value.clone()), Some(CellValue::Empty));
}

#[test]
fn ods_link_on_an_empty_cell_shows_its_label() {
    let workbook = workbook_with(vec![
        ("A1", Cell { link: Some("https://example.org/empty".into()), ..Default::default() }),
        (
            "A2",
            Cell {
                link: Some("https://example.org/named".into()),
                link_display: Some("Named link".into()),
                ..Default::default()
            },
        ),
    ]);
    let read = odf::read_ods(&odf::write_ods(&workbook).unwrap()).unwrap();
    let sheet = sheet_of(&read.workbook);
    assert_eq!(sheet.get("A1").map(|cell| cell.value.clone()), Some(text("https://example.org/empty")));
    assert_eq!(sheet.get("A2").map(|cell| cell.value.clone()), Some(text("Named link")));
    assert_eq!(sheet.get("A2").and_then(|cell| cell.link.clone()).as_deref(), Some("https://example.org/named"));
}

#[test]
fn ods_writer_never_writes_unsafe_targets() {
    let workbook = workbook_with(vec![
        ("A1", link("file", "file:///etc/passwd")),
        ("A2", link("script", "javascript:alert(1)")),
        ("A3", link("macro", "vnd.sun.star.script:Standard.Module1.Main?language=Basic&location=document")),
        ("A4", link("ok", "mailto:ada@example.org")),
    ]);
    let bytes = odf::write_ods(&workbook).unwrap();
    let content = ZipReader::open(bytes.clone()).unwrap().read_text("content.xml").unwrap();
    for forbidden in ["file:", "javascript:", "vnd.sun.star"] {
        assert!(!content.contains(forbidden), "{forbidden}");
    }
    assert_eq!(content.matches("<text:a ").count(), 1);
    let read = odf::read_ods(&bytes).unwrap();
    assert_eq!(sheet_of(&read.workbook).get("A1").map(|cell| cell.value.clone()), Some(text("file")));
}

/// What LibreOffice 7 writes: an annotation first in the cell (creator, date,
/// hidden, several paragraphs), links in `text:a` with `office:title`, an internal
/// reference, and the targets that must not survive.
fn libreoffice_package() -> Vec<u8> {
    let content = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
        "<office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
        "xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" ",
        "xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" ",
        "xmlns:dc=\"http://purl.org/dc/elements/1.1/\" ",
        "xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\" ",
        "xmlns:xlink=\"http://www.w3.org/1999/xlink\" office:version=\"1.3\">",
        "<office:body><office:spreadsheet><table:table table:name=\"Sheet1\">",
        "<table:table-row>",
        "<table:table-cell office:value-type=\"string\" calcext:value-type=\"string\"><office:annotation svg:width=\"3.2cm\" svg:height=\"1.2cm\" svg:x=\"4cm\" svg:y=\"0cm\" office:display=\"false\">",
        "<dc:creator>Ada Lovelace</dc:creator><dc:date>2026-03-02T09:15:00</dc:date><text:p>First line</text:p><text:p>Second line</text:p></office:annotation><text:p>Value</text:p></table:table-cell>",
        "<table:table-cell office:value-type=\"string\"><office:annotation office:display=\"true\"><text:p text:style-name=\"P1\">Shown</text:p></office:annotation><text:p>Open</text:p></table:table-cell>",
        "</table:table-row>",
        "<table:table-row>",
        "<table:table-cell office:value-type=\"string\"><text:p><text:a xlink:href=\"https://example.org/\" office:title=\"The site\" xlink:type=\"simple\">Site</text:a></text:p></table:table-cell>",
        "<table:table-cell office:value-type=\"string\"><text:p><text:a xlink:href=\"#Sheet1.A1\" xlink:type=\"simple\">Back to A1</text:a></text:p></table:table-cell>",
        "<table:table-cell office:value-type=\"string\"><text:p><text:a xlink:href=\"file:///C:/Windows/System32/calc.exe\" xlink:type=\"simple\">Run</text:a></text:p></table:table-cell>",
        "<table:table-cell office:value-type=\"string\"><text:p><text:a xlink:href=\"vnd.sun.star.script:Standard.Module1.Main?language=Basic&amp;location=document\" xlink:type=\"simple\">Macro</text:a></text:p></table:table-cell>",
        "<table:table-cell office:value-type=\"string\"><text:p><text:a xlink:href=\"../other.ods\" xlink:type=\"simple\">Relative</text:a></text:p></table:table-cell>",
        "<table:table-cell office:value-type=\"string\"><text:p><text:a xlink:href=\"mailto:ada@example.org\" xlink:type=\"simple\">Mail</text:a></text:p></table:table-cell>",
        "<table:table-cell office:value-type=\"float\" office:value=\"7\"><text:p><text:a xlink:href=\"https://example.org/n\" xlink:type=\"simple\">7</text:a></text:p></table:table-cell>",
        "</table:table-row>",
        "</table:table></office:spreadsheet></office:body></office:document-content>"
    );
    let mut zip = ZipWriter::new();
    zip.add_text("mimetype", "application/vnd.oasis.opendocument.spreadsheet");
    zip.add_text("content.xml", content);
    zip.finish()
}

#[test]
fn reads_the_notes_and_links_libreoffice_writes() {
    let read = odf::read_ods(&libreoffice_package()).unwrap();
    let sheet = sheet_of(&read.workbook);

    let first = sheet.get("A1").unwrap();
    assert_eq!(first.value, text("Value"), "the annotation's paragraphs are not the cell text");
    assert_eq!(first.comment.as_deref(), Some("First line\nSecond line"));
    assert_eq!(first.comment_author.as_deref(), Some("Ada Lovelace"));
    assert!(!first.comment_visible);

    let shown = sheet.get("B1").unwrap();
    assert_eq!((shown.value.clone(), shown.comment.as_deref()), (text("Open"), Some("Shown")));
    assert!(shown.comment_visible);
    assert_eq!(shown.comment_author, None);

    let site = sheet.get("A2").unwrap();
    assert_eq!(site.value, text("Site"));
    assert_eq!(site.link.as_deref(), Some("https://example.org/"));
    assert_eq!(site.link_tooltip.as_deref(), Some("The site"));
    assert_eq!(sheet.get("B2").and_then(|cell| cell.link.clone()).as_deref(), Some("#Sheet1!A1"));
    assert_eq!(sheet.get("F2").and_then(|cell| cell.link.clone()).as_deref(), Some("mailto:ada@example.org"));
    assert_eq!(sheet.get("G2").and_then(|cell| cell.link.clone()).as_deref(), Some("https://example.org/n"));
    assert_eq!(sheet.get("G2").map(|cell| cell.value.clone()), Some(CellValue::Number(7.0)));

    // file:, a macro link and a relative path are dropped, but the label stays.
    for address in ["C2", "D2", "E2"] {
        let cell = sheet.get(address).unwrap();
        assert_eq!(cell.link, None, "{address}");
        assert!(matches!(cell.value, CellValue::Text(_)), "{address}");
    }
    assert!(
        read.warnings.iter().any(|warning| warning.starts_with("3 hyperlink(s) were dropped")),
        "{:?}",
        read.warnings
    );
}

// ---------------------------------------------------------------------------
// Compatibility report
// ---------------------------------------------------------------------------

#[test]
fn the_compatibility_matrix_documents_notes_and_links() {
    for format in ["xlsx", "ods"] {
        let capabilities = compat::format_capabilities(format);
        for (feature, word) in [("comments", "author"), ("hyperlinks", "https")] {
            let entry = capabilities
                .features
                .iter()
                .find(|entry| entry.feature == feature)
                .unwrap_or_else(|| panic!("{format} has no {feature} entry"));
            assert!(entry.note.contains(word), "{format} {feature}: {}", entry.note);
        }
    }
    let report = compat::workbook_feature_report(&workbook_with(linked_cells()), "xlsx");
    assert!(!report.lossy(), "{:?}", report.items);
}
