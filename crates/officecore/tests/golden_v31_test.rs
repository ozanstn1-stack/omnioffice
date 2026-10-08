//! V3.1 cross-platform golden tests.
//!
//! These tests pin the V3.1 feature contract that every build of the engine
//! (Windows, Linux, Android) must satisfy for the native `.oswk` unit format
//! and for the three interchange formats:
//!
//! * Writer: sections, footnotes + endnotes, tracked insert/delete/format
//!   revisions, comments with replies, bookmarks and fields, a table and an
//!   image.
//! * Calc: a structured table with a calculated column and structured
//!   references, a chart with cached values, a picture, print settings
//!   (print area / titles / breaks), sheet protection and a preserved pivot.
//! * Impress: slide masters with layouts, a nested group, a chart with
//!   caches, animations and speaker notes.
//!
//! Comparison levels (explicit, because they differ per format):
//! * `.oswk` reload and `.oswk` save -> reload are compared as **parsed JSON
//!   values** (deep structural equality). Byte equality is not asserted for
//!   the app-generated envelope because the app stamps `savedAt` with the
//!   current time; the fixture files generated here use a fixed timestamp and
//!   are byte-reproducible, which `committed_fixtures_are_reproducible`
//!   enforces.
//! * DOCX/XLSX/PPTX round trips are compared **feature by feature** - only the
//!   parts that format can represent. DOCX has no bookmark/reply-resolved
//!   storage in this model, XLSX flattens live pivots into preserved raw
//!   parts, PPTX regenerates slide/object ids; the tests below assert exactly
//!   what the format guarantees instead of pretending a full model round trip.
//!
//! The `.oswk` fixtures in `tests/fixtures/` are committed bytes. The Android
//! build uses the same `officecore` engine, so this fixture + summary pair is
//! what makes the cross-platform claim testable in CI; running the app on a
//! real Android device is still a manual step and cannot be replaced by this
//! file.

use base64::Engine as _;
use officecore::model::*;
use officecore::{docx, pptx, revisions, schema, xlsx};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Deterministic assets and the `.oswk` envelope
// ---------------------------------------------------------------------------

/// A real 1x1 transparent PNG (stored once as base64 so the fixture model is
/// small, deterministic and still decodable by the `image` crate).
const PIXEL_PNG_BASE64: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAACklEQVR4nGMAAQAABQABDQottAAAAABJRU5ErkJggg==";

fn pixel_png() -> ImageData {
    let bytes = base64::engine::general_purpose::STANDARD.decode(PIXEL_PNG_BASE64).unwrap();
    ImageData::from_bytes("pixel.png", &bytes)
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures")
}

/// Builds the exact envelope `src-tauri/src/office.rs::save_native` writes:
/// `format`, both version keys (`version` for older readers, `schemaVersion`
/// for V3+), `kind`, `title`, `savedAt`, `warnings` and the model. The only
/// difference is the fixed timestamp, which makes the golden fixture bytes
/// reproducible; the app stamps "now".
fn unit_envelope(kind: &str, title: &str, model: &Value) -> Value {
    json!({
        "format": "office-swiss-army-knife",
        "version": schema::SCHEMA_VERSION,
        "schemaVersion": schema::SCHEMA_VERSION,
        "kind": kind,
        "title": title,
        "savedAt": "2026-01-01T00:00:00Z",
        "warnings": [],
        "model": model,
    })
}

/// Replicates the app's `.oswk` open path (parse -> `migrate_unit` -> model).
fn load_oswk(bytes: &[u8]) -> Value {
    let mut raw: Value = serde_json::from_slice(bytes).expect("the unit file is valid JSON");
    let report = schema::migrate_unit(&mut raw).expect("the unit migrates");
    assert!(!report.migrated, "a golden V3.1 unit is already at the current schema");
    raw
}

fn writer_unit() -> Value {
    let model = serde_json::to_value(v31_writer()).unwrap();
    unit_envelope("writer", "V3.1 Golden Writer", &model)
}

fn calc_unit() -> Value {
    let model = serde_json::to_value(v31_workbook()).unwrap();
    unit_envelope("calc", "V3.1 Golden Calc", &model)
}

fn impress_unit() -> Value {
    let model = serde_json::to_value(v31_deck()).unwrap();
    unit_envelope("impress", "V3.1 Golden Impress", &model)
}

fn golden_units() -> Vec<(&'static str, Value)> {
    vec![("writer-v31.oswk", writer_unit()), ("calc-v31.oswk", calc_unit()), ("impress-v31.oswk", impress_unit())]
}

/// Returns the fixture path, generating the bytes only when the file is
/// missing. Once the fixtures are committed this never writes during a normal
/// test run; `OSAK_REGEN_GOLDEN=1` is the explicit escape hatch used when the
/// model legitimately grows a new field.
fn ensure_fixture(name: &str, unit: &Value) -> PathBuf {
    let path = fixtures_dir().join(name);
    if !path.exists() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, serde_json::to_vec_pretty(unit).unwrap()).unwrap();
    }
    path
}

// ---------------------------------------------------------------------------
// Writer: a document exercising every V3.1 Writer feature
// ---------------------------------------------------------------------------

fn v31_writer() -> TextDocument {
    let mut document = TextDocument::new_blank("V3.1 Golden Writer");
    document.id = "writer-v31".into();
    document.page = PageSetup::from_preset("a4", "portrait");
    document.metadata = DocMetadata {
        title: "V3.1 Golden Writer".into(),
        author: "QA".into(),
        subject: "golden fixture".into(),
        keywords: "v3.1,golden,writer".into(),
        creator: "officecore".into(),
        last_modified_by: "QA".into(),
        created: "2026-01-01T00:00:00Z".into(),
        modified: "2026-01-02T00:00:00Z".into(),
    };
    document.header = vec![Block::paragraph("Golden header")];
    document.footer = vec![Block::paragraph("Golden footer")];
    document.track_changes = true;
    document.show_revisions = true;

    document.footnotes = vec![Footnote {
        id: "fn-1".into(),
        runs: vec![Run { text: "Footnote body".into(), italic: true, ..Default::default() }],
        marker: String::new(),
    }];
    document.endnotes = vec![Footnote {
        id: "en-1".into(),
        runs: vec![Run { text: "Endnote body".into(), ..Default::default() }],
        marker: String::new(),
    }];
    document.bookmarks = vec![Bookmark { id: "bookmark-1".into(), name: "GoldenTarget".into(), block: 2, offset: 0 }];
    document.comments = vec![Comment {
        id: "comment-1".into(),
        author: "Ada".into(),
        text: "Please verify the totals".into(),
        created: "2026-01-02T09:00:00Z".into(),
        resolved: false,
        modified: "2026-01-02T11:00:00Z".into(),
        replies: vec![CommentReply {
            author: "Grace".into(),
            text: "Verified".into(),
            created: "2026-01-02T10:00:00Z".into(),
        }],
    }];

    let insert = RevisionMark {
        id: "rev-insert".into(),
        kind: "insert".into(),
        author: "Ada".into(),
        date: "2026-01-01T10:00:00Z".into(),
        original: None,
    };
    let delete = RevisionMark {
        id: "rev-delete".into(),
        kind: "delete".into(),
        author: "Grace".into(),
        date: "2026-01-01T11:00:00Z".into(),
        original: None,
    };
    let format = RevisionMark {
        id: "rev-format".into(),
        kind: "format".into(),
        author: "Ada".into(),
        date: "2026-01-01T12:00:00Z".into(),
        original: Some(Run::default().format_snapshot()),
    };

    let mut table = TableData::simple(2, 2, 320.0);
    table.rows[0].cells[0].blocks = vec![Block::paragraph("Key")];
    table.rows[0].cells[1].blocks = vec![Block::paragraph("Value")];
    table.rows[1].cells[0].blocks = vec![Block::paragraph("Answer")];
    table.rows[1].cells[1].blocks = vec![Block::paragraph("42")];

    document.blocks = vec![
        Block::heading("V3.1 golden contract", 1),
        Block::Paragraph {
            props: ParaProps::default(),
            runs: vec![
                Run { text: "Intro ".into(), ..Default::default() },
                Run { text: "inserted".into(), revision: Some(insert), ..Default::default() },
                Run { text: "removed".into(), revision: Some(delete), ..Default::default() },
                Run {
                    text: "reformatted".into(),
                    bold: true,
                    italic: true,
                    revision: Some(format),
                    ..Default::default()
                },
                Run { text: "commented".into(), comment: Some("comment-1".into()), ..Default::default() },
                Run { footnote: Some("fn-1".into()), ..Default::default() },
                Run { endnote: Some("en-1".into()), ..Default::default() },
                Run { text: "See page ".into(), ..Default::default() },
                Run {
                    text: "2".into(),
                    field: Some(FieldRef { kind: "refPage".into(), target: "GoldenTarget".into(), cached: "2".into() }),
                    ..Default::default()
                },
                Run { text: " on ".into(), ..Default::default() },
                Run {
                    text: "2026-01-01".into(),
                    field: Some(FieldRef { kind: "date".into(), target: String::new(), cached: "2026-01-01".into() }),
                    ..Default::default()
                },
            ],
        },
        Block::paragraph("Bookmarked paragraph"),
        Block::SectionBreak {
            section: SectionProps {
                page: PageSetup::from_preset("a4", "landscape"),
                header: vec![Block::paragraph("Landscape header")],
                footer: vec![Block::paragraph("Landscape footer")],
                first_header: vec![Block::paragraph("First landscape header")],
                even_header: vec![Block::paragraph("Even landscape header")],
                different_first_page: true,
                different_odd_even: true,
                columns: 2,
                start: "oddPage".into(),
                ..Default::default()
            },
        },
        Block::paragraph("Section two content"),
        Block::Table { table },
        Block::Image {
            image: pixel_png(),
            width_pt: 96.0,
            height_pt: 96.0,
            align: "center".into(),
            caption: "Golden pixel".into(),
        },
    ];
    document
}

// ---------------------------------------------------------------------------
// Calc: a workbook exercising every V3.1 Calc feature
// ---------------------------------------------------------------------------

/// Raw pivot parts are stored exactly as Excel wrote them; the fixture keeps
/// them small but structurally real.
const PIVOT_DEFINITION: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<pivotCacheDefinition xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" refreshOnLoad=\"1\" recordCount=\"3\"><cacheSource type=\"worksheet\"><worksheetSource ref=\"A1:D4\" sheet=\"Data\"/></cacheSource><cacheFields count=\"4\"><cacheField name=\"Item\"/><cacheField name=\"Units\"/><cacheField name=\"Price\"/><cacheField name=\"Amount\"/></cacheFields></pivotCacheDefinition>";
const PIVOT_TABLE: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<pivotTableDefinition xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" name=\"PivotTable1\" cacheId=\"7\" dataCaption=\"Values\" updatedVersion=\"6\"><location ref=\"H1:I4\" firstHeaderRow=\"1\" firstDataRow=\"1\" firstDataCol=\"1\"/><pivotFields count=\"4\"><pivotField axis=\"axisRow\" showAll=\"0\"/><pivotField showAll=\"0\"/><pivotField showAll=\"0\"/><pivotField dataField=\"1\" showAll=\"0\"/></pivotFields><rowFields count=\"1\"><field x=\"0\"/></rowFields><dataFields count=\"1\"><dataField name=\"Sum of Amount\" fld=\"3\"/></dataFields></pivotTableDefinition>";
const PIVOT_RECORDS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<pivotCacheRecords xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" count=\"3\"><r><s v=\"Widget\"/><n v=\"2\"/><n v=\"10\"/><n v=\"20\"/></r><r><s v=\"Gadget\"/><n v=\"3\"/><n v=\"20\"/><n v=\"60\"/></r><r><s v=\"Gizmo\"/><n v=\"4\"/><n v=\"30\"/><n v=\"120\"/></r></pivotCacheRecords>";

fn text_cell(text: &str) -> Cell {
    Cell { value: CellValue::Text(text.into()), ..Default::default() }
}

fn number_cell(value: f64) -> Cell {
    Cell { value: CellValue::Number(value), ..Default::default() }
}

fn v31_workbook() -> Workbook {
    let mut workbook = Workbook::new_blank("V3.1 Golden Calc");
    workbook.id = "calc-v31".into();
    workbook.metadata = DocMetadata {
        title: "V3.1 Golden Calc".into(),
        author: "QA".into(),
        created: "2026-01-01T00:00:00Z".into(),
        modified: "2026-01-02T00:00:00Z".into(),
        ..Default::default()
    };

    let sheet = &mut workbook.sheets[0];
    sheet.id = "sheet-data".into();
    sheet.name = "Data".into();

    // Structured table "Sales" over A1:D4 with a calculated Amount column.
    sheet.set("A1", text_cell("Item"));
    sheet.set("B1", text_cell("Units"));
    sheet.set("C1", text_cell("Price"));
    sheet.set("D1", text_cell("Amount"));
    let rows = [("Widget", 2.0, 10.0, 20.0), ("Gadget", 3.0, 20.0, 60.0), ("Gizmo", 4.0, 30.0, 120.0)];
    for (index, (item, units, price, amount)) in rows.iter().enumerate() {
        let row = index + 2;
        sheet.set(&format!("A{row}"), text_cell(item));
        sheet.set(&format!("B{row}"), number_cell(*units));
        sheet.set(&format!("C{row}"), number_cell(*price));
        sheet.set(
            &format!("D{row}"),
            Cell {
                value: CellValue::Number(*amount),
                formula: Some("=[@Units]*[@Price]".into()),
                ..Default::default()
            },
        );
    }
    // A structured reference outside the table body.
    sheet.set(
        "F1",
        Cell { value: CellValue::Number(200.0), formula: Some("=SUM(Sales[Amount])".into()), ..Default::default() },
    );
    sheet.tables.push(SpreadsheetTable {
        id: "table-sales".into(),
        name: "Sales".into(),
        range: "A1:D4".into(),
        has_headers: true,
        has_totals: false,
        banded_rows: true,
        banded_columns: false,
        header_fill: Some("#1D4ED8".into()),
        header_bold: true,
        style_name: "TableStyleMedium2".into(),
        columns: vec![
            TableColumn { name: "Item".into(), formula: None },
            TableColumn { name: "Units".into(), formula: None },
            TableColumn { name: "Price".into(), formula: None },
            TableColumn { name: "Amount".into(), formula: Some("=[@Units]*[@Price]".into()) },
        ],
        filter: Some(FilterState { range: "A1:D4".into(), column: 0, values: vec!["Widget".into()] }),
    });

    // Chart with caches (the importer must restore the cached values).
    sheet.charts.push(ChartPlacement {
        id: "chart-1".into(),
        chart: ChartData {
            kind: "column".into(),
            title: "Amount by item".into(),
            categories: "A2:A4".into(),
            series: vec![ChartSeries { name: "Amount".into(), range: "D2:D4".into(), color: Some("#1D4ED8".into()) }],
            legend: true,
            x_title: "Item".into(),
            y_title: "Amount".into(),
            stacked: false,
            show_labels: true,
            categories_cache: vec!["Widget".into(), "Gadget".into(), "Gizmo".into()],
            series_values_cache: vec![vec![20.0, 60.0, 120.0]],
            ..Default::default()
        },
        anchor: "F3".into(),
        width_px: 420.0,
        height_px: 260.0,
    });

    // Floating picture with a two-cell anchor corner set.
    sheet.images.push(SheetImage {
        image: pixel_png(),
        anchor: CellAnchor { address: "B8".into(), col_off_emu: 9_525, row_off_emu: 19_050, ..Default::default() },
        width_px: 120.0,
        height_px: 60.0,
        rotation_deg: 15.0,
    });

    // Print layout: area, repeated titles, manual breaks and margins.
    sheet.print = PrintSettings {
        paper_size: 9,
        landscape: false,
        scale: 85,
        fit_to_width: 1,
        fit_to_height: 0,
        center_horizontally: true,
        center_vertically: false,
        print_gridlines: true,
        print_headings: true,
        print_titles_rows: Some("1:1".into()),
        print_titles_cols: Some("A:A".into()),
        print_area: Some("A1:F20".into()),
        different_first_page: false,
        different_odd_even: false,
        header: "Sales report".into(),
        footer: "Page &P".into(),
        margin_left: 0.5,
        margin_right: 0.5,
        margin_top: 0.6,
        margin_bottom: 0.6,
        margin_header: 0.25,
        margin_footer: 0.25,
        first_header: String::new(),
        first_footer: String::new(),
        even_header: String::new(),
        even_footer: String::new(),
        row_breaks: vec![4],
        col_breaks: vec![3],
    };

    // Sheet protection exactly as Excel stores it.
    sheet.protection = SheetProtection {
        enabled: true,
        password_hash: Some("ABCD".into()),
        algorithm_name: "SHA-512".into(),
        hash_value: "aGFzaA==".into(),
        salt_value: "c2FsdA==".into(),
        spin_count: 100_000,
        options: vec!["formatCells".into(), "objects".into()],
    };

    // A live Excel pivot kept as raw parts so a re-export does not flatten it.
    workbook.preserved_pivots.push(PreservedPivot {
        name: "PivotTable1".into(),
        sheet: "Data".into(),
        cache_id: 7,
        definition_xml: PIVOT_DEFINITION.into(),
        records_base64: Some(base64::engine::general_purpose::STANDARD.encode(PIVOT_RECORDS.as_bytes())),
        table_xml: PIVOT_TABLE.into(),
        records_part: Some("xl/pivotCache/pivotCacheRecords1.xml".into()),
        source: "Data!A1:D4".into(),
        fields: vec!["Item".into(), "Units".into(), "Price".into(), "Amount".into()],
    });

    workbook.names.push(NamedRange {
        name: "VAT_RATE".into(),
        definition: "0.2".into(),
        sheet: None,
        comment: String::new(),
    });
    workbook
}

// ---------------------------------------------------------------------------
// Impress: a deck exercising every V3.1 Impress feature
// ---------------------------------------------------------------------------

fn placeholder_shape(id: &str, role: &str, text: &str, x: f64, y: f64, w: f64, h: f64) -> SlideObject {
    let mut object = SlideObject::new("text", x, y, w, h);
    object.id = id.into();
    object.placeholder = Some(role.into());
    object.text = Some(TextFrame {
        paragraphs: vec![TextParagraph { text: text.into(), ..Default::default() }],
        ..Default::default()
    });
    object
}

fn layout_object(id: &str, kind: &str, name: &str) -> SlideLayout {
    SlideLayout {
        id: id.into(),
        name: name.into(),
        kind: kind.into(),
        objects: vec![
            placeholder_shape(&format!("{id}-title"), "title", name, 60.0, 40.0, 840.0, 80.0),
            placeholder_shape(&format!("{id}-body"), "body", "Body", 60.0, 140.0, 840.0, 340.0),
        ],
    }
}

fn group_object(id: &str, kind: &str, x: f64, y: f64, w: f64, h: f64, z: i32) -> SlideObject {
    let mut object = SlideObject::new(kind, x, y, w, h);
    object.id = id.into();
    object.z = z;
    object
}

fn v31_deck() -> Deck {
    let mut deck = Deck::new_blank("V3.1 Golden Impress");
    deck.id = "deck-v31".into();
    deck.metadata = DocMetadata {
        title: "V3.1 Golden Impress".into(),
        author: "QA".into(),
        created: "2026-01-01T00:00:00Z".into(),
        modified: "2026-01-02T00:00:00Z".into(),
        ..Default::default()
    };
    deck.masters = vec![
        SlideMaster {
            id: "master-a".into(),
            name: "Master A".into(),
            theme: "business".into(),
            background: Some("#F8FAFC".into()),
            objects: vec![placeholder_shape("master-a-footer", "footer", "Confidential", 300.0, 496.0, 360.0, 28.0)],
            layouts: vec![
                layout_object("layout-a-title", "title", "Title Slide"),
                layout_object("layout-a-content", "titleContent", "Content"),
            ],
        },
        SlideMaster {
            id: "master-b".into(),
            name: "Master B".into(),
            theme: "modern".into(),
            background: Some("#0F172A".into()),
            objects: Vec::new(),
            layouts: vec![layout_object("layout-b-title", "title", "Title B")],
        },
    ];

    // A group inside a group: coordinates stay absolute, membership is nested.
    let mut inner = group_object("group-inner", "group", 280.0, 160.0, 180.0, 120.0, 2);
    inner.children = vec![
        group_object("leaf-ellipse", "ellipse", 300.0, 180.0, 60.0, 60.0, 1),
        group_object("leaf-rect", "roundRect", 340.0, 200.0, 80.0, 70.0, 2),
    ];
    let mut outer = group_object("group-outer", "group", 100.0, 120.0, 400.0, 300.0, 1);
    outer.children = vec![
        group_object("leaf-first", "rect", 120.0, 140.0, 100.0, 80.0, 1),
        inner,
        group_object("leaf-last", "rect", 140.0, 240.0, 80.0, 60.0, 3),
    ];

    let mut chart = SlideObject::new("chart", 500.0, 120.0, 400.0, 280.0);
    chart.id = "chart-1".into();
    chart.z = 3;
    chart.chart = Some(ChartData {
        kind: "column".into(),
        title: "Quarterly units".into(),
        categories: "A2:A4".into(),
        series: vec![ChartSeries { name: "North".into(), range: "B2:B4".into(), color: Some("#1D4ED8".into()) }],
        legend: true,
        x_title: "Quarter".into(),
        y_title: "Units".into(),
        stacked: false,
        show_labels: true,
        categories_cache: vec!["Q1".into(), "Q2".into(), "Q3".into()],
        series_values_cache: vec![vec![10.0, 20.5, 31.0]],
        ..Default::default()
    });

    let mut image = SlideObject::new("image", 60.0, 440.0, 64.0, 64.0);
    image.id = "slide-image".into();
    image.image = Some(pixel_png());

    let mut title = placeholder_shape("slide-title", "title", "V3.1 slide", 60.0, 40.0, 840.0, 80.0);
    title.z = 0;

    let mut first = Slide {
        id: "slide-1".into(),
        layout: "titleContent".into(),
        master_id: Some("master-a".into()),
        layout_id: Some("layout-a-title".into()),
        objects: vec![title, outer, chart, image],
        ..Default::default()
    };
    first.animations = vec![
        Animation {
            id: "anim-1".into(),
            object_id: "group-outer".into(),
            kind: "entrance".into(),
            effect: "fade".into(),
            trigger: "onClick".into(),
            duration_ms: 500,
            delay_ms: 0,
            order: 0,
        },
        Animation {
            id: "anim-2".into(),
            object_id: "chart-1".into(),
            kind: "emphasis".into(),
            effect: "pulse".into(),
            trigger: "withPrevious".into(),
            duration_ms: 800,
            delay_ms: 200,
            order: 1,
        },
        Animation {
            id: "anim-3".into(),
            object_id: "slide-image".into(),
            kind: "exit".into(),
            effect: "fade".into(),
            trigger: "afterPrevious".into(),
            duration_ms: 400,
            delay_ms: 100,
            order: 2,
        },
    ];
    first.notes = "Golden speaker notes: mention the chart cache.".into();

    let mut second = Slide {
        id: "slide-2".into(),
        master_id: Some("master-b".into()),
        layout_id: Some("layout-b-title".into()),
        objects: vec![placeholder_shape("slide-2-title", "title", "Second slide", 60.0, 40.0, 840.0, 80.0)],
        ..Default::default()
    };
    second.transition = Some("fade".into());
    second.transition_ms = 300;

    deck.slides = vec![first, second];
    deck
}

// ---------------------------------------------------------------------------
// Native `.oswk` round trips: parsed-value equality
// ---------------------------------------------------------------------------

#[test]
fn golden_writer_oswk_save_reload_is_lossless_and_stable() {
    let original = v31_writer();
    let original_value = serde_json::to_value(&original).unwrap();
    let unit = unit_envelope("writer", &original.title, &original_value);

    let reloaded_raw = load_oswk(&serde_json::to_vec_pretty(&unit).unwrap());
    assert_eq!(reloaded_raw["schemaVersion"], json!(3));
    assert_eq!(reloaded_raw["version"], json!(3));
    assert_eq!(reloaded_raw["format"], json!("office-swiss-army-knife"));
    let reloaded: TextDocument = serde_json::from_value(reloaded_raw["model"].clone()).unwrap();
    assert_eq!(serde_json::to_value(&reloaded).unwrap(), original_value, "the native unit must keep every model field");

    // Save the reloaded model again and reload: values must not drift.
    let second = unit_envelope("writer", &reloaded.title, &serde_json::to_value(&reloaded).unwrap());
    let reloaded_again = load_oswk(&serde_json::to_vec_pretty(&second).unwrap());
    assert_eq!(reloaded_again["model"], reloaded_raw["model"], "a second save/reload must be structurally stable");
}

#[test]
fn golden_calc_oswk_save_reload_is_lossless_and_stable() {
    let original = v31_workbook();
    let original_value = serde_json::to_value(&original).unwrap();
    let unit = unit_envelope("calc", &original.title, &original_value);

    let reloaded_raw = load_oswk(&serde_json::to_vec_pretty(&unit).unwrap());
    assert_eq!(reloaded_raw["kind"], json!("calc"));
    let reloaded: Workbook = serde_json::from_value(reloaded_raw["model"].clone()).unwrap();
    assert_eq!(serde_json::to_value(&reloaded).unwrap(), original_value);

    // Spot-check the V3.1 containers so a regression names the feature.
    let sheet = &reloaded.sheets[0];
    assert_eq!(sheet.tables.len(), 1);
    assert_eq!(sheet.tables[0].columns[3].formula.as_deref(), Some("=[@Units]*[@Price]"));
    assert_eq!(sheet.charts[0].chart.series_values_cache, vec![vec![20.0, 60.0, 120.0]]);
    assert_eq!(sheet.print.print_area.as_deref(), Some("A1:F20"));
    assert!(sheet.protection.enabled);
    assert_eq!(reloaded.preserved_pivots.len(), 1);

    let second = unit_envelope("calc", &reloaded.title, &serde_json::to_value(&reloaded).unwrap());
    let reloaded_again = load_oswk(&serde_json::to_vec_pretty(&second).unwrap());
    assert_eq!(reloaded_again["model"], reloaded_raw["model"]);
}

#[test]
fn golden_impress_oswk_save_reload_is_lossless_and_stable() {
    let original = v31_deck();
    let original_value = serde_json::to_value(&original).unwrap();
    let unit = unit_envelope("impress", &original.title, &original_value);

    let reloaded_raw = load_oswk(&serde_json::to_vec_pretty(&unit).unwrap());
    assert_eq!(reloaded_raw["kind"], json!("impress"));
    let reloaded: Deck = serde_json::from_value(reloaded_raw["model"].clone()).unwrap();
    assert_eq!(serde_json::to_value(&reloaded).unwrap(), original_value);

    let second = unit_envelope("impress", &reloaded.title, &serde_json::to_value(&reloaded).unwrap());
    let reloaded_again = load_oswk(&serde_json::to_vec_pretty(&second).unwrap());
    assert_eq!(reloaded_again["model"], reloaded_raw["model"]);
}

// ---------------------------------------------------------------------------
// Interchange round trips: feature survival per format
// ---------------------------------------------------------------------------

#[test]
fn golden_writer_docx_roundtrip_keeps_the_v31_features_docx_can_represent() {
    let original = v31_writer();
    let bytes = docx::write_docx(&original).unwrap();
    let read = docx::read_docx(&bytes).unwrap();
    let document = read.document;

    // Sections: two, the second landscape with repeated columns and an odd
    // page start; every header/footer slot survives.
    assert_eq!(document.all_sections().len(), 2, "warnings: {:?}", read.warnings);
    assert_eq!(document.page.orientation, "portrait");
    assert!(document.header.iter().map(Block::plain_text).any(|text| text.contains("Golden header")));
    assert!(document.footer.iter().map(Block::plain_text).any(|text| text.contains("Golden footer")));
    let section = document
        .blocks
        .iter()
        .find_map(|block| match block {
            Block::SectionBreak { section } => Some(section.clone()),
            _ => None,
        })
        .expect("the section break must survive");
    assert_eq!(section.page.orientation, "landscape");
    assert_eq!(section.page.size, "a4");
    assert_eq!(section.start, "oddPage");
    assert_eq!(section.columns, 2);
    assert!(section.different_first_page);
    assert!(section.different_odd_even);
    for (slot, needle) in [
        (&section.header, "Landscape header"),
        (&section.footer, "Landscape footer"),
        (&section.first_header, "First landscape header"),
        (&section.even_header, "Even landscape header"),
    ] {
        assert!(
            slot.iter().map(Block::plain_text).any(|text| text.contains(needle)),
            "missing header/footer slot {needle}"
        );
    }

    // Footnotes and endnotes, by reference order.
    assert_eq!(document.footnotes.len(), 1);
    assert_eq!(document.endnotes.len(), 1);
    assert_eq!(document.footnote_order().len(), 1);
    assert_eq!(document.endnote_order().len(), 1);
    assert!(document.footnotes[0].runs.iter().any(|run| run.text.contains("Footnote body")));
    assert!(document.endnotes[0].runs.iter().any(|run| run.text.contains("Endnote body")));

    // Tracked revisions: insert, delete and format all come back with kind and
    // author; the format revision keeps its captured original formatting.
    let list = revisions::revision_list(&document);
    assert_eq!(list.len(), 3, "warnings: {:?}", read.warnings);
    assert!(list
        .iter()
        .any(|revision| revision.kind == "insert" && revision.author == "Ada" && revision.text == "inserted"));
    assert!(list
        .iter()
        .any(|revision| revision.kind == "delete" && revision.author == "Grace" && revision.text == "removed"));
    let format = list.iter().find(|revision| revision.kind == "format").expect("format revision");
    assert_eq!(format.text, "reformatted");
    let anchor_run = document.blocks.iter().find_map(|block| match block {
        Block::Paragraph { runs, .. } => runs.iter().find(|run| run.text == "reformatted"),
        _ => None,
    });
    let anchor_run = anchor_run.expect("reformatted run");
    assert!(anchor_run.bold && anchor_run.italic, "the current formatting must stay applied");
    assert_eq!(
        anchor_run.revision.as_ref().and_then(|revision| revision.original.as_ref()).map(|original| original.bold),
        Some(false),
        "the pre-change formatting must be captured"
    );

    // Comments with a reply, still anchored to a run.
    assert_eq!(document.comments.len(), 1);
    assert!(document.comments[0].text.contains("verify"));
    assert_eq!(document.comments[0].replies.len(), 1);
    assert!(document.comments[0].replies[0].text.contains("Verified"));
    let anchored = document.blocks.iter().find_map(|block| match block {
        Block::Paragraph { runs, .. } => runs.iter().find_map(|run| run.comment.clone()),
        _ => None,
    });
    assert!(anchored.is_some(), "the comment must stay anchored");

    // Fields: REF/PAGEREF and DATE survive with their cached values.
    let fields: Vec<&FieldRef> = document
        .blocks
        .iter()
        .filter_map(|block| match block {
            Block::Paragraph { runs, .. } => Some(runs.iter().filter_map(|run| run.field.as_ref()).collect::<Vec<_>>()),
            _ => None,
        })
        .flatten()
        .collect();
    assert!(
        fields.iter().any(|field| field.kind == "refPage" && field.target == "GoldenTarget" && field.cached == "2"),
        "fields: {fields:?}"
    );
    assert!(fields.iter().any(|field| field.kind == "date" && field.cached == "2026-01-01"), "fields: {fields:?}");

    // Table and image survive with their bytes.
    assert!(document.plain_text().contains("Answer"));
    assert!(document.plain_text().contains("42"));
    let image = document
        .blocks
        .iter()
        .find_map(|block| match block {
            Block::Image { image, width_pt, height_pt, .. } => Some((image, width_pt, height_pt)),
            _ => None,
        })
        .expect("image block");
    assert_eq!(
        image.0.bytes(),
        original
            .blocks
            .iter()
            .find_map(|block| match block {
                Block::Image { image, .. } => Some(image.bytes()),
                _ => None,
            })
            .unwrap()
    );
    assert!(
        (image.1 - 96.0).abs() < 0.5 && (image.2 - 96.0).abs() < 0.5,
        "image size drifted: {}x{}",
        image.1,
        image.2
    );

    // A second cycle must not lose anything either.
    let second = docx::read_docx(&docx::write_docx(&document).unwrap()).unwrap().document;
    assert_eq!(second.all_sections().len(), 2);
    assert_eq!(revisions::revision_count(&second), 3);
    assert_eq!(second.comments.len(), 1);
    assert_eq!(second.footnotes.len(), 1);
    assert_eq!(second.endnotes.len(), 1);
}

#[test]
fn golden_calc_xlsx_roundtrip_keeps_the_v31_features_xlsx_can_represent() {
    let original = v31_workbook();
    let bytes = xlsx::write_xlsx(&original).unwrap();
    let read = xlsx::read_workbook_bytes(&bytes).unwrap();
    let sheet = read.workbook.sheets.iter().find(|sheet| sheet.name == "Data").expect("Data sheet");

    // Structured table with its calculated column and its own filter.
    assert_eq!(sheet.tables.len(), 1, "warnings: {:?}", read.warnings);
    let table = &sheet.tables[0];
    assert_eq!(table.name, "Sales");
    assert_eq!(table.range, "A1:D4");
    assert!(table.has_headers);
    assert!(table.banded_rows);
    assert_eq!(table.style_name, "TableStyleMedium2");
    assert_eq!(table.columns.len(), 4);
    assert_eq!(table.columns[3].name, "Amount");
    assert_eq!(table.columns[3].formula.as_deref(), Some("=[@Units]*[@Price]"));
    assert_eq!(table.filter.as_ref().map(|filter| filter.values.clone()), Some(vec!["Widget".to_string()]));

    // Structured reference in a formula outside the table.
    assert_eq!(sheet.get("F1").and_then(|cell| cell.formula.as_deref()), Some("=SUM(Sales[Amount])"));

    // Chart: ranges plus both caches.
    assert_eq!(sheet.charts.len(), 1);
    let chart = &sheet.charts[0];
    assert_eq!(chart.chart.title, "Amount by item");
    assert_eq!(chart.chart.categories, "A2:A4");
    assert_eq!(chart.chart.series[0].range, "D2:D4");
    assert_eq!(chart.chart.series[0].color.as_deref(), Some("#1D4ED8"));
    assert_eq!(chart.chart.categories_cache, vec!["Widget".to_string(), "Gadget".to_string(), "Gizmo".to_string()]);
    assert_eq!(chart.chart.series_values_cache, vec![vec![20.0, 60.0, 120.0]]);
    assert_eq!(chart.anchor, "F3");

    // Picture: bytes, anchor offsets, size and rotation.
    assert_eq!(sheet.images.len(), 1);
    let image = &sheet.images[0];
    assert_eq!(image.image.bytes(), pixel_png().bytes());
    assert_eq!(image.anchor.address, "B8");
    assert_eq!(image.anchor.col_off_emu, 9_525);
    assert_eq!(image.anchor.row_off_emu, 19_050);
    assert!((image.width_px - 120.0).abs() < 0.01 && (image.height_px - 60.0).abs() < 0.01);
    assert!((image.rotation_deg - 15.0).abs() < 0.01);

    // Print settings, protection and the preserved live pivot.
    assert_eq!(sheet.print, original.sheets[0].print);
    assert_eq!(sheet.protection, original.sheets[0].protection);
    assert_eq!(read.workbook.preserved_pivots.len(), 1);
    let pivot = &read.workbook.preserved_pivots[0];
    assert_eq!(pivot.name, "PivotTable1");
    assert_eq!(pivot.cache_id, 7);
    assert_eq!(pivot.source, "Data!A1:D4");
    assert_eq!(pivot.fields, vec!["Item".to_string(), "Units".to_string(), "Price".to_string(), "Amount".to_string()]);
    assert_eq!(pivot.definition_xml, PIVOT_DEFINITION);
    assert_eq!(pivot.table_xml, PIVOT_TABLE);
    assert_eq!(pivot.records_base64, original.preserved_pivots[0].records_base64);

    // A second cycle is stable at the model level.
    let second = xlsx::read_workbook_bytes(&xlsx::write_xlsx(&read.workbook).unwrap()).unwrap();
    let sheet2 = second.workbook.sheets.iter().find(|sheet| sheet.name == "Data").unwrap();
    assert_eq!(sheet2.tables, sheet.tables);
    assert_eq!(sheet2.charts, sheet.charts);
    assert_eq!(sheet2.images, sheet.images);
    assert_eq!(sheet2.print, sheet.print);
    assert_eq!(sheet2.protection, sheet.protection);
    assert_eq!(second.workbook.preserved_pivots, read.workbook.preserved_pivots);
}

#[test]
fn golden_impress_pptx_roundtrip_keeps_the_v31_features_pptx_can_represent() {
    let original = v31_deck();
    let bytes = pptx::write_pptx(&original).unwrap();
    let read = pptx::read_pptx(&bytes).unwrap();
    let deck = read.deck;

    // Masters and layouts keep their identity, theme and placeholders.
    assert_eq!(deck.masters.len(), 2, "warnings: {:?}", read.warnings);
    assert_eq!(deck.masters[0].layouts.len(), 2);
    assert_eq!(deck.masters[1].layouts.len(), 1);
    assert_eq!(deck.masters[0].theme, "business");
    assert_eq!(deck.masters[1].theme, "modern");
    assert_eq!(deck.slides[0].master_id.as_deref(), Some(deck.masters[0].id.as_str()));
    assert_eq!(deck.slides[0].layout_id.as_deref(), Some(deck.masters[0].layouts[0].id.as_str()));
    assert_eq!(deck.slides[1].master_id.as_deref(), Some(deck.masters[1].id.as_str()));
    assert!(deck.masters[0].objects.iter().any(|object| object.placeholder.as_deref() == Some("footer")));
    assert!(deck.masters[0].layouts[0].objects.iter().any(|object| object.placeholder.as_deref() == Some("title")));

    // Nested group with its children and absolute coordinates.
    let group = deck.slides[0].objects.iter().find(|object| object.kind == "group").expect("outer group");
    assert_eq!(group.children.len(), 3);
    let nested = group.children.iter().find(|object| object.kind == "group").expect("nested group");
    assert_eq!(nested.children.len(), 2);
    assert!((nested.x - 280.0).abs() < 0.6 && (nested.w - 180.0).abs() < 0.6);
    assert!(nested.children.iter().any(|object| object.kind == "ellipse"));

    // Chart with cached category labels and values.
    let chart = deck.slides[0].objects.iter().find_map(|object| object.chart.as_ref()).expect("chart");
    assert_eq!(chart.kind, "column");
    assert_eq!(chart.title, "Quarterly units");
    assert_eq!(chart.categories, "A2:A4");
    assert_eq!(chart.categories_cache, vec!["Q1".to_string(), "Q2".to_string(), "Q3".to_string()]);
    assert_eq!(chart.series_values_cache, vec![vec![10.0, 20.5, 31.0]]);

    // Animations keep kind, effect, trigger and timing in order.
    assert_eq!(deck.slides[0].animations.len(), 3);
    let kinds: Vec<&str> = deck.slides[0].animations.iter().map(|animation| animation.kind.as_str()).collect();
    assert_eq!(kinds, vec!["entrance", "emphasis", "exit"]);
    assert_eq!(deck.slides[0].animations[1].effect, "pulse");
    assert_eq!(deck.slides[0].animations[1].trigger, "withPrevious");
    assert_eq!(deck.slides[0].animations[1].duration_ms, 800);
    assert_eq!(deck.slides[0].animations[1].delay_ms, 200);

    // Speaker notes and the second slide's transition.
    assert!(deck.slides[0].notes.contains("Golden speaker notes"));
    assert_eq!(deck.slides[1].transition.as_deref(), Some("fade"));

    // A second cycle stays stable.
    let second = pptx::read_pptx(&pptx::write_pptx(&deck).unwrap()).unwrap().deck;
    assert_eq!(second.masters.len(), 2);
    assert_eq!(second.slides[0].animations.len(), 3);
    let chart2 = second.slides[0].objects.iter().find_map(|object| object.chart.as_ref()).expect("chart");
    assert_eq!(chart2.categories_cache.len(), 3);
    assert_eq!(chart2.series_values_cache[0], vec![10.0, 20.5, 31.0]);
}

// ---------------------------------------------------------------------------
// Committed fixtures: the cross-platform summary contract
// ---------------------------------------------------------------------------

/// This is the contract the Android build has to satisfy. The Android app
/// embeds the same `officecore` crate, so a fixture produced here and loaded
/// on the device must yield the exact same feature summary. Running the suite
/// on a real Android device is a manual release step; this test is what makes
/// the claim testable in CI, and it is deliberately about *features*, not
/// about rendering pixels.
#[test]
fn golden_committed_fixtures_pin_the_v31_feature_summary() {
    for (name, unit) in golden_units() {
        let path = ensure_fixture(name, &unit);
        let bytes = std::fs::read(&path).unwrap();
        let raw = load_oswk(&bytes);

        // Envelope contract for every kind.
        assert_eq!(raw["format"], json!("office-swiss-army-knife"), "{name}");
        assert_eq!(raw["version"], json!(schema::SCHEMA_VERSION), "{name}");
        assert_eq!(raw["schemaVersion"], json!(schema::SCHEMA_VERSION), "{name}");
        assert_eq!(raw["savedAt"], json!("2026-01-01T00:00:00Z"), "{name}: fixtures use a fixed timestamp");

        match name {
            "writer-v31.oswk" => {
                let document: TextDocument = serde_json::from_value(raw["model"].clone()).unwrap();
                assert_eq!(raw["kind"], json!("writer"));
                assert_eq!(document.blocks.len(), 7);
                assert_eq!(document.footnotes.len(), 1);
                assert_eq!(document.endnotes.len(), 1);
                assert_eq!(document.comments.len(), 1);
                assert_eq!(revisions::revision_count(&document), 3);
                assert_eq!(document.footnote_order().len(), 1);
                assert_eq!(document.endnote_order().len(), 1);
                assert!(document.bookmarks.iter().any(|bookmark| bookmark.name == "GoldenTarget"));
                assert!(document.blocks.iter().any(|block| matches!(block, Block::Table { .. })));
                assert!(document.blocks.iter().any(|block| matches!(block, Block::Image { .. })));
                assert!(document.blocks.iter().any(|block| block.is_section_break()));
            }
            "calc-v31.oswk" => {
                let workbook: Workbook = serde_json::from_value(raw["model"].clone()).unwrap();
                assert_eq!(raw["kind"], json!("calc"));
                let sheet = &workbook.sheets[0];
                assert_eq!(sheet.tables.len(), 1);
                assert_eq!(sheet.tables[0].name, "Sales");
                assert_eq!(sheet.tables[0].columns[3].formula.as_deref(), Some("=[@Units]*[@Price]"));
                assert_eq!(sheet.get("F1").and_then(|cell| cell.formula.as_deref()), Some("=SUM(Sales[Amount])"));
                assert_eq!(sheet.charts.len(), 1);
                assert_eq!(sheet.charts[0].chart.categories_cache, vec!["Widget", "Gadget", "Gizmo"]);
                assert_eq!(sheet.charts[0].chart.series_values_cache, vec![vec![20.0, 60.0, 120.0]]);
                assert_eq!(sheet.images.len(), 1);
                assert_eq!(sheet.print.print_area.as_deref(), Some("A1:F20"));
                assert_eq!(sheet.print.print_titles_rows.as_deref(), Some("1:1"));
                assert_eq!(sheet.print.row_breaks, vec![4]);
                assert_eq!(sheet.print.col_breaks, vec![3]);
                assert!(sheet.protection.enabled);
                assert_eq!(workbook.preserved_pivots.len(), 1);
                assert_eq!(workbook.preserved_pivots[0].fields, vec!["Item", "Units", "Price", "Amount"]);
            }
            "impress-v31.oswk" => {
                let deck: Deck = serde_json::from_value(raw["model"].clone()).unwrap();
                assert_eq!(raw["kind"], json!("impress"));
                assert_eq!(deck.masters.len(), 2);
                assert_eq!(deck.masters[0].layouts.len(), 2);
                assert_eq!(deck.slides.len(), 2);
                assert_eq!(deck.slides[0].animations.len(), 3);
                let chart = deck.slides[0].objects.iter().find_map(|object| object.chart.as_ref()).expect("chart");
                assert_eq!(chart.categories_cache, vec!["Q1", "Q2", "Q3"]);
                assert_eq!(chart.series_values_cache, vec![vec![10.0, 20.5, 31.0]]);
                assert!(deck.slides[0].notes.contains("Golden speaker notes"));
                let group = deck.slides[0].objects.iter().find(|object| object.kind == "group").expect("group");
                assert_eq!(group.children.len(), 3);
            }
            other => panic!("unexpected fixture {other}"),
        }
    }
}

/// The fixture writer is deterministic: UUID generators are overwritten with
/// stable ids and the envelope timestamp is fixed, so the engine regenerates
/// the committed bytes exactly. A mismatch means the model changed shape; the
/// message points at the explicit regeneration switch instead of silently
/// accepting drift.
///
/// Windows checkouts can still convert the committed LF bytes to CRLF when
/// `core.autocrlf` is on (the fixtures are JSON, so Git treats them as text).
/// That is a checkout artifact, not model drift, so the comparison normalizes
/// CRLF to LF; `.gitattributes` also marks the fixtures binary to keep the
/// checkout byte-exact where possible.
#[test]
fn golden_committed_fixtures_are_reproducible() {
    fn to_lf(bytes: &[u8]) -> Vec<u8> {
        let mut normalized = Vec::with_capacity(bytes.len());
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] == b'\r' && bytes.get(index + 1) == Some(&b'\n') {
                index += 1;
            }
            normalized.push(bytes[index]);
            index += 1;
        }
        normalized
    }
    for (name, unit) in golden_units() {
        let path = ensure_fixture(name, &unit);
        let expected = serde_json::to_vec_pretty(&unit).unwrap();
        let actual = std::fs::read(&path).unwrap();
        if to_lf(&actual) != expected && std::env::var_os("OSAK_REGEN_GOLDEN").is_some() {
            std::fs::write(&path, &expected).unwrap();
            continue;
        }
        assert_eq!(
            to_lf(&actual),
            expected,
            "fixture {name} is stale; regenerate it with OSAK_REGEN_GOLDEN=1 and commit the new bytes"
        );
    }
    // The fixture assets themselves must be real media, not placeholders.
    assert_eq!(pixel_png().pixel_size(), (1, 1));
}
