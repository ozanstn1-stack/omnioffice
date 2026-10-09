//! Round-trip tests against the real sample documents in `samples/`.
//!
//! These cover the workflow the user performs by hand:
//!   open a file -> verify content -> save it again -> reopen -> verify again.

use officecore::model::{
    Block, CellValue, ChartData, ChartSeries, Deck, Footnote, ParaProps, RevisionMark, Run, SlideObject, TabStop,
    TableCell, TextFrame, Watermark,
};
use officecore::{docx, odf, pptx, rtf, xlsx};
use std::path::{Path, PathBuf};

fn samples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("samples")
}

fn require(path: &Path) -> PathBuf {
    assert!(
        path.exists(),
        "sample {} is missing - run `cargo run -p officecore --example make-office-samples`",
        path.display()
    );
    path.to_path_buf()
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("osak-roundtrip");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

#[test]
fn docx_open_edit_save_reopen() {
    let source = require(&samples_dir().join("test-document.docx"));
    let first = docx::read_docx_file(&source).unwrap();
    assert!(first.document.plain_text().contains("Test document"));
    assert!(first.document.plain_text().contains("Second page"));
    assert_eq!(first.document.header.len(), 1);
    assert_eq!(first.document.footer.len(), 1);
    let tables =
        first.document.blocks.iter().filter(|block| matches!(block, officecore::model::Block::Table { .. })).count();
    assert_eq!(tables, 1, "the sample document must contain one table");
    let images =
        first.document.blocks.iter().filter(|block| matches!(block, officecore::model::Block::Image { .. })).count();
    assert_eq!(images, 1, "the sample document must contain one image");

    // Edit: append a paragraph, then save and reopen.
    let mut edited = first.document;
    edited.blocks.push(officecore::model::Block::paragraph("Round trip marker"));
    let target = temp("roundtrip-document.docx");
    docx::write_docx_file(&target, &edited).unwrap();
    let second = docx::read_docx_file(&target).unwrap();
    assert!(second.document.plain_text().contains("Round trip marker"));
    assert!(second.document.plain_text().contains("Test document"));
    assert_eq!(second.document.header.len(), 1);
    assert_eq!(second.document.footer.len(), 1);
}

#[test]
fn docx_package_can_be_read_by_other_office_suites() {
    // Structural checks that Word/LibreOffice rely on.
    let source = require(&samples_dir().join("test-document.docx"));
    let bytes = std::fs::read(&source).unwrap();
    let reader = officecore::zip::ZipReader::open(bytes).unwrap();
    for part in [
        "[Content_Types].xml",
        "_rels/.rels",
        "word/document.xml",
        "word/styles.xml",
        "word/numbering.xml",
        "word/header1.xml",
        "word/footer1.xml",
    ] {
        assert!(reader.contains(part), "missing package part {part}");
    }
    let document = reader.read_text("word/document.xml").unwrap();
    assert!(document.contains("<w:tbl>"), "table markup missing");
    assert!(document.contains("<w:drawing>"), "image markup missing");
    assert!(document.contains("<w:sectPr"), "section properties missing");
}

#[test]
fn odt_and_rtf_roundtrip() {
    let odt_source = require(&samples_dir().join("test-document.odt"));
    let odt = odf::read_odt_file(&odt_source).unwrap();
    assert!(odt.document.plain_text().contains("Test document"));
    // Extend: footnotes now survive an ODT write -> read cycle.
    let mut edited = odt.document;
    edited.footnotes = vec![Footnote {
        id: "fn-rt".into(),
        runs: vec![Run { text: "Round trip note".into(), ..Default::default() }],
        marker: String::new(),
    }];
    edited.blocks.push(Block::Paragraph {
        props: Default::default(),
        runs: vec![
            Run { text: "note ref".into(), ..Default::default() },
            Run { footnote: Some("fn-rt".into()), ..Default::default() },
        ],
    });
    let odt_target = temp("roundtrip-document.odt");
    std::fs::write(&odt_target, odf::write_odt(&edited).unwrap()).unwrap();
    let odt_again = odf::read_odt_file(&odt_target).unwrap();
    assert!(odt_again.document.plain_text().contains("Test document"));
    assert_eq!(odt_again.document.footnotes.len(), 1);
    assert!(odt_again.document.footnotes[0].runs.iter().any(|run| run.text.contains("Round trip note")));
    assert_eq!(odt_again.document.footnote_order().len(), 1);
    assert!(
        !odt_again.document.plain_text().contains("Round trip note"),
        "the note body must not leak into the paragraph text"
    );

    let rtf_source = require(&samples_dir().join("test-document.rtf"));
    let rtf_read = rtf::read_rtf_file(&rtf_source).unwrap();
    assert!(rtf_read.document.plain_text().contains("Test document"));
    // Extend: RTF keeps notes and tracked insertions.
    let mut edited = rtf_read.document;
    edited.footnotes = vec![Footnote {
        id: "fn-rtf".into(),
        runs: vec![Run { text: "RTF note".into(), ..Default::default() }],
        marker: String::new(),
    }];
    edited.blocks.push(Block::Paragraph {
        props: Default::default(),
        runs: vec![
            Run {
                text: "tracked".into(),
                revision: Some(RevisionMark {
                    id: "r1".into(),
                    kind: "insert".into(),
                    author: "RoundTrip".into(),
                    date: "2026-01-01T00:00:00Z".into(),
                    original: None,
                }),
                ..Default::default()
            },
            Run { footnote: Some("fn-rtf".into()), ..Default::default() },
        ],
    });
    let rtf_target = temp("roundtrip-document.rtf");
    std::fs::write(&rtf_target, rtf::write_rtf(&edited).unwrap()).unwrap();
    let rtf_again = rtf::read_rtf_file(&rtf_target).unwrap();
    assert_eq!(rtf_again.document.footnotes.len(), 1);
    assert!(rtf_again.document.footnotes[0].runs.iter().any(|run| run.text.contains("RTF note")));
    assert_eq!(rtf_again.document.footnote_order().len(), 1);
    let revision = rtf_again.document.blocks.iter().find_map(|block| match block {
        Block::Paragraph { runs, .. } => {
            runs.iter().find_map(|run| run.revision.as_ref().filter(|revision| revision.kind == "insert").cloned())
        }
        _ => None,
    });
    assert_eq!(
        revision.map(|revision| (revision.author, revision.date)),
        Some(("RoundTrip".into(), "2026-01-01T00:00:00Z".into()))
    );
}

/// The v4.5 Writer features survive an ODT write -> read cycle: table spans,
/// tab stops and the watermark (meta entries plus the run-through frame).
#[test]
fn odt_roundtrip_keeps_spans_tabs_and_watermark() {
    use officecore::model::TableData;
    let mut document = officecore::model::TextDocument::new_blank("Writer v4.5");
    let mut table = TableData::simple(2, 2, 300.0);
    table.rows[0].cells =
        vec![TableCell { blocks: vec![Block::paragraph("A")], colspan: 2, rowspan: 2, ..Default::default() }];
    table.rows[1].cells = vec![TableCell { blocks: vec![Block::paragraph("B")], ..Default::default() }];
    document.blocks = vec![
        Block::Paragraph {
            props: ParaProps { tabs: vec![TabStop { pos_pt: 72.0, align: "decimal".into() }], ..Default::default() },
            runs: vec![Run { text: "one\ttwo".into(), ..Default::default() }],
        },
        Block::Table { table },
    ];
    document.watermark = Some(Watermark {
        text: "ROUND TRIP".into(),
        color: Some("#123456".into()),
        opacity: 0.4,
        rotation: 30.0,
        font_pt: 60.0,
        bold: true,
    });

    let target = temp("roundtrip-writer-v45.odt");
    std::fs::write(&target, odf::write_odt(&document).unwrap()).unwrap();
    let again = odf::read_odt_file(&target).unwrap();
    assert_eq!(again.document.watermark, document.watermark, "warnings: {:?}", again.warnings);
    let table = again
        .document
        .blocks
        .iter()
        .find_map(|block| match block {
            Block::Table { table } => Some(table),
            _ => None,
        })
        .expect("table missing");
    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.rows[0].cells.len(), 1);
    assert_eq!(table.rows[0].cells[0].colspan, 2);
    assert_eq!(table.rows[0].cells[0].rowspan, 2);
    assert_eq!(table.rows[1].cells.len(), 1);
    assert_eq!(table.rows[1].cells[0].blocks[0].plain_text(), "B");
    let props = again
        .document
        .blocks
        .iter()
        .find_map(|block| match block {
            Block::Paragraph { props, .. } => Some(props),
            _ => None,
        })
        .expect("paragraph missing");
    assert_eq!(props.tabs.len(), 1);
    assert_eq!(props.tabs[0].align, "decimal");
    assert!((props.tabs[0].pos_pt - 72.0).abs() < 0.01);
}

#[test]
fn xlsx_open_edit_save_reopen() {
    let source = require(&samples_dir().join("test-spreadsheet.xlsx"));
    let first = xlsx::read_workbook_file(&source).unwrap();
    assert_eq!(first.workbook.sheets.len(), 2);
    let data = &first.workbook.sheets[0];
    assert!(data.cells.len() > 300, "the sample workbook should contain 100 data rows");
    assert_eq!(
        data.get("A2").map(|cell| cell.value.clone()),
        Some(officecore::model::CellValue::Text("Item 1".into()))
    );
    let formula = data.get("D2").and_then(|cell| cell.formula.clone());
    assert_eq!(formula.as_deref(), Some("=B2*C2"));

    let mut edited = first.workbook;
    edited.sheets[0].set(
        "A1",
        officecore::model::Cell {
            value: officecore::model::CellValue::Text("Edited header".into()),
            ..Default::default()
        },
    );
    let target = temp("roundtrip-spreadsheet.xlsx");
    xlsx::write_xlsx_file(&target, &edited).unwrap();
    let second = xlsx::read_workbook_file(&target).unwrap();
    assert_eq!(second.workbook.sheets.len(), 2);
    assert_eq!(
        second.workbook.sheets[0].get("A1").map(|cell| cell.value.clone()),
        Some(officecore::model::CellValue::Text("Edited header".into()))
    );
    assert_eq!(
        second.workbook.sheets[1].get("B4").and_then(|cell| cell.formula.clone()).as_deref(),
        Some("=SUM(Data!D2:D101)")
    );
}

#[test]
fn ods_roundtrip() {
    let source = require(&samples_dir().join("test-spreadsheet.ods"));
    let first = odf::read_ods_file(&source).unwrap();
    assert_eq!(first.workbook.sheets.len(), 2);
    assert!(first.workbook.sheets[0].cells.len() > 300);
    let target = temp("roundtrip-spreadsheet.ods");
    std::fs::write(&target, odf::write_ods(&first.workbook).unwrap()).unwrap();
    let second = odf::read_ods_file(&target).unwrap();
    assert_eq!(second.workbook.sheets.len(), 2);
    assert!(second.workbook.sheets[0].plain_text_probe());
}

trait SheetProbe {
    fn plain_text_probe(&self) -> bool;
}

impl SheetProbe for officecore::model::Sheet {
    fn plain_text_probe(&self) -> bool {
        self.cells
            .values()
            .any(|cell| matches!(&cell.value, officecore::model::CellValue::Text(text) if text == "Item 1"))
    }
}

#[test]
fn pptx_open_edit_save_reopen() {
    let source = require(&samples_dir().join("test-presentation.pptx"));
    let first = pptx::read_pptx_file(&source).unwrap();
    assert_eq!(first.deck.slides.len(), 5, "the sample deck must have five slides");
    let texts: Vec<String> = first
        .deck
        .slides
        .iter()
        .flat_map(|slide| slide.objects.iter())
        .filter_map(|object| object.text.as_ref().map(|frame| frame.plain()))
        .collect();
    assert!(texts.iter().any(|text| text.contains("Slide 1 title")));
    assert!(first.deck.slides.iter().any(|slide| slide.objects.iter().any(|object| object.image.is_some())));
    assert!(first
        .deck
        .slides
        .iter()
        .any(|slide| slide.objects.iter().any(|object| object.kind == "rect" || object.kind == "ellipse")));

    let mut edited = first.deck;
    edited.slides[0].notes = "Edited notes".into();
    // Extend: a chart with cached values must keep them (and its embedded
    // workbook) through the save/reopen cycle.
    let mut chart_object = SlideObject::new("chart", 80.0, 80.0, 400.0, 240.0);
    chart_object.id = "chart-roundtrip".into();
    chart_object.chart = Some(ChartData {
        kind: "line".into(),
        title: "Round trip chart".into(),
        categories: "A2:A3".into(),
        series: vec![ChartSeries { name: "Value".into(), range: "B2:B3".into(), color: Some("#2563EB".into()) }],
        legend: true,
        x_title: String::new(),
        y_title: String::new(),
        stacked: false,
        show_labels: false,
        categories_cache: vec!["Alpha".into(), "Beta".into()],
        series_values_cache: vec![vec![1.5, 2.5]],
        ..Default::default()
    });
    edited.slides[0].objects.push(chart_object);
    let target = temp("roundtrip-presentation.pptx");
    pptx::write_pptx_file(&target, &edited).unwrap();
    let second = pptx::read_pptx_file(&target).unwrap();
    assert_eq!(second.deck.slides.len(), 5);
    assert!(second.deck.slides[0].notes.contains("Edited notes"));
    let chart = second.deck.slides[0].objects.iter().find_map(|object| object.chart.as_ref()).expect("cached chart");
    assert_eq!(chart.categories_cache, vec!["Alpha".to_string(), "Beta".to_string()]);
    assert_eq!(chart.series_values_cache, vec![vec![1.5, 2.5]]);

    let bytes = std::fs::read(&target).unwrap();
    let reader = officecore::zip::ZipReader::open(bytes).unwrap();
    let workbook_bytes = reader.read("ppt/embeddings/Microsoft_Excel_Worksheet1.xlsx").unwrap();
    let workbook = xlsx::read_workbook_bytes(&workbook_bytes).unwrap();
    assert_eq!(workbook.workbook.sheets[0].get("B2").map(|cell| cell.value.clone()), Some(CellValue::Number(1.5)));
}

fn odp_text_frames(deck: &Deck) -> Vec<TextFrame> {
    deck.slides.iter().flat_map(|slide| slide.objects.iter()).filter_map(|object| object.text.clone()).collect()
}

#[test]
fn odp_roundtrip() {
    let source = require(&samples_dir().join("test-presentation.odp"));
    let first = odf::read_odp_file(&source).unwrap();
    assert_eq!(first.deck.slides.len(), 5);
    // Text frames import with their text and runs, not just as slide counts.
    let text = odp_text_frames(&first.deck).iter().map(TextFrame::plain).collect::<Vec<_>>().join("\n");
    assert!(text.contains("Slide 1 title"), "text was {text}");
    assert!(text.contains("First point"), "text was {text}");
    assert!(first
        .deck
        .slides
        .iter()
        .flat_map(|slide| slide.objects.iter())
        .filter_map(|object| object.text.as_ref())
        .flat_map(|frame| frame.paragraphs.iter())
        .any(|paragraph| !paragraph.runs.is_empty()));
    assert!(first.deck.footer.is_none(), "the sample declares no footer");

    let target = temp("roundtrip-presentation.odp");
    std::fs::write(&target, odf::write_odp(&first.deck).unwrap()).unwrap();
    let second = odf::read_odp_file(&target).unwrap();
    assert_eq!(second.deck.slides.len(), 5);
    assert_eq!(odp_text_frames(&second.deck), odp_text_frames(&first.deck), "the round trip keeps text and runs");
    assert_eq!(second.deck.footer, first.deck.footer);
    assert_eq!(second.deck.slides[0].notes, first.deck.slides[0].notes);
}

#[test]
fn csv_sample_imports() {
    let source = require(&samples_dir().join("test-spreadsheet.csv"));
    let bytes = std::fs::read(&source).unwrap();
    let read = officecore::csvio::parse_csv(&bytes, &officecore::csvio::CsvOptions::default()).unwrap();
    assert!(read.workbook.sheets[0].cells.len() > 300);
}
