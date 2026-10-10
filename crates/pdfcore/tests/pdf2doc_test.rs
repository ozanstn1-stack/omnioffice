//! Layout recovery on a synthetic two-page report: a title, wrapped
//! paragraphs, a bulleted and a numbered list, and a page-number footer.
//!
//! The content-stream path is checked in full (it needs no pdfium); the
//! `recover_file` entry point, which prefers pdfium when the engine is
//! present, is checked for the same structure.

use lopdf::{dictionary, Document, Object, Stream};
use pdfcore::pdf2doc::{
    content_stream_pages, recover_file, recover_pages, BlockKind, LayoutPage, ListKind, NumberStyle, TextSource,
};
use std::path::Path;

/// A text line: font resource, size, x, baseline and the string (WinAnsi).
type Row = (&'static str, f64, f64, f64, &'static str);

const PARA_ONE: [&str; 3] = [
    "Field teams measured the water level at every station along the river during",
    "the spring survey and compared the readings with the records of the previous",
    "year, which showed a steady rise.",
];
const PARA_TWO: [&str; 2] =
    ["The second paragraph starts after a larger gap and explains how the sensors", "were calibrated before each run."];

fn page_one() -> Vec<Row> {
    vec![
        ("F2", 24.0, 72.0, 760.0, "Water Level Survey"),
        ("F1", 11.0, 72.0, 716.0, PARA_ONE[0]),
        ("F1", 11.0, 72.0, 702.0, PARA_ONE[1]),
        ("F1", 11.0, 72.0, 688.0, PARA_ONE[2]),
        ("F1", 11.0, 72.0, 664.0, PARA_TWO[0]),
        ("F1", 11.0, 72.0, 650.0, PARA_TWO[1]),
        ("F1", 11.0, 72.0, 626.0, "\u{95} Gauges were read twice a day"),
        ("F1", 11.0, 72.0, 612.0, "\u{95} Sensors were checked against a reference stick"),
        ("F1", 11.0, 72.0, 588.0, "1. Collect the raw readings from every station"),
        ("F1", 11.0, 72.0, 574.0, "2. Compare them with the archive"),
        ("F1", 10.0, 296.0, 40.0, "1"),
    ]
}

fn page_two() -> Vec<Row> {
    vec![
        ("F2", 16.0, 72.0, 760.0, "Results"),
        (
            "F1",
            11.0,
            72.0,
            730.0,
            "Levels rose at nearly every station, with the largest change recorded in the delta,",
        ),
        ("F1", 11.0, 72.0, 716.0, "where the river widens and slows down before it reaches the sea."),
        ("F1", 10.0, 296.0, 40.0, "2"),
    ]
}

fn build_pdf(path: &Path, pages: &[Vec<Row>]) {
    let mut doc = Document::new();
    doc.version = "1.7".to_string();
    let regular = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
    }));
    let bold = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica-Bold", "Encoding" => "WinAnsiEncoding",
    }));
    let resources = doc.add_object(Object::Dictionary(dictionary! {
        "Font" => dictionary! { "F1" => regular, "F2" => bold },
    }));
    let mut kids = Vec::new();
    for rows in pages {
        let mut content = String::new();
        for (font, size, x, y, text) in rows {
            let escaped = text.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)");
            content.push_str(&format!("BT\n/{font} {size} Tf\n{x} {y} Td\n({escaped}) Tj\nET\n"));
        }
        // Latin-1 bytes: the bullet is 0x95 in WinAnsi.
        let bytes: Vec<u8> = content.chars().map(|character| character as u32 as u8).collect();
        let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, bytes)));
        let page_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Resources" => resources,
            "Contents" => content_id,
        }));
        kids.push(page_id);
    }
    let pages_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Pages",
        "Kids" => kids.iter().map(|id| Object::Reference(*id)).collect::<Vec<Object>>(),
        "Count" => kids.len() as i64,
    }));
    for id in &kids {
        doc.get_object_mut(*id).unwrap().as_dict_mut().unwrap().set("Parent", Object::Reference(pages_id));
    }
    let catalog = doc.add_object(Object::Dictionary(dictionary! { "Type" => "Catalog", "Pages" => pages_id }));
    doc.trailer.set("Root", Object::Reference(catalog));
    doc.save(path).expect("save pdf");
}

fn texts(page: &LayoutPage) -> Vec<String> {
    page.blocks.iter().map(|block| block.text()).collect()
}

/// The words of a line, each positioned after the previous one plus a space;
/// an empty word stands for a second space.
fn prose(words: &[&'static str], x: f64, y: f64, size: f64) -> Vec<Row> {
    let space = pdfcore::numbering::helvetica_text_width(" ", size);
    let mut cursor = x;
    words
        .iter()
        .map(|word| {
            let row = ("F1", size, cursor, y, *word);
            cursor += pdfcore::numbering::helvetica_text_width(word, size) + space;
            row
        })
        .collect()
}

/// A table row: the left and the right cell on one baseline.
fn table_row(y: f64, left: &'static str, right: &'static str) -> Vec<Row> {
    vec![("F1", 11.0, 72.0, y, left), ("F1", 11.0, 320.0, y, right)]
}

fn check_structure(pages: &[LayoutPage]) {
    assert_eq!(pages.len(), 2);

    let first = &pages[0];
    let kinds: Vec<&BlockKind> = first.blocks.iter().map(|block| &block.kind).collect();
    assert_eq!(first.blocks.len(), 7, "{:#?}", texts(first));
    assert_eq!(kinds[0], &BlockKind::Heading(1));
    assert_eq!(kinds[1], &BlockKind::Paragraph);
    assert_eq!(kinds[2], &BlockKind::Paragraph);
    let list = |index: usize| match kinds[index] {
        BlockKind::ListItem(item) => item.clone(),
        other => panic!("block {index} is {other:?}"),
    };
    assert_eq!(list(3).kind, ListKind::Bullet);
    assert_eq!(list(4).kind, ListKind::Bullet);
    assert_eq!(list(5).kind, ListKind::Numbered(NumberStyle::Decimal));
    assert_eq!(list(6).kind, ListKind::Numbered(NumberStyle::Decimal));
    assert_eq!((list(5).start, list(6).start, list(6).level), (1, 1, 0));

    let text = texts(first);
    assert_eq!(text[0], "Water Level Survey");
    assert_eq!(text[1], PARA_ONE.join(" "));
    assert_eq!(text[2], PARA_TWO.join(" "));
    // Markers are stripped from the text.
    assert_eq!(text[3], "Gauges were read twice a day");
    assert_eq!(text[4], "Sensors were checked against a reference stick");
    assert_eq!(text[5], "Collect the raw readings from every station");
    assert_eq!(text[6], "Compare them with the archive");
    assert!(first.blocks[0].spans.iter().all(|span| span.bold));

    // The footer page number is gone from both pages.
    let second = &pages[1];
    assert_eq!(second.blocks.len(), 2, "{:#?}", texts(second));
    assert_eq!(second.blocks[0].kind, BlockKind::Heading(2));
    assert_eq!(second.blocks[0].text(), "Results");
    assert_eq!(second.blocks[1].kind, BlockKind::Paragraph);
    assert!(second.blocks[1].text().starts_with("Levels rose at nearly every station"));
    for page in pages {
        assert!(page.blocks.iter().all(|block| block.text().trim().parse::<u32>().is_err()));
    }
}

#[test]
fn recovers_structure_from_content_streams_without_pdfium() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.pdf");
    build_pdf(&path, &[page_one(), page_two()]);

    let positioned = content_stream_pages(&path, None).expect("content stream pages");
    assert_eq!(positioned.len(), 2);
    assert!((positioned[0].width - 595.0).abs() < 0.01 && (positioned[0].height - 842.0).abs() < 0.01);
    check_structure(&recover_pages(&positioned));
}

#[test]
fn recover_file_returns_the_same_structure() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.pdf");
    build_pdf(&path, &[page_one(), page_two()]);

    let recovered = recover_file(&path, None).expect("recover");
    assert!(recovered.has_text());
    if pdfcore::render::is_available() {
        eprintln!("pdfium available: source {:?}", recovered.source);
    } else {
        assert_eq!(recovered.source, TextSource::ContentStream);
    }
    check_structure(&recovered.pages);
}

#[test]
fn recovers_a_two_column_three_row_table() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("table.pdf");
    let mut rows = vec![("F1", 11.0, 72.0, 700.0, "Measurements")];
    rows.extend(table_row(660.0, "Station", "Flow rate"));
    rows.extend(table_row(646.0, "North bridge", "12"));
    rows.extend(table_row(632.0, "Old mill", "9"));
    build_pdf(&path, &[rows]);

    let positioned = content_stream_pages(&path, None).expect("content stream pages");
    let recovered = recover_pages(&positioned);
    let table = recovered[0]
        .blocks
        .iter()
        .find_map(|block| match &block.kind {
            BlockKind::Table(table) => Some(table),
            _ => None,
        })
        .expect("a Table block");
    assert_eq!(table.rows.len(), 3);
    let cell = |row: usize, column: usize| {
        table.rows[row][column].spans.iter().map(|span| span.text.as_str()).collect::<String>()
    };
    assert_eq!(cell(0, 0), "Station");
    assert_eq!(cell(0, 1), "Flow rate");
    assert_eq!(cell(1, 0), "North bridge");
    assert_eq!(cell(1, 1), "12");
    assert_eq!(cell(2, 0), "Old mill");
    assert_eq!(cell(2, 1), "9");
    // The line above the table stays a paragraph.
    assert!(recovered[0].blocks.iter().any(|block| block.kind == BlockKind::Paragraph));
}

#[test]
fn a_paragraph_with_two_spaces_stays_a_paragraph() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("prose.pdf");
    let mut rows =
        prose(&["The", "survey", "recorded", "a", "steady", "rise", "at", "every", "station"], 72.0, 700.0, 11.0);
    rows.extend(prose(
        &["and", "", "the", "team", "checked", "the", "gauges", "against", "the", "reference", "stick", "daily"],
        72.0,
        686.0,
        11.0,
    ));
    build_pdf(&path, &[rows]);

    let positioned = content_stream_pages(&path, None).expect("content stream pages");
    let recovered = recover_pages(&positioned);
    assert!(recovered[0].blocks.iter().all(|block| block.kind == BlockKind::Paragraph), "{:#?}", recovered[0].blocks);
}

#[test]
fn a_pdf_without_text_has_no_text() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("blank.pdf");
    build_pdf(&path, &[Vec::new()]);
    let recovered = recover_file(&path, None).expect("recover");
    assert!(!recovered.has_text());
}
