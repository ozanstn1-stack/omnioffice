//! Integration tests for vector PDF chart rendering across Impress and Calc exports.

use lopdf::Document;
use officecore::layout::{deck_to_pdf, workbook_to_pdf};
use officecore::model::{Cell, CellValue, ChartData, ChartPlacement, ChartSeries, Deck, SlideObject, Workbook};

fn cached_chart(kind: &str) -> ChartData {
    ChartData {
        kind: kind.into(),
        title: format!("Sample {kind} chart"),
        categories: "A2:A5".into(),
        series: vec![
            ChartSeries { name: "Gelir".into(), range: "B2:B5".into(), color: Some("#2563EB".into()) },
            ChartSeries { name: "Gider".into(), range: "C2:C5".into(), color: Some("#EA580C".into()) },
        ],
        legend: true,
        x_title: "Çeyrek".into(),
        y_title: "Tutar (TRY)".into(),
        stacked: false,
        show_labels: true,
        categories_cache: vec!["Q1".into(), "Q2".into(), "Q3".into(), "Q4".into()],
        series_values_cache: vec![vec![10.0, 25.0, 18.0, 32.0], vec![6.0, 12.0, 15.0, 20.0]],
        hole_size: if kind == "doughnut" { Some(55) } else { None },
        scatter_style: if kind == "scatter" { Some("smoothMarker".into()) } else { None },
    }
}

fn page_ops(bytes: &[u8]) -> Vec<String> {
    let doc = Document::load_mem(bytes).expect("export must produce a valid PDF");
    doc.get_pages()
        .values()
        .map(|&id| String::from_utf8_lossy(&doc.get_page_content(id)).into_owned())
        .collect()
}

#[test]
fn impress_renders_all_seven_chart_kinds_as_vectors() {
    for kind in ["column", "bar", "line", "area", "pie", "doughnut", "scatter"] {
        let mut deck = Deck::new_blank("Charts");
        let mut obj = SlideObject::new("chart", 60.0, 80.0, 480.0, 300.0);
        obj.chart = Some(cached_chart(kind));
        deck.slides[0].objects.push(obj);

        let bytes = deck_to_pdf(&deck);
        let pages = page_ops(&bytes);
        assert_eq!(pages.len(), 1, "{kind}: expected one slide page");
        let ops = &pages[0];
        assert!(ops.contains("/Figure <</MCID"), "{kind}: missing Tagged-PDF Figure block");
        assert!(!ops.contains("chart · "), "{kind}: placeholder box text must no longer be emitted");
        assert!(!ops.contains("/GS"), "{kind}: ExtGState is not registered and must not be emitted");

        match kind {
            "column" | "bar" => assert!(ops.contains("re\nf\n"), "{kind}: expected vector bar fills"),
            "line" => assert!(ops.contains("l\nS\n"), "{kind}: expected line segments"),
            "area" => {
                assert!(ops.contains("h\n"), "{kind}: expected filled polygon area");
                assert!(ops.contains("l\nS\n"), "{kind}: expected top edge polyline");
            }
            "pie" | "doughnut" => assert!(ops.contains("h\n"), "{kind}: expected pie/doughnut wedge polygons"),
            "scatter" => assert!(ops.contains(" c\n"), "{kind}: expected smooth bezier curve ops"),
            _ => {}
        }
    }
}

#[test]
fn impress_handles_stacked_negative_values_and_turkish_labels() {
    let mut chart = cached_chart("column");
    chart.title = "Yıllık Satış — ĞÜŞİÖÇ".into();
    chart.stacked = true;
    chart.categories_cache = vec!["Şubat".into(), "Ağustos".into(), "Eylül".into(), "Iğdır".into()];
    chart.series_values_cache = vec![vec![15.0, -8.0, 22.0, -4.0], vec![10.0, -5.0, 12.0, 6.0]];

    let mut deck = Deck::new_blank("Stacked Negative");
    let mut obj = SlideObject::new("chart", 50.0, 60.0, 500.0, 320.0);
    obj.chart = Some(chart);
    deck.slides[0].objects.push(obj);

    let bytes = deck_to_pdf(&deck);
    let pages = page_ops(&bytes);
    assert_eq!(pages.len(), 1);
    assert!(pages[0].contains("re\nf\n"));
}

#[test]
fn calc_resolves_live_cell_ranges_onto_dedicated_chart_page() {
    let mut workbook = Workbook::new_blank("Satış Raporu");
    let sheet = &mut workbook.sheets[0];
    sheet.set("A1", Cell { value: CellValue::Text("Ay".into()), ..Default::default() });
    sheet.set("B1", Cell { value: CellValue::Text("Ciro".into()), ..Default::default() });
    sheet.set("A2", Cell { value: CellValue::Text("Ocak".into()), ..Default::default() });
    sheet.set("B2", Cell { value: CellValue::Number(120.0), ..Default::default() });
    sheet.set("A3", Cell { value: CellValue::Text("Şubat".into()), ..Default::default() });
    sheet.set("B3", Cell { value: CellValue::Number(240.0), ..Default::default() });
    sheet.set("A4", Cell { value: CellValue::Text("Mart".into()), ..Default::default() });
    sheet.set("B4", Cell { value: CellValue::Number(180.0), ..Default::default() });

    sheet.charts.push(ChartPlacement {
        id: "c1".into(),
        chart: ChartData {
            kind: "column".into(),
            title: "Aylık Ciro".into(),
            categories: "Sheet1!$A$2:$A$4".into(),
            series: vec![ChartSeries { name: "Ciro".into(), range: "Sheet1!$B$2:$B$4".into(), color: None }],
            legend: true,
            show_labels: true,
            ..Default::default()
        },
        anchor: "D2".into(),
        width_px: 480.0,
        height_px: 300.0,
    });

    let bytes = workbook_to_pdf(&workbook, 1);
    let doc = Document::load_mem(&bytes).expect("valid Calc PDF with charts");
    let pages = doc.get_pages();
    assert_eq!(pages.len(), 2, "expected table page followed by chart page");
    let second_id = *pages.values().nth(1).expect("chart page");
    let ops = String::from_utf8_lossy(&doc.get_page_content(second_id)).into_owned();
    assert!(ops.contains("/Figure <</MCID"), "chart page missing Figure block");
    assert!(ops.contains("/Artifact <</Type /Pagination /Subtype /Header>> BDC"));
    assert!(ops.contains("/Artifact <</Type /Pagination /Subtype /Footer>> BDC"));
    assert!(ops.contains("re\nf\n"), "Calc chart page must render vector bars from live cells");
}

#[test]
fn calc_batches_two_charts_per_page_after_table() {
    let mut workbook = Workbook::new_blank("Multi");
    let sheet = &mut workbook.sheets[0];
    sheet.set("A1", Cell { value: CellValue::Number(10.0), ..Default::default() });
    for i in 0..3 {
        sheet.charts.push(ChartPlacement {
            id: format!("c{i}"),
            chart: cached_chart(if i == 2 { "pie" } else { "line" }),
            anchor: "C2".into(),
            width_px: 400.0,
            height_px: 260.0,
        });
    }
    let bytes = workbook_to_pdf(&workbook, 1);
    let pages = page_ops(&bytes);
    assert_eq!(pages.len(), 3, "expected 1 table page + 2 chart pages (2+1 charts)");
    assert_eq!(pages[1].matches("/Figure <</MCID").count(), 2);
    assert_eq!(pages[2].matches("/Figure <</MCID").count(), 1);
}

#[test]
fn empty_chart_degrades_gracefully_without_faking_data() {
    let mut deck = Deck::new_blank("Empty");
    let mut obj = SlideObject::new("chart", 40.0, 40.0, 360.0, 220.0);
    obj.chart = Some(ChartData { kind: "pie".into(), title: "Boş Grafik".into(), ..Default::default() });
    deck.slides[0].objects.push(obj);

    let bytes = deck_to_pdf(&deck);
    let pages = page_ops(&bytes);
    assert_eq!(pages.len(), 1);
    assert!(pages[0].contains("/Figure <</MCID"));
}
