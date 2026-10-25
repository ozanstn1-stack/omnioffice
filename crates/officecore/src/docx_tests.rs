use super::*;

fn sample_png() -> Vec<u8> {
    let mut buffer = image::RgbaImage::new(4, 4);
    for pixel in buffer.pixels_mut() {
        *pixel = image::Rgba([200, 30, 60, 255]);
    }
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(buffer).write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn sample_document() -> TextDocument {
    let mut document = TextDocument::new_blank("Sample");
    document.blocks = vec![
        Block::heading("Introduction", 1),
        Block::Paragraph {
            props: ParaProps::default(),
            runs: vec![
                Run { text: "Hello ".into(), ..Default::default() },
                Run { text: "bold".into(), bold: true, ..Default::default() },
                Run { text: " and ".into(), ..Default::default() },
                Run { text: "italic".into(), italic: true, color: Some("#FF0000".into()), ..Default::default() },
            ],
        },
        Block::Paragraph {
            props: ParaProps {
                list: Some(ListInfo { kind: "bullet".into(), level: 0, start: 1, marker: "•".into() }),
                ..Default::default()
            },
            runs: vec![Run { text: "first".into(), ..Default::default() }],
        },
        Block::Paragraph {
            props: ParaProps {
                list: Some(ListInfo { kind: "number".into(), level: 0, start: 1, marker: "1.".into() }),
                ..Default::default()
            },
            runs: vec![Run { text: "second".into(), ..Default::default() }],
        },
        Block::Image {
            image: ImageData::from_bytes("pic.png", &sample_png()),
            width_pt: 120.0,
            height_pt: 90.0,
            align: "center".into(),
            caption: String::new(),
            wrap: "inline".into(),
        },
    ];
    document.header = vec![Block::paragraph("Header text")];
    document.footer = vec![Block::paragraph("Footer text")];
    document
}

#[test]
fn roundtrip_paragraphs_and_lists() {
    let document = sample_document();
    let bytes = write_docx(&document).unwrap();
    let read = read_docx(&bytes).unwrap();
    assert!(read.document.blocks.len() >= 5);
    let text = read.document.plain_text();
    assert!(text.contains("Introduction"));
    assert!(text.contains("bold"));
    let bold = read.document.blocks.iter().find_map(|block| match block {
        Block::Paragraph { runs, .. } => runs.iter().find(|run| run.text == "bold"),
        _ => None,
    });
    assert!(bold.map(|run| run.bold).unwrap_or(false));
    let lists: Vec<_> = read
        .document
        .blocks
        .iter()
        .filter_map(|block| match block {
            Block::Paragraph { props, .. } => props.list.as_ref(),
            _ => None,
        })
        .collect();
    assert!(lists.iter().any(|list| list.kind == "bullet"));
    assert!(lists.iter().any(|list| list.kind == "number"));
}

#[test]
fn adjacent_numbered_lists_get_their_own_numbering() {
    let numbered = |text: &str, start: u32| Block::Paragraph {
        props: ParaProps {
            list: Some(ListInfo { kind: "number".into(), level: 0, start, marker: "1.".into() }),
            ..Default::default()
        },
        runs: vec![Run { text: text.into(), ..Default::default() }],
    };
    let mut document = TextDocument::new_blank("Lists");
    document.blocks = vec![numbered("one", 1), numbered("two", 1), numbered("three", 5), numbered("four", 5)];
    let bytes = write_docx(&document).unwrap();

    // Each list owns a `w:num` with its own start, so Word restarts the
    // second list instead of continuing the first at 3.
    let reader = ZipReader::open(bytes.clone()).unwrap();
    let numbering = reader.read_text("word/numbering.xml").unwrap();
    assert!(
        numbering.contains("<w:num w:numId=\"3\"><w:abstractNumId w:val=\"1\"/><w:lvlOverride w:ilvl=\"0\"><w:startOverride w:val=\"1\"/></w:lvlOverride></w:num>"),
        "{numbering}"
    );
    assert!(
        numbering.contains("<w:num w:numId=\"4\"><w:abstractNumId w:val=\"1\"/><w:lvlOverride w:ilvl=\"0\"><w:startOverride w:val=\"5\"/></w:lvlOverride></w:num>"),
        "{numbering}"
    );
    let body = reader.read_text("word/document.xml").unwrap();
    assert!(body.contains("<w:numId w:val=\"3\"/>"), "{body}");
    assert!(body.contains("<w:numId w:val=\"4\"/>"), "{body}");

    let read = read_docx(&bytes).unwrap();
    let items: Vec<&ParaProps> = read
        .document
        .blocks
        .iter()
        .filter_map(|block| match block {
            Block::Paragraph { props, .. } => props.list.as_ref().filter(|list| list.kind == "number").map(|_| props),
            _ => None,
        })
        .collect();
    assert_eq!(items.len(), 4, "warnings: {:?}", read.warnings);
}

#[test]
fn roundtrip_table() {
    let mut document = TextDocument::new_blank("Table");
    let mut table = TableData::simple(2, 3, 450.0);
    table.rows[0].cells[0].blocks = vec![Block::paragraph("Name")];
    table.rows[1].cells[2].blocks = vec![Block::paragraph("42")];
    document.blocks = vec![Block::Table { table }];
    let bytes = write_docx(&document).unwrap();
    let read = read_docx(&bytes).unwrap();
    let table = read
        .document
        .blocks
        .iter()
        .find_map(|block| match block {
            Block::Table { table } => Some(table),
            _ => None,
        })
        .expect("table missing");
    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.rows[0].cells.len(), 3);
    assert!(table.rows[0].cells[0].blocks[0].plain_text().contains("Name"));
    assert!(table.rows[1].cells[2].blocks[0].plain_text().contains("42"));
}

#[test]
fn roundtrip_image() {
    let document = sample_document();
    let bytes = write_docx(&document).unwrap();
    let read = read_docx(&bytes).unwrap();
    let image = read.document.blocks.iter().find_map(|block| match block {
        Block::Image { image, .. } => Some(image.clone()),
        _ => None,
    });
    assert!(image.is_some(), "image missing; warnings: {:?}", read.warnings);
    let image = image.unwrap();
    assert_eq!(image.mime, "image/png");
    assert!(!image.data_base64.is_empty());
}

#[test]
fn roundtrip_page_setup() {
    let mut document = TextDocument::new_blank("Page");
    document.page = PageSetup::from_preset("letter", "landscape");
    document.page.columns = 2;
    let bytes = write_docx(&document).unwrap();
    let read = read_docx(&bytes).unwrap();
    assert_eq!(read.document.page.orientation, "landscape");
    assert_eq!(read.document.page.size, "letter");
    assert_eq!(read.document.page.columns, 2);
}

#[test]
fn roundtrip_header_footer() {
    let document = sample_document();
    let bytes = write_docx(&document).unwrap();
    let read = read_docx(&bytes).unwrap();
    assert!(read.document.header.iter().map(Block::plain_text).any(|text| text.contains("Header")));
    assert!(read.document.footer.iter().map(Block::plain_text).any(|text| text.contains("Footer")));
}

#[test]
fn roundtrip_hyperlink() {
    let mut document = TextDocument::new_blank("Links");
    document.blocks = vec![Block::Paragraph {
        props: ParaProps::default(),
        runs: vec![Run { text: "Open".into(), link: Some("https://example.com".into()), ..Default::default() }],
    }];
    let bytes = write_docx(&document).unwrap();
    let read = read_docx(&bytes).unwrap();
    let link = read.document.blocks.iter().find_map(|block| match block {
        Block::Paragraph { runs, .. } => runs.iter().find_map(|run| run.link.clone()),
        _ => None,
    });
    assert_eq!(link.as_deref(), Some("https://example.com"));
}

#[test]
fn reads_minimal_hand_built_package() {
    let mut zip = ZipWriter::new();
    zip.add_text(
        "[Content_Types].xml",
        r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"/>"#,
    );
    zip.add_text("_rels/.rels", r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#);
    zip.add_text("word/document.xml", r#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Minimal</w:t></w:r></w:p><w:p><w:r><w:rPr><w:b/></w:rPr><w:t>Bold</w:t></w:r></w:p></w:body></w:document>"#);
    let read = read_docx(&zip.finish()).unwrap();
    assert_eq!(read.document.plain_text().trim(), "Minimal\nBold");
}

#[test]
fn malformed_inputs_never_panic() {
    assert!(read_docx(&[]).is_err());
    assert!(read_docx(&[0u8; 64]).is_err());
    let mut zip = ZipWriter::new();
    zip.add_text("word/document.xml", "<w:document><broken>");
    // Truncated XML is tolerated (content is best effort), but must not panic.
    let _ = read_docx(&zip.finish());
}

#[test]
fn package_structure() {
    let bytes = write_docx(&sample_document()).unwrap();
    assert!(bytes.len() > 2000);
    let reader = ZipReader::open(bytes).unwrap();
    assert!(reader.contains("[Content_Types].xml"));
    assert!(reader.contains("word/document.xml"));
    assert!(reader.contains("word/styles.xml"));
    assert!(reader.contains("word/numbering.xml"));
    assert!(reader.contains("word/footer1.xml"));
}

#[test]
fn roundtrip_sections_with_headers() {
    let mut document = TextDocument::new_blank("Sections");
    document.page = PageSetup::from_preset("a4", "portrait");
    document.header = vec![Block::paragraph("First section header")];
    document.blocks = vec![
        Block::paragraph("Section one body"),
        Block::SectionBreak {
            section: SectionProps {
                page: PageSetup::from_preset("a5", "landscape"),
                header: vec![Block::paragraph("Second section header")],
                start: "newPage".into(),
                ..Default::default()
            },
        },
        Block::paragraph("Section two body"),
        Block::SectionBreak {
            section: SectionProps {
                page: PageSetup::from_preset("letter", "portrait"),
                start: "oddPage".into(),
                ..Default::default()
            },
        },
        Block::paragraph("Section three body"),
    ];
    let bytes = write_docx(&document).unwrap();
    let reader = ZipReader::open(bytes.clone()).unwrap();
    assert!(reader.contains("word/header1.xml"));
    assert!(reader.contains("word/header2.xml"));
    let read = read_docx(&bytes).unwrap();
    let breaks: Vec<&SectionProps> = read
        .document
        .blocks
        .iter()
        .filter_map(|block| match block {
            Block::SectionBreak { section } => Some(section),
            _ => None,
        })
        .collect();
    assert_eq!(breaks.len(), 2, "warnings: {:?}", read.warnings);
    assert_eq!(breaks[0].page.size, "a5");
    assert_eq!(breaks[0].page.orientation, "landscape");
    assert!(breaks[0].header.iter().map(Block::plain_text).any(|text| text.contains("Second section")));
    assert_eq!(breaks[1].page.size, "letter");
    assert_eq!(breaks[1].start, "oddPage");
    assert_eq!(read.document.page.size, "a4");
    assert!(read.document.header.iter().map(Block::plain_text).any(|text| text.contains("First section")));
    // The section body order is preserved.
    let text = read.document.plain_text();
    assert!(text.find("Section one").unwrap() < text.find("Section two").unwrap());
    assert!(text.find("Section two").unwrap() < text.find("Section three").unwrap());
}

#[test]
fn roundtrip_footnotes_and_endnotes() {
    let mut document = TextDocument::new_blank("Notes");
    document.footnotes = vec![
        Footnote {
            id: "fn-a".into(),
            runs: vec![Run { text: "First note".into(), ..Default::default() }],
            marker: String::new(),
        },
        Footnote {
            id: "fn-b".into(),
            runs: vec![Run { text: "Second note".into(), ..Default::default() }],
            marker: String::new(),
        },
    ];
    document.endnotes = vec![Footnote {
        id: "en-a".into(),
        runs: vec![Run { text: "End note".into(), ..Default::default() }],
        marker: String::new(),
    }];
    document.blocks = vec![Block::Paragraph {
        props: ParaProps::default(),
        runs: vec![
            Run { text: "one".into(), footnote: Some("fn-a".into()), ..Default::default() },
            Run { text: " two".into(), footnote: Some("fn-b".into()), ..Default::default() },
            Run { text: " end".into(), endnote: Some("en-a".into()), ..Default::default() },
        ],
    }];
    let bytes = write_docx(&document).unwrap();
    let reader = ZipReader::open(bytes.clone()).unwrap();
    assert!(reader.contains("word/footnotes.xml"));
    assert!(reader.contains("word/endnotes.xml"));
    let read = read_docx(&bytes).unwrap();
    assert_eq!(read.document.footnotes.len(), 2, "warnings: {:?}", read.warnings);
    assert_eq!(read.document.endnotes.len(), 1);
    assert!(read.document.footnotes[0].runs.iter().any(|run| run.text.contains("First note")));
    assert!(read.document.footnotes[1].runs.iter().any(|run| run.text.contains("Second note")));
    assert!(read.document.endnotes[0].runs.iter().any(|run| run.text.contains("End note")));
    // Ids are regenerated from the Word numbering, but the automatic
    // numbering follows reference order.
    let first_id = read.document.footnotes[0].id.clone();
    let second_id = read.document.footnotes[1].id.clone();
    assert_eq!(read.document.footnote_number(&first_id), Some(1));
    assert_eq!(read.document.footnote_number(&second_id), Some(2));
    let references: Vec<String> = read
        .document
        .blocks
        .iter()
        .filter_map(|block| match block {
            Block::Paragraph { runs, .. } => {
                Some(runs.iter().filter_map(|run| run.footnote.clone()).collect::<Vec<_>>())
            }
            _ => None,
        })
        .flatten()
        .collect();
    assert_eq!(references.len(), 2);
}

#[test]
fn roundtrip_tracked_changes() {
    let mut document = TextDocument::new_blank("Review");
    document.track_changes = true;
    let mut format_run = Run { text: "reformatted".into(), bold: true, ..Default::default() };
    format_run.revision = Some(RevisionMark {
        id: "f1".into(),
        kind: "format".into(),
        author: "Ada".into(),
        date: "2026-02-02T10:00:00Z".into(),
        original: Some(Run::default().format_snapshot()),
    });
    document.blocks = vec![Block::Paragraph {
        props: ParaProps::default(),
        runs: vec![
            Run { text: "kept ".into(), ..Default::default() },
            Run {
                text: "inserted".into(),
                revision: Some(RevisionMark {
                    id: "i1".into(),
                    kind: "insert".into(),
                    author: "Ada".into(),
                    date: "2026-02-02T10:00:00Z".into(),
                    original: None,
                }),
                ..Default::default()
            },
            Run {
                text: "removed".into(),
                revision: Some(RevisionMark {
                    id: "d1".into(),
                    kind: "delete".into(),
                    author: "Grace".into(),
                    date: "2026-02-02T11:00:00Z".into(),
                    original: None,
                }),
                ..Default::default()
            },
            format_run,
        ],
    }];
    let bytes = write_docx(&document).unwrap();
    let read = read_docx(&bytes).unwrap();
    assert!(read.document.track_changes);
    let revisions = crate::revisions::revision_list(&read.document);
    assert_eq!(revisions.len(), 3, "warnings: {:?}", read.warnings);
    assert!(revisions.iter().any(|revision| revision.kind == "insert" && revision.author == "Ada"));
    assert!(revisions.iter().any(|revision| revision.kind == "delete" && revision.text.contains("removed")));
    let format = revisions.iter().find(|revision| revision.kind == "format").expect("format revision missing");
    assert_eq!(format.text, "reformatted");

    // Accepting everything yields a clean document.
    let mut accepted = read.document.clone();
    crate::revisions::accept_all(&mut accepted);
    assert_eq!(crate::revisions::revision_count(&accepted), 0);
    assert!(accepted.plain_text().contains("inserted"));
    assert!(!accepted.plain_text().contains("removed"));
    // Rejecting everything restores the original text.
    let mut rejected = read.document;
    crate::revisions::reject_all(&mut rejected);
    assert!(!rejected.plain_text().contains("inserted"));
    assert!(rejected.plain_text().contains("removed"));
}

#[test]
fn roundtrip_comments_with_replies() {
    let mut document = TextDocument::new_blank("Comments");
    document.comments = vec![Comment {
        id: "c1".into(),
        author: "Ada".into(),
        text: "Please verify this number".into(),
        created: "2026-03-03T09:00:00Z".into(),
        resolved: false,
        modified: "2026-03-03T09:00:00Z".into(),
        replies: vec![CommentReply {
            author: "Grace".into(),
            text: "Checked".into(),
            created: "2026-03-03T10:00:00Z".into(),
        }],
    }];
    document.blocks = vec![Block::Paragraph {
        props: ParaProps::default(),
        runs: vec![Run { text: "Total: 42".into(), comment: Some("c1".into()), ..Default::default() }],
    }];
    let bytes = write_docx(&document).unwrap();
    let reader = ZipReader::open(bytes.clone()).unwrap();
    assert!(reader.contains("word/comments.xml"));
    let read = read_docx(&bytes).unwrap();
    assert_eq!(read.document.comments.len(), 1, "warnings: {:?}", read.warnings);
    assert!(read.document.comments[0].text.contains("verify"));
    assert_eq!(read.document.comments[0].author, "Ada", "comment author survives the round trip");
    assert_eq!(read.document.comments[0].replies.len(), 1);
    assert!(read.document.comments[0].replies[0].text.contains("Checked"));
    assert_eq!(read.document.comments[0].replies[0].author, "Grace", "reply author survives the round trip");
    let anchored = read.document.blocks.iter().find_map(|block| match block {
        Block::Paragraph { runs, .. } => runs.iter().find_map(|run| run.comment.clone()),
        _ => None,
    });
    assert!(anchored.is_some());
}

#[test]
fn roundtrip_cross_reference_fields() {
    let mut document = TextDocument::new_blank("Fields");
    document.bookmarks = vec![Bookmark { id: "b1".into(), name: "Target".into(), block: 0, offset: 0 }];
    document.blocks = vec![Block::Paragraph {
        props: ParaProps::default(),
        runs: vec![
            Run { text: "See page ".into(), ..Default::default() },
            Run {
                text: "3".into(),
                field: Some(FieldRef { kind: "refPage".into(), target: "Target".into(), cached: "3".into() }),
                ..Default::default()
            },
            Run { text: " and ".into(), ..Default::default() },
            Run {
                text: "Target".into(),
                field: Some(FieldRef { kind: "ref".into(), target: "Target".into(), cached: "Target".into() }),
                ..Default::default()
            },
        ],
    }];
    let bytes = write_docx(&document).unwrap();
    let read = read_docx(&bytes).unwrap();
    let fields: Vec<&FieldRef> = read
        .document
        .blocks
        .iter()
        .filter_map(|block| match block {
            Block::Paragraph { runs, .. } => Some(runs.iter().filter_map(|run| run.field.as_ref()).collect::<Vec<_>>()),
            _ => None,
        })
        .flatten()
        .collect();
    assert_eq!(fields.len(), 2, "warnings: {:?}", read.warnings);
    assert!(fields.iter().any(|field| field.kind == "refPage" && field.target == "Target"));
    assert!(fields.iter().any(|field| field.kind == "ref" && field.target == "Target"));
}

#[test]
fn roundtrip_table_spanning_cells() {
    let mut document = TextDocument::new_blank("Spans");
    let mut table = TableData::simple(3, 3, 450.0);
    table.rows[0].cells = vec![
        TableCell { blocks: vec![Block::paragraph("Span")], colspan: 2, rowspan: 2, ..Default::default() },
        TableCell { blocks: vec![Block::paragraph("B")], ..Default::default() },
    ];
    table.rows[1].cells = vec![TableCell { blocks: vec![Block::paragraph("C")], ..Default::default() }];
    document.blocks = vec![Block::Table { table }];
    let bytes = write_docx(&document).unwrap();
    let reader = ZipReader::open(bytes.clone()).unwrap();
    let xml = reader.read_text("word/document.xml").unwrap();
    assert!(xml.contains("<w:vMerge w:val=\"restart\"/>"), "xml: {xml}");
    assert!(xml.contains("<w:vMerge/>"), "xml: {xml}");
    assert!(xml.contains("<w:tc><w:tcPr><w:tcW w:w=\"3000\" w:type=\"dxa\"/><w:vMerge/></w:tcPr>"), "xml: {xml}");
    assert_eq!(xml.matches("<w:gridCol").count(), 3, "xml: {xml}");
    let read = read_docx(&bytes).unwrap();
    let table = read
        .document
        .blocks
        .iter()
        .find_map(|block| match block {
            Block::Table { table } => Some(table),
            _ => None,
        })
        .expect("table missing");
    assert_eq!(table.rows.len(), 3, "warnings: {:?}", read.warnings);
    assert_eq!(table.rows[0].cells.len(), 2);
    assert_eq!(table.rows[0].cells[0].colspan, 2);
    assert_eq!(table.rows[0].cells[0].rowspan, 2);
    assert_eq!(table.rows[0].cells[0].blocks[0].plain_text(), "Span");
    assert_eq!(table.rows[1].cells.len(), 1);
    assert_eq!(table.rows[1].cells[0].blocks[0].plain_text(), "C");
    assert_eq!(table.rows[2].cells.len(), 3);
}

#[test]
fn roundtrip_image_wrapping() {
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
    let bytes = write_docx(&document).unwrap();
    let reader = ZipReader::open(bytes.clone()).unwrap();
    let xml = reader.read_text("word/document.xml").unwrap();
    assert!(xml.contains("<wp:wrapSquare wrapText=\"bothSides\"/>"), "xml: {xml}");
    assert!(xml.contains("<wp:wrapTopAndBottom/>"), "xml: {xml}");
    assert!(xml.contains("<wp:anchor "), "xml: {xml}");
    assert!(xml.contains("<wp:inline "), "xml: {xml}");
    assert!(
        xml.contains("<wp:positionH relativeFrom=\"column\"><wp:align>right</wp:align></wp:positionH>"),
        "xml: {xml}"
    );
    assert!(
        xml.contains("<wp:positionV relativeFrom=\"paragraph\"><wp:align>top</wp:align></wp:positionV>"),
        "xml: {xml}"
    );
    let read = read_docx(&bytes).unwrap();
    let images: Vec<(&String, &String)> = read
        .document
        .blocks
        .iter()
        .filter_map(|block| match block {
            Block::Image { align, wrap, .. } => Some((align, wrap)),
            _ => None,
        })
        .collect();
    assert_eq!(images.len(), 3, "warnings: {:?}", read.warnings);
    assert_eq!((images[0].0.as_str(), images[0].1.as_str()), ("right", "square"));
    assert_eq!((images[1].0.as_str(), images[1].1.as_str()), ("left", "topBottom"));
    assert_eq!((images[2].0.as_str(), images[2].1.as_str()), ("center", "inline"));
}

#[test]
fn roundtrip_tab_stops() {
    let mut document = TextDocument::new_blank("Tabs");
    document.blocks = vec![Block::Paragraph {
        props: ParaProps {
            tabs: vec![
                TabStop { pos_pt: 36.0, align: "center".into() },
                TabStop { pos_pt: 144.0, align: "right".into() },
                TabStop { pos_pt: 72.0, align: "decimal".into() },
                TabStop { pos_pt: 10.0, align: "bar".into() },
            ],
            ..Default::default()
        },
        runs: vec![Run { text: "a\tb".into(), ..Default::default() }],
    }];
    let bytes = write_docx(&document).unwrap();
    let reader = ZipReader::open(bytes.clone()).unwrap();
    let xml = reader.read_text("word/document.xml").unwrap();
    assert!(xml.contains("<w:tabs>"), "xml: {xml}");
    assert!(xml.contains("<w:tab w:val=\"center\" w:pos=\"720\"/>"), "xml: {xml}");
    assert!(xml.contains("<w:tab w:val=\"right\" w:pos=\"2880\"/>"), "xml: {xml}");
    assert!(xml.contains("<w:tab w:val=\"decimal\" w:pos=\"1440\"/>"), "xml: {xml}");
    assert!(!xml.contains("w:val=\"bar\""), "xml: {xml}");
    let read = read_docx(&bytes).unwrap();
    let props = read
        .document
        .blocks
        .iter()
        .find_map(|block| match block {
            Block::Paragraph { props, .. } => Some(props),
            _ => None,
        })
        .expect("paragraph missing");
    assert_eq!(
        props.tabs,
        vec![
            TabStop { pos_pt: 36.0, align: "center".into() },
            TabStop { pos_pt: 144.0, align: "right".into() },
            TabStop { pos_pt: 72.0, align: "decimal".into() },
        ],
        "warnings: {:?}",
        read.warnings
    );
}

#[test]
fn roundtrip_watermark() {
    let mut document = TextDocument::new_blank("Watermark");
    document.watermark = Some(Watermark {
        text: "GİZLİ <taslak> & \"x\"".into(),
        color: Some("#FF0000".into()),
        opacity: 0.25,
        rotation: -45.0,
        font_pt: 48.0,
        bold: false,
    });
    let bytes = write_docx(&document).unwrap();
    let reader = ZipReader::open(bytes.clone()).unwrap();
    assert!(reader.contains("word/header1.xml"), "watermark header part missing");
    let header = reader.read_text("word/header1.xml").unwrap();
    assert!(header.contains("v:textpath"), "header: {header}");
    assert!(header.contains("string=\"GİZLİ &lt;taslak&gt; &amp; &quot;x&quot;\""), "header: {header}");
    assert!(header.contains("fillcolor=\"#FF0000\""), "header: {header}");
    assert!(header.contains("rotation:-45"), "header: {header}");
    assert!(header.contains("font-weight:normal"), "header: {header}");
    let read = read_docx(&bytes).unwrap();
    let watermark = read.document.watermark.clone().expect("watermark missing");
    assert_eq!(watermark.text, "GİZLİ <taslak> & \"x\"");
    assert_eq!(watermark.color.as_deref(), Some("#FF0000"));
    assert!((watermark.opacity - 0.25).abs() < 1e-9);
    assert!((watermark.rotation + 45.0).abs() < 1e-9);
    assert!((watermark.font_pt - 48.0).abs() < 1e-9);
    assert!(!watermark.bold);
    // A second write→read keeps every field identical.
    let read_again = read_docx(&write_docx(&read.document).unwrap()).unwrap();
    assert_eq!(read_again.document.watermark, read.document.watermark);
}

#[test]
fn watermark_defaults_fall_back_to_silver() {
    let mut document = TextDocument::new_blank("Watermark");
    document.watermark = Some(Watermark { color: None, ..Watermark::default() });
    let bytes = write_docx(&document).unwrap();
    let reader = ZipReader::open(bytes.clone()).unwrap();
    let header = reader.read_text("word/header1.xml").unwrap();
    assert!(header.contains("fillcolor=\"#C0C0C0\""), "header: {header}");
    assert!(header.contains("font-weight:bold"), "header: {header}");
    let read = read_docx(&bytes).unwrap();
    let watermark = read.document.watermark.expect("watermark missing");
    assert_eq!(watermark.text, "TASLAK");
    assert_eq!(watermark.color.as_deref(), Some("#C0C0C0"));
    assert!(watermark.bold);
}

#[test]
fn reads_hand_written_watermark_header() {
    let mut zip = ZipWriter::new();
    zip.add_text("word/document.xml", r#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body><w:p><w:r><w:t>Body</w:t></w:r></w:p><w:sectPr><w:headerReference w:type="default" r:id="rId1"/></w:sectPr></w:body></w:document>"#);
    zip.add_text("word/_rels/document.xml.rels", r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/></Relationships>"#);
    zip.add_text("word/header1.xml", r##"<?xml version="1.0"?><w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office"><w:p><w:r><w:pict><v:shapetype id="_x0000_t136" coordsize="21600,21600" o:spt="136"/><v:shape id="PowerPlusWaterMarkObject" type="#_x0000_t136" style="position:absolute;rotation:-45;width:415pt;height:207.5pt" fillcolor="#FF0000" stroked="f"><v:fill opacity="25%"/><v:textpath style="font-family:&quot;Calibri&quot;;font-size:36pt;font-weight:bold" string="GIZLI"/></v:shape></w:pict></w:r></w:p></w:hdr>"##);
    let read = read_docx(&zip.finish()).unwrap();
    let watermark = read.document.watermark.clone().expect("watermark missing");
    assert_eq!(watermark.text, "GIZLI");
    assert_eq!(watermark.color.as_deref(), Some("#FF0000"));
    assert!((watermark.opacity - 0.25).abs() < 1e-9);
    assert!((watermark.rotation + 45.0).abs() < 1e-9);
    assert!((watermark.font_pt - 36.0).abs() < 1e-9);
    assert!(watermark.bold);
    assert!(read.document.header.iter().map(Block::plain_text).all(|text| !text.contains("GIZLI")));
}

#[test]
fn headers_without_textpath_do_not_invent_a_watermark() {
    let mut zip = ZipWriter::new();
    zip.add_text("word/document.xml", r#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body><w:p><w:r><w:t>Body</w:t></w:r></w:p><w:sectPr><w:headerReference w:type="default" r:id="rId1"/></w:sectPr></w:body></w:document>"#);
    zip.add_text("word/_rels/document.xml.rels", r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/></Relationships>"#);
    zip.add_text("word/header1.xml", r##"<?xml version="1.0"?><w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:r><w:t>Plain header</w:t></w:r></w:p></w:hdr>"##);
    let read = read_docx(&zip.finish()).unwrap();
    assert!(read.document.watermark.is_none());
    assert!(read.document.header.iter().map(Block::plain_text).any(|text| text.contains("Plain header")));
}

#[test]
fn reads_foreign_and_malformed_vertical_merges() {
    let mut zip = ZipWriter::new();
    zip.add_text("word/document.xml", r#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:tbl><w:tblGrid><w:gridCol w:w="1000"/><w:gridCol w:w="1000"/><w:gridCol w:w="1000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:gridSpan w:val="2"/><w:vMerge w:val="restart"/></w:tcPr><w:p/></w:tc><w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p/></w:tc></w:tr><w:tr><w:tc><w:tcPr><w:gridSpan w:val="2"/><w:vMerge/></w:tcPr><w:p/></w:tc><w:tc><w:tcPr/><w:p/></w:tc></w:tr></w:tbl><w:p/></w:body></w:document>"#);
    let read = read_docx(&zip.finish()).unwrap();
    let table = read
        .document
        .blocks
        .iter()
        .find_map(|block| match block {
            Block::Table { table } => Some(table),
            _ => None,
        })
        .expect("table missing");
    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.rows[0].cells.len(), 2);
    assert_eq!(table.rows[0].cells[0].colspan, 2);
    // The colspan-two continuation extends the origin once, not per column.
    assert_eq!(table.rows[0].cells[0].rowspan, 2);
    // The stray continuation with no restart above it stays a normal cell.
    assert_eq!(table.rows[0].cells[1].rowspan, 1);
    assert_eq!(table.rows[1].cells.len(), 1);
    // Rewriting the model produces the same grid again.
    let mut document = TextDocument::new_blank("Foreign merge");
    document.blocks = vec![Block::Table { table: table.clone() }];
    let bytes = write_docx(&document).unwrap();
    let xml = ZipReader::open(bytes.clone()).unwrap().read_text("word/document.xml").unwrap();
    assert!(xml.contains("<w:vMerge w:val=\"restart\"/>"), "xml: {xml}");
    assert_eq!(xml.matches("<w:vMerge/>").count(), 2, "xml: {xml}");
    let read_again = read_docx(&bytes).unwrap();
    let table_again = read_again
        .document
        .blocks
        .iter()
        .find_map(|block| match block {
            Block::Table { table } => Some(table),
            _ => None,
        })
        .expect("table missing");
    assert_eq!(table_again.rows[0].cells.len(), 2);
    assert_eq!(table_again.rows[0].cells[0].rowspan, 2);
    assert_eq!(table_again.rows[1].cells.len(), 1);
}

#[test]
fn reads_the_run_language_for_hyphenation() {
    // `w:lang` carries the run's language; without it the Writer cannot
    // hyphenate Turkish text (the hyphenation engine was never wired).
    let text = r#"<w:rPr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:lang w:val="tr-TR"/><w:b/></w:rPr>"#;
    let root = parse_xml(text).expect("xml");
    let run = read_run_properties(Some(&root));
    assert_eq!(run.lang.as_deref(), Some("tr-TR"));
    assert!(run.bold, "the other properties still parse");
}
