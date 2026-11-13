//! Hierarchical Tagged-PDF (`Table` / `TR` / `TH` / `TD` / `P` / `H1` /
//! `Figure`) and pagination `/Artifact` tests across Writer, Calc, and Impress.

use lopdf::{Document, Object, ObjectId};
use officecore::layout::{deck_to_pdf, document_to_pdf, workbook_to_pdf};
use officecore::model::{
    Block, Cell, CellValue, Deck, ImageData, ParaProps, Run, Slide, SlideFooter, SlideMaster, SlideObject, TableCell,
    TableData, TableRow, TextDocument, TextFrame, TextParagraph, Workbook,
};
use std::collections::BTreeSet;

fn tiny_png_bytes() -> Vec<u8> {
    let mut img = image::RgbaImage::new(2, 2);
    for pixel in img.pixels_mut() {
        *pixel = image::Rgba([24, 96, 192, 255]);
    }
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).expect("encode tiny png");
    out
}

fn struct_tag(doc: &Document, id: ObjectId) -> Option<String> {
    let dict = doc.get_dictionary(id).ok()?;
    let name = dict.get(b"S").ok()?.as_name().ok()?;
    Some(String::from_utf8_lossy(name).into_owned())
}

fn struct_kids(doc: &Document, id: ObjectId) -> Vec<ObjectId> {
    let Ok(dict) = doc.get_dictionary(id) else {
        return Vec::new();
    };
    let Ok(kids) = dict.get(b"K") else {
        return Vec::new();
    };
    match kids {
        Object::Array(items) => items.iter().filter_map(|item| item.as_reference().ok()).collect(),
        Object::Reference(child) => vec![*child],
        _ => Vec::new(),
    }
}

/// Validates every `/P` back-pointer under `node` and collects structure tags.
fn verify_tree_links(doc: &Document, node: ObjectId, expected_parent: ObjectId, seen_tags: &mut BTreeSet<String>) {
    let dict = doc.get_dictionary(node).expect("StructElem dictionary");
    let parent = dict.get(b"P").and_then(Object::as_reference).expect("StructElem /P reference");
    assert_eq!(parent, expected_parent, "broken /P back-pointer on {node:?}");
    if let Some(tag) = struct_tag(doc, node) {
        seen_tags.insert(tag);
    }
    for kid in struct_kids(doc, node) {
        verify_tree_links(doc, kid, node, seen_tags);
    }
}

fn validate_struct_root_and_parent_tree(doc: &Document) -> (ObjectId, BTreeSet<String>) {
    let catalog = doc.catalog().expect("catalog");
    let root_id = catalog.get(b"StructTreeRoot").and_then(Object::as_reference).expect("StructTreeRoot");
    let root_dict = doc.get_dictionary(root_id).expect("StructTreeRoot dict");
    let doc_id = root_dict
        .get(b"K")
        .and_then(Object::as_array)
        .and_then(|arr| arr.first().ok_or(lopdf::Error::DictKey("K".into())))
        .and_then(Object::as_reference)
        .expect("Document StructElem");
    let mut tags = BTreeSet::new();
    verify_tree_links(doc, doc_id, root_id, &mut tags);

    let parent_tree_id = root_dict.get(b"ParentTree").and_then(Object::as_reference).expect("ParentTree");
    let parent_tree = doc.get_dictionary(parent_tree_id).expect("ParentTree dict");
    let nums = parent_tree.get(b"Nums").and_then(Object::as_array).expect("ParentTree /Nums");
    assert!(!nums.is_empty(), "ParentTree /Nums must map pages to StructElems");
    for pair in nums.chunks(2) {
        let elems = pair[1].as_array().expect("ParentTree page array");
        for elem in elems {
            let elem_id = elem.as_reference().expect("ParentTree element ref");
            assert!(doc.get_dictionary(elem_id).is_ok(), "ParentTree points to missing object {elem_id:?}");
        }
    }
    (doc_id, tags)
}

#[test]
fn writer_table_emits_nested_structure_and_parent_tree() {
    let mut document = TextDocument::new_blank("Tagged Writer");
    document.header = vec![Block::paragraph("Gizli Üstbilgi")];
    document.footer = vec![Block::paragraph("Sayfa {{page}} / {{pages}}")];

    let mut rows = Vec::new();
    rows.push(TableRow {
        header: true,
        cells: vec![
            TableCell { blocks: vec![Block::paragraph("Kalem")], ..Default::default() },
            TableCell { blocks: vec![Block::paragraph("Tutar")], ..Default::default() },
        ],
        ..Default::default()
    });
    for i in 1..=50 {
        rows.push(TableRow {
            header: false,
            cells: vec![
                TableCell { blocks: vec![Block::paragraph(&format!("Satır {i}"))], ..Default::default() },
                TableCell { blocks: vec![Block::paragraph(&format!("{i}00 TL"))], ..Default::default() },
            ],
            ..Default::default()
        });
    }
    let table = TableData { rows, column_widths_pt: vec![220.0, 220.0], borders: true, ..TableData::default() };

    let mut image = ImageData::from_bytes("logo.png", &tiny_png_bytes());
    image.alt = "Kurum Logosu".into();
    document.blocks = vec![
        Block::heading("Özet Rapor", 1),
        Block::Image {
            image,
            width_pt: 60.0,
            height_pt: 60.0,
            align: "left".into(),
            caption: String::new(),
            wrap: "inline".into(),
        },
        Block::Table { table },
    ];

    let bytes = document_to_pdf(&document);
    let doc = Document::load_mem(&bytes).expect("valid Writer PDF");
    assert!(doc.get_pages().len() >= 2, "50-row table must span multiple pages");

    let (doc_id, tags) = validate_struct_root_and_parent_tree(&doc);
    for expected in ["Document", "H1", "Figure", "Table", "TR", "TH", "TD", "P"] {
        assert!(tags.contains(expected), "missing structure tag <{expected}> in {tags:?}");
    }

    let doc_kids = struct_kids(&doc, doc_id);
    let table_ids: Vec<ObjectId> =
        doc_kids.into_iter().filter(|&id| struct_tag(&doc, id).as_deref() == Some("Table")).collect();
    assert!(table_ids.len() >= 2, "split table across pages must open fresh <Table> per page slice");

    let first_table = table_ids[0];
    let tr_ids = struct_kids(&doc, first_table);
    assert!(!tr_ids.is_empty());
    assert_eq!(struct_tag(&doc, tr_ids[0]).as_deref(), Some("TR"));
    let th_ids = struct_kids(&doc, tr_ids[0]);
    assert!(th_ids.iter().all(|&id| struct_tag(&doc, id).as_deref() == Some("TH")));
    assert_eq!(struct_tag(&doc, struct_kids(&doc, th_ids[0])[0]).as_deref(), Some("P"));
}

#[test]
fn writer_headers_and_footers_are_pagination_artifacts() {
    let mut document = TextDocument::new_blank("Header Footer Artifacts");
    document.header = vec![Block::paragraph("Kurumsal Başlık")];
    document.footer = vec![Block::paragraph("Sayfa {{page}}")];
    document.blocks = vec![Block::Paragraph {
        props: ParaProps::default(),
        runs: vec![Run { text: "Tek gövde paragrafı".into(), ..Default::default() }],
    }];

    let bytes = document_to_pdf(&document);
    let doc = Document::load_mem(&bytes).expect("valid PDF");
    let page_id = *doc.get_pages().values().next().expect("page 1");
    let ops = String::from_utf8_lossy(&doc.get_page_content(page_id)).into_owned();
    assert!(ops.contains("/Artifact <</Type /Pagination /Subtype /Header>> BDC"), "missing Header artifact in:\n{ops}");
    assert!(ops.contains("/Artifact <</Type /Pagination /Subtype /Footer>> BDC"), "missing Footer artifact in:\n{ops}");

    let (doc_id, _) = validate_struct_root_and_parent_tree(&doc);
    assert_eq!(
        struct_kids(&doc, doc_id).len(),
        1,
        "only the body paragraph should appear in StructTreeRoot, not header/footer artifacts"
    );
}

#[test]
fn calc_workbook_pdf_emits_table_structure_and_artifacts() {
    let mut workbook = Workbook::new_blank("Gelir Tablosu");
    let sheet = &mut workbook.sheets[0];
    sheet.set("A1", Cell { value: CellValue::Text("Kalem".into()), ..Default::default() });
    sheet.set("B1", Cell { value: CellValue::Text("Tutar".into()), ..Default::default() });
    sheet.set("A2", Cell { value: CellValue::Text("Danışmanlık".into()), ..Default::default() });
    sheet.set("B2", Cell { value: CellValue::Number(1500.0), ..Default::default() });

    let bytes = workbook_to_pdf(&workbook, 1);
    let doc = Document::load_mem(&bytes).expect("valid Calc PDF");
    let (_, tags) = validate_struct_root_and_parent_tree(&doc);
    for expected in ["Document", "Table", "TR", "TH", "TD", "P"] {
        assert!(tags.contains(expected), "Calc PDF missing <{expected}> in {tags:?}");
    }

    let page_id = *doc.get_pages().values().next().expect("Calc page");
    let ops = String::from_utf8_lossy(&doc.get_page_content(page_id)).into_owned();
    assert!(ops.contains("/Artifact <</Type /Pagination /Subtype /Header>> BDC"));
    assert!(ops.contains("/Artifact <</Type /Pagination /Subtype /Footer>> BDC"));
}

#[test]
fn impress_deck_pdf_emits_slide_table_hierarchy_and_footer_artifacts() {
    let mut deck = Deck::new_blank("Sunum");
    deck.footer = Some(SlideFooter {
        enabled: true,
        text: "Gizli Altbilgi".into(),
        show_text: true,
        show_slide_number: true,
        show_date: true,
        date_text: "2026-11-13".into(),
    });
    let mut master = SlideMaster::default();
    let mut f = SlideObject::new("text", 40.0, 500.0, 260.0, 30.0);
    f.placeholder = Some("footer".into());
    let mut d = SlideObject::new("text", 320.0, 500.0, 160.0, 30.0);
    d.placeholder = Some("date".into());
    let mut n = SlideObject::new("text", 800.0, 500.0, 80.0, 30.0);
    n.placeholder = Some("slideNumber".into());
    master.objects = vec![f, d, n];
    deck.masters = vec![master];

    let mut slide = Slide::default();
    let mut title = SlideObject::new("text", 40.0, 40.0, 500.0, 60.0);
    title.placeholder = Some("title".into());
    title.text = Some(TextFrame {
        paragraphs: vec![TextParagraph {
            text: "Yıllık Değerlendirme".into(),
            size_pt: Some(26.0),
            ..Default::default()
        }],
        ..Default::default()
    });
    let mut table_obj = SlideObject::new("table", 40.0, 120.0, 420.0, 140.0);
    let mut table = TableData::simple(2, 2, 420.0);
    table.rows[0].cells[0].blocks = vec![Block::paragraph("Ölçüt")];
    table.rows[0].cells[1].blocks = vec![Block::paragraph("Skor")];
    table.rows[1].cells[0].blocks = vec![Block::paragraph("Sadakat")];
    table.rows[1].cells[1].blocks = vec![Block::paragraph("100")];
    table_obj.table = Some(table);
    slide.objects = vec![title, table_obj];
    deck.slides = vec![slide];

    let bytes = deck_to_pdf(&deck);
    let doc = Document::load_mem(&bytes).expect("valid Impress PDF");
    let (doc_id, tags) = validate_struct_root_and_parent_tree(&doc);
    for expected in ["Document", "H1", "Table", "TR", "TH", "TD", "P"] {
        assert!(tags.contains(expected), "Impress PDF missing <{expected}> in {tags:?}");
    }

    let page_id = *doc.get_pages().values().next().expect("slide page");
    let ops = String::from_utf8_lossy(&doc.get_page_content(page_id)).into_owned();
    assert_eq!(
        ops.matches("/Artifact <</Type /Pagination /Subtype /Footer>> BDC").count(),
        3,
        "footer, date, and slideNumber placeholders must all emit Footer artifacts"
    );

    // Only the slide title (H1) and table (Table) should sit directly under Document;
    // the three footer placeholders must stay outside StructTreeRoot.
    let top_tags: Vec<Option<String>> = struct_kids(&doc, doc_id).into_iter().map(|id| struct_tag(&doc, id)).collect();
    assert_eq!(top_tags, vec![Some("H1".into()), Some("Table".into())]);
}
