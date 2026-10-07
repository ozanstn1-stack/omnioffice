//! Writer comments in ODT and RTF: export, import and the compatibility
//! report that describes what survives.

use officecore::compat::document_feature_report;
use officecore::model::{Block, Comment, CommentReply, ParaProps, Run, TextDocument};
use officecore::zip::{ZipReader, ZipWriter};
use officecore::{odf, rtf};

fn run(text: &str, comment: Option<&str>) -> Run {
    Run { text: text.into(), comment: comment.map(str::to_string), ..Default::default() }
}

fn paragraph(runs: Vec<Run>) -> Block {
    Block::Paragraph { props: ParaProps::default(), runs }
}

/// Two comments: `c1` (with a reply) spans two runs across two paragraphs,
/// `c2` is resolved and covers one bold run in the middle of a paragraph.
fn commented_document() -> TextDocument {
    let mut document = TextDocument::new_blank("Comments");
    document.blocks = vec![
        paragraph(vec![run("Total: ", None), run("42 units", Some("c1"))]),
        paragraph(vec![run("per week", Some("c1")), run(" in Q3.", None)]),
        paragraph(vec![
            run("Status ", None),
            Run { text: "approved".into(), bold: true, comment: Some("c2".into()), ..Default::default() },
            run(" by the board.", None),
        ]),
    ];
    document.comments = vec![
        Comment {
            id: "c1".into(),
            author: "Ayşe Yılmaz".into(),
            text: "Please verify this figure.\nIt looks high.".into(),
            created: "2026-03-01T09:30:00Z".into(),
            resolved: false,
            modified: "2026-03-01T09:30:00Z".into(),
            replies: vec![CommentReply {
                author: "Bob".into(),
                text: "Checked, it is right.".into(),
                created: "2026-03-02T10:00:00Z".into(),
            }],
        },
        Comment {
            id: "c2".into(),
            author: "Carol".into(),
            text: "Decision recorded.".into(),
            created: "2026-04-05T16:45:00Z".into(),
            resolved: true,
            modified: "2026-04-05T16:45:00Z".into(),
            replies: Vec::new(),
        },
    ];
    document
}

fn paragraphs(document: &TextDocument) -> Vec<&Vec<Run>> {
    document
        .blocks
        .iter()
        .filter_map(|block| match block {
            Block::Paragraph { runs, .. } => Some(runs),
            _ => None,
        })
        .collect()
}

fn paragraph_text(runs: &[Run]) -> String {
    runs.iter().map(|run| run.text.as_str()).collect()
}

/// The comment id each run with text is anchored to, as (text, id) pairs.
fn anchors(document: &TextDocument) -> Vec<(String, Option<String>)> {
    paragraphs(document)
        .into_iter()
        .flatten()
        .filter(|run| !run.text.is_empty())
        .map(|run| (run.text.clone(), run.comment.clone()))
        .collect()
}

fn comment_with_text<'a>(document: &'a TextDocument, needle: &str) -> &'a Comment {
    document
        .comments
        .iter()
        .find(|comment| comment.text.contains(needle))
        .unwrap_or_else(|| panic!("no comment containing {needle:?} in {:?}", document.comments))
}

fn odt_package(content: &str) -> Vec<u8> {
    let mut zip = ZipWriter::new();
    zip.add_text("mimetype", "application/vnd.oasis.opendocument.text");
    zip.add_text("content.xml", content);
    zip.finish()
}

#[test]
fn odt_round_trip_keeps_comments_replies_resolved_state_and_anchors() {
    let document = commented_document();
    let bytes = odf::write_odt(&document).unwrap();

    let content = ZipReader::open(bytes.clone()).unwrap().read_text("content.xml").unwrap();
    assert!(content.contains("xmlns:loext=\"urn:org:documentfoundation:names:experimental:office:xmlns:loext:1.0\""));
    assert!(content.contains("<office:annotation office:name=\"c1\"><dc:creator>Ayşe Yılmaz</dc:creator>"));
    assert!(content.contains("<office:annotation office:name=\"c2\" loext:resolved=\"true\">"), "content: {content}");
    assert!(content.contains("<dc:date>2026-03-01T09:30:00Z</dc:date>"));
    assert!(content.contains("<text:p>Re: Bob: Checked, it is right.</text:p>"));
    // The c1 range opens before "42 units" and closes after "per week", in the
    // next paragraph.
    let start = content.find("<office:annotation office:name=\"c1\"").unwrap();
    let end = content.find("<office:annotation-end office:name=\"c1\"/>").unwrap();
    let range = &content[start..end];
    assert!(range.contains("42 units") && range.contains("per week") && !range.contains(" in Q3."), "range: {range}");
    assert_eq!(content.matches("<office:annotation-end").count(), 2);

    let read = odf::read_odt(&bytes).unwrap();
    let back = &read.document;
    assert_eq!(back.comments.len(), 2, "warnings: {:?}", read.warnings);
    let first = back.comments.iter().find(|comment| comment.id == "c1").expect("c1 keeps its id");
    assert_eq!(first.author, "Ayşe Yılmaz");
    assert_eq!(first.text, "Please verify this figure.\nIt looks high.");
    assert_eq!(first.created, "2026-03-01T09:30:00Z");
    assert!(!first.resolved);
    assert_eq!(first.replies.len(), 1);
    assert_eq!(first.replies[0].author, "Bob");
    assert_eq!(first.replies[0].text, "Checked, it is right.");
    let second = back.comments.iter().find(|comment| comment.id == "c2").expect("c2 keeps its id");
    assert_eq!(second.author, "Carol");
    assert_eq!(second.text, "Decision recorded.");
    assert!(second.resolved);
    assert!(second.replies.is_empty());

    // Body text is unchanged and in order; no comment content leaked into it.
    let texts: Vec<String> = paragraphs(back).into_iter().map(|runs| paragraph_text(runs)).collect();
    assert_eq!(texts, vec!["Total: 42 units", "per week in Q3.", "Status approved by the board."]);
    assert_eq!(
        anchors(back),
        vec![
            ("Total: ".to_string(), None),
            ("42 units".to_string(), Some("c1".to_string())),
            ("per week".to_string(), Some("c1".to_string())),
            (" in Q3.".to_string(), None),
            ("Status ".to_string(), None),
            ("approved".to_string(), Some("c2".to_string())),
            (" by the board.".to_string(), None),
        ]
    );
}

#[test]
fn odt_point_and_unanchored_comments_are_written_and_read_back() {
    let mut document = TextDocument::new_blank("Points");
    document.blocks = vec![paragraph(vec![run("Before ", None), run("", Some("p1")), run("after.", None)])];
    document.comments = vec![
        Comment { id: "p1".into(), author: "Dana".into(), text: "Point comment".into(), ..Default::default() },
        Comment { id: "o1".into(), author: "Eve".into(), text: "Its text was deleted".into(), ..Default::default() },
    ];
    let bytes = odf::write_odt(&document).unwrap();
    let content = ZipReader::open(bytes.clone()).unwrap().read_text("content.xml").unwrap();
    // A point comment has no end marker, and the unanchored one is kept at the
    // start of the body.
    assert!(!content.contains("annotation-end"), "content: {content}");
    let unanchored = content.find("office:name=\"o1\"").unwrap();
    assert!(unanchored < content.find("Before ").unwrap());

    let read = odf::read_odt(&bytes).unwrap();
    assert_eq!(read.document.comments.len(), 2);
    assert_eq!(paragraph_text(paragraphs(&read.document)[0]), "Before after.");
    // Like DOCX import, a point comment anchors the run that follows it.
    let anchored = anchors(&read.document);
    assert!(anchored.contains(&("after.".to_string(), Some("p1".to_string()))), "anchors: {anchored:?}");
}

/// Regression: an inline annotation used to fall through the catch-all branch
/// of the run walker, which folded the author, date and comment text into the
/// paragraph (twice). Mixed content also lost its order.
#[test]
fn libreoffice_inline_annotation_imports_as_a_comment_and_leaves_the_body_clean() {
    let content = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
        "<office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
        "xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" ",
        "xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" ",
        "xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" ",
        "xmlns:dc=\"http://purl.org/dc/elements/1.1/\" ",
        "xmlns:meta=\"urn:oasis:names:tc:opendocument:xmlns:meta:1.0\" ",
        "xmlns:loext=\"urn:org:documentfoundation:names:experimental:office:xmlns:loext:1.0\" office:version=\"1.3\">",
        "<office:automatic-styles><style:style style:name=\"T1\" style:family=\"text\">",
        "<style:text-properties fo:font-weight=\"bold\"/></style:style></office:automatic-styles>",
        "<office:body><office:text>",
        "<text:p text:style-name=\"Standard\">Before <text:span text:style-name=\"T1\">",
        "<office:annotation office:name=\"__Annotation__45_1424615014\" loext:resolved=\"true\">",
        "<dc:creator>Ayşe Yılmaz</dc:creator><dc:date>2026-03-01T09:30:12.123456789</dc:date>",
        "<meta:creator-initials>AY</meta:creator-initials>",
        "<text:p text:style-name=\"P1\"><text:span text:style-name=\"T2\">Check</text:span> this<text:s/>figure</text:p>",
        "<text:p>Second line</text:p>",
        "</office:annotation>bold words</text:span>",
        "<office:annotation-end office:name=\"__Annotation__45_1424615014\"/> and plain",
        "<office:annotation><dc:creator>Bob</dc:creator><text:p>A point comment</text:p></office:annotation>",
        " tail.</text:p>",
        "</office:text></office:body></office:document-content>"
    );
    let read = odf::read_odt(&odt_package(content)).unwrap();
    let document = &read.document;

    let body = document.plain_text();
    assert_eq!(body, "Before bold words and plain tail.");
    for leaked in ["Ayşe", "2026", "Check", "figure", "Second line", "AY", "Bob", "point comment"] {
        assert!(!body.contains(leaked), "comment content {leaked:?} leaked into the body: {body}");
    }
    assert!(!read.warnings.iter().any(|warning| warning.contains("Comments")), "warnings: {:?}", read.warnings);

    assert_eq!(document.comments.len(), 2, "comments: {:?}", document.comments);
    let ranged = &document.comments[0];
    assert_eq!(ranged.id, "__Annotation__45_1424615014");
    assert_eq!(ranged.author, "Ayşe Yılmaz");
    assert_eq!(ranged.created, "2026-03-01T09:30:12.123456789");
    assert_eq!(ranged.text, "Check this figure\nSecond line");
    assert!(ranged.resolved);
    let point = &document.comments[1];
    assert_eq!(point.author, "Bob");
    assert_eq!(point.text, "A point comment");
    assert!(!point.resolved);

    let runs = paragraphs(document)[0];
    let bold = runs.iter().find(|run| run.text == "bold words").expect("the span text is one run");
    assert_eq!(bold.comment.as_deref(), Some(ranged.id.as_str()));
    assert!(runs
        .iter()
        .filter(|run| run.text.contains("Before") || run.text.contains("and plain"))
        .all(|run| run.comment.is_none()));
    let tail = runs.iter().find(|run| run.text == " tail.").expect("text after the point comment");
    assert_eq!(tail.comment.as_deref(), Some(point.id.as_str()));
}

#[test]
fn rtf_round_trip_keeps_comments_replies_resolved_state_and_anchors() {
    let document = commented_document();
    let bytes = rtf::write_rtf(&document).unwrap();
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(text.contains("{\\*\\atrfstart 1}{42 units}"), "rtf: {text}");
    assert!(text.contains("{per week}{\\*\\atrfend 1}"), "rtf: {text}");
    assert!(text.contains("{\\*\\atnid AY}{\\*\\atnauthor Ay\\u351?e Y\\u305?lmaz}\\chatn {\\*\\annotation{\\*\\atnref 1}{\\*\\atndate "), "rtf: {text}");
    assert!(text.contains("Please verify this figure.\\par It looks high.\\par Re: Bob: Checked, it is right.}"));
    assert!(text.contains("{\\*\\atnref 2}"));
    assert!(text.contains("{\\*\\oswkresolved}"));

    let read = rtf::read_rtf(&bytes).unwrap();
    let back = &read.document;
    assert_eq!(back.comments.len(), 2, "warnings: {:?}", read.warnings);
    let first = comment_with_text(back, "verify");
    assert_eq!(first.author, "Ayşe Yılmaz");
    assert_eq!(first.text, "Please verify this figure.\nIt looks high.");
    assert_eq!(first.created, "2026-03-01T09:30:00Z");
    assert!(!first.resolved);
    assert_eq!(first.replies.len(), 1);
    assert_eq!(first.replies[0].author, "Bob");
    assert_eq!(first.replies[0].text, "Checked, it is right.");
    let second = comment_with_text(back, "Decision");
    assert_eq!(second.author, "Carol");
    assert_eq!(second.created, "2026-04-05T16:45:00Z");
    assert!(second.resolved);

    let body = back.plain_text();
    assert!(!body.contains("verify") && !body.contains("Bob") && !body.contains("Decision"), "body: {body}");
    let anchored = anchors(back);
    let anchor_of = |needle: &str| anchored.iter().find(|(text, _)| text == needle).map(|(_, id)| id.clone());
    assert_eq!(anchor_of("42 units"), Some(Some(first.id.clone())), "anchors: {anchored:?}");
    assert_eq!(anchor_of("per week"), Some(Some(first.id.clone())), "anchors: {anchored:?}");
    assert_eq!(anchor_of("approved"), Some(Some(second.id.clone())), "anchors: {anchored:?}");
    for plain in ["Total: ", " in Q3.", "Status ", " by the board."] {
        assert_eq!(anchor_of(plain), Some(None), "{plain:?} must stay unanchored: {anchored:?}");
    }
}

#[test]
fn word_style_rtf_annotation_imports_as_a_comment() {
    // The layout Word writes: a bookmark-numbered range, the author group,
    // \chatn and the annotation destination with its own paragraph formatting.
    let rtf = concat!(
        "{\\rtf1\\ansi\\ansicpg1252\\deff0{\\fonttbl{\\f0\\fswiss Calibri;}}",
        "\\pard\\plain Before {\\*\\atrfstart 1158498823}commented text{\\*\\atrfend 1158498823}",
        "{\\*\\atnid JD}{\\*\\atnauthor John Do\\'e9}\\chatn {\\*\\annotation{\\*\\atnref 1158498823}",
        "{\\*\\atndate 130386846}\\pard\\plain \\s20\\ql \\rtlch\\fcs1 \\af0\\afs20 \\ltrch\\fcs0 \\fs20\\lang1033 ",
        "{\\rtlch\\fcs1 \\af0 \\ltrch\\fcs0 \\cs19\\fs16 \\chatn }",
        "{\\field{\\*\\fldinst {\\rtlch PAGE }}{\\fldrslt }}",
        "{\\rtlch\\fcs1 \\af0 \\ltrch\\fcs0 Please r\\'e9view \\u351?u paragraf.}\\par Second line}",
        " after.\\par}"
    );
    let read = rtf::read_rtf(rtf.as_bytes()).unwrap();
    let document = &read.document;
    assert_eq!(document.comments.len(), 1, "comments: {:?}", document.comments);
    let comment = &document.comments[0];
    assert_eq!(comment.id, "1158498823");
    assert_eq!(comment.author, "John Doé");
    assert_eq!(comment.text, "Please réview şu paragraf.\nSecond line");
    assert_eq!(comment.created, "2024-05-17T14:30:00Z");
    assert!(!comment.resolved);

    let body = document.plain_text();
    for leaked in ["John", "JD", "review", "réview", "PAGE", "Second line", "1158498823"] {
        assert!(!body.contains(leaked), "annotation content {leaked:?} leaked into the body: {body}");
    }
    assert!(body.contains("Before") && body.contains("commented text") && body.contains("after."), "body: {body}");
    let anchored = anchors(document);
    assert!(
        anchored.contains(&("commented text".to_string(), Some("1158498823".to_string()))),
        "anchors: {anchored:?}"
    );
    assert!(anchored.iter().filter(|(text, _)| text != "commented text").all(|(_, id)| id.is_none()));
}

#[test]
fn rtf_point_annotation_anchors_the_following_text() {
    let rtf = "{\\rtf1\\ansi\\pard Start {\\*\\atnid AB}{\\*\\atnauthor Alice Brown}\\chatn {\\*\\annotation \\pard\\plain Point note}{next words}\\par}";
    let read = rtf::read_rtf(rtf.as_bytes()).unwrap();
    assert_eq!(read.document.comments.len(), 1);
    let comment = &read.document.comments[0];
    assert_eq!(comment.author, "Alice Brown");
    assert_eq!(comment.text, "Point note");
    let anchored = anchors(&read.document);
    assert!(anchored.contains(&("next words".to_string(), Some(comment.id.clone()))), "anchors: {anchored:?}");
    assert!(!read.document.plain_text().contains("Point note"));
}

#[test]
fn compatibility_report_describes_comments_in_odt_and_rtf() {
    let mut document = commented_document();
    for target in ["odt", "rtf"] {
        let report = document_feature_report(&document, target);
        let comments = report.items.iter().find(|item| item.feature == "comments");
        let comments = comments.unwrap_or_else(|| panic!("{target}: replies must be reported: {:?}", report.items));
        assert_eq!(comments.status, "transformed", "{target}: {comments:?}");
        assert!(comments.message.contains("repl"), "{target}: {comments:?}");
        assert!(report.items.iter().all(|item| item.status != "lost"), "{target}: {:?}", report.items);
    }

    // Without replies, nothing about ODT comments is lost or converted.
    for comment in &mut document.comments {
        comment.replies.clear();
    }
    let odt = document_feature_report(&document, "odt");
    assert!(!odt.items.iter().any(|item| item.feature == "comments"), "odt: {:?}", odt.items);
    assert!(!odt.lossy());
    let rtf = document_feature_report(&document, "rtf");
    assert!(rtf.items.iter().any(|item| item.feature == "comments" && item.status == "transformed"));

    let odt_capabilities = officecore::compat::format_capabilities("odt");
    let rtf_capabilities = officecore::compat::format_capabilities("rtf");
    for capabilities in [odt_capabilities, rtf_capabilities] {
        let comments = capabilities.features.iter().find(|feature| feature.feature == "comments").unwrap();
        assert_eq!(comments.level, officecore::compat::SupportLevel::Partial, "{}", capabilities.extension);
    }
}

/// Thousands of unnamed annotations import in linear time: ids used to be
/// checked against every earlier comment, so a 5 KB file took half a minute.
#[test]
fn many_unnamed_comments_import_quickly() {
    const COUNT: usize = 20_000;
    let annotations = "<office:annotation><text:p>a</text:p></office:annotation>x".repeat(COUNT);
    let content = format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
            "xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" office:version=\"1.2\">",
            "<office:body><office:text><text:p>{}</text:p></office:text></office:body></office:document-content>"
        ),
        annotations
    );
    let started = std::time::Instant::now();
    let read = odf::read_odt(&odt_package(&content)).unwrap();
    assert_eq!(read.document.comments.len(), COUNT);
    assert_eq!(read.document.comments[COUNT - 1].id, format!("odt-comment-{COUNT}"));

    let rtf = format!("{{\\rtf1\\ansi {}\\par}}", "{\\*\\annotation a}x".repeat(COUNT));
    let rtf_read = rtf::read_rtf(rtf.as_bytes()).unwrap();
    assert_eq!(rtf_read.document.comments.len(), COUNT);
    assert!(started.elapsed().as_secs() < 20, "took {:?}", started.elapsed());
}
