use super::*;

fn sample_deck() -> Deck {
    let mut deck = Deck::new_blank("Sample deck");
    deck.theme = "business".into();
    let mut slide = Slide::default();
    let mut title = SlideObject::new("text", 60.0, 60.0, 600.0, 100.0);
    title.text = Some(TextFrame {
        paragraphs: vec![TextParagraph {
            text: "Başlık slaytı".into(),
            size_pt: Some(32.0),
            bold: true,
            ..Default::default()
        }],
        ..Default::default()
    });
    let mut rect = SlideObject::new("rect", 60.0, 200.0, 320.0, 160.0);
    rect.style = Some(ShapeStyle { fill: Some("#1D4ED8".into()), ..Default::default() });
    rect.text = Some(TextFrame {
        paragraphs: vec![TextParagraph { text: "Kutu".into(), color: Some("#FFFFFF".into()), ..Default::default() }],
        ..Default::default()
    });
    slide.objects = vec![title, rect];
    slide.notes = "Notlar burada".into();
    slide.transition = Some("fade".into());
    let mut second = Slide::default();
    let mut table = SlideObject::new("table", 60.0, 120.0, 500.0, 200.0);
    table.table = Some(TableData::simple(2, 3, 500.0));
    second.objects = vec![table];
    deck.slides = vec![slide, second];
    deck
}

#[test]
fn pptx_package_structure() {
    let result = write_pptx_package(&sample_deck()).unwrap();
    let reader = ZipReader::open(result.bytes.clone()).unwrap();
    for part in [
        "[Content_Types].xml",
        "ppt/presentation.xml",
        "ppt/slideMasters/slideMaster1.xml",
        "ppt/slideLayouts/slideLayout1.xml",
        "ppt/theme/theme1.xml",
        "ppt/slides/slide1.xml",
        "ppt/slides/slide2.xml",
        "ppt/slides/_rels/slide1.xml.rels",
        "ppt/notesSlides/notesSlide1.xml",
    ] {
        assert!(reader.contains(part), "missing {part}");
    }
    let presentation = reader.read_text("ppt/presentation.xml").unwrap();
    assert!(presentation.contains("sldIdLst"));
    assert!(presentation.contains("sldSz"));
}

#[test]
fn pptx_roundtrip() {
    let bytes = write_pptx(&sample_deck()).unwrap();
    let read = read_pptx(&bytes).unwrap();
    assert_eq!(read.deck.slides.len(), 2);
    let texts: Vec<String> =
        read.deck.slides[0].objects.iter().filter_map(|object| object.text.as_ref().map(TextFrame::plain)).collect();
    assert!(texts.iter().any(|text| text.contains("Başlık slaytı")), "texts: {texts:?}");
    assert!(read.deck.slides[0].notes.contains("Notlar"));
    assert_eq!(read.deck.size.width_pt.round() as i64, 960);
}

fn bullet_deck() -> Deck {
    let mut deck = Deck::new_blank("Bullets");
    let mut slide = Slide::default();
    let mut body = SlideObject::new("text", 60.0, 60.0, 600.0, 300.0);
    body.text = Some(TextFrame {
        paragraphs: vec![
            TextParagraph { text: "Top".into(), bullet: true, ..Default::default() },
            TextParagraph { text: "Nested".into(), bullet: true, level: 2, ..Default::default() },
            TextParagraph { text: "Plain".into(), ..Default::default() },
        ],
        ..Default::default()
    });
    slide.objects = vec![body];
    deck.slides = vec![slide];
    deck
}

#[test]
fn pptx_bullet_is_a_real_bullet_and_level_roundtrips() {
    let result = write_pptx_package(&bullet_deck()).unwrap();
    let reader = ZipReader::open(result.bytes.clone()).unwrap();
    let slide_xml = reader.read_text("ppt/slides/slide1.xml").unwrap();
    assert!(slide_xml.contains("<a:buChar char=\"\u{2022}\"/>"), "bullet glyph: {slide_xml}");
    assert!(!slide_xml.contains('\u{00C3}') && !slide_xml.contains('\u{00E2}'), "mojibake in slide xml");

    let read = read_pptx(&result.bytes).unwrap();
    let paragraphs = &read.deck.slides[0].objects[0].text.as_ref().unwrap().paragraphs;
    assert_eq!(paragraphs.len(), 3);
    assert!(paragraphs[0].bullet && paragraphs[0].level == 0);
    assert!(paragraphs[1].bullet && paragraphs[1].level == 2);
    assert!(!paragraphs[2].bullet);
}

fn lang_deck(deck_lang: Option<&str>, run_lang: Option<&str>) -> Deck {
    let mut deck = Deck::new_blank("Lang");
    deck.lang = deck_lang.map(str::to_string);
    let mut slide = Slide::default();
    let mut body = SlideObject::new("text", 60.0, 60.0, 600.0, 300.0);
    body.text = Some(TextFrame {
        paragraphs: vec![
            TextParagraph {
                text: "Hello".into(),
                runs: vec![Run { text: "Hello".into(), lang: run_lang.map(str::to_string), ..Default::default() }],
                ..Default::default()
            },
            TextParagraph { text: "Bare".into(), ..Default::default() },
        ],
        ..Default::default()
    });
    slide.objects = vec![body];
    slide.notes = "Note".into();
    let mut table = SlideObject::new("table", 60.0, 400.0, 400.0, 100.0);
    table.table = Some(TableData::simple(1, 1, 400.0));
    slide.objects.push(table);
    deck.slides = vec![slide];
    deck
}

fn lang_parts(deck: &Deck) -> (String, String, String) {
    let result = write_pptx_package(deck).unwrap();
    let reader = ZipReader::open(result.bytes).unwrap();
    (
        reader.read_text("ppt/slides/slide1.xml").unwrap(),
        reader.read_text("ppt/notesSlides/notesSlide1.xml").unwrap(),
        reader.read_text("ppt/presentation.xml").unwrap(),
    )
}

#[test]
fn pptx_writes_no_language_when_unknown() {
    let (slide, notes, presentation) = lang_parts(&lang_deck(None, None));
    assert!(!slide.contains("lang="), "slide: {slide}");
    assert!(!notes.contains("lang="), "notes: {notes}");
    assert!(!presentation.contains("lang="), "presentation: {presentation}");
}

#[test]
fn pptx_uses_run_and_deck_language() {
    let (slide, notes, presentation) = lang_parts(&lang_deck(Some("en-GB"), Some("de-DE")));
    // The run keeps its own language, the paragraph without runs, the
    // table cell and the notes fall back to the deck language.
    assert_eq!(slide.matches("lang=\"de-DE\"").count(), 1, "slide: {slide}");
    assert_eq!(slide.matches("lang=\"en-GB\"").count(), 2, "slide: {slide}");
    assert!(notes.contains("lang=\"en-GB\""));
    assert!(presentation.contains("<a:defRPr lang=\"en-GB\"/>"), "presentation: {presentation}");
    assert!(!slide.contains("tr-TR") && !notes.contains("tr-TR"));
}

#[test]
fn pptx_language_roundtrips() {
    let bytes = write_pptx(&lang_deck(Some("en-GB"), Some("de-DE"))).unwrap();
    let read = read_pptx(&bytes).unwrap();
    assert_eq!(read.deck.lang.as_deref(), Some("en-GB"));
    let paragraphs = &read.deck.slides[0].objects[0].text.as_ref().unwrap().paragraphs;
    assert_eq!(paragraphs[0].runs[0].lang.as_deref(), Some("de-DE"));
    assert_eq!(paragraphs[0].lang.as_deref(), Some("de-DE"));
    // The paragraph without runs was written with the deck language.
    assert_eq!(paragraphs[1].lang.as_deref(), Some("en-GB"));

    let unknown = read_pptx(&write_pptx(&lang_deck(None, None)).unwrap()).unwrap();
    assert_eq!(unknown.deck.lang, None);
}

fn runs_deck(paragraphs: Vec<TextParagraph>) -> Deck {
    let mut deck = Deck::new_blank("Edited");
    let mut slide = Slide::default();
    let mut body = SlideObject::new("text", 60.0, 60.0, 600.0, 300.0);
    body.text = Some(TextFrame { paragraphs, ..Default::default() });
    slide.objects = vec![body];
    deck.slides = vec![slide];
    deck
}

fn formatted(text: &str, level: u32) -> TextParagraph {
    TextParagraph {
        text: text.into(),
        level,
        bullet: true,
        runs: vec![Run { text: text.into(), bold: true, ..Default::default() }],
        ..Default::default()
    }
}

fn exported_paragraphs(deck: &Deck) -> (String, Vec<TextParagraph>) {
    let bytes = write_pptx(deck).unwrap();
    let slide_xml = ZipReader::open(bytes.clone()).unwrap().read_text("ppt/slides/slide1.xml").unwrap();
    let read = read_pptx(&bytes).unwrap();
    (slide_xml, read.deck.slides[0].objects[0].text.clone().unwrap().paragraphs)
}

#[test]
fn pptx_edited_paragraph_exports_its_new_text() {
    // An imported slide: both paragraphs carry runs. The editor changes
    // the second paragraph and clears its runs, the first one is kept.
    let (_, imported) = exported_paragraphs(&runs_deck(vec![formatted("Title", 0), formatted("Old detail", 1)]));
    let mut edited = imported.clone();
    edited[1].text = "New detail".into();
    edited[1].runs.clear();

    let (slide_xml, paragraphs) = exported_paragraphs(&runs_deck(edited));
    assert!(slide_xml.contains("New detail"), "slide: {slide_xml}");
    assert!(!slide_xml.contains("Old detail"), "slide: {slide_xml}");
    assert_eq!(paragraphs.len(), 2);
    assert_eq!(paragraphs[0].text, "Title");
    assert_eq!(paragraphs[0].runs.len(), 1);
    assert!(paragraphs[0].runs[0].bold);
    assert_eq!(paragraphs[1].text, "New detail");
    assert!(paragraphs[1].bullet && paragraphs[1].level == 1);
}

#[test]
fn pptx_never_writes_runs_that_contradict_the_paragraph_text() {
    // Decks saved by 4.2.0 can hold a changed paragraph text next to the
    // runs of the old text; the paragraph text is what the user sees.
    let mut stale = formatted("Old detail", 1);
    stale.text = "New detail".into();
    let (slide_xml, paragraphs) = exported_paragraphs(&runs_deck(vec![stale]));
    assert!(slide_xml.contains("New detail"), "slide: {slide_xml}");
    assert!(!slide_xml.contains("Old detail"), "slide: {slide_xml}");
    assert_eq!(paragraphs[0].text, "New detail");
    assert!(paragraphs[0].bullet && paragraphs[0].level == 1);
}

#[test]
fn pptx_roundtrip_image() {
    let mut buffer = image::RgbaImage::new(4, 4);
    for pixel in buffer.pixels_mut() {
        *pixel = image::Rgba([10, 200, 90, 255]);
    }
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(buffer).write_to(&mut png, image::ImageFormat::Png).unwrap();
    let mut deck = Deck::new_blank("Images");
    let mut slide = Slide::default();
    let mut object = SlideObject::new("image", 40.0, 40.0, 200.0, 150.0);
    object.image = Some(ImageData::from_bytes("pic.png", &png.into_inner()));
    slide.objects = vec![object];
    deck.slides = vec![slide];
    let bytes = write_pptx(&deck).unwrap();
    let read = read_pptx(&bytes).unwrap();
    let image = read.deck.slides[0].objects.iter().find_map(|object| object.image.clone());
    assert!(image.map(|image| !image.data_base64.is_empty()).unwrap_or(false));
}

#[test]
fn pptx_run_formatting_roundtrips() {
    let mut deck = Deck::new_blank("Runs");
    let mut slide = Slide::default();
    let mut body = SlideObject::new("text", 60.0, 60.0, 600.0, 300.0);
    body.text = Some(TextFrame {
        paragraphs: vec![TextParagraph {
            text: "AlphaBetaGammaDelta".into(),
            runs: vec![
                Run { text: "Alpha".into(), strike: true, ..Default::default() },
                Run { text: "Beta".into(), highlight: Some("#ffff00".into()), ..Default::default() },
                Run { text: "Gamma".into(), superscript: true, ..Default::default() },
                Run {
                    text: "Delta".into(),
                    underline: true,
                    subscript: true,
                    font: Some("Georgia".into()),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }],
        ..Default::default()
    });
    slide.objects = vec![body];
    deck.slides = vec![slide];

    let bytes = write_pptx(&deck).unwrap();
    let slide_xml = ZipReader::open(bytes.clone()).unwrap().read_text("ppt/slides/slide1.xml").unwrap();
    assert!(slide_xml.contains("strike=\"sngStrike\""), "slide: {slide_xml}");
    assert!(slide_xml.contains("<a:highlight><a:srgbClr val=\"ffff00\"/></a:highlight>"), "slide: {slide_xml}");
    assert!(slide_xml.contains("baseline=\"30000\""), "slide: {slide_xml}");
    assert!(slide_xml.contains("baseline=\"-25000\""), "slide: {slide_xml}");

    let read = read_pptx(&bytes).unwrap();
    let paragraphs = &read.deck.slides[0].objects[0].text.as_ref().unwrap().paragraphs;
    assert_eq!(paragraphs.len(), 1);
    let paragraph = &paragraphs[0];
    assert!(paragraph.underline, "run underline unions into the paragraph");
    assert_eq!(paragraph.runs.len(), 4);
    assert!(paragraph.runs[0].strike);
    assert!(paragraph.runs[0].color.is_none(), "strike must not invent a color");
    assert_eq!(paragraph.runs[1].highlight.as_deref(), Some("#ffff00"));
    assert!(paragraph.runs[1].color.is_none(), "a highlight is not the run color");
    assert!(paragraph.runs[2].superscript && !paragraph.runs[2].subscript);
    assert!(paragraph.runs[3].subscript && !paragraph.runs[3].superscript && paragraph.runs[3].underline);
    assert_eq!(paragraph.runs[3].font.as_deref(), Some("Georgia"));
    assert!(!paragraph.runs[0].superscript && !paragraph.runs[0].subscript);
}

#[test]
fn pptx_hidden_slides_roundtrip() {
    let mut deck = Deck::new_blank("Hidden");
    let first = Slide { hidden: true, ..Default::default() };
    let second = Slide::default();
    deck.slides = vec![first, second];

    let bytes = write_pptx(&deck).unwrap();
    let presentation = ZipReader::open(bytes.clone()).unwrap().read_text("ppt/presentation.xml").unwrap();
    assert_eq!(presentation.matches("show=\"0\"").count(), 1, "presentation: {presentation}");

    let read = read_pptx(&bytes).unwrap();
    assert_eq!(read.deck.slides.len(), 2);
    assert!(read.deck.slides[0].hidden);
    assert!(!read.deck.slides[1].hidden);
}

#[test]
fn pptx_footer_uses_the_simple_master_and_roundtrips() {
    let mut deck = Deck::new_blank("Footer");
    deck.footer = Some(SlideFooter {
        enabled: true,
        text: "Confidential".into(),
        show_text: true,
        show_slide_number: true,
        show_date: true,
        date_text: "2026-10-09".into(),
    });
    let bytes = write_pptx(&deck).unwrap();
    let master = ZipReader::open(bytes.clone()).unwrap().read_text("ppt/slideMasters/slideMaster1.xml").unwrap();
    assert!(
        master.contains("type=\"slidenum\"><a:rPr lang=\"en-US\" sz=\"1200\"/><a:t>1</a:t></a:fld>"),
        "master: {master}"
    );
    assert!(master.contains("type=\"datetime1\"><a:t>2026-10-09</a:t></a:fld>"), "master: {master}");
    assert!(master.contains("<a:t>Confidential</a:t>"), "master: {master}");

    let read = read_pptx(&bytes).unwrap();
    let footer = read.deck.footer.expect("footer must be rebuilt from the master");
    assert!(footer.enabled && footer.show_text && footer.show_slide_number && footer.show_date);
    assert_eq!(footer.text, "Confidential");
    assert_eq!(footer.date_text, "2026-10-09");
}

#[test]
fn pptx_footer_flags_roundtrip_and_absent_footer_stays_absent() {
    let deck = Deck::new_blank("No footer");
    let read = read_pptx(&write_pptx(&deck).unwrap()).unwrap();
    assert_eq!(read.deck.footer, None, "an empty master must not invent footer settings");

    let mut deck = Deck::new_blank("Footer flags");
    deck.footer = Some(SlideFooter {
        enabled: true,
        text: "Not shown".into(),
        show_text: false,
        show_slide_number: true,
        show_date: false,
        date_text: "ignored".into(),
    });
    let bytes = write_pptx(&deck).unwrap();
    let master = ZipReader::open(bytes.clone()).unwrap().read_text("ppt/slideMasters/slideMaster1.xml").unwrap();
    assert!(master.contains("type=\"slidenum\""), "master: {master}");
    assert!(!master.contains("type=\"datetime1\""), "master: {master}");
    assert!(!master.contains("Not shown"), "master: {master}");

    let read = read_pptx(&bytes).unwrap();
    let footer = read.deck.footer.expect("the slide number must rebuild the footer");
    assert!(footer.enabled && footer.show_slide_number);
    assert!(!footer.show_text && !footer.show_date);
    assert!(footer.text.is_empty());
    assert!(footer.date_text.is_empty());
}

#[test]
fn pptx_footer_roundtrips_through_planned_masters() {
    let mut deck = Deck::new_blank("Footer masters");
    deck.masters = vec![SlideMaster {
        id: "master-1".into(),
        name: "Master".into(),
        theme: "business".into(),
        background: Some("#FFFFFF".into()),
        objects: Vec::new(),
        layouts: Vec::new(),
    }];
    deck.footer = Some(SlideFooter {
        enabled: true,
        text: "Planned footer".into(),
        show_text: true,
        show_slide_number: true,
        show_date: true,
        date_text: "2026-10-09".into(),
    });
    let bytes = write_pptx(&deck).unwrap();
    let master = ZipReader::open(bytes.clone()).unwrap().read_text("ppt/slideMasters/slideMaster1.xml").unwrap();
    assert!(master.contains("<a:t>Planned footer</a:t>"), "master: {master}");
    assert!(master.contains("type=\"slidenum\""), "master: {master}");
    assert!(master.contains("type=\"datetime1\""), "master: {master}");

    let read = read_pptx(&bytes).unwrap();
    assert_eq!(read.deck.masters.len(), 1);
    let footer = read.deck.footer.expect("footer must be rebuilt from the master");
    assert!(footer.enabled && footer.show_text && footer.show_slide_number && footer.show_date);
    assert_eq!(footer.text, "Planned footer");
    assert_eq!(footer.date_text, "2026-10-09");
}

#[test]
fn pptx_connectors_reference_their_objects_and_roundtrip() {
    let mut deck = Deck::new_blank("Connectors");
    let mut slide = Slide::default();
    let mut from = SlideObject::new("rect", 60.0, 60.0, 120.0, 80.0);
    from.id = "from-rect".into();
    from.z = 1;
    let mut to = SlideObject::new("ellipse", 400.0, 260.0, 120.0, 80.0);
    to.id = "to-ellipse".into();
    to.z = 2;
    let mut connector = SlideObject::new("arrow", 180.0, 100.0, 220.0, 160.0);
    connector.id = "connector".into();
    connector.z = 3;
    connector.line = Some(LineSpec {
        x2: 220.0,
        y2: 160.0,
        end_arrow: true,
        begin_object: Some("from-rect".into()),
        end_object: Some("to-ellipse".into()),
        begin_site: 3,
        end_site: 1,
        ..Default::default()
    });
    slide.objects = vec![from, to, connector];
    deck.slides = vec![slide];

    let bytes = write_pptx(&deck).unwrap();
    let slide_xml = ZipReader::open(bytes.clone()).unwrap().read_text("ppt/slides/slide1.xml").unwrap();
    assert!(slide_xml.contains("<a:stCxn id=\"2\" idx=\"3\"/>"), "slide: {slide_xml}");
    assert!(slide_xml.contains("<a:endCxn id=\"3\" idx=\"1\"/>"), "slide: {slide_xml}");

    let read = read_pptx(&bytes).unwrap();
    let objects = &read.deck.slides[0].objects;
    let connector = objects.iter().find(|object| object.kind == "line").expect("connector");
    let line = connector.line.as_ref().unwrap();
    assert_eq!(line.begin_site, 3);
    assert_eq!(line.end_site, 1);
    let begin = line.begin_object.clone().expect("begin target");
    let begin_object = objects.iter().find(|object| object.id == begin).expect("begin object");
    assert_eq!(begin_object.kind, "rect");
    let end = line.end_object.clone().expect("end target");
    let end_object = objects.iter().find(|object| object.id == end).expect("end object");
    assert_eq!(end_object.kind, "ellipse");
}

#[test]
fn pptx_image_crop_roundtrips() {
    let mut buffer = image::RgbaImage::new(4, 4);
    for pixel in buffer.pixels_mut() {
        *pixel = image::Rgba([10, 200, 90, 255]);
    }
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(buffer).write_to(&mut png, image::ImageFormat::Png).unwrap();
    let png = png.into_inner();

    let mut deck = Deck::new_blank("Crop");
    let mut slide = Slide::default();
    let mut object = SlideObject::new("image", 40.0, 40.0, 200.0, 150.0);
    let mut image = ImageData::from_bytes("pic.png", &png);
    image.crop = Some(ImageCrop { left: 0.1, top: 0.2, right: 0.3, bottom: 0.05 });
    object.image = Some(image);
    slide.objects = vec![object];
    deck.slides = vec![slide];

    let bytes = write_pptx(&deck).unwrap();
    let slide_xml = ZipReader::open(bytes.clone()).unwrap().read_text("ppt/slides/slide1.xml").unwrap();
    assert!(slide_xml.contains("<a:srcRect l=\"10000\" t=\"20000\" r=\"30000\" b=\"5000\"/>"), "slide: {slide_xml}");

    let read = read_pptx(&bytes).unwrap();
    let image = read.deck.slides[0].objects.iter().find_map(|object| object.image.clone()).expect("image");
    let crop = image.crop.expect("crop");
    assert!((crop.left - 0.1).abs() < 1e-9);
    assert!((crop.top - 0.2).abs() < 1e-9);
    assert!((crop.right - 0.3).abs() < 1e-9);
    assert!((crop.bottom - 0.05).abs() < 1e-9);

    let mut zero = deck.clone();
    zero.slides[0].objects[0].image.as_mut().unwrap().crop = Some(ImageCrop::default());
    let zero_xml = ZipReader::open(write_pptx(&zero).unwrap()).unwrap().read_text("ppt/slides/slide1.xml").unwrap();
    assert!(!zero_xml.contains("srcRect"), "an all-zero crop writes nothing: {zero_xml}");
}

#[test]
fn malformed_inputs_are_errors() {
    assert!(read_pptx(b"").is_err());
    assert!(read_pptx(&[3u8; 40]).is_err());
}
