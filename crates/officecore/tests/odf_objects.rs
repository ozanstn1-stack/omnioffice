//! ODF object coverage through real write -> read cycles: nested ODP groups,
//! ODP slide timing (SMIL animations with LibreOffice presets), ODS charts as
//! embedded chart objects and ODS pivot output written as plain values.

use officecore::compat;
use officecore::model::*;
use officecore::odf;
use officecore::zip::ZipReader;

fn shape(kind: &str, id: &str, x: f64, y: f64, w: f64, h: f64, z: i32) -> SlideObject {
    let mut object = SlideObject::new(kind, x, y, w, h);
    object.id = id.to_string();
    object.z = z;
    object
}

fn group(id: &str, z: i32, children: Vec<SlideObject>) -> SlideObject {
    // The editor sizes a group as the bounding box of its children.
    let left = children.iter().map(|child| child.x).fold(f64::INFINITY, f64::min);
    let top = children.iter().map(|child| child.y).fold(f64::INFINITY, f64::min);
    let right = children.iter().map(|child| child.x + child.w).fold(f64::NEG_INFINITY, f64::max);
    let bottom = children.iter().map(|child| child.y + child.h).fold(f64::NEG_INFINITY, f64::max);
    let mut object = shape("group", id, left, top, right - left, bottom - top, z);
    object.name = format!("Group {id}");
    object.children = children;
    object
}

fn text_box(id: &str, text: &str, x: f64, y: f64, z: i32) -> SlideObject {
    let mut object = shape("text", id, x, y, 300.0, 60.0, z);
    object.text = Some(TextFrame {
        paragraphs: vec![TextParagraph { text: text.to_string(), ..Default::default() }],
        ..Default::default()
    });
    object
}

fn grouped_slide() -> Slide {
    let mut first = shape("rect", "leaf-first", 120.0, 140.0, 100.0, 80.0, 1);
    first.style = Some(ShapeStyle { fill: Some("#1D4ED8".into()), ..Default::default() });
    let inner = group(
        "group-inner",
        2,
        vec![
            shape("rect", "leaf-a", 300.0, 180.0, 60.0, 60.0, 1),
            text_box("leaf-text", "Inside the inner group", 340.0, 200.0, 2),
        ],
    );
    let outer = group("group-outer", 2, vec![first, inner, shape("rect", "leaf-last", 140.0, 240.0, 80.0, 60.0, 3)]);
    Slide { objects: vec![text_box("title-box", "Grouped slide", 60.0, 40.0, 1), outer], ..Default::default() }
}

fn animation(object_id: &str, kind: &str, effect: &str, trigger: &str, duration_ms: u32, delay_ms: u32) -> Animation {
    Animation {
        id: format!("anim-{object_id}-{effect}"),
        object_id: object_id.to_string(),
        kind: kind.to_string(),
        effect: effect.to_string(),
        trigger: trigger.to_string(),
        duration_ms,
        delay_ms,
        order: 0,
    }
}

/// Every editor effect, all three triggers and a spread of durations/delays.
fn animated_deck() -> Deck {
    let mut slide = grouped_slide();
    let mut animations = vec![
        animation("title-box", "entrance", "appear", "onClick", 500, 0),
        animation("group-outer", "entrance", "fade", "withPrevious", 750, 250),
        animation("leaf-first", "entrance", "flyIn", "afterPrevious", 400, 100),
        animation("leaf-a", "entrance", "zoom", "onClick", 1200, 0),
        animation("leaf-text", "emphasis", "pulse", "afterPrevious", 600, 0),
        animation("group-inner", "emphasis", "spin", "withPrevious", 2000, 500),
        animation("leaf-last", "emphasis", "grow", "onClick", 900, 0),
        animation("leaf-first", "emphasis", "shrink", "afterPrevious", 300, 50),
        animation("leaf-a", "exit", "disappear", "onClick", 250, 0),
        animation("leaf-text", "exit", "fadeOut", "withPrevious", 1000, 0),
        animation("group-outer", "exit", "flyOut", "afterPrevious", 650, 1500),
    ];
    for (order, entry) in animations.iter_mut().enumerate() {
        entry.order = order as u32;
    }
    slide.animations = animations;
    let mut deck = Deck::new_blank("ODP objects");
    deck.slides = vec![slide, Slide::default()];
    deck
}

fn find<'a>(objects: &'a [SlideObject], id: &str) -> Option<&'a SlideObject> {
    objects.iter().find_map(|object| if object.id == id { Some(object) } else { find(&object.children, id) })
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.05
}

fn assert_geometry(read: &SlideObject, source: &SlideObject) {
    assert!(
        close(read.x, source.x) && close(read.y, source.y) && close(read.w, source.w) && close(read.h, source.h),
        "{}: read ({}, {}, {}, {}), wrote ({}, {}, {}, {})",
        source.id,
        read.x,
        read.y,
        read.w,
        read.h,
        source.x,
        source.y,
        source.w,
        source.h
    );
}

#[test]
fn odp_nested_groups_round_trip_with_child_geometry() {
    let deck = animated_deck();
    let write = odf::write_odp_package(&deck).unwrap();
    assert!(
        !write.warnings.iter().any(|warning| warning.contains("individual shapes")),
        "groups must not be flattened: {:?}",
        write.warnings
    );
    let content = ZipReader::open(write.bytes.clone()).unwrap().read_text("content.xml").unwrap();
    assert_eq!(content.matches("<draw:g ").count(), 2, "both groups are written as draw:g: {content}");
    assert!(content.contains("<draw:g draw:name=\"Group group-outer\" draw:id=\"group-outer\" xml:id=\"group-outer\""));

    let read = odf::read_odp(&write.bytes).unwrap();
    let objects = &read.deck.slides[0].objects;
    assert_eq!(objects.len(), 2, "title and outer group stay the only top-level objects");
    let outer = find(objects, "group-outer").expect("outer group");
    assert_eq!(outer.kind, "group");
    assert_eq!(outer.name, "Group group-outer");
    let child_ids: Vec<&str> = outer.children.iter().map(|child| child.id.as_str()).collect();
    assert_eq!(child_ids, vec!["leaf-first", "group-inner", "leaf-last"]);
    let inner = &outer.children[1];
    assert_eq!(inner.kind, "group");
    assert_eq!(inner.children.len(), 2);
    assert_eq!(
        inner.children[1].text.as_ref().map(TextFrame::plain).as_deref(),
        Some("Inside the inner group"),
        "a text box inside a nested group keeps its text"
    );
    assert_eq!(
        find(objects, "leaf-first").and_then(|object| object.style.as_ref()).and_then(|style| style.fill.clone()),
        Some("#1D4ED8".to_string())
    );

    // Every child keeps its absolute geometry, and each group's box is the
    // bounding box of its children, which is how the editor built them.
    let source = &deck.slides[0].objects;
    for id in ["group-outer", "group-inner", "leaf-first", "leaf-a", "leaf-text", "leaf-last", "title-box"] {
        assert_geometry(find(objects, id).unwrap(), find(source, id).unwrap());
    }

    // A second cycle is stable.
    let again = odf::read_odp(&odf::write_odp(&read.deck).unwrap()).unwrap();
    let outer_again = find(&again.deck.slides[0].objects, "group-outer").unwrap();
    assert_eq!(outer_again.children.len(), 3);
    assert_eq!(outer_again.children[1].children.len(), 2);
}

#[test]
fn odp_animations_round_trip_through_smil_timing() {
    let deck = animated_deck();
    let write = odf::write_odp_package(&deck).unwrap();
    assert!(write.warnings.is_empty(), "editor effects need no simplification: {:?}", write.warnings);
    let content = ZipReader::open(write.bytes.clone()).unwrap().read_text("content.xml").unwrap();
    assert!(content.contains("xmlns:anim=\"urn:oasis:names:tc:opendocument:xmlns:animation:1.0\""));
    assert!(content.contains("xmlns:smil=\"urn:oasis:names:tc:opendocument:xmlns:smil-compatible:1.0\""));
    assert!(content.contains(
        "<anim:par presentation:node-type=\"timing-root\"><anim:seq presentation:node-type=\"main-sequence\">"
    ));
    for preset in [
        "ooo-entrance-appear",
        "ooo-entrance-fade-in",
        "ooo-entrance-fly-in",
        "ooo-entrance-zoom",
        "ooo-emphasis-flash-bulb",
        "ooo-emphasis-spin",
        "ooo-emphasis-grow-and-shrink",
        "ooo-exit-disappear",
        "ooo-exit-fade-out",
        "ooo-exit-fly-out",
    ] {
        assert!(content.contains(&format!("presentation:preset-id=\"{preset}\"")), "missing {preset}");
    }
    for node_type in ["on-click", "with-previous", "after-previous"] {
        assert!(content.contains(&format!("presentation:node-type=\"{node_type}\"")), "missing {node_type}");
    }
    for class in ["entrance", "emphasis", "exit"] {
        assert!(content.contains(&format!("presentation:preset-class=\"{class}\"")));
    }
    assert!(content.contains("smil:targetElement=\"leaf-text\""));
    assert!(content.contains("smil:begin=\"1.5s\""), "the flyOut delay is the effect's begin");
    assert!(content.contains("smil:dur=\"2s\""), "the spin duration is on its animation node");
    // The timing sits after the shapes and before the notes of the page.
    let timing = content.find("timing-root").unwrap();
    assert!(content.rfind("</draw:g>").unwrap() < timing);

    let read = odf::read_odp(&write.bytes).unwrap();
    let slide = &read.deck.slides[0];
    assert!(read.deck.slides[1].animations.is_empty());
    let expected: Vec<(String, String, String, String, u32, u32, u32)> = deck.slides[0]
        .animations
        .iter()
        .map(|entry| {
            (
                entry.object_id.clone(),
                entry.kind.clone(),
                entry.effect.clone(),
                entry.trigger.clone(),
                entry.duration_ms,
                entry.delay_ms,
                entry.order,
            )
        })
        .collect();
    let actual: Vec<(String, String, String, String, u32, u32, u32)> = slide
        .animations
        .iter()
        .map(|entry| {
            (
                entry.object_id.clone(),
                entry.kind.clone(),
                entry.effect.clone(),
                entry.trigger.clone(),
                entry.duration_ms,
                entry.delay_ms,
                entry.order,
            )
        })
        .collect();
    assert_eq!(actual, expected, "warnings: {:?}", read.warnings);
    for entry in &slide.animations {
        assert!(find(&slide.objects, &entry.object_id).is_some(), "{} targets an imported shape", entry.object_id);
    }
}

#[test]
fn odp_animation_targets_get_xml_ids_when_the_model_id_is_not_an_ncname() {
    let mut deck = Deck::new_blank("Ids");
    // A UUID that starts with a digit is not a valid xml:id, and the same id on
    // two slides would not be unique in content.xml.
    let slide = |id: &str, text: &str, kind: &str, effect: &str, trigger: &str| Slide {
        objects: vec![text_box(id, text, 40.0, 40.0, 1)],
        animations: vec![animation(id, kind, effect, trigger, 500, 0)],
        ..Default::default()
    };
    deck.slides = vec![
        slide("1f0c5d2e-uuid", "First", "entrance", "fade", "onClick"),
        slide("shape2", "Second", "exit", "fadeOut", "onClick"),
        slide("shape2", "Third", "emphasis", "spin", "afterPrevious"),
    ];

    let bytes = odf::write_odp(&deck).unwrap();
    let content = ZipReader::open(bytes.clone()).unwrap().read_text("content.xml").unwrap();
    assert!(!content.contains("xml:id=\"1f0c5d2e-uuid\""));
    assert_eq!(content.matches("xml:id=\"shape2\"").count(), 1, "xml:id stays unique across slides");

    let read = odf::read_odp(&bytes).unwrap();
    for slide in &read.deck.slides {
        assert_eq!(slide.animations.len(), 1, "warnings: {:?}", read.warnings);
        assert_eq!(slide.animations[0].object_id, slide.objects[0].id);
    }
    assert_eq!(read.deck.slides[1].objects[0].id, "shape2");
    assert_eq!(read.deck.slides[2].animations[0].trigger, "afterPrevious");
}

#[test]
fn odp_foreign_effects_are_written_as_the_closest_preset_with_a_warning() {
    let mut deck = Deck::new_blank("Foreign effects");
    deck.slides = vec![Slide {
        objects: vec![text_box("box", "Box", 40.0, 40.0, 1)],
        animations: vec![
            animation("box", "entrance", "wipe(down)", "onClick", 500, 0),
            animation("box", "exit", "fade", "onClick", 500, 0),
            animation("missing", "entrance", "fade", "onClick", 500, 0),
        ],
        ..Default::default()
    }];
    let write = odf::write_odp_package(&deck).unwrap();
    assert!(write
        .warnings
        .iter()
        .any(|warning| warning.contains("\"wipe(down)\"") && warning.contains("ooo-entrance-fade-in")));
    assert!(
        !write.warnings.iter().any(|warning| warning.contains("\"fade\" has no")),
        "a PPTX exit fade is the editor's fadeOut"
    );
    assert!(write.warnings.iter().any(|warning| warning.contains("\"missing\"")));
    let read = odf::read_odp(&write.bytes).unwrap();
    let effects: Vec<&str> = read.deck.slides[0].animations.iter().map(|entry| entry.effect.as_str()).collect();
    assert_eq!(effects, vec!["fade", "fadeOut"]);
}

/// A minimal LibreOffice-style page with presets the editor does not have.
fn libreoffice_timing_package() -> Vec<u8> {
    let content = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
        "<office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
        "xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" ",
        "xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" ",
        "xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\" ",
        "xmlns:presentation=\"urn:oasis:names:tc:opendocument:xmlns:presentation:1.0\" ",
        "xmlns:anim=\"urn:oasis:names:tc:opendocument:xmlns:animation:1.0\" ",
        "xmlns:smil=\"urn:oasis:names:tc:opendocument:xmlns:smil-compatible:1.0\" office:version=\"1.2\">",
        "<office:body><office:presentation><draw:page draw:name=\"page1\">",
        "<draw:g><draw:frame draw:id=\"id1\" xml:id=\"id1\" svg:x=\"1cm\" svg:y=\"1cm\" svg:width=\"4cm\" svg:height=\"2cm\">",
        "<draw:text-box><text:p>Wiped</text:p></draw:text-box></draw:frame></draw:g>",
        "<anim:par presentation:node-type=\"timing-root\"><anim:seq presentation:node-type=\"main-sequence\">",
        "<anim:par smil:begin=\"indefinite\"><anim:par smil:begin=\"0s\">",
        "<anim:par smil:begin=\"0.25s\" smil:fill=\"hold\" presentation:node-type=\"on-click\" ",
        "presentation:preset-class=\"entrance\" presentation:preset-id=\"ooo-entrance-wipe\" presentation:preset-sub-type=\"from-bottom\">",
        "<anim:set smil:begin=\"0s\" smil:dur=\"0.001s\" smil:fill=\"hold\" smil:targetElement=\"id1\" smil:attributeName=\"visibility\" smil:to=\"visible\"/>",
        "<anim:transitionFilter smil:dur=\"0.5s\" smil:targetElement=\"id1\" smil:type=\"barWipe\" smil:subtype=\"topToBottom\"/>",
        "</anim:par></anim:par></anim:par>",
        "<anim:par smil:begin=\"indefinite\"><anim:par smil:begin=\"0s\">",
        "<anim:par smil:begin=\"0s\" smil:fill=\"hold\" presentation:node-type=\"on-click\" ",
        "presentation:preset-class=\"emphasis\" presentation:preset-id=\"ooo-emphasis-blink\">",
        "<anim:animate smil:dur=\"1s\" smil:fill=\"hold\" smil:targetElement=\"id1\" smil:attributeName=\"visibility\" smil:values=\"hidden;visible\" smil:keyTimes=\"0;0.5\"/>",
        "</anim:par>",
        "<anim:par smil:begin=\"0s\" smil:fill=\"hold\" presentation:node-type=\"with-previous\" ",
        "presentation:preset-class=\"emphasis\" presentation:preset-id=\"ooo-emphasis-grow-and-shrink\">",
        "<anim:animateTransform smil:dur=\"2s\" smil:fill=\"hold\" smil:targetElement=\"id1\" smil:by=\"0.25,0.25\" svg:type=\"scale\"/>",
        "</anim:par>",
        "<anim:par smil:begin=\"0s\" smil:fill=\"hold\" presentation:node-type=\"after-previous\" ",
        "presentation:preset-class=\"motion-path\" presentation:preset-id=\"ooo-motionpath-circle\">",
        "<anim:animateMotion smil:dur=\"2s\" smil:targetElement=\"id1\"/>",
        "</anim:par></anim:par></anim:par>",
        "</anim:seq></anim:par></draw:page></office:presentation></office:body></office:document-content>"
    );
    let mut zip = officecore::zip::ZipWriter::new();
    zip.add_text("mimetype", "application/vnd.oasis.opendocument.presentation");
    zip.add_text("content.xml", content);
    zip.finish()
}

#[test]
fn odp_unknown_presets_import_as_the_closest_effect_with_a_warning() {
    let read = odf::read_odp(&libreoffice_timing_package()).unwrap();
    let slide = &read.deck.slides[0];
    assert_eq!(slide.objects[0].kind, "group");
    assert_eq!(slide.objects[0].children[0].id, "id1");
    let animations: Vec<(&str, &str, &str, u32, u32)> = slide
        .animations
        .iter()
        .map(|entry| {
            (entry.kind.as_str(), entry.effect.as_str(), entry.trigger.as_str(), entry.duration_ms, entry.delay_ms)
        })
        .collect();
    assert_eq!(
        animations,
        vec![
            ("entrance", "fade", "onClick", 500, 250),
            ("emphasis", "pulse", "onClick", 1000, 0),
            ("emphasis", "shrink", "withPrevious", 2000, 0),
        ]
    );
    assert!(slide.animations.iter().all(|entry| entry.object_id == "id1"));
    assert!(read.warnings.iter().any(|warning| warning.contains("ooo-entrance-wipe") && warning.contains("\"fade\"")));
    assert!(read
        .warnings
        .iter()
        .any(|warning| warning.contains("ooo-emphasis-blink") && warning.contains("\"pulse\"")));
    assert!(read.warnings.iter().any(|warning| warning.contains("motion-path")));
}

// ---------------------------------------------------------------------------
// ODS
// ---------------------------------------------------------------------------

fn number(value: f64) -> Cell {
    Cell { value: CellValue::Number(value), ..Default::default() }
}

fn text(value: &str) -> Cell {
    Cell { value: CellValue::Text(value.to_string()), ..Default::default() }
}

fn chart_workbook() -> Workbook {
    let mut workbook = Workbook::new_blank("Charts");
    let sheet = &mut workbook.sheets[0];
    sheet.name = "Sales Data".into();
    for (row, (label, north, south)) in [("Q1", 10.0, 4.0), ("Q2", 20.5, 6.0), ("Q3", 31.0, 7.5)].iter().enumerate() {
        let row = row + 2;
        sheet.set(&format!("A{row}"), text(label));
        sheet.set(&format!("B{row}"), number(*north));
        sheet.set(&format!("C{row}"), number(*south));
    }
    sheet.set("B1", text("North"));
    sheet.set("C1", text("South"));
    sheet.charts = vec![
        ChartPlacement {
            id: "chart-column".into(),
            chart: ChartData {
                kind: "column".into(),
                title: "Sales by quarter".into(),
                categories: "A2:A4".into(),
                series: vec![
                    ChartSeries { name: "North".into(), range: "B2:B4".into(), color: Some("#1D4ED8".into()) },
                    ChartSeries { name: "South & \"co\"".into(), range: "C2:C4".into(), color: Some("#DC2626".into()) },
                ],
                legend: true,
                x_title: "Quarter".into(),
                y_title: "Units".into(),
                stacked: true,
                show_labels: true,
                // Caches that differ from the cells prove they come from the
                // chart's local table and not from the sheet.
                categories_cache: vec!["Q1 cached".into(), "Q2 cached".into(), "Q3 cached".into()],
                series_values_cache: vec![vec![11.0, 21.5, 32.0], vec![5.0, 7.0, 8.5]],
            },
            anchor: "E2".into(),
            width_px: 480.0,
            height_px: 288.0,
        },
        ChartPlacement {
            id: "chart-pie".into(),
            chart: ChartData {
                kind: "pie".into(),
                title: "Share".into(),
                categories: "A2:A4".into(),
                series: vec![ChartSeries { name: "North".into(), range: "B2:B4".into(), color: None }],
                legend: false,
                ..Default::default()
            },
            anchor: "B8".into(),
            width_px: 320.0,
            height_px: 240.0,
        },
    ];
    workbook
}

#[test]
fn ods_column_and_pie_charts_round_trip() {
    let workbook = chart_workbook();
    let bytes = odf::write_ods(&workbook).unwrap();
    let read = odf::read_ods(&bytes).unwrap();
    let sheet = &read.workbook.sheets[0];
    assert_eq!(sheet.get("B3").map(|cell| cell.value.clone()), Some(CellValue::Number(20.5)));
    assert_eq!(sheet.charts.len(), 2, "warnings: {:?}", read.warnings);

    let column = sheet.charts.iter().find(|chart| chart.id == "chart-column").expect("column chart");
    let source = &workbook.sheets[0].charts[0];
    assert_eq!(column.anchor, "E2");
    assert!((column.width_px - 480.0).abs() < 0.1 && (column.height_px - 288.0).abs() < 0.1);
    assert_eq!(column.chart, source.chart, "every column chart field round-trips");

    let pie = sheet.charts.iter().find(|chart| chart.id == "chart-pie").expect("pie chart");
    assert_eq!(pie.anchor, "B8");
    assert_eq!(pie.chart.kind, "pie");
    assert_eq!(pie.chart.title, "Share");
    assert!(!pie.chart.legend);
    assert_eq!(pie.chart.categories, "A2:A4");
    assert_eq!(pie.chart.series, workbook.sheets[0].charts[1].chart.series);
    // A chart without caches gets its local table from the sheet cells.
    assert_eq!(pie.chart.categories_cache, vec!["Q1".to_string(), "Q2".to_string(), "Q3".to_string()]);
    assert_eq!(pie.chart.series_values_cache, vec![vec![10.0, 20.5, 31.0]]);

    // A second cycle keeps the charts unchanged.
    let again = odf::read_ods(&odf::write_ods(&read.workbook).unwrap()).unwrap();
    let charts: Vec<&ChartData> = again.workbook.sheets[0].charts.iter().map(|placement| &placement.chart).collect();
    assert_eq!(charts, sheet.charts.iter().map(|placement| &placement.chart).collect::<Vec<_>>());
}

#[test]
fn ods_chart_objects_are_listed_in_the_manifest() {
    let bytes = odf::write_ods(&chart_workbook()).unwrap();
    let reader = ZipReader::open(bytes).unwrap();
    let manifest = reader.read_text("META-INF/manifest.xml").unwrap();
    for object in ["Object 1", "Object 2"] {
        assert!(
            manifest.contains(&format!(
                "manifest:full-path=\"{object}/\" manifest:version=\"1.2\" manifest:media-type=\"application/vnd.oasis.opendocument.chart\""
            )),
            "manifest: {manifest}"
        );
        for part in ["content.xml", "styles.xml"] {
            let path = format!("{object}/{part}");
            assert!(manifest.contains(&format!("manifest:full-path=\"{path}\" manifest:media-type=\"text/xml\"")));
            assert!(reader.contains(&path), "{path} is listed and present");
        }
    }
    // Every file in the package except the manifest and mimetype is listed.
    for name in reader.names() {
        if name == "mimetype" || name.starts_with("META-INF/") {
            continue;
        }
        assert!(manifest.contains(&format!("manifest:full-path=\"{name}\"")), "{name} is not in the manifest");
    }

    let content = reader.read_text("content.xml").unwrap();
    assert!(content.contains("xlink:href=\"./Object 1\""));
    assert!(
        content.contains("draw:notify-on-update-of-ranges=\"&apos;Sales Data&apos;.$A$2:&apos;Sales Data&apos;.$A$4")
    );
    let chart = reader.read_text("Object 1/content.xml").unwrap();
    assert!(chart.contains("xmlns:chart=\"urn:oasis:names:tc:opendocument:xmlns:chart:1.0\""));
    assert!(chart.contains("chart:class=\"chart:bar\""));
    assert!(chart.contains("chart:vertical=\"false\" chart:stacked=\"true\""));
    assert!(chart.contains(
        "<chart:categories table:cell-range-address=\"&apos;Sales Data&apos;.$A$2:&apos;Sales Data&apos;.$A$4\"/>"
    ));
    assert!(
        chart.contains("chart:values-cell-range-address=\"&apos;Sales Data&apos;.$C$2:&apos;Sales Data&apos;.$C$4\"")
    );
    // Literal series names are quoted string literals, as LibreOffice writes them.
    assert!(chart.contains("loext:label-string=\"&quot;North&quot;\""));
    assert!(chart.contains("<chart:legend chart:legend-position=\"bottom\"/>"));
    assert!(chart.contains("<table:table table:name=\"local-table\">"));
    let pie = reader.read_text("Object 2/content.xml").unwrap();
    assert!(pie.contains("chart:class=\"chart:circle\""));
}

#[test]
fn ods_bar_line_and_area_charts_keep_their_kind() {
    let mut workbook = chart_workbook();
    let template = workbook.sheets[0].charts[0].clone();
    workbook.sheets[0].charts = ["bar", "line", "area"]
        .iter()
        .enumerate()
        .map(|(index, kind)| {
            let mut placement = template.clone();
            placement.id = format!("chart-{kind}");
            placement.anchor = format!("H{}", index * 20 + 1);
            placement.chart.kind = kind.to_string();
            placement.chart.stacked = *kind == "area";
            placement.chart.show_labels = false;
            placement.chart.legend = *kind != "line";
            placement
        })
        .collect();
    let read = odf::read_ods(&odf::write_ods(&workbook).unwrap()).unwrap();
    let charts = &read.workbook.sheets[0].charts;
    assert_eq!(charts.len(), 3);
    for (read, source) in charts.iter().zip(&workbook.sheets[0].charts) {
        assert_eq!(read.anchor, source.anchor);
        assert_eq!(read.chart, source.chart, "{} chart", source.chart.kind);
    }
}

#[test]
fn ods_charts_that_cannot_be_written_are_skipped_and_reported() {
    let mut workbook = chart_workbook();
    workbook.sheets[0].charts[1].chart.kind = "radar".into();
    let read = odf::read_ods(&odf::write_ods(&workbook).unwrap()).unwrap();
    assert_eq!(read.workbook.sheets[0].charts.len(), 1);
    let report = compat::workbook_feature_report(&workbook, "ods");
    let charts = report.items.iter().find(|item| item.feature == "charts").expect("charts item");
    assert_eq!(charts.status, "lost");
    assert!(charts.message.contains("radar"), "{}", charts.message);
}

fn pivot_workbook() -> Workbook {
    let mut workbook = Workbook::new_blank("Pivot");
    let sheet = &mut workbook.sheets[0];
    sheet.name = "Data".into();
    let rows = [
        ["Department", "Year", "Sales"],
        ["Hardware", "2025", "100"],
        ["Hardware", "2025", "150"],
        ["Software", "2025", "200"],
        ["Hardware", "2026", "50"],
        ["Software", "2026", "300"],
    ];
    for (row, line) in rows.iter().enumerate() {
        for (column, value) in line.iter().enumerate() {
            let address = format!("{}{}", ["A", "B", "C"][column], row + 1);
            let cell = match value.parse::<f64>() {
                Ok(parsed) => number(parsed),
                Err(_) => text(value),
            };
            sheet.set(&address, cell);
        }
    }
    sheet.pivot_tables = vec![PivotTable {
        id: "p1".into(),
        name: "Pivot".into(),
        source_sheet: "Data".into(),
        source: "A1:C6".into(),
        rows: vec!["Department".into()],
        columns: vec!["Year".into()],
        values: vec![PivotValueField { field: "Sales".into(), aggregation: "sum".into() }],
        filters: vec![],
        anchor: "F1".into(),
    }];
    // A value the sheet already holds at the pivot position wins.
    sheet.set("H3", text("kept"));
    workbook
}

#[test]
fn ods_pivot_tables_are_written_as_computed_values() {
    let workbook = pivot_workbook();
    let read = odf::read_ods(&odf::write_ods(&workbook).unwrap()).unwrap();
    let sheet = &read.workbook.sheets[0];
    let value = |address: &str| sheet.get(address).map(|cell| cell.value.clone()).unwrap_or_default();
    assert_eq!(value("F1"), CellValue::Text("Department".into()));
    assert_eq!(value("G1"), CellValue::Text("2025".into()));
    assert_eq!(value("H1"), CellValue::Text("2026".into()));
    assert_eq!(value("F2"), CellValue::Text("Hardware".into()));
    assert_eq!(value("G2"), CellValue::Number(250.0));
    assert_eq!(value("H2"), CellValue::Number(50.0));
    assert_eq!(value("G3"), CellValue::Number(200.0));
    assert_eq!(value("H3"), CellValue::Text("kept".into()));
    // The source data is untouched.
    assert_eq!(value("C6"), CellValue::Number(300.0));
}

#[test]
fn compatibility_reports_reflect_odf_object_support() {
    let deck = animated_deck();
    let odp = compat::deck_feature_report(&deck, "odp");
    let groups = odp.items.iter().find(|item| item.feature == "groups").expect("groups item");
    assert_eq!(groups.status, "unchanged");
    let animations = odp.items.iter().find(|item| item.feature == "animations").expect("animations item");
    assert_eq!(animations.status, "partial");
    assert!(animations.message.contains("LibreOffice"), "{}", animations.message);
    assert!(!odp.items.iter().any(|item| item.status == "lost"));

    let ods = compat::workbook_feature_report(&chart_workbook(), "ods");
    let charts = ods.items.iter().find(|item| item.feature == "charts").expect("charts item");
    assert_eq!(charts.status, "unchanged");
    let pivots = compat::workbook_feature_report(&pivot_workbook(), "ods");
    let pivot = pivots.items.iter().find(|item| item.feature == "pivotTables").expect("pivot item");
    assert_eq!(pivot.status, "transformed");
    assert!(pivot.message.contains("computed values"), "{}", pivot.message);

    let capabilities = compat::format_capabilities("odp");
    let level = |feature: &str| capabilities.features.iter().find(|entry| entry.feature == feature).unwrap().level;
    assert_eq!(level("groups"), compat::SupportLevel::Full);
    assert_eq!(level("animations"), compat::SupportLevel::Partial);
    let capabilities = compat::format_capabilities("ods");
    let level = |feature: &str| capabilities.features.iter().find(|entry| entry.feature == feature).unwrap().level;
    assert_eq!(level("charts"), compat::SupportLevel::Full);
    assert_eq!(level("pivotTables"), compat::SupportLevel::Partial);
}

#[test]
fn ods_far_anchored_charts_and_sheet_names_survive_a_round_trip() {
    let mut workbook = chart_workbook();
    workbook.sheets[0].charts[1].anchor = "A100002".into();
    let bytes = odf::write_ods(&workbook).unwrap();
    // The empty rows up to the far anchor are written as one repeated row.
    let content = ZipReader::open(bytes.clone()).unwrap().read_text("content.xml").unwrap();
    assert!(content.len() < 64 * 1024, "content.xml is {} bytes", content.len());
    assert!(content.contains("table:number-rows-repeated="));

    let read = odf::read_ods(&bytes).unwrap();
    let sheet = &read.workbook.sheets[0];
    assert_eq!(sheet.name, "Sales Data");
    assert_eq!(sheet.charts.len(), 2, "warnings: {:?}", read.warnings);
    // Past the imported area the anchor is clamped to its last row.
    let far = sheet.charts.iter().find(|chart| chart.id == "chart-pie").expect("far chart");
    assert_eq!(far.anchor, "A100001");
}

/// A small package whose sheet shows the same chart object five times, the
/// object declaring 300 series over 5,000 local rows.
fn oversized_chart_package() -> Vec<u8> {
    let frames: String = (0..5)
        .map(|index| {
            format!(
                "<draw:frame draw:name=\"c{index}\" svg:width=\"8cm\" svg:height=\"6cm\"><draw:object xlink:href=\"./Object 1\"/></draw:frame>"
            )
        })
        .collect();
    let content = format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
            "xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" ",
            "xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" ",
            "xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\" ",
            "xmlns:xlink=\"http://www.w3.org/1999/xlink\" office:version=\"1.2\">",
            "<office:body><office:spreadsheet><table:table table:name=\"Big\">",
            "<table:table-row><table:table-cell>{}</table:table-cell></table:table-row>",
            "</table:table></office:spreadsheet></office:body></office:document-content>"
        ),
        frames
    );
    let series: String = (0..300)
        .map(|index| format!("<chart:series chart:values-cell-range-address=\"Big.B1:Big.B{}\"/>", index + 1))
        .collect();
    let rows: String = (0..5_000)
        .map(|_| "<table:table-row><table:table-cell office:value-type=\"float\" office:value=\"1\"/><table:table-cell office:value-type=\"float\" office:value=\"2\"/></table:table-row>")
        .collect();
    let object = format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
            "xmlns:chart=\"urn:oasis:names:tc:opendocument:xmlns:chart:1.0\" ",
            "xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" office:version=\"1.2\">",
            "<office:body><office:chart><chart:chart chart:class=\"chart:bar\"><chart:plot-area>{}</chart:plot-area>",
            "<table:table table:name=\"local-table\"><table:table-rows>{}</table:table-rows></table:table>",
            "</chart:chart></office:chart></office:body></office:document-content>"
        ),
        series, rows
    );
    let mut zip = officecore::zip::ZipWriter::new();
    zip.add_text("mimetype", "application/vnd.oasis.opendocument.spreadsheet");
    zip.add_text("content.xml", &content);
    zip.add_text("Object 1/content.xml", &object);
    zip.finish()
}

#[test]
fn oversized_or_repeated_chart_objects_are_bounded() {
    let read = odf::read_ods(&oversized_chart_package()).unwrap();
    let sheet = &read.workbook.sheets[0];
    assert_eq!(sheet.name, "Big");
    // One object shown five times is read once.
    assert_eq!(sheet.charts.len(), 1, "warnings: {:?}", read.warnings);
    let chart = &sheet.charts[0].chart;
    assert_eq!(chart.series.len(), 255);
    // 255 series x 5,000 rows is past the cache budget: ranges stay, caches go.
    assert!(chart.series_values_cache.is_empty());
    assert!(chart.categories_cache.is_empty());
    assert!(chart.series[0].range.ends_with("B1:B1"), "range {}", chart.series[0].range);
    assert!(read.warnings.iter().any(|warning| warning.contains("more than once")));
    assert!(read.warnings.iter().any(|warning| warning.contains("300 series")));
    assert!(read.warnings.iter().any(|warning| warning.contains("cached values")));
}

// ---------------------------------------------------------------------------
// Large files: the ODS import budget (100,000 rows, 1,000 columns) is reported.
// ---------------------------------------------------------------------------

fn ods_with_content(body: &str) -> Vec<u8> {
    let content = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-content \
         xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" \
         xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" \
         xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\"><office:body><office:spreadsheet>{body}\
         </office:spreadsheet></office:body></office:document-content>"
    );
    let mut writer = officecore::zip::ZipWriter::new();
    writer.add("content.xml", content.as_bytes());
    writer.finish()
}

fn import_limit_warnings(warnings: &[String]) -> Vec<&String> {
    warnings.iter().filter(|warning| warning.starts_with("Import limit:")).collect()
}

#[test]
fn ods_rows_beyond_the_budget_are_reported() {
    // An empty repeated gap (what LibreOffice writes) moves the next row far down.
    let bytes = ods_with_content(
        "<table:table table:name=\"Log\">\
         <table:table-row><table:table-cell office:value-type=\"string\"><text:p>kept</text:p></table:table-cell></table:table-row>\
         <table:table-row table:number-rows-repeated=\"100100\"><table:table-cell/></table:table-row>\
         <table:table-row><table:table-cell office:value-type=\"float\" office:value=\"7\"/></table:table-row>\
         </table:table>",
    );
    let read = odf::read_ods(&bytes).unwrap();
    let found = import_limit_warnings(&read.warnings);
    assert_eq!(found.len(), 1, "warnings: {:?}", read.warnings);
    assert_eq!(
        found[0],
        "Import limit: sheet \"Log\" has cells beyond row 100000 or column 1000; they were not imported."
    );
    assert!(read.workbook.sheets[0].get("A1").is_some());
}

#[test]
fn ods_columns_beyond_the_budget_are_reported() {
    let bytes = ods_with_content(
        "<table:table table:name=\"Wide\"><table:table-row>\
         <table:table-cell table:number-columns-repeated=\"1100\"/>\
         <table:table-cell office:value-type=\"float\" office:value=\"7\"/>\
         </table:table-row></table:table>",
    );
    let read = odf::read_ods(&bytes).unwrap();
    assert_eq!(import_limit_warnings(&read.warnings).len(), 1, "warnings: {:?}", read.warnings);
}

#[test]
fn ods_trailing_empty_rows_are_not_a_data_loss_warning() {
    // LibreOffice pads sheets with a million empty rows; that is not lost data.
    let bytes = ods_with_content(
        "<table:table table:name=\"Padded\">\
         <table:table-row><table:table-cell office:value-type=\"string\"><text:p>x</text:p></table:table-cell></table:table-row>\
         <table:table-row table:number-rows-repeated=\"1048000\"><table:table-cell table:number-columns-repeated=\"1024\"/></table:table-row>\
         </table:table>",
    );
    let read = odf::read_ods(&bytes).unwrap();
    assert!(import_limit_warnings(&read.warnings).is_empty(), "warnings: {:?}", read.warnings);
}
