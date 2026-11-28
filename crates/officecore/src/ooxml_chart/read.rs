//! ChartML reader (`word/charts/chartN.xml` -> `ChartData`).

use crate::model::{ChartData, ChartSeries};
use crate::xml::{parse_xml, XmlNode};

const DOUGHNUT_DEFAULT_HOLE: u32 = 50;
const MAX_CACHE_POINTS: usize = 100_000;

/// Depth-first search for the first descendant whose tag or local name matches `name`.
pub(crate) fn first_descendant<'a>(node: &'a XmlNode, name: &str) -> Option<&'a XmlNode> {
    for child in &node.children {
        if child.name == name || child.local_name() == name {
            return Some(child);
        }
        if let Some(found) = first_descendant(child, name) {
            return Some(found);
        }
    }
    None
}

fn cache_text_values(node: &XmlNode) -> Vec<String> {
    let mut found = Vec::new();
    node.find_all("strCache", &mut found);
    if found.is_empty() {
        node.find_all("numCache", &mut found);
    }
    let cache = match found.into_iter().next() {
        Some(cache) => cache,
        None => return Vec::new(),
    };
    let mut points: Vec<(u32, String)> = cache
        .children_named("pt")
        .filter_map(|pt| {
            let idx = pt.attr("idx").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
            pt.child("v").map(|v| (idx, v.deep_text()))
        })
        .collect();
    points.sort_by_key(|(idx, _)| *idx);
    points.into_iter().map(|(_, v)| v).take(MAX_CACHE_POINTS).collect()
}

fn cache_num_values(node: &XmlNode) -> Vec<f64> {
    let mut found = Vec::new();
    node.find_all("numCache", &mut found);
    let cache = match found.into_iter().next() {
        Some(cache) => cache,
        None => return Vec::new(),
    };
    let mut points: Vec<(u32, f64)> = cache
        .children_named("pt")
        .filter_map(|pt| {
            let idx = pt.attr("idx").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
            let val = pt.child("v")?.deep_text().trim().parse::<f64>().ok()?;
            Some((idx, val))
        })
        .collect();
    points.sort_by_key(|(idx, _)| *idx);
    points.into_iter().map(|(_, v)| v).take(MAX_CACHE_POINTS).collect()
}

fn line_hidden(ser: &XmlNode) -> bool {
    ser.child("spPr").and_then(|p| p.child("ln")).is_some_and(|l| l.child("noFill").is_some())
}

fn marker_hidden(ser: &XmlNode) -> bool {
    ser.child("marker").and_then(|m| m.child("symbol")).and_then(|s| s.attr("val")) == Some("none")
}

fn srgb_color(props: Option<&XmlNode>) -> Option<String> {
    props
        .and_then(|p| first_descendant(p, "srgbClr"))
        .and_then(|c| c.attr("val"))
        .map(|v| format!("#{}", v.trim_start_matches('#').to_ascii_uppercase()))
}

fn axis_title(axis: Option<&XmlNode>) -> String {
    axis.and_then(|a| first_descendant(a, "title")).map(XmlNode::deep_text).unwrap_or_default()
}

fn series_name(ser: &XmlNode) -> String {
    let tx = match ser.child("tx") {
        Some(tx) => tx,
        None => return String::new(),
    };
    first_descendant(tx, "v").map(XmlNode::deep_text).unwrap_or_else(|| tx.deep_text())
}

fn is_horizontal_axis(axis: &XmlNode) -> bool {
    matches!(axis.child("axPos").and_then(|p| p.attr("val")), Some("b" | "t"))
}

fn read_scatter_style(plot_child: &XmlNode) -> Option<String> {
    let declared = plot_child.child("scatterStyle").and_then(|n| n.attr("val")).unwrap_or("lineMarker");
    let series: Vec<&XmlNode> = plot_child.children_named("ser").collect();
    let smooth = declared.starts_with("smooth")
        || series
            .iter()
            .filter_map(|ser| ser.child("smooth"))
            .any(|n| n.attr("val").map(|v| matches!(v.trim(), "1" | "true")).unwrap_or(true));
    let lines = !series.iter().all(|s| line_hidden(s)) && !matches!(declared, "marker" | "none");
    let markers =
        !(!series.is_empty() && series.iter().all(|s| marker_hidden(s))) && !matches!(declared, "line" | "smooth");
    match (lines, markers, smooth) {
        (false, ..) => None,
        (true, true, false) => Some("lineMarker".into()),
        (true, false, false) => Some("line".into()),
        (true, true, true) => Some("smoothMarker".into()),
        (true, false, true) => Some("smooth".into()),
    }
}

/// Parses one ChartML part (`word/charts/chartN.xml`) into `ChartData`.
pub(crate) fn read_chart_xml(xml: &str, warnings: &mut Vec<String>) -> Option<ChartData> {
    let root = parse_xml(xml).ok()?;
    let plot = first_descendant(&root, "plotArea")?;
    let plot_child = plot.children.iter().find(|child| child.local_name().ends_with("Chart"))?;
    let kind = match plot_child.local_name() {
        "barChart" => {
            if first_descendant(plot_child, "barDir").and_then(|n| n.attr("val")) == Some("bar") {
                "bar".to_string()
            } else {
                "column".to_string()
            }
        }
        "lineChart" => "line".to_string(),
        "pieChart" => "pie".to_string(),
        "doughnutChart" => "doughnut".to_string(),
        "areaChart" => "area".to_string(),
        "scatterChart" => "scatter".to_string(),
        other => {
            let raw = other.trim_end_matches("Chart").to_ascii_lowercase();
            if !raw.is_empty() {
                warnings.push(format!("A \"{raw}\" chart was imported with limited support; it is kept in .oswk."));
            }
            raw
        }
    };
    let scatter = kind == "scatter";
    let chart_node = root.child("chart");
    let title = chart_node.and_then(|n| n.child("title")).map(XmlNode::deep_text).unwrap_or_default();
    let legend = chart_node.and_then(|n| n.child("legend")).is_some();
    let mut categories = String::new();
    let mut categories_cache = Vec::new();
    let mut series = Vec::new();
    let mut series_values_cache = Vec::new();

    for ser in plot_child.children_named("ser") {
        let name = series_name(ser);
        let (val_node, cat_node) =
            if scatter { (ser.child("yVal"), ser.child("xVal")) } else { (ser.child("val"), ser.child("cat")) };
        let range = val_node.and_then(|v| first_descendant(v, "f")).map(XmlNode::deep_text).unwrap_or_default();
        let color = if scatter {
            srgb_color(ser.child("spPr")).or_else(|| srgb_color(ser.child("marker").and_then(|m| m.child("spPr"))))
        } else {
            srgb_color(first_descendant(ser, "spPr"))
        };
        if let Some(cat) = cat_node {
            if categories.is_empty() {
                categories = first_descendant(cat, "f").map(XmlNode::deep_text).unwrap_or_default();
                categories_cache = cache_text_values(cat);
            }
        }
        series_values_cache.push(val_node.map(cache_num_values).unwrap_or_default());
        series.push(ChartSeries { name, range, color });
    }
    if series_values_cache.iter().all(Vec::is_empty) {
        series_values_cache.clear();
    }
    let stacked = first_descendant(plot_child, "grouping").and_then(|n| n.attr("val")) == Some("stacked");
    let labels = first_descendant(&root, "dLbls");
    let show_labels = labels.and_then(|l| first_descendant(l, "showVal")).and_then(|n| n.attr("val")) == Some("1");
    let (x_title, y_title) = if scatter {
        let axes = plot.children_of("valAx");
        let xi = axes.iter().position(|a| is_horizontal_axis(a)).unwrap_or(0);
        (axis_title(axes.get(xi).copied()), axis_title(axes.get(usize::from(xi == 0)).copied()))
    } else {
        (axis_title(first_descendant(plot, "catAx")), axis_title(first_descendant(plot, "valAx")))
    };
    let hole_size = if kind == "doughnut" {
        first_descendant(plot_child, "holeSize")
            .and_then(|n| n.attr("val"))
            .and_then(|v| v.parse::<u32>().ok())
            .map(|h| h.clamp(10, 90))
            .filter(|h| *h != DOUGHNUT_DEFAULT_HOLE)
    } else {
        None
    };
    let scatter_style = if scatter { read_scatter_style(plot_child) } else { None };
    Some(ChartData {
        kind,
        title,
        categories,
        series,
        legend,
        x_title,
        y_title,
        stacked,
        show_labels,
        categories_cache,
        series_values_cache,
        hole_size,
        scatter_style,
    })
}

#[cfg(test)]
mod tests {
    use super::read_chart_xml;
    use crate::model::{ChartData, ChartSeries};
    use crate::ooxml_chart::{chart_part_xml, chart_text_summary, embedded_workbook};
    use crate::xlsx::read_workbook_bytes;

    fn sample(kind: &str, stacked: bool, scatter_style: Option<&str>) -> ChartData {
        let cats = if kind == "scatter" { vec!["1".into(), "2".into()] } else { vec!["Jan".into(), "Feb".into()] };
        ChartData {
            kind: kind.into(),
            title: format!("{kind} Chart"),
            categories: String::new(),
            series: vec![ChartSeries { name: "Rev".into(), range: String::new(), color: Some("#2563EB".into()) }],
            legend: true,
            x_title: "X".into(),
            y_title: "Y".into(),
            stacked,
            show_labels: kind == "pie" || kind == "column",
            categories_cache: cats,
            series_values_cache: vec![vec![10.5, 25.0]],
            hole_size: if kind == "doughnut" { Some(65) } else { None },
            scatter_style: scatter_style.map(str::to_string),
        }
    }

    #[test]
    fn chart_xml_and_workbook_round_trip_all_kinds() {
        for (kind, stacked, style) in [
            ("column", true, None),
            ("bar", false, None),
            ("line", false, None),
            ("area", true, None),
            ("pie", false, None),
            ("doughnut", false, None),
            ("scatter", false, Some("line")),
            ("scatter", false, Some("lineMarker")),
            ("scatter", false, Some("smooth")),
            ("scatter", false, Some("smoothMarker")),
        ] {
            let chart = sample(kind, stacked, style);
            let xml = chart_part_xml(&chart, Some("rId1")).expect("serialize chart");
            let mut warnings = Vec::new();
            let parsed = read_chart_xml(&xml, &mut warnings).expect("parse chart");
            assert_eq!(parsed.kind, kind);
            assert_eq!(parsed.title, chart.title);
            assert_eq!(parsed.stacked, stacked);
            assert_eq!(parsed.hole_size, chart.hole_size);
            assert!(warnings.is_empty());
        }
        let mut text_scatter = sample("scatter", false, None);
        text_scatter.categories_cache = vec!["low".into(), "high".into()];
        let xml = chart_part_xml(&text_scatter, None).unwrap();
        let parsed = read_chart_xml(&xml, &mut Vec::new()).unwrap();
        assert_eq!(parsed.categories_cache, vec!["low", "high"]);

        let wb_bytes = embedded_workbook(&sample("column", false, None)).unwrap();
        let wb = read_workbook_bytes(&wb_bytes).unwrap();
        assert_eq!(wb.workbook.sheets[0].name, "Sheet1");
    }

    #[test]
    fn chart_xml_errors_summary_and_unknown_warnings() {
        let mut chart = sample("radar", false, None);
        assert!(chart_part_xml(&chart, None).is_err());
        chart.kind = "column".into();
        chart.series_values_cache.clear();
        chart.categories_cache.clear();
        assert!(chart_part_xml(&chart, None).is_err());
        chart.series.clear();
        assert!(chart_part_xml(&chart, None).is_err());

        let summary_cap = chart_text_summary(&sample("bar", false, None), "Cap");
        assert!(summary_cap.contains("Cap"));
        assert!(chart_text_summary(&sample("bar", false, None), "").contains("bar Chart"));
        chart.title.clear();
        assert!(chart_text_summary(&chart, "").contains("[Chart (column): column]"));

        let radar = concat!(
            "<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\">",
            "<c:chart><c:plotArea><c:radarChart><c:ser><c:tx><c:v>R</c:v></c:tx></c:ser>",
            "</c:radarChart></c:plotArea></c:chart></c:chartSpace>"
        );
        let mut warnings = Vec::new();
        let parsed = read_chart_xml(radar, &mut warnings).unwrap();
        assert_eq!(parsed.kind, "radar");
        assert_eq!(warnings.len(), 1);
        assert!(read_chart_xml("<not-xml", &mut warnings).is_none());
    }
}
