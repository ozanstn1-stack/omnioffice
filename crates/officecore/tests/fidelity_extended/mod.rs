use base64::Engine as _;
use officecore::model::*;

const PIXEL_PNG_BASE64: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAACklEQVR4nGMAAQAABQABDQottAAAAABJRU5ErkJggg==";

pub fn pixel_png() -> ImageData {
    let bytes = base64::engine::general_purpose::STANDARD.decode(PIXEL_PNG_BASE64).expect("decode png");
    ImageData::from_bytes("pixel.png", &bytes)
}

fn txt_run(text: &str, comment: Option<&str>) -> Run {
    Run { text: text.into(), comment: comment.map(str::to_owned), ..Default::default() }
}

fn list_para(kind: &str, marker: &str, text: &str) -> Block {
    let list = Some(ListInfo { kind: kind.into(), level: 0, start: 1, marker: marker.into() });
    Block::Paragraph { props: ParaProps { list, ..Default::default() }, runs: vec![txt_run(text, None)] }
}

fn cell_txt(s: &str) -> Cell {
    Cell { value: CellValue::Text(s.into()), ..Default::default() }
}

fn slide_obj(id: &str, kind: &str, x: f64, y: f64, w: f64, h: f64, z: i32) -> SlideObject {
    let mut obj = SlideObject::new(kind, x, y, w, h);
    obj.id = id.into();
    obj.z = z;
    obj
}

fn sample_chart(title: &str, cats: &str, sname: &str, srange: &str, c_cache: &[&str], v_cache: &[f64]) -> ChartData {
    ChartData {
        kind: "column".into(),
        title: title.into(),
        categories: cats.into(),
        series: vec![ChartSeries { name: sname.into(), range: srange.into(), color: Some("#1D4ED8".into()) }],
        legend: true,
        categories_cache: c_cache.iter().map(|s| (*s).into()).collect(),
        series_values_cache: vec![v_cache.to_vec()],
        ..Default::default()
    }
}

fn anim(spec: (&str, &str, &str, &str, &str, u32, u32, u32)) -> Animation {
    let (id, oid, kind, effect, trig, dur, delay, order) = spec;
    Animation {
        id: id.into(),
        object_id: oid.into(),
        kind: kind.into(),
        effect: effect.into(),
        trigger: trig.into(),
        duration_ms: dur,
        delay_ms: delay,
        order,
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ExtendedWriterStats {
    pub images: usize,
    pub image_bytes: usize,
    pub comments: usize,
    pub comment_replies: usize,
    pub resolved_comments: usize,
    pub anchored_runs: usize,
    pub headers: usize,
    pub footers: usize,
    pub list_items: usize,
    pub bullet_items: usize,
    pub numbered_items: usize,
    pub sections: usize,
}

fn count_slot_blocks(slots: &[&[Block]]) -> usize {
    slots.iter().flat_map(|s| s.iter()).filter(|b| !b.plain_text().trim().is_empty()).count()
}

pub fn collect_writer_stats(doc: &TextDocument) -> ExtendedWriterStats {
    let mut s = ExtendedWriterStats {
        comments: doc.comments.len(),
        comment_replies: doc.comments.iter().map(|c| c.replies.len()).sum(),
        resolved_comments: doc.comments.iter().filter(|c| c.resolved).count(),
        ..Default::default()
    };
    for block in &doc.blocks {
        match block {
            Block::Paragraph { props, runs } => {
                if let Some(list) = &props.list {
                    s.list_items += 1;
                    if list.kind == "number" {
                        s.numbered_items += 1;
                    } else {
                        s.bullet_items += 1;
                    }
                }
                s.anchored_runs += runs.iter().filter(|r| r.comment.is_some() && !r.text.is_empty()).count();
            }
            Block::Image { image, .. } => {
                s.images += 1;
                s.image_bytes += image.bytes().len();
            }
            _ => {}
        }
    }
    let sections = doc.all_sections();
    s.sections = sections.len();
    for sec in &sections {
        s.headers += count_slot_blocks(&[&sec.header, &sec.first_header, &sec.even_header]);
        s.footers += count_slot_blocks(&[&sec.footer, &sec.first_footer, &sec.even_footer]);
    }
    s
}

pub fn extended_writer_document(with_section_break: bool) -> TextDocument {
    let mut doc = TextDocument::new_blank("Extended Fidelity");
    doc.header = vec![Block::paragraph("Corporate Header")];
    doc.footer = vec![Block::paragraph("Confidential Footer")];
    doc.comments = vec![Comment {
        id: "c1".into(),
        author: "Ada".into(),
        text: "Verify section figure".into(),
        created: "2026-06-01T09:00:00Z".into(),
        resolved: !with_section_break,
        modified: "2026-06-01T09:00:00Z".into(),
        replies: vec![CommentReply {
            author: "Grace".into(),
            text: "Confirmed".into(),
            created: "2026-06-01T10:00:00Z".into(),
        }],
    }];
    let mut blocks = vec![
        Block::Paragraph {
            props: ParaProps::default(),
            runs: vec![txt_run("Reviewed ", None), txt_run("clause alpha", Some("c1"))],
        },
        list_para("bullet", "•", "Bullet point"),
        list_para("number", "1.", "Numbered item"),
        Block::Image {
            image: pixel_png(),
            width_pt: 72.0,
            height_pt: 48.0,
            align: "center".into(),
            caption: String::new(),
            wrap: "inline".into(),
        },
    ];
    if with_section_break {
        blocks.push(Block::SectionBreak {
            section: SectionProps {
                page: PageSetup::from_preset("a4", "landscape"),
                header: vec![Block::paragraph("Landscape Header")],
                footer: vec![Block::paragraph("Landscape Footer")],
                first_header: vec![Block::paragraph("First Page Header")],
                even_header: vec![Block::paragraph("Even Page Header")],
                different_first_page: true,
                different_odd_even: true,
                columns: 2,
                start: "oddPage".into(),
                ..Default::default()
            },
        });
        blocks.push(Block::paragraph("Landscape section text"));
    }
    doc.blocks = blocks;
    doc
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ExtendedCalcStats {
    pub images: usize,
    pub charts: usize,
    pub chart_cache_points: usize,
    pub tables: usize,
    pub hyperlinks: usize,
    pub tooltips: usize,
    pub comments: usize,
    pub freeze_rows: u32,
    pub freeze_cols: u32,
    pub breaks: usize,
    pub names: usize,
    pub print_area: usize,
    pub print_titles: usize,
    pub protected: usize,
}

pub fn collect_calc_stats(wb: &Workbook) -> ExtendedCalcStats {
    let sh = &wb.sheets[0];
    ExtendedCalcStats {
        images: sh.images.len(),
        charts: sh.charts.len(),
        chart_cache_points: sh
            .charts
            .iter()
            .map(|c| c.chart.categories_cache.len() + c.chart.series_values_cache.iter().map(Vec::len).sum::<usize>())
            .sum(),
        tables: sh.tables.len(),
        hyperlinks: sh.cells.values().filter(|c| c.link.is_some()).count(),
        tooltips: sh.cells.values().filter(|c| c.link_tooltip.is_some()).count(),
        comments: sh.cells.values().filter(|c| c.comment.is_some()).count(),
        freeze_rows: sh.freeze_rows,
        freeze_cols: sh.freeze_cols,
        breaks: sh.print.row_breaks.len() + sh.print.col_breaks.len(),
        names: wb.names.len(),
        print_area: usize::from(sh.print.print_area.is_some()),
        print_titles: usize::from(sh.print.print_titles_rows.is_some())
            + usize::from(sh.print.print_titles_cols.is_some()),
        protected: usize::from(sh.protection.enabled),
    }
}

pub fn extended_workbook() -> Workbook {
    let mut wb = Workbook::new_blank("Extended Calc");
    wb.names.push(NamedRange { name: "TAX_RATE".into(), definition: "0.2".into(), ..Default::default() });
    let sh = &mut wb.sheets[0];
    sh.name = "Ledger".into();
    sh.freeze_rows = 1;
    sh.freeze_cols = 2;
    for (addr, label) in [("A1", "Item"), ("B1", "Qty"), ("C1", "Price"), ("D1", "Total"), ("A2", "Widget")] {
        sh.set(addr, cell_txt(label));
    }
    sh.set("B2", Cell { value: CellValue::Number(3.0), ..Default::default() });
    sh.set("C2", Cell { value: CellValue::Number(10.0), ..Default::default() });
    sh.set(
        "D2",
        Cell { value: CellValue::Number(30.0), formula: Some("=[@Qty]*[@Price]".into()), ..Default::default() },
    );
    sh.set(
        "E1",
        Cell {
            value: CellValue::Text("Spec".into()),
            link: Some("https://example.org/spec".into()),
            link_display: Some("Open Spec".into()),
            link_tooltip: Some("Spec tooltip".into()),
            comment: Some("Reviewed link".into()),
            comment_author: Some("Ada".into()),
            ..Default::default()
        },
    );
    sh.tables.push(SpreadsheetTable {
        id: "tbl-1".into(),
        name: "Orders".into(),
        range: "A1:D2".into(),
        has_headers: true,
        banded_rows: true,
        style_name: "TableStyleMedium2".into(),
        columns: vec![
            TableColumn { name: "Item".into(), formula: None },
            TableColumn { name: "Qty".into(), formula: None },
            TableColumn { name: "Price".into(), formula: None },
            TableColumn { name: "Total".into(), formula: Some("=[@Qty]*[@Price]".into()) },
        ],
        filter: Some(FilterState { range: "A1:D2".into(), column: 0, values: vec!["Widget".into()] }),
        ..Default::default()
    });
    sh.charts.push(ChartPlacement {
        id: "chart-1".into(),
        chart: sample_chart("Totals", "A2:A2", "Total", "D2:D2", &["Widget"], &[30.0]),
        anchor: "F3".into(),
        width_px: 360.0,
        height_px: 220.0,
    });
    sh.images.push(SheetImage {
        image: pixel_png(),
        anchor: CellAnchor { address: "B6".into(), col_off_emu: 9_525, row_off_emu: 19_050, ..Default::default() },
        width_px: 80.0,
        height_px: 40.0,
        rotation_deg: 0.0,
    });
    sh.print = PrintSettings {
        landscape: true,
        scale: 85,
        print_area: Some("A1:E10".into()),
        print_titles_rows: Some("1:1".into()),
        print_titles_cols: Some("A:A".into()),
        header: "Ledger".into(),
        footer: "Page &P".into(),
        row_breaks: vec![4],
        col_breaks: vec![3],
        ..Default::default()
    };
    sh.protection = SheetProtection {
        enabled: true,
        password_hash: Some("ABCD".into()),
        options: vec!["formatCells".into()],
        ..Default::default()
    };
    wb
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ExtendedDeckStats {
    pub masters: usize,
    pub layouts: usize,
    pub groups: usize,
    pub cropped_images: usize,
    pub charts: usize,
    pub chart_cache_points: usize,
    pub animations: usize,
    pub notes: usize,
    pub transitions: usize,
    pub hidden_slides: usize,
}

fn count_groups_and_crops(objects: &[SlideObject]) -> (usize, usize, usize, usize) {
    objects.iter().fold((0, 0, 0, 0), |(g, c, ch, pts), obj| {
        let (cg, cc, cch, cpts) = count_groups_and_crops(&obj.children);
        let obj_pts = obj
            .chart
            .as_ref()
            .map_or(0, |x| x.categories_cache.len() + x.series_values_cache.iter().map(Vec::len).sum::<usize>());
        (
            g + usize::from(obj.kind == "group") + cg,
            c + usize::from(obj.image.as_ref().and_then(|i| i.crop.as_ref()).is_some()) + cc,
            ch + usize::from(obj.chart.is_some()) + cch,
            pts + obj_pts + cpts,
        )
    })
}

pub fn collect_deck_stats(deck: &Deck) -> ExtendedDeckStats {
    let mut s = ExtendedDeckStats {
        masters: deck.masters.len(),
        layouts: deck.masters.iter().map(|m| m.layouts.len()).sum(),
        hidden_slides: deck.slides.iter().filter(|sl| sl.hidden).count(),
        ..Default::default()
    };
    for sl in &deck.slides {
        let (g, c, ch, cpt) = count_groups_and_crops(&sl.objects);
        s.groups += g;
        s.cropped_images += c;
        s.charts += ch;
        s.chart_cache_points += cpt;
        s.animations += sl.animations.len();
        s.notes += usize::from(!sl.notes.trim().is_empty());
        s.transitions += usize::from(sl.transition.is_some());
    }
    s
}

pub fn extended_deck() -> Deck {
    let mut deck = Deck::new_blank("Extended Impress");
    let mut mf = slide_obj("master-footer", "text", 300.0, 500.0, 360.0, 28.0, 0);
    mf.placeholder = Some("footer".into());
    mf.text = Some(TextFrame {
        paragraphs: vec![TextParagraph { text: "Footer".into(), ..Default::default() }],
        ..Default::default()
    });
    deck.masters = vec![SlideMaster {
        id: "master-1".into(),
        name: "Primary".into(),
        theme: "business".into(),
        background: Some("#F8FAFC".into()),
        objects: vec![mf],
        layouts: vec![SlideLayout {
            id: "lay-1".into(),
            name: "Title".into(),
            kind: "title".into(),
            objects: Vec::new(),
        }],
    }];

    let mut inner = slide_obj("group-inner", "group", 120.0, 120.0, 140.0, 100.0, 1);
    inner.children = vec![slide_obj("leaf-rect", "rect", 140.0, 140.0, 80.0, 50.0, 1)];
    let mut outer = slide_obj("group-outer", "group", 100.0, 100.0, 260.0, 140.0, 1);
    outer.children = vec![inner, slide_obj("leaf-ellipse", "ellipse", 280.0, 140.0, 60.0, 60.0, 2)];

    let mut img = pixel_png();
    img.crop = Some(ImageCrop { left: 0.1, top: 0.15, right: 0.2, bottom: 0.05 });
    let mut image_obj = slide_obj("img-cropped", "image", 60.0, 320.0, 120.0, 80.0, 2);
    image_obj.image = Some(img);

    let mut chart_obj = slide_obj("chart-1", "chart", 480.0, 100.0, 420.0, 260.0, 3);
    chart_obj.chart =
        Some(sample_chart("Revenue", "A2:A4", "North", "B2:B4", &["Q1", "Q2", "Q3"], &[12.0, 24.0, 36.0]));

    let slide1 = Slide {
        id: "slide-1".into(),
        master_id: Some("master-1".into()),
        layout_id: Some("lay-1".into()),
        objects: vec![outer, image_obj, chart_obj],
        notes: "Speaker reminder".into(),
        transition: Some("fade".into()),
        transition_ms: 600,
        animations: vec![
            anim(("a1", "group-outer", "entrance", "fade", "onClick", 400, 0, 0)),
            anim(("a2", "chart-1", "emphasis", "pulse", "withPrevious", 500, 100, 1)),
            anim(("a3", "img-cropped", "exit", "fadeOut", "afterPrevious", 300, 0, 2)),
        ],
        ..Default::default()
    };
    let slide2 = Slide { id: "slide-2".into(), hidden: true, master_id: Some("master-1".into()), ..Default::default() };
    deck.slides = vec![slide1, slide2];
    deck
}
