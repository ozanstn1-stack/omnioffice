//! DOCX ChartML import and export helpers.

use super::package::{resolve_media_path, RelLink};
use super::*;
use crate::ooxml_chart::{
    chart_has_cache, chart_part_xml, chart_text_summary, embedded_workbook, first_descendant, read_chart_xml,
};

const REL_CHART: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart";
const REL_PACKAGE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/package";

pub(super) struct DocxChartPart {
    pub(super) chart_path: String,
    pub(super) chart_xml: String,
    pub(super) rels_path: Option<String>,
    pub(super) rels_xml: Option<String>,
    pub(super) workbook_path: Option<String>,
    pub(super) workbook_bytes: Option<Vec<u8>>,
}

#[derive(Default)]
pub(super) struct DocxCharts {
    pub(super) parts: Vec<DocxChartPart>,
    pub(super) embed_count: usize,
}

fn chart_rels_xml(workbook_name: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"{REL_PACKAGE}\" Target=\"../embeddings/{workbook_name}\"/></Relationships>"
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn write_chart_block(
    charts: &mut DocxCharts,
    rels_add: &mut impl FnMut(&str, &str) -> String,
    id_counter: &mut usize,
    writer: &mut XmlWriter,
    chart: &ChartData,
    width_pt: f64,
    height_pt: f64,
    align: &str,
    caption: &str,
) {
    let mut external_rid = None;
    let mut embed_name = None;
    let mut workbook_bytes = None;
    if chart_has_cache(chart) {
        if let Ok(bytes) = embedded_workbook(chart) {
            charts.embed_count += 1;
            let name = format!("Microsoft_Excel_Worksheet{}.xlsx", charts.embed_count);
            external_rid = Some("rId1");
            embed_name = Some(name);
            workbook_bytes = Some(bytes);
        }
    }
    let Ok(chart_xml) = chart_part_xml(chart, external_rid) else {
        writer.raw(&format!(
            "<w:p><w:r><w:t>{}</w:t></w:r></w:p>",
            crate::xml::escape_text(&chart_text_summary(chart, caption))
        ));
        return;
    };
    let n = charts.parts.len() + 1;
    let rel_target = format!("charts/chart{n}.xml");
    let rid = rels_add(REL_CHART, &rel_target);
    let (rels_path, rels_xml, workbook_path) = match embed_name {
        Some(name) => (
            Some(format!("word/charts/_rels/chart{n}.xml.rels")),
            Some(chart_rels_xml(&name)),
            Some(format!("word/embeddings/{name}")),
        ),
        None => (None, None, None),
    };
    charts.parts.push(DocxChartPart {
        chart_path: format!("word/{rel_target}"),
        chart_xml,
        rels_path,
        rels_xml,
        workbook_path,
        workbook_bytes,
    });

    *id_counter += 1;
    let id = *id_counter;
    let cx = (width_pt.max(72.0) * 12700.0).round() as i64;
    let cy = (height_pt.max(48.0) * 12700.0).round() as i64;
    let jc = match align {
        "left" => "left",
        "right" => "right",
        _ => "center",
    };
    let descr = crate::xml::escape_attr(caption);
    writer.raw(&format!(
        "<w:p><w:pPr><w:jc w:val=\"{jc}\"/></w:pPr><w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\"><wp:extent cx=\"{cx}\" cy=\"{cy}\"/><wp:effectExtent l=\"0\" t=\"0\" r=\"0\" b=\"0\"/><wp:docPr id=\"{id}\" name=\"Chart {n}\" descr=\"{descr}\"/><wp:cNvGraphicFramePr/><a:graphic xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" r:id=\"{rid}\"/></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"
    ));
}

pub(super) fn read_chart_block(
    node: &XmlNode,
    rels: &HashMap<String, RelLink>,
    reader: &ZipReader,
    para_align: &str,
    warnings: &mut Vec<String>,
) -> Option<Block> {
    let mut charts = Vec::new();
    node.find_all("chart", &mut charts);
    let chart_ref = charts.into_iter().find(|n| n.attr_any_ns("id").is_some())?;
    let rid = chart_ref.attr_any_ns("id")?;
    let Some(link) = rels.get(rid) else {
        warnings.push("A chart relationship in the document could not be resolved.".into());
        return None;
    };
    let path = resolve_media_path(&link.target);
    let Ok(xml) = reader.read_text(&path) else {
        warnings.push("An embedded chart part could not be read from the package.".into());
        return None;
    };
    let chart = read_chart_xml(&xml, warnings)?;
    let mut width_pt = 420.0;
    let mut height_pt = 260.0;
    if let Some(extent) = first_descendant(node, "extent") {
        if let Some(cx) = extent.attr("cx").and_then(|v| v.trim().parse::<f64>().ok()) {
            width_pt = (cx / 12700.0).max(72.0);
        }
        if let Some(cy) = extent.attr("cy").and_then(|v| v.trim().parse::<f64>().ok()) {
            height_pt = (cy / 12700.0).max(48.0);
        }
    }
    let caption = first_descendant(node, "docPr").and_then(|d| d.attr("descr")).unwrap_or("").to_string();
    let align = match para_align {
        "left" | "right" | "center" => para_align.to_string(),
        _ => "center".to_string(),
    };
    Some(Block::Chart { chart, width_pt, height_pt, align, caption })
}

#[cfg(test)]
mod tests {
    use super::{read_chart_block, RelLink};
    use crate::docx::{read_docx, write_docx};
    use crate::model::{Block, ChartData, ChartSeries, TextDocument};
    use crate::xml::parse_xml;
    use crate::zip::{ZipReader, ZipWriter};
    use std::collections::HashMap;

    fn chart_for(kind: &str) -> ChartData {
        ChartData {
            kind: kind.into(),
            title: format!("{kind} title"),
            categories: String::new(),
            series: vec![ChartSeries { name: "S1".into(), range: String::new(), color: Some("#1d4ed8".into()) }],
            legend: true,
            x_title: "X".into(),
            y_title: "Y".into(),
            stacked: kind == "column",
            show_labels: kind == "pie",
            categories_cache: vec!["10".into(), "20".into()],
            series_values_cache: vec![vec![5.0, 12.0]],
            hole_size: if kind == "doughnut" { Some(65) } else { None },
            scatter_style: if kind == "scatter" { Some("line".into()) } else { None },
        }
    }

    #[test]
    fn docx_round_trips_seven_chart_kinds_and_fallbacks() {
        let mut doc = TextDocument::new_blank("DOCX charts");
        for (i, kind) in ["column", "bar", "line", "area", "pie", "doughnut", "scatter"].iter().enumerate() {
            let align = ["left", "center", "right"][i % 3];
            doc.blocks.push(Block::Chart {
                chart: chart_for(kind),
                width_pt: 360.0,
                height_pt: 220.0,
                align: align.into(),
                caption: format!("Cap {kind}"),
            });
        }
        doc.blocks.push(Block::Chart {
            chart: chart_for("radar"),
            width_pt: 300.0,
            height_pt: 180.0,
            align: "center".into(),
            caption: "Fallback".into(),
        });
        let bytes = write_docx(&doc).expect("write docx");
        let zip = ZipReader::open(bytes.clone()).expect("open zip");
        assert!(zip.contains("word/charts/chart1.xml"));
        assert!(zip.contains("word/embeddings/Microsoft_Excel_Worksheet1.xlsx"));
        let read = read_docx(&bytes).expect("read docx");
        let count = read.document.blocks.iter().filter(|b| matches!(b, Block::Chart { .. })).count();
        assert_eq!(count, 7);
    }

    #[test]
    fn read_chart_block_warns_on_missing_rel_and_part() {
        let xml = concat!(
            "<w:p xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">",
            "<c:chart xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" ",
            "xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" r:id=\"rId9\"/>",
            "</w:p>",
        );
        let root = parse_xml(xml).unwrap();
        let node = root.children.first().unwrap();
        let reader = ZipReader::open(ZipWriter::new().finish()).unwrap();
        let mut warnings = Vec::new();
        assert!(read_chart_block(node, &HashMap::new(), &reader, "center", &mut warnings).is_none());
        assert_eq!(warnings.len(), 1);
        let mut rels = HashMap::new();
        rels.insert("rId9".into(), RelLink { target: "charts/missing.xml".into(), external: false });
        assert!(read_chart_block(node, &rels, &reader, "center", &mut warnings).is_none());
        assert_eq!(warnings.len(), 2);
    }
}
