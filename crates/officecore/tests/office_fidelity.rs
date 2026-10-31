//! The round-trip fidelity corpus (D6).
//!
//! Each test builds a document in the model, writes it in its format, reads it
//! back and compares what came out with what went in. The counters are printed
//! as one line so the release report can quote a measured delta instead of a
//! promise: `cargo test -p officecore --test office_fidelity -- --nocapture`.
//!
//! The corpus is programmatic on purpose: a binary fixture would hide which
//! construct was lost, while these tests name the delta (blocks, runs, bold
//! runs, table cells, revisions, formulas) and fail on a silent loss.

use officecore::docx::{read_docx, write_docx};
use officecore::model::{
    Block, Cell, CellValue, Deck, ParaProps, RevisionMark, Run, Sheet, Slide, TableData, TextDocument, Workbook,
};
use officecore::odf::{read_odt, write_odt};
use officecore::pptx::{read_pptx, write_pptx};
use officecore::xlsx::{read_workbook_bytes, write_xlsx};
use officecore::zip::ZipWriter;

#[derive(Debug, Default, PartialEq)]
struct Fidelity {
    blocks: usize,
    paragraphs: usize,
    runs: usize,
    bold_runs: usize,
    tables: usize,
    table_cells: usize,
    revisions: usize,
    text_chars: usize,
}

/// Counts the constructs that carry content. An empty paragraph (the writer
/// adds the required one after a table) is packaging, not fidelity, so it is
/// left out; a lost run or a lost revision still changes the numbers.
fn count_text(document: &TextDocument) -> Fidelity {
    let mut fidelity = Fidelity::default();
    for block in &document.blocks {
        match block {
            Block::Paragraph { runs, .. } => {
                let content: Vec<&Run> = runs.iter().filter(|run| !run.text.is_empty()).collect();
                if content.is_empty() {
                    continue;
                }
                fidelity.blocks += 1;
                fidelity.paragraphs += 1;
                fidelity.runs += content.len();
                fidelity.bold_runs += content.iter().filter(|run| run.bold).count();
                fidelity.revisions += content.iter().filter(|run| run.revision.is_some()).count();
                fidelity.text_chars += content.iter().map(|run| run.text.chars().count()).sum::<usize>();
            }
            Block::Table { table } => {
                fidelity.blocks += 1;
                fidelity.tables += 1;
                fidelity.table_cells += table.rows.iter().map(|row| row.cells.len()).sum::<usize>();
                for row in &table.rows {
                    for cell in &row.cells {
                        for block in &cell.blocks {
                            if let Block::Paragraph { runs, .. } = block {
                                let content: Vec<&Run> = runs.iter().filter(|run| !run.text.is_empty()).collect();
                                fidelity.paragraphs += content.len().min(1);
                                fidelity.runs += content.len();
                                fidelity.text_chars +=
                                    content.iter().map(|run| run.text.chars().count()).sum::<usize>();
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    fidelity
}

fn revision(kind: &str, author: &str) -> RevisionMark {
    RevisionMark {
        id: format!("{kind}-1"),
        kind: kind.into(),
        author: author.into(),
        date: "2026-10-31T09:00:00Z".into(),
        original: None,
    }
}

fn sample_document() -> TextDocument {
    let mut document = TextDocument::new_blank("Fidelity");
    document.track_changes = true;
    let mut table = TableData::simple(2, 2, 450.0);
    table.rows[0].cells[0].blocks = vec![Block::paragraph("Name")];
    table.rows[0].cells[1].blocks = vec![Block::paragraph("Value")];
    table.rows[1].cells[0].blocks = vec![Block::paragraph("Total")];
    table.rows[1].cells[1].blocks = vec![Block::paragraph("42")];
    document.blocks = vec![
        Block::Paragraph {
            props: ParaProps::default(),
            runs: vec![
                Run { text: "Kept text. ".into(), ..Default::default() },
                Run { text: "Bold".into(), bold: true, ..Default::default() },
                Run { text: " inserted".into(), revision: Some(revision("insert", "Ada")), ..Default::default() },
                Run { text: " removed".into(), revision: Some(revision("delete", "Grace")), ..Default::default() },
            ],
        },
        Block::Table { table },
    ];
    document
}

fn fidelity_line(format: &str, before: &Fidelity, after: &Fidelity) -> String {
    format!(
        "{format:<6} blocks {}/{}  paragraphs {}/{}  runs {}/{}  bold {}/{}  tables {}/{}  cells {}/{}  revisions {}/{}  chars {}/{}",
        after.blocks,
        before.blocks,
        after.paragraphs,
        before.paragraphs,
        after.runs,
        before.runs,
        after.bold_runs,
        before.bold_runs,
        after.tables,
        before.tables,
        after.table_cells,
        before.table_cells,
        after.revisions,
        before.revisions,
        after.text_chars,
        before.text_chars
    )
}

#[test]
fn docx_round_trip_preserves_every_supported_construct() {
    let document = sample_document();
    let before = count_text(&document);
    let bytes = write_docx(&document).expect("write DOCX");
    let read = read_docx(&bytes).expect("read DOCX");
    let after = count_text(&read.document);
    println!("{}", fidelity_line("DOCX", &before, &after));
    assert!(read.warnings.is_empty(), "warnings: {:?}", read.warnings);
    assert_eq!(after, before, "a DOCX round trip must not lose a construct");
    assert!(read.document.track_changes, "tracked-changes mode survives");
}

#[test]
fn odt_round_trip_preserves_text_styles_and_tables() {
    let mut document = sample_document();
    // ODF tracked changes are a separate importer path; the corpus measures
    // text, styles and tables here and reports the revision delta separately.
    document.blocks[0] = Block::Paragraph {
        props: ParaProps::default(),
        runs: vec![
            Run { text: "Kept text. ".into(), ..Default::default() },
            Run { text: "Bold".into(), bold: true, ..Default::default() },
        ],
    };
    let before = count_text(&document);
    let bytes = write_odt(&document).expect("write ODT");
    let read = read_odt(&bytes).expect("read ODT");
    let after = count_text(&read.document);
    println!("{}", fidelity_line("ODT", &before, &after));
    assert_eq!(after.blocks, before.blocks);
    assert_eq!(after.paragraphs, before.paragraphs);
    assert_eq!(after.runs, before.runs);
    assert_eq!(after.bold_runs, before.bold_runs);
    assert_eq!(after.tables, before.tables);
    assert_eq!(after.table_cells, before.table_cells);
    assert_eq!(after.text_chars, before.text_chars);
}

fn sample_workbook() -> Workbook {
    let mut workbook = Workbook::new_blank("Fidelity");
    workbook.sheets.clear();
    let mut sheet = Sheet::new("Sheet1");
    sheet.set("A1", Cell { value: CellValue::Number(2.0), ..Default::default() });
    sheet.set("B1", Cell { value: CellValue::Number(4.0), formula: Some("=A1*2".into()), ..Default::default() });
    sheet.set("C1", Cell { value: CellValue::Text("Toplam".into()), ..Default::default() });
    workbook.sheets.push(sheet);
    workbook
}

fn count_workbook(workbook: &Workbook) -> (usize, usize, usize) {
    let mut cells = 0;
    let mut formulas = 0;
    let mut text = 0;
    for sheet in &workbook.sheets {
        for address in ["A1", "B1", "C1"] {
            if let Some(cell) = sheet.get(address) {
                cells += 1;
                if cell.formula.is_some() {
                    formulas += 1;
                }
                if matches!(cell.value, CellValue::Text(_)) {
                    text += 1;
                }
            }
        }
    }
    (cells, formulas, text)
}

#[test]
fn xlsx_round_trip_preserves_values_and_formulas() {
    let workbook = sample_workbook();
    let before = count_workbook(&workbook);
    let bytes = write_xlsx(&workbook).expect("write XLSX");
    let read = read_workbook_bytes(&bytes).expect("read XLSX");
    let after = count_workbook(&read.workbook);
    println!(
        "XLSX   cells {}/{}  formulas {}/{}  text cells {}/{}  warnings {}",
        after.0,
        before.0,
        after.1,
        before.1,
        after.2,
        before.2,
        read.warnings.len()
    );
    assert_eq!(after, before, "values and formulas must survive the round trip");
    assert_eq!(read.workbook.sheets[0].get("B1").and_then(|cell| cell.formula.clone()), Some("=A1*2".into()));
}

fn shared_formula_workbook() -> Vec<u8> {
    let mut zip = ZipWriter::new();
    zip.add_text(
        "[Content_Types].xml",
        r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#,
    );
    zip.add_text(
        "_rels/.rels",
        r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#,
    );
    zip.add_text(
        "xl/workbook.xml",
        r#"<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
    );
    zip.add_text(
        "xl/_rels/workbook.xml.rels",
        r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#,
    );
    zip.add_text(
        "xl/worksheets/sheet1.xml",
        r#"<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f t="shared" si="0" ref="B1:B3">A1*2</f><v>2</v></c></row><row r="2"><c r="A2"><v>2</v></c><c r="B2"><f t="shared" si="0"/><v>4</v></c></row><row r="3"><c r="A3"><v>3</v></c><c r="B3"><f t="shared" si="0"/><v>6</v></c></row></sheetData></worksheet>"#,
    );
    zip.finish()
}

#[test]
fn shared_formulas_are_expanded_on_import() {
    let read = read_workbook_bytes(&shared_formula_workbook()).expect("read shared-formula workbook");
    let sheet = &read.workbook.sheets[0];
    assert_eq!(sheet.get("B1").and_then(|cell| cell.formula.clone()), Some("=A1*2".into()));
    let follower = sheet.get("B2").and_then(|cell| cell.formula.clone());
    assert_eq!(follower, Some("=A2*2".into()), "the item below the base formula must be filled down");
    assert_eq!(
        sheet.get("B3").and_then(|cell| cell.formula.clone()),
        Some("=A3*2".into()),
        "the second follower must be shifted by two rows"
    );
    println!("XLSX   shared formulas expanded: 2 followers (B2, B3)");
}

#[test]
fn pptx_transition_and_duration_round_trip() {
    let mut deck = Deck::new_blank("Fidelity");
    deck.slides.clear();
    deck.slides.push(Slide {
        id: "slide-1".into(),
        transition: Some("fade".into()),
        transition_ms: 800,
        ..Default::default()
    });
    let bytes = write_pptx(&deck).expect("write PPTX");
    let read = read_pptx(&bytes).expect("read PPTX");
    let slide = &read.deck.slides[0];
    println!(
        "PPTX   transition {}/{}  duration {}/{} ms  warnings {}",
        slide.transition.as_deref().unwrap_or("-"),
        "fade",
        slide.transition_ms,
        800,
        read.warnings.len()
    );
    assert_eq!(slide.transition.as_deref(), Some("fade"));
    assert_eq!(slide.transition_ms, 800);
}
