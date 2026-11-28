//! Integration tests for Writer `Block::Chart` across DOCX, ODT, PDF, fallback formats, and compat.

use officecore::compat::{document_feature_report, format_capabilities, model_capabilities, SupportLevel};
use officecore::docx::{read_docx, write_docx};
use officecore::layout::document_to_pdf;
use officecore::model::{Block, ChartData, ChartSeries, TextDocument};
use officecore::odf::{read_odt, write_odt};
use officecore::rtf::write_rtf;
use officecore::textio::{document_to_html, document_to_markdown, document_to_text};
use officecore::udf::write_udf;
use officecore::zip::ZipReader;
use std::collections::BTreeSet;

fn chart_block(
    kind: &str,
    title: &str,
    stacked: bool,
    hole_size: Option<u32>,
    scatter_style: Option<&str>,
    caption: &str,
) -> Block {
    let categories_cache = if kind == "scatter" {
        vec!["1".into(), "2".into(), "3".into(), "4".into()]
    } else {
        vec!["Ocak".into(), "Şubat".into(), "Mart".into(), "Nisan".into()]
    };
    let series = if matches!(kind, "pie" | "doughnut") {
        vec![ChartSeries { name: "Pay".into(), range: "Sheet1!$B$2:$B$5".into(), color: Some("#2563EB".into()) }]
    } else {
        vec![
            ChartSeries { name: "Gelir".into(), range: "Sheet1!$B$2:$B$5".into(), color: Some("#2563EB".into()) },
            ChartSeries { name: "Gider".into(), range: "Sheet1!$C$2:$C$5".into(), color: Some("#F97316".into()) },
        ]
    };
    let series_values_cache = if series.len() == 1 {
        vec![vec![45.0, 25.0, 20.0, 10.0]]
    } else {
        vec![vec![120.0, 145.0, 132.0, 168.0], vec![80.0, 92.0, 88.0, 104.0]]
    };
    Block::Chart {
        chart: ChartData {
            kind: kind.into(),
            title: title.into(),
            categories: "Sheet1!$A$2:$A$5".into(),
            series,
            legend: true,
            x_title: "Dönem".into(),
            y_title: "Tutar".into(),
            stacked,
            show_labels: kind == "column",
            categories_cache,
            series_values_cache,
            hole_size,
            scatter_style: scatter_style.map(str::to_string),
        },
        width_pt: 420.0,
        height_pt: 250.0,
        align: "center".into(),
        caption: caption.into(),
    }
}

fn seven_kind_document() -> TextDocument {
    let mut doc = TextDocument::new_blank("Writer Grafikleri");
    doc.blocks = vec![
        Block::heading("Finansal Özet", 1),
        chart_block("column", "Yıllık Sütun", true, None, None, "Şekil 1: Sütun"),
        chart_block("bar", "Yatay Çubuk", false, None, None, "Şekil 2: Çubuk"),
        chart_block("line", "Trend Çizgi", false, None, None, ""),
        chart_block("area", "Alan Dağılımı", true, None, None, ""),
        chart_block("pie", "Pazar Payı", false, None, None, ""),
        chart_block("doughnut", "Halka Oranı", false, Some(45), None, ""),
        chart_block("scatter", "Saçılım Analizi", false, None, Some("smoothMarker"), ""),
    ];
    doc
}

#[test]
fn writer_chart_docx_roundtrip_all_seven_kinds() {
    let doc = seven_kind_document();
    let bytes = write_docx(&doc).expect("write DOCX with charts");
    let zip = ZipReader::open(bytes.clone()).expect("open DOCX zip");
    assert!(zip.contains("word/charts/chart1.xml"));
    assert!(zip.contains("word/charts/chart7.xml"));
    assert!(zip.contains("word/embeddings/Microsoft_Excel_Worksheet1.xlsx"));
    let types = zip.read_text("[Content_Types].xml").expect("content types");
    assert!(types.contains("/word/charts/chart7.xml"));
    let rels = zip.read_text("word/_rels/document.xml.rels").expect("document rels");
    assert!(rels.contains("relationships/chart"));

    let read = read_docx(&bytes).expect("read DOCX back");
    assert!(read.warnings.is_empty(), "warnings: {:?}", read.warnings);
    let charts: Vec<(&ChartData, f64, f64, &str, &str)> = read
        .document
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Chart { chart, width_pt, height_pt, align, caption } => {
                Some((chart, *width_pt, *height_pt, align.as_str(), caption.as_str()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(charts.len(), 7);
    let kinds: Vec<&str> = charts.iter().map(|(c, ..)| c.kind.as_str()).collect();
    assert_eq!(kinds, ["column", "bar", "line", "area", "pie", "doughnut", "scatter"]);
    assert_eq!(charts[0].0.title, "Yıllık Sütun");
    assert!(charts[0].0.stacked);
    assert!(charts[0].0.show_labels);
    assert_eq!(charts[0].4, "Şekil 1: Sütun");
    assert!((charts[0].1 - 420.0).abs() < 1.0 && (charts[0].2 - 250.0).abs() < 1.0);
    assert_eq!(charts[0].3, "center");
    assert_eq!(charts[5].0.hole_size, Some(45));
    assert_eq!(charts[6].0.scatter_style.as_deref(), Some("smoothMarker"));
    assert_eq!(charts[0].0.categories_cache, ["Ocak", "Şubat", "Mart", "Nisan"]);
    assert_eq!(charts[0].0.series[0].color.as_deref(), Some("#2563EB"));
}

#[test]
fn writer_chart_odt_roundtrip_all_seven_kinds() {
    let doc = seven_kind_document();
    let bytes = write_odt(&doc).expect("write ODT with charts");
    let zip = ZipReader::open(bytes.clone()).expect("open ODT zip");
    assert!(zip.contains("Object 1/content.xml"));
    assert!(zip.contains("Object 7/content.xml"));
    let manifest = zip.read_text("META-INF/manifest.xml").expect("ODT manifest");
    assert!(manifest.contains("Object 1/content.xml"));
    assert!(manifest.contains("application/vnd.oasis.opendocument.chart"));

    let read = read_odt(&bytes).expect("read ODT back");
    assert!(read.warnings.is_empty(), "warnings: {:?}", read.warnings);
    let charts: Vec<(&ChartData, &str)> = read
        .document
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Chart { chart, caption, .. } => Some((chart, caption.as_str())),
            _ => None,
        })
        .collect();
    assert_eq!(charts.len(), 7);
    let kinds: Vec<&str> = charts.iter().map(|(c, _)| c.kind.as_str()).collect();
    assert_eq!(kinds, ["column", "bar", "line", "area", "pie", "doughnut", "scatter"]);
    assert_eq!(charts[0].0.title, "Yıllık Sütun");
    assert_eq!(charts[0].1, "Şekil 1: Sütun");
    assert_eq!(charts[0].0.categories_cache, ["Ocak", "Şubat", "Mart", "Nisan"]);
    assert_eq!(charts[0].0.series_values_cache[0], [120.0, 145.0, 132.0, 168.0]);
    assert_eq!(charts[6].0.scatter_style.as_deref(), Some("smoothMarker"));
}

#[test]
fn writer_chart_pdf_vector_export_and_tagged_figure() {
    let doc = seven_kind_document();
    let pdf = document_to_pdf(&doc);
    let parsed = lopdf::Document::load_mem(&pdf).expect("parse exported PDF");
    assert!(parsed.get_pages().len() >= 2, "expected multiple pages for 7 charts");

    let mut tags = BTreeSet::new();
    for obj in parsed.objects.values() {
        if let Ok(dict) = obj.as_dict() {
            if let Ok(s) = dict.get(b"S").and_then(|o| o.as_name()) {
                tags.insert(String::from_utf8_lossy(s).into_owned());
            }
        }
    }
    assert!(tags.contains("Figure"), "expected Tagged-PDF Figure element, got {tags:?}");
    assert!(tags.contains("Caption"), "expected Tagged-PDF Caption element, got {tags:?}");

    let mut vector_ops = 0usize;
    for page_id in parsed.get_pages().values() {
        let raw = parsed.get_page_content(*page_id);
        let text = String::from_utf8_lossy(&raw);
        vector_ops += text.lines().filter(|l| l.ends_with(" re") || l.ends_with(" l") || l.ends_with(" c")).count();
    }
    assert!(vector_ops > 40, "expected rich vector drawing primitives, got {vector_ops}");
}

#[test]
fn writer_chart_fallback_formats_and_compat_report() {
    assert!(model_capabilities("writer").supports_charts);
    for ext in ["docx", "odt"] {
        let cap = format_capabilities(ext);
        let row = cap.features.iter().find(|f| f.feature == "charts").expect("charts capability row");
        assert_eq!(row.level, SupportLevel::Full, "{ext} should report Full chart support");
    }

    let doc = seven_kind_document();
    assert!(!document_feature_report(&doc, "docx").items.iter().any(|i| i.feature == "charts"));
    assert!(!document_feature_report(&doc, "odt").items.iter().any(|i| i.feature == "charts"));
    for target in ["rtf", "html", "md", "txt", "udf"] {
        let rep = document_feature_report(&doc, target);
        assert!(
            rep.items.iter().any(|i| i.feature == "charts" && i.status == "transformed"),
            "{target} should report transformed chart summary"
        );
    }

    let rtf = String::from_utf8_lossy(&write_rtf(&doc).expect("write RTF")).into_owned();
    assert!(rtf.contains("Chart (pie)"));
    assert!(document_to_markdown(&doc).contains("Yıllık") || document_to_markdown(&doc).contains("Şekil 1"));
    assert!(document_to_html(&doc).contains("chart-fallback"));
    assert!(document_to_text(&doc).contains("Chart (scatter)"));
    let udf = write_udf(&doc).expect("write UDF");
    assert!(!udf.is_empty());
}
