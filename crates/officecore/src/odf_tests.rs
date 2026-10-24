//! Unit tests for `odf` (kept apart to respect the file-size ratchet).
use super::*;

fn sample_document() -> TextDocument {
    let mut document = TextDocument::new_blank("ODT sample");
    document.blocks = vec![
        Block::heading("Başlık", 1),
        Block::Paragraph {
            props: ParaProps::default(),
            runs: vec![
                Run { text: "düz ".into(), ..Default::default() },
                Run { text: "kalın".into(), bold: true, ..Default::default() },
            ],
        },
        Block::Paragraph {
            props: ParaProps {
                list: Some(ListInfo { kind: "bullet".into(), level: 0, start: 1, marker: "•".into() }),
                ..Default::default()
            },
            runs: vec![Run { text: "madde".into(), ..Default::default() }],
        },
        Block::Table { table: TableData::simple(2, 2, 400.0) },
    ];
    document.footer = vec![Block::paragraph("Alt bilgi")];
    document
}

fn sample_png() -> Vec<u8> {
    let mut buffer = image::RgbaImage::new(4, 4);
    for pixel in buffer.pixels_mut() {
        *pixel = image::Rgba([200, 30, 60, 255]);
    }
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(buffer).write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn package_with(content: &str, styles: Option<&str>) -> Vec<u8> {
    let mut zip = ZipWriter::new();
    zip.add_text("mimetype", "application/vnd.oasis.opendocument.text");
    zip.add_text("content.xml", content);
    if let Some(styles) = styles {
        zip.add_text("styles.xml", styles);
    }
    zip.finish()
}

fn first_table(document: &TextDocument) -> &TableData {
    document
        .blocks
        .iter()
        .find_map(|block| match block {
            Block::Table { table } => Some(table),
            _ => None,
        })
        .expect("the document must contain a table")
}

/// Every manifest entry must name a real package entry and every package
/// entry must be named by the manifest. LibreOffice refuses packages where
/// the two disagree ("source file could not be loaded"), which is how the
/// old static manifest shipped with dangling `Pictures/`/`Images/` entries
/// and an unlisted `settings.xml`.
fn assert_manifest_matches_package(bytes: &[u8]) {
    let reader = ZipReader::open(bytes.to_vec()).expect("paket açılmalı");
    let manifest = reader.read_text("META-INF/manifest.xml").expect("manifest okunmalı");
    assert!(!manifest.contains("media-type=\"\""), "boş media-type olmamalı");
    let mut listed: Vec<String> = Vec::new();
    let mut rest = manifest.as_str();
    while let Some(index) = rest.find("manifest:full-path=\"") {
        rest = &rest[index + "manifest:full-path=\"".len()..];
        let end = rest.find('"').expect("kapanış tırnağı");
        if &rest[..end] != "/" {
            listed.push(rest[..end].to_string());
        }
        rest = &rest[end..];
    }
    let mut packaged: Vec<String> = reader
        .names()
        .filter(|name| *name != "mimetype" && *name != "META-INF/manifest.xml")
        .map(|name| name.to_string())
        .collect();
    listed.sort();
    packaged.sort();
    assert_eq!(listed, packaged, "manifest ile paket birebir örtüşmeli");
}

#[test]
fn odt_manifest_matches_the_package() {
    let mut document = sample_document();
    document.blocks.push(Block::Image {
        image: ImageData::from_bytes("resim.png", &sample_png()),
        width_pt: 100.0,
        height_pt: 60.0,
        align: "center".into(),
        caption: String::new(),
        wrap: "inline".into(),
    });
    assert_manifest_matches_package(&write_odt(&document).unwrap());
}

#[test]
fn ods_manifest_matches_the_package() {
    assert_manifest_matches_package(&write_ods(&Workbook::new_blank("Manifest")).unwrap());
}

#[test]
fn odp_manifest_matches_the_package() {
    let mut deck = Deck::new_blank("Manifest");
    let mut picture = SlideObject::new("image", 60.0, 60.0, 200.0, 120.0);
    picture.image = Some(ImageData::from_bytes("resim.png", &sample_png()));
    deck.slides = vec![Slide { objects: vec![picture], ..Default::default() }];
    assert_manifest_matches_package(&write_odp(&deck).unwrap());
}

#[test]
fn odt_roundtrip() {
    let bytes = write_odt(&sample_document()).unwrap();
    let read = read_odt(&bytes).unwrap();
    let text = read.document.plain_text();
    assert!(text.contains("Başlık"), "text was {text}");
    assert!(text.contains("kalın"));
    assert!(text.contains("madde"));
    assert!(read.document.footer.iter().map(Block::plain_text).any(|text| text.contains("Alt bilgi")));
}

#[test]
fn odt_mimetype_first_and_stored() {
    let bytes = write_odt(&sample_document()).unwrap();
    assert_eq!(&bytes[30..38], b"mimetype");
    assert_eq!(u16::from_le_bytes([bytes[8], bytes[9]]), 0);
}

#[test]
fn odt_notes_roundtrip() {
    let mut document = TextDocument::new_blank("Notes");
    document.footnotes = vec![
        Footnote {
            id: "fn-a".into(),
            runs: vec![Run { text: "Birinci not".into(), bold: true, ..Default::default() }],
            marker: String::new(),
        },
        Footnote {
            id: "fn-b".into(),
            runs: vec![Run { text: "Tablo notu".into(), ..Default::default() }],
            marker: String::new(),
        },
    ];
    document.endnotes = vec![Footnote {
        id: "en-a".into(),
        runs: vec![Run { text: "Son not".into(), ..Default::default() }],
        marker: String::new(),
    }];
    let mut table = TableData::simple(1, 1, 300.0);
    table.rows[0].cells[0].blocks = vec![Block::Paragraph {
        props: ParaProps::default(),
        runs: vec![
            Run { text: "hucre".into(), ..Default::default() },
            Run { footnote: Some("fn-b".into()), ..Default::default() },
        ],
    }];
    document.blocks = vec![
        Block::Paragraph {
            props: ParaProps::default(),
            runs: vec![
                Run { text: "govde".into(), ..Default::default() },
                Run { footnote: Some("fn-a".into()), ..Default::default() },
                Run { text: " son".into(), ..Default::default() },
                Run { endnote: Some("en-a".into()), ..Default::default() },
            ],
        },
        Block::Table { table },
    ];

    let bytes = write_odt(&document).unwrap();
    let reader = ZipReader::open(bytes.clone()).unwrap();
    let content = reader.read_text("content.xml").unwrap();
    assert!(content.contains("text:note-class=\"footnote\""), "content: {content}");
    assert!(content.contains("text:note-class=\"endnote\""));
    assert!(content.contains("Birinci not"));
    assert!(content.contains("Tablo notu"));
    assert!(content.contains("Son not"));

    let read = read_odt(&bytes).unwrap();
    assert_eq!(read.document.footnotes.len(), 2, "warnings: {:?}", read.warnings);
    assert_eq!(read.document.endnotes.len(), 1);
    let order = read.document.footnote_order();
    assert_eq!(order.len(), 2);
    assert_eq!(read.document.footnote_number(&order[0]), Some(1));
    assert_eq!(read.document.footnote_number(&order[1]), Some(2));
    let first = read.document.footnotes.iter().find(|note| note.id == order[0]).unwrap();
    assert!(first.runs.iter().any(|run| run.text.contains("Birinci not")));
    let second = read.document.footnotes.iter().find(|note| note.id == order[1]).unwrap();
    assert!(second.runs.iter().any(|run| run.text.contains("Tablo notu")));
    assert!(read.document.endnotes[0].runs.iter().any(|run| run.text.contains("Son not")));

    // Note bodies must not be folded into the paragraph text.
    let body = read.document.blocks.iter().map(Block::plain_text).collect::<Vec<_>>().join("\n");
    assert!(!body.contains("Birinci not"), "note body leaked: {body}");
    assert!(!body.contains("Tablo notu"), "table note body leaked: {body}");
    assert!(body.contains("govde"));
    assert!(body.contains("hucre"));

    let paragraph = match &read.document.blocks[0] {
        Block::Paragraph { runs, .. } => runs,
        _ => panic!("first block should be a paragraph"),
    };
    assert!(paragraph.iter().any(|run| run.footnote.is_some()));
    assert!(paragraph.iter().any(|run| run.endnote.is_some()));
    let cell = match &read.document.blocks[1] {
        Block::Table { table } => &table.rows[0].cells[0],
        _ => panic!("second block should be a table"),
    };
    let cell_ref = cell.blocks.iter().find_map(|block| match block {
        Block::Paragraph { runs, .. } => runs.iter().find(|run| run.footnote.is_some()),
        _ => None,
    });
    assert!(cell_ref.is_some(), "the table cell footnote reference was lost");
}

#[test]
fn odt_table_spans_roundtrip() {
    let mut document = TextDocument::new_blank("Spans");
    let mut table = TableData::simple(3, 3, 450.0);
    table.rows[0].cells = vec![
        TableCell { blocks: vec![Block::paragraph("Span")], colspan: 2, rowspan: 2, ..Default::default() },
        TableCell { blocks: vec![Block::paragraph("B")], ..Default::default() },
    ];
    table.rows[1].cells = vec![TableCell { blocks: vec![Block::paragraph("C")], ..Default::default() }];
    document.blocks = vec![Block::Table { table }];

    let bytes = write_odt(&document).unwrap();
    let content = ZipReader::open(bytes.clone()).unwrap().read_text("content.xml").unwrap();
    assert!(content.contains("table:number-columns-spanned=\"2\""), "content: {content}");
    assert!(content.contains("table:number-rows-spanned=\"2\""), "content: {content}");
    // The two grid positions below the origin are covered placeholders, and
    // the table still has three grid columns even though no row lists three
    // origin cells.
    assert_eq!(content.matches("<table:covered-table-cell/>").count(), 2, "content: {content}");
    assert_eq!(content.matches("<table:table-column ").count(), 3, "content: {content}");

    let read = read_odt(&bytes).unwrap();
    let table = first_table(&read.document);
    assert_eq!(table.rows.len(), 3, "warnings: {:?}", read.warnings);
    assert_eq!(table.rows[0].cells.len(), 2);
    assert_eq!(table.rows[0].cells[0].colspan, 2);
    assert_eq!(table.rows[0].cells[0].rowspan, 2);
    assert_eq!(table.rows[0].cells[0].blocks[0].plain_text(), "Span");
    assert_eq!(table.rows[0].cells[1].blocks[0].plain_text(), "B");
    assert_eq!(table.rows[1].cells.len(), 1);
    assert_eq!(table.rows[1].cells[0].blocks[0].plain_text(), "C");
    assert_eq!(table.rows[2].cells.len(), 3);
    assert_eq!(table.column_widths_pt.len(), 3);
    assert!((table.column_widths_pt[0] - 150.0).abs() < 0.1, "widths: {:?}", table.column_widths_pt);
}

#[test]
fn odt_table_import_absorbs_repeated_covered_cells() {
    let content = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
        "<office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
        "xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" ",
        "xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" office:version=\"1.2\">",
        "<office:body><office:text><table:table>",
        "<table:table-column table:number-columns-repeated=\"3\"/>",
        // A covered cell with no origin at all is malformed; it is skipped
        // instead of panicking and A/B start at the first grid columns.
        "<table:table-row><table:covered-table-cell/>",
        "<table:table-cell office:value-type=\"string\"><text:p>A</text:p></table:table-cell>",
        "<table:table-cell office:value-type=\"string\"><text:p>B</text:p></table:table-cell></table:table-row>",
        "<table:table-row>",
        "<table:table-cell office:value-type=\"string\" table:number-columns-spanned=\"2\" ",
        "table:number-rows-spanned=\"2\"><text:p>Span</text:p></table:table-cell>",
        "<table:table-cell office:value-type=\"string\"><text:p>C</text:p></table:table-cell>",
        "</table:table-row>",
        // The repeated covered cells absorb into the declared rowspan,
        // which stays 2 because number-rows-spanned already counts them.
        "<table:table-row><table:covered-table-cell table:number-columns-repeated=\"2\"/>",
        "<table:table-cell office:value-type=\"string\"><text:p>D</text:p></table:table-cell></table:table-row>",
        "<table:table-row><table:table-cell office:value-type=\"string\"><text:p>E</text:p></table:table-cell>",
        "<table:table-cell office:value-type=\"string\"><text:p>F</text:p></table:table-cell>",
        "<table:table-cell office:value-type=\"string\"><text:p>G</text:p></table:table-cell></table:table-row>",
        "</table:table></office:text></office:body></office:document-content>"
    );
    let read = read_odt(&package_with(content, None)).unwrap();
    let table = first_table(&read.document);
    assert_eq!(table.rows.len(), 4, "warnings: {:?}", read.warnings);
    assert_eq!(table.rows[0].cells.len(), 2);
    assert_eq!(table.rows[0].cells[0].blocks[0].plain_text(), "A");
    assert_eq!(table.rows[0].cells[1].blocks[0].plain_text(), "B");
    assert_eq!(table.rows[1].cells.len(), 2);
    assert_eq!(table.rows[1].cells[0].colspan, 2);
    assert_eq!(table.rows[1].cells[0].rowspan, 2);
    assert_eq!(table.rows[2].cells.len(), 1);
    assert_eq!(table.rows[2].cells[0].blocks[0].plain_text(), "D");
    assert_eq!(table.rows[3].cells.len(), 3);
    assert_eq!(table.column_widths_pt.len(), 3);
}

#[test]
fn odt_image_wrapping_roundtrip() {
    let mut document = TextDocument::new_blank("Wrap");
    document.blocks = vec![
        Block::Image {
            image: ImageData::from_bytes("a.png", &sample_png()),
            width_pt: 120.0,
            height_pt: 90.0,
            align: "right".into(),
            caption: String::new(),
            wrap: "square".into(),
        },
        Block::Image {
            image: ImageData::from_bytes("b.png", &sample_png()),
            width_pt: 100.0,
            height_pt: 80.0,
            align: "left".into(),
            caption: String::new(),
            wrap: "topBottom".into(),
        },
        Block::Image {
            image: ImageData::from_bytes("c.png", &sample_png()),
            width_pt: 80.0,
            height_pt: 60.0,
            align: "center".into(),
            caption: String::new(),
            wrap: "inline".into(),
        },
    ];
    let bytes = write_odt(&document).unwrap();
    let content = ZipReader::open(bytes.clone()).unwrap().read_text("content.xml").unwrap();
    assert!(content.contains("style:family=\"graphic\""), "content: {content}");
    assert!(content.contains("style:wrap=\"parallel\""), "content: {content}");
    assert!(content.contains("style:wrap=\"none\""), "content: {content}");
    // Only the wrapped frames reference a graphic style; the inline one
    // keeps the plain paragraph-anchored frame.
    assert_eq!(content.matches("draw:style-name").count(), 2, "content: {content}");

    let read = read_odt(&bytes).unwrap();
    let images: Vec<(&String, f64, f64, &String)> = read
        .document
        .blocks
        .iter()
        .filter_map(|block| match block {
            Block::Image { image, width_pt, height_pt, wrap, .. } => Some((&image.name, *width_pt, *height_pt, wrap)),
            _ => None,
        })
        .collect();
    assert_eq!(images.len(), 3, "warnings: {:?}", read.warnings);
    assert_eq!(images[0].3, "square");
    assert_eq!(images[1].3, "topBottom");
    assert_eq!(images[2].3, "inline");
    assert!((images[0].1 - 120.0).abs() < 0.1, "widths: {images:?}");
    assert!((images[0].2 - 90.0).abs() < 0.1, "heights: {images:?}");
    assert_eq!(images[0].0, "image1.png");
}

#[test]
fn odt_tab_stops_roundtrip() {
    let mut document = TextDocument::new_blank("Tabs");
    document.blocks = vec![Block::Paragraph {
        props: ParaProps {
            align: "right".into(),
            tabs: vec![
                TabStop { pos_pt: 72.0, align: "left".into() },
                TabStop { pos_pt: 144.0, align: "center".into() },
                TabStop { pos_pt: 216.0, align: "right".into() },
                TabStop { pos_pt: 288.0, align: "decimal".into() },
            ],
            ..Default::default()
        },
        runs: vec![Run { text: "a\tb".into(), ..Default::default() }],
    }];
    let bytes = write_odt(&document).unwrap();
    let content = ZipReader::open(bytes.clone()).unwrap().read_text("content.xml").unwrap();
    assert!(content.contains("style:position=\"2.540cm\""), "content: {content}");
    assert!(content.contains("style:type=\"char\" style:char=\".\""), "content: {content}");
    assert!(content.contains("style:type=\"center\""), "content: {content}");

    let read = read_odt(&bytes).unwrap();
    let props = match &read.document.blocks[0] {
        Block::Paragraph { props, .. } => props,
        _ => panic!("first block should be a paragraph"),
    };
    assert_eq!(props.align, "right");
    assert_eq!(props.tabs.len(), 4, "tabs: {:?}", props.tabs);
    assert!((props.tabs[0].pos_pt - 72.0).abs() < 0.01, "tabs: {:?}", props.tabs);
    assert!((props.tabs[1].pos_pt - 144.0).abs() < 0.01, "tabs: {:?}", props.tabs);
    assert_eq!(props.tabs[1].align, "center");
    assert_eq!(props.tabs[3].align, "decimal");
}

#[test]
fn odt_named_paragraph_style_and_text_style_import() {
    let styles = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
        "<office:document-styles xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
        "xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" ",
        "xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" office:version=\"1.2\">",
        "<office:styles><style:style style:name=\"Fancy\" style:family=\"paragraph\">",
        "<style:paragraph-properties fo:text-align=\"center\" fo:margin-left=\"1.27cm\" ",
        "fo:break-before=\"page\"><style:tab-stops>",
        "<style:tab-stop style:position=\"2.54cm\" style:type=\"left\"/>",
        "</style:tab-stops></style:paragraph-properties></style:style>",
        "<style:style style:name=\"Bold\" style:family=\"text\">",
        "<style:text-properties fo:font-weight=\"bold\" fo:color=\"#ff0000\"/>",
        "</style:style></office:styles></office:document-styles>"
    );
    let content = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
        "<office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
        "xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" office:version=\"1.2\">",
        "<office:body><office:text><text:p text:style-name=\"Fancy\">",
        "plain <text:span text:style-name=\"Bold\">kalın</text:span></text:p>",
        "</office:text></office:body></office:document-content>"
    );
    let read = read_odt(&package_with(content, Some(styles))).unwrap();
    let props = match &read.document.blocks[0] {
        Block::Paragraph { props, runs } => {
            let bold = runs.iter().find(|run| run.text == "kalın").expect("the span run");
            assert!(bold.bold, "the named text style must resolve bold: {runs:?}");
            assert_eq!(bold.color.as_deref(), Some("#FF0000"));
            props
        }
        _ => panic!("first block should be a paragraph"),
    };
    assert_eq!(props.align, "center");
    assert!(props.page_break_before);
    assert!((props.indent_left_pt - 36.0).abs() < 0.1, "indent: {}", props.indent_left_pt);
    assert_eq!(props.tabs.len(), 1);
    assert!((props.tabs[0].pos_pt - 72.0).abs() < 0.01);
}

#[test]
fn odt_watermark_roundtrip() {
    let mut document = TextDocument::new_blank("WM");
    document.blocks = vec![Block::paragraph("Body")];
    document.watermark = Some(Watermark {
        text: "GİZLİ <taslak> & \"x\"".into(),
        color: Some("#FF0000".into()),
        opacity: 0.25,
        rotation: -45.0,
        font_pt: 48.0,
        bold: false,
    });
    let bytes = write_odt(&document).unwrap();
    let reader = ZipReader::open(bytes.clone()).unwrap();
    let styles = reader.read_text("styles.xml").unwrap();
    assert!(styles.contains("style:wrap=\"run-through\" style:run-through=\"background\""), "styles: {styles}");
    assert!(styles.contains("draw:opacity=\"0.25\""), "styles: {styles}");
    assert!(styles.contains("draw:transform=\"rotate(-45)\""), "styles: {styles}");
    assert!(styles.contains("fo:color=\"#FF0000\""), "styles: {styles}");
    assert!(styles.contains("<style:header>"), "styles: {styles}");
    let meta = reader.read_text("meta.xml").unwrap();
    assert!(
        meta.contains("meta:name=\"OSAK:Watermark:Text\" meta:value=\"GİZLİ &lt;taslak&gt; &amp; &quot;x&quot;\""),
        "meta: {meta}"
    );
    assert!(meta.contains("OSAK:Watermark:Opacity"), "meta: {meta}");
    assert!(meta.contains("OSAK:Watermark:Bold"), "meta: {meta}");

    let read = read_odt(&bytes).unwrap();
    assert_eq!(read.document.watermark, document.watermark, "warnings: {:?}", read.warnings);
    // The visible frame is not imported as header content.
    assert!(read.document.header.is_empty(), "header: {:?}", read.document.header);
    assert!(read.document.plain_text().contains("Body"));
    assert!(!read.document.plain_text().contains("GİZLİ"));
}

#[test]
fn odt_watermark_color_none_roundtrips_and_defaults_visibly() {
    let mut document = TextDocument::new_blank("WM");
    document.watermark = Some(Watermark { color: None, ..Watermark::default() });
    let bytes = write_odt(&document).unwrap();
    let styles = ZipReader::open(bytes.clone()).unwrap().read_text("styles.xml").unwrap();
    // The frame still needs a visible color even when the model has none.
    assert!(styles.contains("fo:color=\"#C0C0C0\""), "styles: {styles}");
    let read = read_odt(&bytes).unwrap();
    let watermark = read.document.watermark.expect("watermark missing");
    assert_eq!(watermark.color, None);
    assert_eq!(watermark.text, "TASLAK");
    assert!(watermark.bold);
}

#[test]
fn odt_watermark_falls_back_to_a_run_through_frame() {
    let styles = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
        "<office:document-styles xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
        "xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" ",
        "xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" ",
        "xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" ",
        "xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" office:version=\"1.2\">",
        "<office:automatic-styles>",
        "<style:style style:name=\"WMG\" style:family=\"graphic\"><style:graphic-properties ",
        "style:wrap=\"run-through\" style:run-through=\"background\" draw:opacity=\"0.3\"/></style:style>",
        "<style:style style:name=\"WMT\" style:family=\"text\"><style:text-properties ",
        "fo:color=\"#FF0000\" fo:font-size=\"36pt\" fo:font-weight=\"bold\"/></style:style>",
        "</office:automatic-styles><office:master-styles>",
        "<style:master-page style:name=\"Standard\"><style:header>",
        "<draw:frame draw:name=\"Watermark\" draw:style-name=\"WMG\" draw:transform=\"rotate(-45)\">",
        "<draw:text-box><text:p><text:span text:style-name=\"WMT\">GIZLI</text:span></text:p>",
        "</draw:text-box></draw:frame></style:header></style:master-page></office:master-styles>",
        "</office:document-styles>"
    );
    let content = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
        "<office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
        "xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" office:version=\"1.2\">",
        "<office:body><office:text><text:p>Body</text:p></office:text></office:body>",
        "</office:document-content>"
    );
    let read = read_odt(&package_with(content, Some(styles))).unwrap();
    let watermark = read.document.watermark.expect("watermark missing");
    assert_eq!(watermark.text, "GIZLI");
    assert_eq!(watermark.color.as_deref(), Some("#FF0000"));
    assert!((watermark.opacity - 0.3).abs() < 1e-9);
    assert!((watermark.rotation + 45.0).abs() < 1e-9);
    assert!((watermark.font_pt - 36.0).abs() < 1e-9);
    assert!(watermark.bold);
    assert!(read.document.header.is_empty(), "header: {:?}", read.document.header);
}

#[test]
fn odt_without_a_watermark_has_none() {
    let read = read_odt(&write_odt(&sample_document()).unwrap()).unwrap();
    assert!(read.document.watermark.is_none());
}

#[test]
fn odt_hand_written_note_fixture_parses() {
    let content = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
        "<office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
        "xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" office:version=\"1.2\">",
        "<office:body><office:text><text:p>Merhaba",
        "<text:note text:id=\"ftn1\" text:note-class=\"footnote\"><text:note-citation>1</text:note-citation>",
        "<text:note-body><text:p>El yazisi dipnot</text:p></text:note-body></text:note>",
        " dunya<text:note text:id=\"edn1\" text:note-class=\"endnote\"><text:note-citation>i</text:note-citation>",
        "<text:note-body><text:p>El yazisi sonnot</text:p></text:note-body></text:note>",
        "</text:p></office:text></office:body></office:document-content>"
    );
    let mut zip = ZipWriter::new();
    zip.add_text("mimetype", "application/vnd.oasis.opendocument.text");
    zip.add_text("content.xml", content);
    let read = read_odt(&zip.finish()).unwrap();
    assert_eq!(read.document.footnotes.len(), 1);
    assert_eq!(read.document.endnotes.len(), 1);
    assert_eq!(read.document.footnotes[0].id, "ftn1");
    assert_eq!(read.document.endnotes[0].id, "edn1");
    assert!(read.document.footnotes[0].runs.iter().any(|run| run.text == "El yazisi dipnot"));
    assert!(read.document.endnotes[0].runs.iter().any(|run| run.text == "El yazisi sonnot"));
    let paragraph = match &read.document.blocks[0] {
        Block::Paragraph { runs, .. } => runs,
        _ => panic!("first block should be a paragraph"),
    };
    let text: String = paragraph.iter().map(|run| run.text.as_str()).collect();
    assert_eq!(text, "Merhaba dunya", "note bodies leaked into the paragraph: {text}");
    assert!(paragraph.iter().any(|run| run.footnote.as_deref() == Some("ftn1")));
    assert!(paragraph.iter().any(|run| run.endnote.as_deref() == Some("edn1")));
    // A citation that is not the automatic number is preserved as a marker.
    assert_eq!(read.document.footnotes[0].marker, "");
    assert_eq!(read.document.endnotes[0].marker, "i");
}

#[test]
fn ods_roundtrip_with_formulas() {
    let mut workbook = Workbook::new_blank("ODS");
    let sheet = &mut workbook.sheets[0];
    sheet.set("A1", Cell { value: CellValue::Number(10.0), ..Default::default() });
    sheet.set("A2", Cell { value: CellValue::Number(20.0), ..Default::default() });
    sheet.set("A3", Cell { value: CellValue::Number(30.0), formula: Some("=SUM(A1:A2)".into()), ..Default::default() });
    sheet.set("B1", Cell { value: CellValue::Text("metin".into()), ..Default::default() });
    let bytes = write_ods(&workbook).unwrap();
    let read = read_ods(&bytes).unwrap();
    let sheet = &read.workbook.sheets[0];
    assert_eq!(sheet.get("A1").map(|cell| cell.value.clone()), Some(CellValue::Number(10.0)));
    assert_eq!(sheet.get("B1").map(|cell| cell.value.clone()), Some(CellValue::Text("metin".into())));
    assert_eq!(sheet.get("A3").and_then(|cell| cell.formula.clone()).as_deref(), Some("=SUM(A1:A2)"));
}

#[test]
fn ods_compresses_empty_cells() {
    let mut workbook = Workbook::new_blank("Big");
    workbook.sheets[0].set("A1", Cell { value: CellValue::Text("x".into()), ..Default::default() });
    workbook.sheets[0].set("T500", Cell { value: CellValue::Number(1.0), ..Default::default() });
    let bytes = write_ods(&workbook).unwrap();
    assert!(bytes.len() < 20_000, "package was {} bytes", bytes.len());
}

#[test]
fn odp_roundtrip() {
    let mut deck = Deck::new_blank("ODP");
    let mut slide = Slide::default();
    let mut text = SlideObject::new("text", 60.0, 60.0, 400.0, 120.0);
    text.text = Some(TextFrame {
        paragraphs: vec![TextParagraph { text: "Slayt metni".into(), ..Default::default() }],
        ..Default::default()
    });
    let mut rect = SlideObject::new("rect", 40.0, 240.0, 200.0, 80.0);
    rect.style = Some(ShapeStyle { fill: Some("#1D4ED8".into()), ..Default::default() });
    slide.objects = vec![text, rect];
    slide.notes = "konuşmacı notu".into();
    deck.slides = vec![slide];
    let bytes = write_odp(&deck).unwrap();
    let read = read_odp(&bytes).unwrap();
    assert_eq!(read.deck.slides.len(), 1);
    let texts: Vec<String> =
        read.deck.slides[0].objects.iter().filter_map(|object| object.text.as_ref().map(TextFrame::plain)).collect();
    assert!(texts.iter().any(|text| text.contains("Slayt metni")));
    assert!(read.deck.slides[0].notes.contains("konuşmacı"));
}

#[test]
fn odp_rich_text_runs_bullets_and_alignment_round_trip() {
    let mut deck = Deck::new_blank("Rich");
    let mut text = SlideObject::new("text", 60.0, 60.0, 500.0, 300.0);
    text.text = Some(TextFrame {
        paragraphs: vec![
            TextParagraph {
                text: "Center".into(),
                align: "center".into(),
                runs: vec![Run { text: "Center".into(), ..Default::default() }],
                ..Default::default()
            },
            TextParagraph {
                text: "Bold red 24".into(),
                runs: vec![
                    Run { text: "Bold".into(), bold: true, ..Default::default() },
                    Run { text: " red ".into(), color: Some("#FF0000".into()), ..Default::default() },
                    Run { text: "24".into(), size_pt: Some(24.0), ..Default::default() },
                ],
                ..Default::default()
            },
            TextParagraph { text: "Top".into(), bullet: true, ..Default::default() },
            TextParagraph { text: "Nested".into(), bullet: true, level: 1, ..Default::default() },
        ],
        ..Default::default()
    });
    deck.slides = vec![Slide { objects: vec![text], ..Default::default() }];

    let bytes = write_odp(&deck).unwrap();
    let content = ZipReader::open(bytes.clone()).unwrap().read_text("content.xml").unwrap();
    assert!(content.contains("<text:span "), "content: {content}");
    assert!(content.contains("<text:list text:style-name=\"LB\">"), "content: {content}");
    assert_eq!(content.matches("<text:list-item>").count(), 2, "content: {content}");
    assert!(content.contains("fo:text-align=\"center\""), "content: {content}");
    assert!(content.contains("<text:list-style style:name=\"LB\">"), "content: {content}");

    let read = read_odp(&bytes).unwrap();
    let frame = read.deck.slides[0].objects[0].text.clone().expect("text frame");
    let paragraphs = &frame.paragraphs;
    assert_eq!(paragraphs.len(), 4, "warnings: {:?}", read.warnings);
    assert_eq!(paragraphs[0].text, "Center");
    assert_eq!(paragraphs[0].align, "center");
    assert_eq!(paragraphs[1].text, "Bold red 24");
    let bold = paragraphs[1].runs.iter().find(|run| run.text == "Bold").expect("bold run");
    assert!(bold.bold);
    let red = paragraphs[1].runs.iter().find(|run| run.text.contains("red")).expect("red run");
    assert_eq!(red.color.as_deref(), Some("#FF0000"));
    let sized = paragraphs[1].runs.iter().find(|run| run.text == "24").expect("sized run");
    assert_eq!(sized.size_pt, Some(24.0));
    assert!(paragraphs[2].bullet && paragraphs[2].level == 0, "{:?}", paragraphs[2]);
    assert!(paragraphs[3].bullet && paragraphs[3].level == 1, "{:?}", paragraphs[3]);

    // A second cycle is stable.
    let again = read_odp(&write_odp(&read.deck).unwrap()).unwrap();
    assert_eq!(again.deck.slides[0].objects[0].text, read.deck.slides[0].objects[0].text);
}

#[test]
fn odp_hidden_slides_round_trip() {
    let mut deck = Deck::new_blank("Hidden");
    deck.slides = vec![Slide::default(), Slide { hidden: true, ..Default::default() }];
    let bytes = write_odp(&deck).unwrap();
    let content = ZipReader::open(bytes.clone()).unwrap().read_text("content.xml").unwrap();
    assert!(content.contains("presentation:visibility=\"hidden\""), "content: {content}");
    assert_eq!(
        content.matches("draw:style-name=\"dp-hidden\"").count(),
        1,
        "only the hidden slide references the style: {content}"
    );

    let read = read_odp(&bytes).unwrap();
    assert!(!read.deck.slides[0].hidden);
    assert!(read.deck.slides[1].hidden);

    // A deck without hidden slides does not declare the style at all.
    let visible = write_odp(&Deck::new_blank("Visible")).unwrap();
    let content = ZipReader::open(visible).unwrap().read_text("content.xml").unwrap();
    assert!(!content.contains("dp-hidden"), "content: {content}");
}

fn odp_chart_deck() -> Deck {
    let mut column = SlideObject::new("chart", 60.0, 60.0, 480.0, 288.0);
    column.id = "chart-column".into();
    column.chart = Some(ChartData {
        kind: "column".into(),
        title: "Sales".into(),
        categories: "A2:A4".into(),
        series: vec![ChartSeries { name: "North".into(), range: "B2:B4".into(), color: Some("#1D4ED8".into()) }],
        legend: true,
        x_title: "Quarter".into(),
        y_title: "Units".into(),
        show_labels: true,
        categories_cache: vec!["Q1".into(), "Q2".into(), "Q3".into()],
        series_values_cache: vec![vec![10.0, 20.5, 31.0]],
        ..Default::default()
    });
    let mut pie = SlideObject::new("chart", 60.0, 380.0, 320.0, 240.0);
    pie.id = "chart-pie".into();
    pie.chart = Some(ChartData {
        kind: "pie".into(),
        title: "Share".into(),
        categories: "A2:A4".into(),
        series: vec![ChartSeries { name: "North".into(), range: "B2:B4".into(), color: None }],
        categories_cache: vec!["Q1".into(), "Q2".into(), "Q3".into()],
        series_values_cache: vec![vec![10.0, 20.5, 31.0]],
        ..Default::default()
    });
    let mut deck = Deck::new_blank("Charts");
    deck.slides = vec![Slide { objects: vec![column, pie], ..Default::default() }];
    deck
}

#[test]
fn odp_charts_are_written_as_objects_and_round_trip() {
    let deck = odp_chart_deck();
    let write = write_odp_package(&deck).unwrap();
    assert!(write.warnings.is_empty(), "supported charts need no warning: {:?}", write.warnings);
    let reader = ZipReader::open(write.bytes.clone()).unwrap();
    let manifest = reader.read_text("META-INF/manifest.xml").unwrap();
    assert!(manifest.contains(
            "manifest:full-path=\"Object 1/\" manifest:version=\"1.2\" manifest:media-type=\"application/vnd.oasis.opendocument.chart\""
        ), "manifest: {manifest}");
    for object in ["Object 1", "Object 2"] {
        for part in ["content.xml", "styles.xml"] {
            let path = format!("{object}/{part}");
            assert!(manifest.contains(&format!("manifest:full-path=\"{path}\"")), "{path} is missing");
            assert!(reader.contains(&path), "{path} is present");
        }
    }
    let content = reader.read_text("content.xml").unwrap();
    assert!(content.contains("xlink:href=\"./Object 1\""));
    assert!(content.contains("xlink:href=\"./Object 2\""));
    let chart = reader.read_text("Object 1/content.xml").unwrap();
    assert!(chart.contains("chart:class=\"chart:bar\""));
    assert!(chart.contains("<table:table table:name=\"local-table\">"));
    assert!(reader.read_text("Object 2/content.xml").unwrap().contains("chart:class=\"chart:circle\""));

    let read = read_odp(&write.bytes).unwrap();
    let objects = &read.deck.slides[0].objects;
    let column = objects.iter().find(|object| object.id == "chart-column").expect("column chart");
    assert_eq!(column.kind, "chart");
    assert_eq!(column.chart.as_ref(), deck.slides[0].objects[0].chart.as_ref());
    let pie = objects.iter().find(|object| object.id == "chart-pie").expect("pie chart");
    assert_eq!(pie.chart.as_ref(), deck.slides[0].objects[1].chart.as_ref());

    // A second cycle keeps the charts unchanged.
    let again = read_odp(&write_odp(&read.deck).unwrap()).unwrap();
    let charts: Vec<&ChartData> =
        again.deck.slides[0].objects.iter().filter_map(|object| object.chart.as_ref()).collect();
    assert_eq!(charts, objects.iter().filter_map(|object| object.chart.as_ref()).collect::<Vec<_>>());
}

#[test]
fn odp_unsupported_charts_warn_and_keep_a_placeholder() {
    let mut deck = odp_chart_deck();
    deck.slides[0].objects[1].chart.as_mut().unwrap().kind = "radar".into();
    let write = write_odp_package(&deck).unwrap();
    assert!(
            write
                .warnings
                .iter()
                .any(|warning| warning.contains("Chart data is kept in the native .oswk file")
                    && warning.contains("radar")),
            "warnings: {:?}",
            write.warnings
        );
    let content = ZipReader::open(write.bytes.clone()).unwrap().read_text("content.xml").unwrap();
    assert!(!content.contains("xlink:href=\"./Object 2\""));
    assert!(content.contains("Share"), "the unsupported chart keeps its title placeholder: {content}");
    // The placeholder comes back as a text shape; the chart object itself
    // stays in the native .oswk file, which is what the warning says.
    let read = read_odp(&write.bytes).unwrap();
    let placeholder = read.deck.slides[0]
        .objects
        .iter()
        .find(|object| object.text.as_ref().map(TextFrame::plain).as_deref() == Some("Share"))
        .expect("chart placeholder");
    assert!(placeholder.chart.is_none());
}

#[test]
fn odp_connectors_round_trip_with_glue_points() {
    let mut deck = Deck::new_blank("Connectors");
    let mut source = SlideObject::new("rect", 100.0, 100.0, 120.0, 80.0);
    source.id = "from-rect".into();
    source.z = 1;
    let mut connector = SlideObject::new("line", 220.0, 140.0, 180.0, 160.0);
    connector.id = "conn".into();
    connector.z = 2;
    connector.line = Some(LineSpec {
        x2: 180.0,
        y2: 160.0,
        begin_object: Some("from-rect".into()),
        end_object: Some("to-rect".into()),
        begin_site: 3,
        end_site: 1,
        ..Default::default()
    });
    let mut target = SlideObject::new("rect", 400.0, 300.0, 120.0, 80.0);
    target.id = "to-rect".into();
    target.z = 3;
    deck.slides = vec![Slide { objects: vec![source, connector, target], ..Default::default() }];

    let bytes = write_odp(&deck).unwrap();
    let content = ZipReader::open(bytes.clone()).unwrap().read_text("content.xml").unwrap();
    assert!(content.contains("<draw:connector "), "content: {content}");
    assert!(content.contains("draw:start-shape=\"from-rect\" draw:start-glue-point=\"3\""), "content: {content}");
    assert!(content.contains("draw:end-shape=\"to-rect\" draw:end-glue-point=\"1\""), "content: {content}");

    let read = read_odp(&bytes).unwrap();
    let connector = read.deck.slides[0].objects.iter().find(|object| object.id == "conn").expect("connector");
    assert_eq!(connector.kind, "line");
    let line = connector.line.clone().expect("line spec");
    assert_eq!(line.begin_object.as_deref(), Some("from-rect"));
    assert_eq!(line.end_object.as_deref(), Some("to-rect"));
    assert_eq!(line.begin_site, 3);
    assert_eq!(line.end_site, 1);
    assert!((connector.x - 220.0).abs() < 0.05 && (connector.y - 140.0).abs() < 0.05, "{connector:?}");
    assert!((line.x2 - 180.0).abs() < 0.05 && (line.y2 - 160.0).abs() < 0.05, "{line:?}");
}

#[test]
fn odp_image_crop_round_trips_within_tolerance() {
    let mut deck = Deck::new_blank("Crop");
    let mut object = SlideObject::new("image", 60.0, 60.0, 400.0, 300.0);
    let mut image = ImageData::from_bytes("crop.png", &sample_png());
    image.crop = Some(ImageCrop { left: 0.1, top: 0.2, right: 0.05, bottom: 0.15 });
    object.image = Some(image);
    deck.slides = vec![Slide { objects: vec![object], ..Default::default() }];

    let bytes = write_odp(&deck).unwrap();
    let content = ZipReader::open(bytes.clone()).unwrap().read_text("content.xml").unwrap();
    assert!(content.contains("fo:clip=\"rect("), "content: {content}");

    let read = read_odp(&bytes).unwrap();
    let crop = read.deck.slides[0].objects[0].image.as_ref().unwrap().crop.clone().expect("crop");
    assert!((crop.left - 0.1).abs() < 0.01, "{crop:?}");
    assert!((crop.top - 0.2).abs() < 0.01, "{crop:?}");
    assert!((crop.right - 0.05).abs() < 0.01, "{crop:?}");
    assert!((crop.bottom - 0.15).abs() < 0.01, "{crop:?}");

    // An uncropped image writes no clip at all.
    let mut plain = deck.clone();
    plain.slides[0].objects[0].image.as_mut().unwrap().crop = None;
    let content = ZipReader::open(write_odp(&plain).unwrap()).unwrap().read_text("content.xml").unwrap();
    assert!(!content.contains("fo:clip"), "content: {content}");
}

#[test]
fn odp_footer_slide_number_and_date_round_trip() {
    let mut deck = Deck::new_blank("Footer");
    deck.footer = Some(SlideFooter {
        enabled: true,
        text: "Alt bilgi".into(),
        show_text: true,
        show_slide_number: true,
        show_date: true,
        date_text: "2026-10-10".into(),
    });
    let bytes = write_odp(&deck).unwrap();
    let styles = ZipReader::open(bytes.clone()).unwrap().read_text("styles.xml").unwrap();
    assert!(styles.contains("presentation:display-footer=\"true\""), "styles: {styles}");
    assert!(styles.contains("presentation:display-page-number=\"true\""), "styles: {styles}");
    assert!(styles.contains("presentation:display-date-time=\"true\""), "styles: {styles}");
    assert!(styles.contains("presentation:class=\"footer\""), "styles: {styles}");
    assert!(styles.contains("presentation:class=\"page-number\""), "styles: {styles}");
    assert!(styles.contains("<text:page-number>"), "styles: {styles}");
    assert!(styles.contains("2026-10-10"), "styles: {styles}");

    let read = read_odp(&bytes).unwrap();
    let footer = read.deck.footer.clone().expect("footer");
    assert!(footer.enabled && footer.show_text && footer.show_slide_number && footer.show_date);
    assert_eq!(footer.text, "Alt bilgi");
    assert_eq!(footer.date_text, "2026-10-10");

    // A second cycle keeps the footer.
    assert_eq!(read_odp(&write_odp(&read.deck).unwrap()).unwrap().deck.footer, read.deck.footer);

    // A deck without a footer reads back without one.
    let plain = read_odp(&write_odp(&Deck::new_blank("No footer")).unwrap()).unwrap();
    assert!(plain.deck.footer.is_none());
}

#[test]
fn malformed_inputs_are_errors() {
    assert!(read_odt(b"").is_err());
    assert!(read_ods(&[0u8; 16]).is_err());
    assert!(read_odp(&[1u8; 32]).is_err());
}

#[test]
fn formula_conversion_both_ways() {
    assert_eq!(odf_formula_to_ours("of:=SUM([.A1:.A2])"), "=SUM(A1:A2)");
    assert_eq!(our_formula_to_odf("=SUM(A1:B2)+1"), "of:=SUM([.A1:.B2])+1");
    assert_eq!(odf_formula_to_ours("of:=[.A1]*2"), "=A1*2");
}

#[test]
fn condition_formulas_convert_references_and_separators() {
    let ours = "AND($D1>4,MOD($D1,2)=0,Data!$B1<3,'My Sheet'!A1:B2<>\"a,b A1\")";
    let odf = "AND([.$D1]>4;MOD([.$D1];2)=0;[$Data.$B1]<3;[$'My Sheet'.A1:.B2]<>\"a,b A1\")";
    assert_eq!(formula_to_ods(ours), odf);
    assert_eq!(formula_from_ods(odf), ours);
    // Function names with digits and names that end in digits are not references.
    assert_eq!(formula_to_ods("LOG10(Rate1)+SUM(A1:A3)"), "LOG10(Rate1)+SUM([.A1:.A3])");
    assert_eq!(formula_from_ods("LOG10(Rate1)+SUM([.A1:.A3])"), "LOG10(Rate1)+SUM(A1:A3)");
    assert_eq!(formula_to_ods("1.5*A1"), "1.5*[.A1]");
}

#[test]
fn condition_arguments_split_outside_quotes_and_parentheses() {
    assert_eq!(super::sheets_read::split_ods_args("1,9"), ["1", "9"]);
    assert_eq!(super::sheets_read::split_ods_args("\"a,b\";MAX(1;2)"), ["\"a,b\"", "MAX(1;2)"]);
    assert_eq!(super::sheets_read::unquote_ods("\"say \"\"hi\"\"\""), "say \"hi\"");
}
