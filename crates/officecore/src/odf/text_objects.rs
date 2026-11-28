//! ODT manifest, metadata, image, and embedded chart object helpers.

use super::text_write::{cm, escape, paragraph_style_xml, parse_cm, AutoStyles};
use super::{ods_chart_content, ods_chart_problem, read_ods_chart, ReadStyles, NS};
use crate::model::*;
use crate::xml::{XmlNode, XmlWriter};
use crate::zip::ZipReader;
use std::collections::{BTreeMap, HashSet};

/// Media type for an ODF package entry.
fn entry_media_type(name: &str) -> &'static str {
    if name.ends_with('/') {
        return "application/vnd.oasis.opendocument.chart";
    }
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else {
        "text/xml"
    }
}

/// Builds the package manifest from the entries actually written.
pub(crate) fn manifest_for(mime: &str, files: &[String]) -> String {
    let mut out = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<manifest:manifest xmlns:manifest=\"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0\" manifest:version=\"1.2\"><manifest:file-entry manifest:full-path=\"/\" manifest:media-type=\"{mime}\"/>"
    );
    for file in files {
        let name = escape(file);
        if file.ends_with('/') {
            out.push_str(&format!(
                "<manifest:file-entry manifest:full-path=\"{name}\" manifest:version=\"1.2\" manifest:media-type=\"{}\"/>",
                entry_media_type(file)
            ));
        } else {
            out.push_str(&format!(
                "<manifest:file-entry manifest:full-path=\"{name}\" manifest:media-type=\"{}\"/>",
                entry_media_type(file)
            ));
        }
    }
    out.push_str("</manifest:manifest>");
    out
}

fn watermark_meta_xml(watermark: &Watermark) -> String {
    let entries = [
        ("OSAK:Watermark:Text", watermark.text.clone()),
        ("OSAK:Watermark:Color", watermark.color.clone().unwrap_or_default()),
        ("OSAK:Watermark:Opacity", watermark.opacity.to_string()),
        ("OSAK:Watermark:Rotation", watermark.rotation.to_string()),
        ("OSAK:Watermark:FontPt", watermark.font_pt.to_string()),
        ("OSAK:Watermark:Bold", watermark.bold.to_string()),
    ];
    entries
        .iter()
        .map(|(name, value)| {
            format!("<meta:user-defined meta:name=\"{}\" meta:value=\"{}\"/>", escape(name), escape(value))
        })
        .collect()
}

pub(crate) fn meta_xml(title: &str, generator: &str, watermark: Option<&Watermark>) -> String {
    let user_defined = watermark.map(watermark_meta_xml).unwrap_or_default();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-meta {NS} office:version=\"1.2\"><office:meta><meta:generator>{}</meta:generator><dc:title>{}</dc:title>{user_defined}</office:meta></office:document-meta>",
        crate::xml::escape_text(generator),
        crate::xml::escape_text(title)
    )
}

pub(crate) fn settings_xml() -> String {
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-settings {NS} office:version=\"1.2\"><office:settings/></office:document-settings>")
}

fn normalize_odt_chart(chart: &ChartData) -> ChartData {
    let strip = |range: &str| range.rsplit_once('!').map(|(_, r)| r).unwrap_or(range).trim().replace('$', "");
    let rows = chart
        .series_values_cache
        .iter()
        .map(Vec::len)
        .chain(std::iter::once(chart.categories_cache.len()))
        .max()
        .unwrap_or(1)
        .max(1);
    let col_range = |col: u32| {
        let letter = crate::address::column_name(col);
        format!("{letter}2:{letter}{}", rows + 1)
    };
    let mut normalized = chart.clone();
    let cats = strip(&chart.categories);
    normalized.categories = if crate::address::parse_range(&cats).is_some() {
        cats
    } else if chart.kind == "scatter" && chart.categories_cache.is_empty() {
        String::new()
    } else {
        col_range(0)
    };
    for (idx, series) in normalized.series.iter_mut().enumerate() {
        let sref = strip(&series.range);
        series.range = if crate::address::parse_range(&sref).is_some() { sref } else { col_range(idx as u32 + 1) };
    }
    normalized
}

#[allow(clippy::too_many_arguments)]
pub(super) fn write_odt_chart(
    writer: &mut XmlWriter,
    styles: &mut AutoStyles,
    charts: &mut Vec<super::OdsChartObject>,
    chart: &ChartData,
    width_pt: f64,
    height_pt: f64,
    align: &str,
    caption: &str,
) {
    let normalized = normalize_odt_chart(chart);
    if ods_chart_problem(&normalized).is_some() {
        writer.raw(&format!(
            "<text:p>{}</text:p>",
            crate::xml::escape_text(&crate::ooxml_chart::chart_text_summary(chart, caption))
        ));
        return;
    }
    let name = format!("Object {}", charts.len() + 1);
    let w = width_pt.max(72.0);
    let h = height_pt.max(48.0);
    let placement = ChartPlacement {
        id: name.clone(),
        chart: normalized,
        anchor: String::new(),
        width_px: w / 0.75,
        height_px: h / 0.75,
    };
    let content = ods_chart_content(&placement, "Sheet1", &BTreeMap::new());
    charts.push(super::OdsChartObject { name: name.clone(), content });
    let jc = match align {
        "left" | "right" => align,
        _ => "center",
    };
    let p_style = styles.paragraph(paragraph_style_xml(&ParaProps { align: jc.into(), ..Default::default() }, false));
    let desc = if caption.trim().is_empty() {
        String::new()
    } else {
        format!("<svg:desc>{}</svg:desc>", crate::xml::escape_text(caption))
    };
    writer.raw(&format!(
        "<text:p text:style-name=\"{p_style}\"><draw:frame draw:name=\"{name}\" text:anchor-type=\"paragraph\" svg:width=\"{}\" svg:height=\"{}\"><draw:object xlink:href=\"./{name}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"/>{desc}</draw:frame></text:p>",
        cm(w),
        cm(h)
    ));
}

pub(super) fn read_odt_chart_block(
    frame: &XmlNode,
    reader: &ZipReader,
    seen_objects: &mut HashSet<String>,
    imported_objects: &mut HashSet<String>,
    align: &str,
    warnings: &mut Vec<String>,
) -> Option<Block> {
    let placement = read_ods_chart(reader, frame, String::new(), &Sheet::default(), seen_objects, warnings)?;
    if let Some(href) =
        frame.child("object").and_then(|o| o.attr_any_ns("href")).map(|h| h.trim().trim_start_matches("./"))
    {
        if let Some(dir) = href.trim_end_matches('/').split('/').next() {
            imported_objects.insert(dir.to_string());
        }
    }
    let width_pt = frame.attr_any_ns("width").and_then(parse_cm).unwrap_or(placement.width_px * 0.75).max(72.0);
    let height_pt = frame.attr_any_ns("height").and_then(parse_cm).unwrap_or(placement.height_px * 0.75).max(48.0);
    let caption = frame.child("desc").map(XmlNode::deep_text).unwrap_or_default();
    let align = match align {
        "left" | "right" | "center" => align.to_string(),
        _ => "center".to_string(),
    };
    Some(Block::Chart { chart: placement.chart, width_pt, height_pt, align, caption })
}

pub(super) fn frame_wrap(frame: &XmlNode, styles: &ReadStyles) -> String {
    let wrap = frame
        .attr_any_ns("style-name")
        .and_then(|name| styles.graphics.get(name))
        .map(|style| style.wrap.as_str())
        .unwrap_or_default();
    match wrap {
        "parallel" | "dynamic" | "left" | "right" | "biggest" => "square".into(),
        "none" | "run-through" => "topBottom".into(),
        _ => "inline".into(),
    }
}

pub(super) fn frame_run_through(frame: &XmlNode, styles: &ReadStyles) -> bool {
    let styled = frame
        .attr_any_ns("style-name")
        .and_then(|name| styles.graphics.get(name))
        .map(|style| style.run_through)
        .unwrap_or(false);
    styled || frame.attr_any_ns("run-through") == Some("background")
}

pub(super) fn read_image_block(
    node: &XmlNode,
    styles: &ReadStyles,
    reader: &ZipReader,
    warnings: &mut Vec<String>,
) -> Option<Block> {
    let mut images = Vec::new();
    node.find_all("image", &mut images);
    let image_node = images.first()?;
    let href = image_node.attr_any_ns("href")?;
    let width = node.attr_any_ns("width").and_then(parse_cm).unwrap_or(320.0);
    let height = node.attr_any_ns("height").and_then(parse_cm).unwrap_or(200.0);
    let path = href.trim_start_matches("./");
    let data = match reader.read(path) {
        Ok(data) => data,
        Err(_) => {
            warnings.push("An embedded image could not be read from the package.".into());
            return None;
        }
    };
    let name = path.rsplit('/').next().unwrap_or("image.png").to_string();
    Some(Block::Image {
        image: ImageData::from_bytes(&name, &data),
        width_pt: width,
        height_pt: height,
        align: "center".into(),
        caption: String::new(),
        wrap: frame_wrap(node, styles),
    })
}

#[cfg(test)]
mod tests {
    use super::{manifest_for, meta_xml, settings_xml};
    use crate::compat::{document_feature_report, format_capabilities};
    use crate::model::{Block, ChartData, ChartSeries, TextDocument, Watermark};
    use crate::odf::{read_odt, write_odt};
    use crate::rtf::write_rtf;
    use crate::textio::{document_to_html, document_to_markdown, document_to_text};
    use crate::udf::write_udf;

    fn chart_for(kind: &str) -> ChartData {
        ChartData {
            kind: kind.into(),
            title: format!("{kind} title"),
            categories: String::new(),
            series: vec![ChartSeries { name: "S1".into(), range: String::new(), color: Some("#0ea5e9".into()) }],
            legend: true,
            x_title: "Cat".into(),
            y_title: "Val".into(),
            stacked: kind == "area",
            show_labels: kind == "column",
            categories_cache: vec!["A".into(), "B".into()],
            series_values_cache: vec![vec![3.0, 9.0]],
            hole_size: None,
            scatter_style: if kind == "scatter" { Some("smoothMarker".into()) } else { None },
        }
    }

    #[test]
    fn odt_round_trips_charts_manifest_and_text_fallbacks() {
        let mut doc = TextDocument::new_blank("ODT charts");
        doc.watermark = Some(Watermark { text: "DRAFT".into(), ..Default::default() });
        for (i, kind) in ["column", "bar", "line", "area", "pie", "doughnut", "scatter"].iter().enumerate() {
            let align = ["left", "center", "right"][i % 3];
            doc.blocks.push(Block::Chart {
                chart: chart_for(kind),
                width_pt: 320.0,
                height_pt: 200.0,
                align: align.into(),
                caption: format!("ODT {kind}"),
            });
        }
        doc.blocks.push(Block::Chart {
            chart: chart_for("radar"),
            width_pt: 240.0,
            height_pt: 160.0,
            align: "center".into(),
            caption: "Fallback".into(),
        });

        let odt = write_odt(&doc).expect("write odt");
        let read = read_odt(&odt).expect("read odt");
        let charts = read.document.blocks.iter().filter(|b| matches!(b, Block::Chart { .. })).count();
        assert_eq!(charts, 7);
        assert!(read.document.watermark.is_some());

        let md = document_to_markdown(&doc);
        let txt = document_to_text(&doc);
        let html = document_to_html(&doc);
        let rtf = String::from_utf8_lossy(&write_rtf(&doc).unwrap()).into_owned();
        let udf = String::from_utf8_lossy(&write_udf(&doc).unwrap()).into_owned();
        assert!(md.contains("ODT column") && txt.contains("ODT bar") && html.contains("chart-fallback"));
        assert!(rtf.contains("ODT line") && udf.contains("ODT pie"));

        assert!(!format_capabilities("docx").features.is_empty());
        assert!(!document_feature_report(&doc, "rtf").features.is_empty());
        assert!(!document_feature_report(&doc, "udf").features.is_empty());
        assert!(!document_feature_report(&doc, "txt").features.is_empty());

        let files: Vec<String> = ["a.png", "b.jpg", "c.gif", "d.svg", "e.webp", "Object 1/", "content.xml"]
            .into_iter()
            .map(str::to_string)
            .collect();
        let manifest = manifest_for("application/vnd.oasis.opendocument.text", &files);
        assert!(manifest.contains("image/webp") && manifest.contains("application/vnd.oasis.opendocument.chart"));
        assert!(meta_xml("T", "G", doc.watermark.as_ref()).contains("DRAFT"));
        assert!(settings_xml().contains("office:settings"));
    }
}
