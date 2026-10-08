//! V3 Impress coverage: masters and layouts, placeholders, nested groups,
//! ChartML charts and entrance/emphasis/exit animations through a real
//! write -> read cycle, plus what the ODP export keeps and degrades.

use officecore::model::*;
use officecore::zip::ZipReader;
use officecore::{odf, pptx};

fn placeholder_shape(role: &str, text: &str, x: f64, y: f64, w: f64, h: f64) -> SlideObject {
    let mut object = SlideObject::new("text", x, y, w, h);
    object.placeholder = Some(role.to_string());
    object.text = Some(TextFrame {
        paragraphs: vec![TextParagraph { text: text.to_string(), ..Default::default() }],
        ..Default::default()
    });
    object
}

fn layout(id: &str, name: &str, kind: &str) -> SlideLayout {
    SlideLayout {
        id: id.to_string(),
        name: name.to_string(),
        kind: kind.to_string(),
        objects: vec![
            placeholder_shape("title", name, 60.0, 40.0, 840.0, 80.0),
            placeholder_shape("body", "Body", 60.0, 140.0, 840.0, 340.0),
        ],
    }
}

fn grouped_object(kind: &str, id: &str, x: f64, y: f64, w: f64, h: f64, z: i32) -> SlideObject {
    let mut object = SlideObject::new(kind, x, y, w, h);
    object.id = id.to_string();
    object.z = z;
    object
}

fn sample_deck() -> Deck {
    let mut deck = Deck::new_blank("V3 deck");
    deck.theme = "minimal".into();

    let master_a = SlideMaster {
        id: "master-a".into(),
        name: "Master A".into(),
        theme: "business".into(),
        background: Some("#F8FAFC".into()),
        objects: vec![placeholder_shape("footer", "Confidential", 300.0, 496.0, 360.0, 28.0)],
        layouts: vec![
            layout("layout-a-title", "Title Slide", "title"),
            layout("layout-a-content", "Content", "titleContent"),
        ],
    };
    let master_b = SlideMaster {
        id: "master-b".into(),
        name: "Master B".into(),
        theme: "modern".into(),
        background: Some("#0F172A".into()),
        objects: Vec::new(),
        layouts: vec![layout("layout-b-title", "Title B", "title")],
    };
    deck.masters = vec![master_a, master_b];

    let mut inner = grouped_object("group", "group-inner", 280.0, 160.0, 180.0, 120.0, 2);
    inner.children = vec![
        grouped_object("ellipse", "leaf-ellipse", 300.0, 180.0, 60.0, 60.0, 1),
        grouped_object("roundRect", "leaf-rect", 340.0, 200.0, 80.0, 70.0, 2),
    ];
    let mut outer = grouped_object("group", "group-outer", 100.0, 120.0, 400.0, 300.0, 1);
    outer.children = vec![
        grouped_object("rect", "leaf-first", 120.0, 140.0, 100.0, 80.0, 1),
        inner,
        grouped_object("rect", "leaf-last", 140.0, 240.0, 80.0, 60.0, 3),
    ];

    let mut chart = SlideObject::new("chart", 500.0, 120.0, 400.0, 280.0);
    chart.id = "chart-1".into();
    chart.z = 3;
    chart.chart = Some(ChartData {
        kind: "column".into(),
        title: "Sales".into(),
        categories: "A2:A4".into(),
        series: vec![
            ChartSeries { name: "North".into(), range: "B2:B4".into(), color: Some("#1D4ED8".into()) },
            ChartSeries { name: "South".into(), range: "C2:C4".into(), color: Some("#DC2626".into()) },
        ],
        legend: true,
        x_title: "Quarter".into(),
        y_title: "Units".into(),
        stacked: false,
        show_labels: true,
        ..Default::default()
    });

    let mut badge = SlideObject::new("rect", 60.0, 440.0, 220.0, 60.0);
    badge.id = "badge".into();
    badge.z = 4;
    badge.style = Some(ShapeStyle { fill: Some("#1D4ED8".into()), ..Default::default() });
    badge.text = Some(TextFrame {
        paragraphs: vec![TextParagraph { text: "Badge".into(), ..Default::default() }],
        ..Default::default()
    });

    let mut title = placeholder_shape("title", "V3 slide", 60.0, 40.0, 840.0, 80.0);
    title.id = "slide-title".into();
    title.z = 0;

    let mut first = Slide {
        layout: "titleContent".into(),
        master_id: Some("master-a".into()),
        layout_id: Some("layout-a-title".into()),
        objects: vec![title, outer, chart, badge],
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
            object_id: "badge".into(),
            kind: "exit".into(),
            effect: "fade".into(),
            trigger: "afterPrevious".into(),
            duration_ms: 400,
            delay_ms: 100,
            order: 2,
        },
    ];

    let second = Slide {
        master_id: Some("master-b".into()),
        layout_id: Some("layout-b-title".into()),
        objects: vec![placeholder_shape("title", "Second slide", 60.0, 40.0, 840.0, 80.0)],
        ..Default::default()
    };

    deck.slides = vec![first, second];
    deck
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.6
}

#[test]
fn pptx_package_contains_the_v3_parts() {
    let write = pptx::write_pptx_package(&sample_deck()).unwrap();
    assert!(write.warnings.is_empty(), "unexpected warnings: {:?}", write.warnings);
    let reader = ZipReader::open(write.bytes).unwrap();
    for part in [
        "ppt/slideMasters/slideMaster1.xml",
        "ppt/slideMasters/slideMaster2.xml",
        "ppt/slideLayouts/slideLayout1.xml",
        "ppt/slideLayouts/slideLayout2.xml",
        "ppt/slideLayouts/slideLayout3.xml",
        "ppt/theme/theme1.xml",
        "ppt/theme/theme2.xml",
        "ppt/charts/chart1.xml",
        "ppt/slides/slide1.xml",
        "ppt/slides/slide2.xml",
    ] {
        assert!(reader.contains(part), "missing {part}");
    }

    let master1 = reader.read_text("ppt/slideMasters/slideMaster1.xml").unwrap();
    assert!(master1.contains("<p:sldLayoutIdLst>"));
    assert!(master1.contains("<p:ph type=\"ftr\""));
    assert!(master1.contains("<p:ph type=\"sldNum\""));
    assert!(master1.contains("<p:ph type=\"dt\""));

    let theme1 = reader.read_text("ppt/theme/theme1.xml").unwrap();
    assert!(theme1.contains("name=\"Business\""));

    let layout1 = reader.read_text("ppt/slideLayouts/slideLayout1.xml").unwrap();
    assert!(layout1.contains("<p:ph type=\"title\""));
    assert!(layout1.contains("<p:ph type=\"body\" idx=\"1\""));

    let slide1 = reader.read_text("ppt/slides/slide1.xml").unwrap();
    assert!(slide1.contains("<p:grpSp>"));
    assert!(slide1.contains("chOff"));
    assert!(slide1.contains("<p:timing>"));
    let slide1_rels = reader.read_text("ppt/slides/_rels/slide1.xml.rels").unwrap();
    assert!(slide1_rels.contains("slideLayout1.xml"));
    assert!(slide1_rels.contains("chart1.xml"));

    let chart = reader.read_text("ppt/charts/chart1.xml").unwrap();
    assert!(chart.contains("<c:barChart>"));
    assert!(chart.contains("A2:A4"));
    assert!(chart.contains("North"));
    assert!(chart.contains("South"));

    let content_types = reader.read_text("[Content_Types].xml").unwrap();
    assert!(content_types.contains("/ppt/charts/chart1.xml"));
    assert!(content_types.contains("/ppt/slideMasters/slideMaster2.xml"));
    assert!(content_types.contains("/ppt/slideLayouts/slideLayout3.xml"));

    let parts: Vec<String> =
        reader.names().filter(|name| name.ends_with(".xml") || name.ends_with(".rels")).map(str::to_string).collect();
    for part in parts {
        let text = reader.read_text(&part).unwrap();
        officecore::xml::parse_xml(&text).unwrap_or_else(|error| panic!("{part} is not well-formed XML: {error}"));
    }
}

#[test]
fn masters_layouts_groups_charts_and_animations_roundtrip() {
    let write = pptx::write_pptx_package(&sample_deck()).unwrap();
    let read = pptx::read_pptx(&write.bytes).unwrap();

    assert_eq!(read.deck.masters.len(), 2, "warnings: {:?}", read.warnings);
    assert_eq!(read.deck.masters[0].layouts.len(), 2);
    assert_eq!(read.deck.masters[1].layouts.len(), 1);
    assert_eq!(read.deck.masters[0].theme, "business");
    assert_eq!(read.deck.masters[1].theme, "modern");

    assert_eq!(read.deck.slides[0].master_id.as_deref(), Some(read.deck.masters[0].id.as_str()));
    assert_eq!(read.deck.slides[0].layout_id.as_deref(), Some(read.deck.masters[0].layouts[0].id.as_str()));
    assert_eq!(read.deck.slides[1].master_id.as_deref(), Some(read.deck.masters[1].id.as_str()));
    assert_eq!(read.deck.slides[1].layout_id.as_deref(), Some(read.deck.masters[1].layouts[0].id.as_str()));
    assert_ne!(read.deck.slides[0].layout_id, read.deck.slides[1].layout_id);

    let first_layout = &read.deck.masters[0].layouts[0];
    assert_eq!(first_layout.kind, "title");
    assert!(first_layout.objects.iter().any(|object| object.placeholder.as_deref() == Some("title")));
    assert!(first_layout.objects.iter().any(|object| object.placeholder.as_deref() == Some("body")));
    assert!(read.deck.masters[0].objects.iter().any(|object| object.placeholder.as_deref() == Some("footer")));
    assert!(read.deck.masters[0].objects.iter().any(|object| object.placeholder.as_deref() == Some("slideNumber")));
    assert_eq!(read.deck.masters[0].layouts[1].kind, "titleContent");

    let group = read.deck.slides[0].objects.iter().find(|object| object.kind == "group").expect("outer group");
    assert_eq!(group.children.len(), 3);
    assert!(close(group.x, 100.0) && close(group.y, 120.0) && close(group.w, 400.0) && close(group.h, 300.0));

    let nested = group.children.iter().find(|object| object.kind == "group").expect("nested group");
    assert_eq!(nested.children.len(), 2);
    assert!(close(nested.x, 280.0) && close(nested.y, 160.0) && close(nested.w, 180.0) && close(nested.h, 120.0));

    let ellipse = nested.children.iter().find(|object| object.kind == "ellipse").expect("nested ellipse");
    assert!(close(ellipse.x, 300.0) && close(ellipse.y, 180.0) && close(ellipse.w, 60.0) && close(ellipse.h, 60.0));
    let leaf_rect = nested.children.iter().find(|object| object.kind == "roundRect").expect("nested rect");
    assert!(
        close(leaf_rect.x, 340.0) && close(leaf_rect.y, 200.0) && close(leaf_rect.w, 80.0) && close(leaf_rect.h, 70.0)
    );
    let first_leaf = group.children.iter().find(|object| object.kind == "rect").expect("group rect");
    assert!(
        close(first_leaf.x, 120.0)
            && close(first_leaf.y, 140.0)
            && close(first_leaf.w, 100.0)
            && close(first_leaf.h, 80.0)
    );

    let chart = read.deck.slides[0].objects.iter().find_map(|object| object.chart.as_ref()).expect("chart");
    assert_eq!(chart.kind, "column");
    assert_eq!(chart.title, "Sales");
    assert_eq!(chart.categories, "A2:A4");
    assert_eq!(chart.series.len(), 2);
    assert_eq!(chart.series[0].name, "North");
    assert_eq!(chart.series[0].range, "B2:B4");
    assert_eq!(chart.series[0].color.as_deref(), Some("#1D4ED8"));
    assert_eq!(chart.series[1].name, "South");
    assert_eq!(chart.series[1].range, "C2:C4");
    assert_eq!(chart.series[1].color.as_deref(), Some("#DC2626"));
    assert!(chart.legend);
    assert_eq!(chart.x_title, "Quarter");
    assert_eq!(chart.y_title, "Units");
    assert!(!chart.stacked);
    assert!(chart.show_labels);

    let animations = &read.deck.slides[0].animations;
    assert_eq!(animations.len(), 3);
    let entrance = animations.iter().find(|animation| animation.kind == "entrance").expect("entrance");
    assert_eq!(entrance.effect, "fade");
    assert_eq!(entrance.trigger, "onClick");
    assert_eq!(entrance.duration_ms, 500);
    assert_eq!(entrance.delay_ms, 0);
    let emphasis = animations.iter().find(|animation| animation.kind == "emphasis").expect("emphasis");
    assert_eq!(emphasis.effect, "pulse");
    assert_eq!(emphasis.trigger, "withPrevious");
    assert_eq!(emphasis.duration_ms, 800);
    assert_eq!(emphasis.delay_ms, 200);
    let exit = animations.iter().find(|animation| animation.kind == "exit").expect("exit");
    assert_eq!(exit.effect, "fade");
    assert_eq!(exit.trigger, "afterPrevious");
    assert_eq!(exit.duration_ms, 400);
    assert_eq!(exit.delay_ms, 100);

    assert_eq!(animations[0].kind, "entrance");
    assert_eq!(animations[1].kind, "emphasis");
    assert_eq!(animations[2].kind, "exit");

    for animation in animations {
        assert!(
            read.deck.slides[0].objects.iter().any(|object| object.id == animation.object_id),
            "animation target {} is not a slide object",
            animation.object_id
        );
    }
}

#[test]
fn a_second_write_read_cycle_stays_stable() {
    let first = pptx::read_pptx(&pptx::write_pptx(&sample_deck()).unwrap()).unwrap().deck;
    let second = pptx::read_pptx(&pptx::write_pptx(&first).unwrap()).unwrap().deck;
    assert_eq!(second.masters.len(), 2);
    assert_eq!(first.slides[0].layout_id, second.slides[0].layout_id);
    assert_eq!(first.slides[1].layout_id, second.slides[1].layout_id);
    assert_eq!(first.masters[0].layouts.len(), second.masters[0].layouts.len());
    let mut roles: Vec<String> =
        second.masters[0].objects.iter().filter_map(|object| object.placeholder.clone()).collect();
    let total = roles.len();
    roles.sort();
    roles.dedup();
    assert_eq!(roles.len(), total, "master placeholders must not be duplicated on re-export");
    assert_eq!(second.slides[0].animations.len(), 3);
    let group = second.slides[0].objects.iter().find(|object| object.kind == "group").expect("group");
    assert_eq!(group.children.len(), 3);
    let nested = group.children.iter().find(|object| object.kind == "group").expect("nested");
    assert_eq!(nested.children.len(), 2);
    let chart = second.slides[0].objects.iter().find_map(|object| object.chart.as_ref()).expect("chart");
    assert_eq!(chart.series.len(), 2);
    assert_eq!(chart.categories, "A2:A4");
}

#[test]
fn unsupported_charts_stay_in_the_native_file_with_a_warning() {
    let mut deck = sample_deck();
    if let Some(object) = deck.slides[0].objects.iter_mut().find(|object| object.chart.is_some()) {
        if let Some(chart) = object.chart.as_mut() {
            chart.kind = "scatter".into();
        }
    }
    let write = pptx::write_pptx_package(&deck).unwrap();
    assert!(
        write.warnings.iter().any(|warning| warning.contains("scatter") && warning.contains(".oswk")),
        "warnings: {:?}",
        write.warnings
    );
    let reader = ZipReader::open(write.bytes.clone()).unwrap();
    assert!(!reader.contains("ppt/charts/chart1.xml"));
    let read = pptx::read_pptx(&write.bytes).unwrap();
    assert!(read.deck.slides[0].objects.iter().all(|object| object.chart.is_none()));
}

#[test]
fn malformed_masters_fall_back_without_failing_the_import() {
    let write = pptx::write_pptx_package(&sample_deck()).unwrap();
    let reader = ZipReader::open(write.bytes).unwrap();
    let parts = reader.read_all(officecore::zip::ZipLimits::default()).unwrap();
    let mut zip = officecore::zip::ZipWriter::new();
    for (name, data) in parts {
        if name == "ppt/slideMasters/slideMaster1.xml" {
            zip.add(&name, b"<p:sldMaster><p:ph type==\"title\"/></p:sldMaster>");
        } else {
            zip.add(&name, &data);
        }
    }
    let read = pptx::read_pptx(&zip.finish()).unwrap();
    assert!(read.deck.masters.is_empty());
    assert_eq!(read.deck.slides.len(), 2);
    assert!(
        read.warnings.iter().any(|warning| warning.to_ascii_lowercase().contains("master")),
        "warnings: {:?}",
        read.warnings
    );
}

fn cached_chart_deck() -> Deck {
    let mut deck = Deck::new_blank("Cached chart");
    let mut chart = SlideObject::new("chart", 80.0, 60.0, 520.0, 320.0);
    chart.id = "chart-cached".into();
    chart.z = 1;
    chart.chart = Some(ChartData {
        kind: "column".into(),
        title: "Cached sales".into(),
        categories: "A2:A4".into(),
        series: vec![
            ChartSeries { name: "North".into(), range: "B2:B4".into(), color: Some("#1D4ED8".into()) },
            ChartSeries { name: "South".into(), range: "C2:C4".into(), color: None },
        ],
        legend: true,
        x_title: String::new(),
        y_title: String::new(),
        stacked: false,
        show_labels: false,
        categories_cache: vec!["Q1".into(), "Q2".into(), "Q3".into()],
        series_values_cache: vec![vec![10.0, 20.5, 31.0], vec![5.0, 6.0, 7.0]],
        ..Default::default()
    });
    deck.slides = vec![Slide { objects: vec![chart], ..Default::default() }];
    deck
}

#[test]
fn charts_with_caches_export_an_embedded_workbook_and_roundtrip() {
    let deck = cached_chart_deck();
    let write = pptx::write_pptx_package(&deck).unwrap();
    assert!(write.warnings.is_empty(), "unexpected warnings: {:?}", write.warnings);

    let reader = ZipReader::open(write.bytes.clone()).unwrap();
    assert!(reader.contains("ppt/embeddings/Microsoft_Excel_Worksheet1.xlsx"));
    assert!(reader.contains("ppt/charts/_rels/chart1.xml.rels"));
    let content_types = reader.read_text("[Content_Types].xml").unwrap();
    assert!(content_types.contains("spreadsheetml.sheet"), "content types: {content_types}");
    let chart_xml = reader.read_text("ppt/charts/chart1.xml").unwrap();
    officecore::xml::parse_xml(&chart_xml).unwrap_or_else(|error| panic!("chart XML is malformed: {error}"));
    assert!(chart_xml.contains("<c:strCache>"), "chart: {chart_xml}");
    assert!(chart_xml.contains("<c:numCache>"));
    assert!(chart_xml.contains("<c:pt idx=\"0\"><c:v>Q1</c:v></c:pt>"));
    assert!(chart_xml.contains("<c:pt idx=\"1\"><c:v>20.5</c:v></c:pt>"));
    assert!(chart_xml.contains("<c:externalData r:id=\"rId1\">"));
    let chart_rels = reader.read_text("ppt/charts/_rels/chart1.xml.rels").unwrap();
    officecore::xml::parse_xml(&chart_rels).unwrap_or_else(|error| panic!("chart rels are malformed: {error}"));
    assert!(chart_rels.contains("relationships/package"), "rels: {chart_rels}");
    assert!(chart_rels.contains("../embeddings/Microsoft_Excel_Worksheet1.xlsx"));

    // The embedded workbook must be a real xlsx package: validate it with the
    // crate's own reader, independently of the chart code.
    let workbook_bytes = reader.read("ppt/embeddings/Microsoft_Excel_Worksheet1.xlsx").unwrap();
    let workbook = officecore::xlsx::read_workbook_bytes(&workbook_bytes).unwrap();
    assert_eq!(workbook.workbook.sheets.len(), 1);
    let sheet = &workbook.workbook.sheets[0];
    assert_eq!(sheet.get("A2").map(|cell| cell.value.clone()), Some(CellValue::Text("Q1".into())));
    assert_eq!(sheet.get("B3").map(|cell| cell.value.clone()), Some(CellValue::Number(20.5)));
    assert_eq!(sheet.get("C4").map(|cell| cell.value.clone()), Some(CellValue::Number(7.0)));

    // Import restores the cache fields and keeps the range information.
    let read = pptx::read_pptx(&write.bytes).unwrap();
    let chart = read.deck.slides[0].objects.iter().find_map(|object| object.chart.as_ref()).expect("chart");
    assert_eq!(chart.categories, "A2:A4");
    assert_eq!(chart.series[0].range, "B2:B4");
    assert_eq!(chart.categories_cache, vec!["Q1".to_string(), "Q2".to_string(), "Q3".to_string()]);
    assert_eq!(chart.series_values_cache, vec![vec![10.0, 20.5, 31.0], vec![5.0, 6.0, 7.0]]);

    // A second write -> read cycle stays stable.
    let second = pptx::read_pptx(&pptx::write_pptx(&read.deck).unwrap()).unwrap().deck;
    let chart = second.slides[0].objects.iter().find_map(|object| object.chart.as_ref()).expect("chart");
    assert_eq!(chart.categories_cache.len(), 3);
    assert_eq!(chart.series_values_cache[0], vec![10.0, 20.5, 31.0]);
    assert_eq!(chart.series_values_cache[1], vec![5.0, 6.0, 7.0]);
}

#[test]
fn charts_without_caches_stay_range_only() {
    let write = pptx::write_pptx_package(&sample_deck()).unwrap();
    let reader = ZipReader::open(write.bytes).unwrap();
    assert!(!reader.names().any(|name| name.contains("embeddings")), "range-only charts must not embed a workbook");
    let chart_xml = reader.read_text("ppt/charts/chart1.xml").unwrap();
    assert!(!chart_xml.contains("<c:strCache>"));
    assert!(!chart_xml.contains("<c:numCache>"));
    assert!(!chart_xml.contains("c:externalData"));
}

#[test]
fn odp_keeps_groups_and_animations_and_degrades_charts_with_a_warning() {
    let write = odf::write_odp_package(&sample_deck()).unwrap();
    assert!(
        !write.warnings.iter().any(|warning| warning.contains("individual shapes")),
        "warnings: {:?}",
        write.warnings
    );
    assert!(write.warnings.iter().any(|warning| warning.contains("Chart data is kept in the native .oswk file")));
    assert!(!write.warnings.iter().any(|warning| warning.contains("Animations are kept in the native .oswk file")));
    let reader = ZipReader::open(write.bytes).unwrap();
    let content = reader.read_text("content.xml").unwrap();
    assert!(content.contains("Sales"), "the chart placeholder must not be dropped silently");
    assert!(content.contains("Badge"));
    assert!(content.contains("<draw:g "), "groups are written as draw:g");
    assert!(content.contains("smil:targetElement=\"chart-1\""), "animations are written as SMIL timing");
}
