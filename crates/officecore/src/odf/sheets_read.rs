//! ODS (Calc) import: sheets, cells, charts, conditional formats and links.

use super::sheets_write::OdsLocalTable;
use super::*;
use crate::xml::{parse_xml, XmlNode};
use std::collections::HashMap;
use std::path::Path;

fn read_ods_table_row(row: &XmlNode) -> Vec<(String, Option<f64>)> {
    let mut cells = Vec::new();
    for cell in row.children.iter().filter(|cell| matches!(cell.local_name(), "table-cell" | "covered-table-cell")) {
        let repeat = cell
            .attr_any_ns("number-columns-repeated")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(1)
            .clamp(1, 256);
        let mut paragraphs = Vec::new();
        cell.find_all("p", &mut paragraphs);
        let text = paragraphs.iter().map(|node| node.deep_text()).collect::<Vec<_>>().join("\n");
        let number = cell
            .attr_any_ns("value")
            .and_then(|value| value.trim().parse::<f64>().ok())
            .or_else(|| text.trim().parse::<f64>().ok())
            .filter(|value| value.is_finite());
        for _ in 0..repeat {
            cells.push((text.clone(), number));
        }
        if cells.len() > 1_024 {
            break;
        }
    }
    cells
}

fn read_ods_local_table(table: &XmlNode) -> OdsLocalTable {
    let mut header = Vec::new();
    let mut rows = Vec::new();
    for child in &table.children {
        match child.local_name() {
            "table-header-rows" => {
                if let Some(row) = child.child("table-row") {
                    header = read_ods_table_row(row).into_iter().map(|(text, _)| text).collect();
                }
            }
            "table-rows" => rows.extend(child.children_named("table-row").map(read_ods_table_row)),
            "table-row" => rows.push(read_ods_table_row(child)),
            _ => {}
        }
    }
    rows.truncate(ODS_MAX_CHART_ROWS);
    OdsLocalTable { header, rows }
}

/// The `style:<properties>` child of the automatic style `name`.
fn ods_style_child<'a>(
    styles: &HashMap<&str, &'a XmlNode>,
    name: Option<&str>,
    properties: &str,
) -> Option<&'a XmlNode> {
    styles.get(name?).copied()?.child(properties)
}

fn odf_paragraph_text(node: &XmlNode) -> String {
    let mut paragraphs = Vec::new();
    node.find_all("p", &mut paragraphs);
    paragraphs.iter().map(|paragraph| paragraph.deep_text()).collect::<Vec<_>>().join("\n")
}

/// Reads the chart object a sheet `draw:frame` embeds (`Object N/content.xml`)
/// back into a placement. Frames holding anything else are skipped; caches
/// come from the object's local table, the inverse of the export.
pub(super) fn read_ods_chart(
    reader: &ZipReader,
    frame: &XmlNode,
    anchor: String,
    sheet: &Sheet,
    seen_objects: &mut HashSet<String>,
    warnings: &mut Vec<String>,
) -> Option<ChartPlacement> {
    let object = frame.child("object")?;
    let href = object.attr_any_ns("href")?.trim().trim_start_matches("./").trim_end_matches('/').to_string();
    if href.is_empty() {
        return None;
    }
    // Each object is read once: frames repeating one href would otherwise each
    // hold a full copy of its caches.
    if !seen_objects.insert(href.clone()) {
        warnings.push(format!("The embedded object \"{href}\" is shown more than once; only the first copy was kept."));
        return None;
    }
    let Some(root) = reader.read_text(&format!("{href}/content.xml")).ok().and_then(|text| parse_xml(&text).ok())
    else {
        warnings.push(format!("The embedded object \"{href}\" could not be read and was skipped."));
        return None;
    };
    let mut nodes = Vec::new();
    root.find_all("chart", &mut nodes);
    let Some(chart) = nodes.into_iter().find(|node| node.attr_any_ns("class").is_some()) else {
        warnings.push(format!("The embedded object \"{href}\" is not a chart and was skipped."));
        return None;
    };
    let mut style_nodes = Vec::new();
    root.find_all("style", &mut style_nodes);
    let styles: HashMap<&str, &XmlNode> =
        style_nodes.into_iter().filter_map(|node| node.attr("style:name").map(|name| (name, node))).collect();
    let plot = chart.child("plot-area");
    let plot_properties =
        ods_style_child(&styles, plot.and_then(|plot| plot.attr_any_ns("style-name")), "chart-properties");
    let plot_flag = |name: &str| plot_properties.and_then(|node| node.attr_any_ns(name)) == Some("true");
    let class = chart.attr_any_ns("class").unwrap_or("").rsplit(':').next().unwrap_or("");
    let kind = match class {
        "bar" if plot_flag("vertical") => "bar",
        "bar" => "column",
        "line" => "line",
        "circle" => "pie",
        "ring" => "doughnut",
        "area" => "area",
        "scatter" => "scatter",
        other => {
            let mapped = match other {
                "filled-radar" => "area",
                "bubble" | "stock" | "radar" => "line",
                _ => "column",
            };
            warnings.push(format!("A \"{other}\" chart was imported as a {mapped} chart."));
            mapped
        }
    };
    let mut x_title = String::new();
    let mut y_title = String::new();
    let mut categories = String::new();
    for axis in plot.map(|plot| plot.children_of("axis")).unwrap_or_default() {
        let title = axis.child("title").map(odf_paragraph_text).unwrap_or_default();
        match axis.attr_any_ns("dimension") {
            Some("x") => {
                x_title = title;
                if let Some(range) = axis.child("categories").and_then(|node| node.attr_any_ns("cell-range-address")) {
                    categories = ods_range_to_ours(range);
                }
            }
            Some("y") => y_title = title,
            _ => {}
        }
    }
    let local = chart.child("table").map(read_ods_local_table);
    let labels_on = |node: Option<&XmlNode>| {
        node.and_then(|node| node.attr_any_ns("data-label-number")).map(|value| value != "none").unwrap_or(false)
    };
    let mut show_labels = labels_on(plot_properties);
    let mut series = Vec::new();
    let mut series_values_cache = Vec::new();
    let mut series_nodes = plot.map(|plot| plot.children_of("series")).unwrap_or_default();
    if series_nodes.len() > ODS_MAX_CHART_SERIES {
        warnings
            .push(format!("A chart with {} series was cut to its first {ODS_MAX_CHART_SERIES}.", series_nodes.len()));
        series_nodes.truncate(ODS_MAX_CHART_SERIES);
    }
    // Cached values are a convenience (the ranges stay): drop them rather than
    // allocate series x rows values past the limit.
    let cache_rows = local.as_ref().map_or(0, |table| table.rows.len());
    let keep_caches = cache_rows.saturating_mul(series_nodes.len()) <= ODS_MAX_CHART_VALUES;
    if !keep_caches {
        warnings.push("A chart's cached values were too large and were left out; its cell ranges are kept.".into());
    }
    let mut lines_hidden = true;
    let mut markers_hidden = true;
    for (index, node) in series_nodes.into_iter().enumerate() {
        let style_name = node.attr_any_ns("style-name");
        show_labels |= labels_on(ods_style_child(&styles, style_name, "chart-properties"));
        let graphic = ods_style_child(&styles, style_name, "graphic-properties");
        if kind == "scatter" {
            // A scatter series draws a line unless its stroke is `none` and
            // shows markers unless its symbol type is `none`.
            lines_hidden &= graphic.and_then(|graphic| graphic.attr_any_ns("stroke")) == Some("none");
            markers_hidden &= ods_style_child(&styles, style_name, "chart-properties")
                .and_then(|properties| properties.attr_any_ns("symbol-type"))
                == Some("none");
            // The X values are the series' domain; the first one is the chart's.
            if categories.is_empty() {
                if let Some(range) = node.child("domain").and_then(|domain| domain.attr_any_ns("cell-range-address")) {
                    categories = ods_range_to_ours(range);
                }
            }
        }
        let color = graphic
            .and_then(|graphic| {
                let stroke = graphic.attr_any_ns("stroke-color");
                let fill = graphic.attr_any_ns("fill-color");
                if kind == "line" {
                    stroke.or(fill)
                } else {
                    fill.or(stroke)
                }
            })
            .and_then(ods_color)
            .map(|color| color.to_ascii_uppercase());
        let name = node
            .attr_any_ns("label-string")
            .map(|literal| {
                let literal = literal.trim();
                match literal.strip_prefix('"').and_then(|inner| inner.strip_suffix('"')) {
                    Some(inner) => inner.replace("\"\"", "\""),
                    None => literal.to_string(),
                }
            })
            .or_else(|| {
                local.as_ref().and_then(|table| table.header.get(index + 1)).filter(|name| !name.is_empty()).cloned()
            })
            .or_else(|| {
                let address = ods_range_to_ours(node.attr_any_ns("label-cell-address")?);
                sheet.get(address.split(':').next()?).map(|cell| ods_cell_label(&cell.value))
            })
            .unwrap_or_else(|| format!("Series {}", index + 1));
        let range = node.attr_any_ns("values-cell-range-address").map(ods_range_to_ours).unwrap_or_default();
        let values: Vec<f64> = local
            .as_ref()
            .filter(|_| keep_caches)
            .filter(|table| table.rows.iter().any(|row| row.len() > index + 1))
            .map(|table| {
                table.rows.iter().map(|row| row.get(index + 1).and_then(|cell| cell.1).unwrap_or(0.0)).collect()
            })
            .unwrap_or_default();
        series_values_cache.push(values);
        series.push(ChartSeries { name, range, color });
    }
    // Empty caches are dropped, as the XLSX importer does, so a chart that only
    // carries ranges stays range-only.
    if series_values_cache.iter().all(Vec::is_empty) {
        series_values_cache.clear();
    }
    let mut categories_cache: Vec<String> = local
        .as_ref()
        .filter(|_| keep_caches)
        .map(|table| table.rows.iter().map(|row| row.first().map(|cell| cell.0.clone()).unwrap_or_default()).collect())
        .unwrap_or_default();
    if categories_cache.iter().all(String::is_empty) {
        categories_cache.clear();
    }
    let size = |attribute: &str, fallback: f64| {
        frame
            .attr_any_ns(attribute)
            .or_else(|| chart.attr_any_ns(attribute))
            .and_then(parse_cm)
            .map(|pt| pt / 0.75)
            .unwrap_or(fallback)
    };
    Some(ChartPlacement {
        id: frame
            .attr("draw:name")
            .filter(|name| !name.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        chart: ChartData {
            kind: kind.to_string(),
            title: chart.child("title").map(odf_paragraph_text).unwrap_or_default(),
            categories,
            series,
            legend: chart.child("legend").is_some(),
            x_title,
            y_title,
            stacked: plot_flag("stacked") || plot_flag("percentage"),
            show_labels,
            categories_cache,
            series_values_cache,
            hole_size: None,
            scatter_style: (kind == "scatter" && !lines_hidden).then(|| {
                let smooth = plot_properties.and_then(|node| node.attr_any_ns("interpolation")) == Some("cubic-spline");
                match (markers_hidden, smooth) {
                    (false, false) => "lineMarker",
                    (true, false) => "line",
                    (false, true) => "smoothMarker",
                    (true, true) => "smooth",
                }
                .to_string()
            }),
        },
        anchor,
        width_px: size("width", 480.0),
        height_px: size("height", 288.0),
    })
}

/// Anchor cell for a chart placed on the sheet (`table:shapes`) rather than in
/// a cell, estimated from its position with LibreOffice's default cell size.
fn ods_shape_anchor(frame: &XmlNode) -> String {
    let x = frame.attr_any_ns("x").and_then(parse_cm).unwrap_or(0.0).max(0.0);
    let y = frame.attr_any_ns("y").and_then(parse_cm).unwrap_or(0.0).max(0.0);
    crate::address::format(
        ((y / ODS_DEFAULT_ROW_PT).floor() as u32).min(ODS_MAX_READ_ROW),
        ((x / ODS_DEFAULT_COLUMN_PT).floor() as u32).min(ODS_MAX_READ_COLUMN),
    )
}

/// True when a cell carries a value, a formula or text. The empty padding
/// LibreOffice writes past the data is not content.
fn ods_cell_has_content(cell: &XmlNode) -> bool {
    if ["formula", "value", "boolean-value", "date-value"].iter().any(|name| cell.attr_any_ns(name).is_some()) {
        return true;
    }
    let mut paragraphs = Vec::new();
    cell.find_all("p", &mut paragraphs);
    paragraphs.iter().any(|node| !node.deep_text().is_empty())
}

/// A cell note as `office:annotation`, the first child of its cell.
pub(super) fn ods_annotation(text: &str, author: Option<&str>, visible: bool) -> String {
    let paragraphs: String =
        text.lines().map(|line| format!("<text:p>{}</text:p>", crate::xml::escape_text(line))).collect();
    let creator = author
        .filter(|author| !author.trim().is_empty())
        .map(|author| format!("<dc:creator>{}</dc:creator>", crate::xml::escape_text(author.trim())))
        .unwrap_or_default();
    format!("<office:annotation office:display=\"{visible}\">{creator}{paragraphs}</office:annotation>")
}

/// An internal link in ODS spells the sheet separator `.` (`#Sheet2.A1`,
/// `#'My Sheet'.A1`); the model, like XLSX, uses `!`.
fn swap_sheet_separator(reference: &str, from: char, to: char) -> String {
    let mut quoted = false;
    let mut last = None;
    for (index, ch) in reference.char_indices() {
        match ch {
            '\'' => quoted = !quoted,
            ch if ch == from && !quoted => last = Some(index),
            _ => {}
        }
    }
    match last {
        Some(index) => format!("{}{to}{}", &reference[..index], &reference[index + 1..]),
        None => reference.to_string(),
    }
}

/// The `xlink:href` for a model link target.
pub(super) fn ods_href(target: &str) -> String {
    match target.strip_prefix('#') {
        Some(reference) => format!("#{}", swap_sheet_separator(reference, '!', '.')),
        None => target.to_string(),
    }
}

/// A cell's `office:annotation` as (text, author, shown permanently).
fn read_ods_annotation(annotation: &XmlNode) -> Option<(String, Option<String>, bool)> {
    let text = annotation.children_of("p").iter().map(|node| node.deep_text()).collect::<Vec<_>>().join("\n");
    if text.trim().is_empty() {
        return None;
    }
    let author = annotation
        .child("creator")
        .map(|creator| creator.deep_text().trim().to_string())
        .filter(|author| !author.is_empty());
    Some((text, author, annotation.attr_any_ns("display") == Some("true")))
}

/// The hyperlink of a cell: (target, screen tip) of the first `text:a` in its
/// paragraphs. A target that is not http, https, mailto or internal is counted in
/// `dropped` and ignored.
fn read_ods_cell_link(cell: &XmlNode, dropped: &mut usize) -> Option<(String, Option<String>)> {
    let mut anchors = Vec::new();
    for paragraph in cell.children_of("p") {
        paragraph.find_all("a", &mut anchors);
    }
    let anchor = anchors.into_iter().find(|anchor| anchor.attr_any_ns("href").is_some())?;
    let href = anchor.attr_any_ns("href")?.trim();
    let target = match href.strip_prefix('#') {
        Some(reference) => format!("#{}", swap_sheet_separator(reference, '.', '!')),
        None => href.to_string(),
    };
    let Some(target) = safe_link_target(&target) else {
        *dropped += 1;
        return None;
    };
    let tooltip = anchor.attr_any_ns("title").map(str::to_string).filter(|title| !title.is_empty());
    Some((target, tooltip))
}

/// Conditional-format entries kept per sheet; a hostile file cannot make the
/// importer build an unbounded rule list.
const ODS_MAX_CONDITIONAL_RULES: usize = 5_000;

/// What a table-cell style paints: fill, font colour, bold, italic.
type OdsLook = (Option<String>, Option<String>, bool, bool);

/// The looks of every table-cell style in `roots` (content.xml's automatic
/// styles and styles.xml), by style name.
fn ods_cell_looks(roots: &[&XmlNode]) -> HashMap<String, OdsLook> {
    let mut looks = HashMap::new();
    for root in roots {
        let mut nodes = Vec::new();
        root.find_all("style", &mut nodes);
        for node in nodes {
            if node.attr("style:family") != Some("table-cell") {
                continue;
            }
            let Some(name) = node.attr("style:name") else { continue };
            let fill = node
                .child("table-cell-properties")
                .and_then(|properties| properties.attr_any_ns("background-color"))
                .and_then(ods_color)
                .map(|color| color.to_ascii_uppercase());
            let text = node.child("text-properties");
            let color = text
                .and_then(|text| text.attr_any_ns("color"))
                .and_then(ods_color)
                .map(|color| color.to_ascii_uppercase());
            let bold = text.and_then(|text| text.attr_any_ns("font-weight")) == Some("bold");
            let italic = text.and_then(|text| text.attr_any_ns("font-style")) == Some("italic");
            // LibreOffice escapes odd characters in `style:name` (`ConditionalStyle_5f_1`)
            // and refers to the style by its display name.
            if let Some(display) = node.attr("style:display-name") {
                looks.insert(display.to_string(), (fill.clone(), color.clone(), bold, italic));
            }
            looks.insert(name.to_string(), (fill, color, bold, italic));
        }
    }
    looks
}

/// Splits an ODF cell range list on whitespace outside quoted sheet names and
/// maps each range to the sheet-local form (`Sheet1.A1:Sheet1.B5` -> `A1:B5`).
fn ods_ranges_to_ours(addresses: &str) -> String {
    let mut parts = vec![String::new()];
    let mut quoted = false;
    for ch in addresses.trim().chars() {
        match ch {
            '\'' => {
                quoted = !quoted;
                parts.last_mut().unwrap().push(ch);
            }
            ch if ch.is_whitespace() && !quoted => parts.push(String::new()),
            ch => parts.last_mut().unwrap().push(ch),
        }
    }
    parts
        .iter()
        .filter(|part| !part.is_empty())
        .map(|part| {
            let range = ods_range_to_ours(part);
            // LibreOffice spells a single cell as `Sheet1.A1:Sheet1.A1`.
            match range.split_once(':') {
                Some((start, end)) if start == end => start.to_string(),
                _ => range,
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Splits condition arguments on `,` or `;` outside string literals and nested
/// parentheses.
pub(crate) fn split_ods_args(inner: &str) -> Vec<String> {
    let mut args = vec![String::new()];
    let (mut quoted, mut depth) = (false, 0i32);
    for ch in inner.chars() {
        match ch {
            '"' => quoted = !quoted,
            '(' if !quoted => depth += 1,
            ')' if !quoted => depth -= 1,
            ',' | ';' if !quoted && depth == 0 => {
                args.push(String::new());
                continue;
            }
            _ => {}
        }
        args.last_mut().unwrap().push(ch);
    }
    args.into_iter().map(|arg| arg.trim().to_string()).collect()
}

/// `"a ""b"""` -> `a "b"`; text without quotes is returned as written.
pub(crate) fn unquote_ods(text: &str) -> String {
    let text = text.trim();
    match text.strip_prefix('"').and_then(|inner| inner.strip_suffix('"')) {
        Some(inner) => inner.replace("\"\"", "\""),
        None => text.to_string(),
    }
}

/// A `calcext:value` condition as a model rule (kind, operands and formula
/// only). Conditions that are only formulas in the editor's vocabulary
/// (`>=`, begins-with, unique, above-average, ...) become `expression` rules.
fn ods_condition_rule(value: &str, anchor: &str, all: &str) -> Option<CondRule> {
    let value = value.trim();
    let mut rule = CondRule::default();
    let call = |name: &str| value.strip_prefix(name).and_then(|rest| rest.strip_suffix(')'));
    let mut expression = |formula: String| {
        rule.kind = "expression".into();
        rule.formula = Some(formula);
    };
    // LibreOffice writes `>5`, `between(1,9)` and `formula-is(...)`; the ODF
    // style-map spellings (`cell-content()>5`, `cell-content-is-between(1,9)`,
    // `is-true-formula(...)`) come from other producers and are read as well.
    let operand_form = value.strip_prefix("cell-content()").unwrap_or(value);
    if let Some(inner) = call("formula-is(").or_else(|| call("is-true-formula(")) {
        expression(formula_from_ods(inner.trim()));
    } else if let Some(inner) = call("between(").or_else(|| call("cell-content-is-between(")) {
        let args = split_ods_args(inner);
        let [low, high] = args.as_slice() else { return None };
        rule.kind = "between".into();
        rule.values = vec![low.clone(), high.clone()];
    } else if let Some(inner) = call("not-between(").or_else(|| call("cell-content-is-not-between(")) {
        let args = split_ods_args(inner);
        let [low, high] = args.as_slice() else { return None };
        expression(format!("OR({anchor}<{low},{anchor}>{high})"));
    } else if let Some(operator) =
        [">=", "<=", "!=", "<>", ">", "<", "="].into_iter().find(|operator| operand_form.starts_with(operator))
    {
        let operand = operand_form[operator.len()..].trim().to_string();
        match operator {
            ">" | "<" | "=" => {
                rule.kind = match operator {
                    ">" => "greater",
                    "<" => "less",
                    _ => "equal",
                }
                .into();
                rule.values = vec![operand];
            }
            "!=" | "<>" => expression(format!("{anchor}<>{operand}")),
            _ => expression(format!("{anchor}{operator}{operand}")),
        }
    } else if let Some(inner) = call("contains-text(") {
        rule.kind = "textContains".into();
        rule.values = vec![unquote_ods(inner)];
    } else if let Some(inner) = call("not-contains-text(") {
        let text = unquote_ods(inner).replace('"', "\"\"");
        expression(format!("ISERROR(SEARCH(\"{text}\",{anchor}))"));
    } else if let Some(inner) = call("begins-with(") {
        let text = unquote_ods(inner).replace('"', "\"\"");
        expression(format!("LEFT({anchor},LEN(\"{text}\"))=\"{text}\""));
    } else if let Some(inner) = call("ends-with(") {
        let text = unquote_ods(inner).replace('"', "\"\"");
        expression(format!("RIGHT({anchor},LEN(\"{text}\"))=\"{text}\""));
    } else if matches!(value, "duplicate" | "is-duplicate") {
        rule.kind = "duplicate".into();
    } else if matches!(value, "unique" | "is-unique") {
        expression(format!("COUNTIF({all},{anchor})=1"));
    } else if let Some(inner) = call("top-elements(") {
        rule.kind = "top".into();
        rule.top_n = Some(inner.trim().parse().ok()?);
    } else if let Some(inner) = call("bottom-elements(") {
        rule.kind = "bottom".into();
        rule.top_n = Some(inner.trim().parse().ok()?);
    } else if let Some(inner) = call("top-percent(") {
        let rank: u32 = inner.trim().parse().ok()?;
        expression(format!("{anchor}>=PERCENTILE({all},1-{rank}/100)"));
    } else if let Some(inner) = call("bottom-percent(") {
        let rank: u32 = inner.trim().parse().ok()?;
        expression(format!("{anchor}<=PERCENTILE({all},{rank}/100)"));
    } else if let Some(operator) = match value {
        "above-average" => Some(">"),
        "above-equal-average" => Some(">="),
        "below-average" => Some("<"),
        "below-equal-average" => Some("<="),
        _ => None,
    } {
        expression(format!("{anchor}{operator}AVERAGE({all})"));
    } else if value == "is-error" {
        expression(format!("ISERROR({anchor})"));
    } else if value == "is-no-error" {
        expression(format!("NOT(ISERROR({anchor}))"));
    } else {
        return None;
    }
    Some(rule)
}

/// One `calcext:formatting-entry` / `color-scale-entry` as a threshold.
fn ods_threshold(node: &XmlNode) -> Option<CondThreshold> {
    let kind = match node.attr_any_ns("type")? {
        "minimum" | "auto-minimum" => "min",
        "maximum" | "auto-maximum" => "max",
        "value" | "number" => "num",
        "percent" => "percent",
        "percentile" => "percentile",
        "formula" => "formula",
        _ => return None,
    };
    let value = if matches!(kind, "min" | "max") {
        String::new()
    } else {
        node.attr_any_ns("value").unwrap_or("").trim().to_string()
    };
    let color = node.attr_any_ns("color").and_then(ods_color).map(|color| color.to_ascii_uppercase());
    Some(CondThreshold { kind: kind.to_string(), value, color })
}

/// Reads the `calcext:conditional-formats` of one sheet; returns how many
/// entries the model could not hold.
fn read_ods_conditional_formats(formats: &XmlNode, sheet: &mut Sheet, looks: &HashMap<String, OdsLook>) -> usize {
    let mut unsupported = 0usize;
    for format in formats.children_of("conditional-format") {
        let range = ods_ranges_to_ours(format.attr_any_ns("target-range-address").unwrap_or(""));
        if range.is_empty() {
            continue;
        }
        let all = crate::xlsx::absolute_area(&range);
        for entry in &format.children {
            if sheet.conditional.len() >= ODS_MAX_CONDITIONAL_RULES {
                return unsupported + 1;
            }
            let model = match entry.local_name() {
                "condition" => {
                    let base = entry.attr_any_ns("base-cell-address").map(ods_range_to_ours).unwrap_or_default();
                    let anchor = base.split(':').next().filter(|cell| !cell.is_empty()).map(str::to_string);
                    let anchor = anchor.unwrap_or_else(|| crate::xlsx::anchor_of(&range));
                    let rule = ods_condition_rule(entry.attr_any_ns("value").unwrap_or(""), &anchor, &all);
                    rule.map(|mut rule| {
                        if let Some((fill, color, bold, italic)) =
                            entry.attr_any_ns("apply-style-name").and_then(|name| looks.get(name))
                        {
                            rule.fill = fill.clone();
                            rule.color = color.clone();
                            rule.bold = *bold;
                            rule.italic = *italic;
                        }
                        rule
                    })
                }
                "color-scale" => {
                    let stops: Option<Vec<CondThreshold>> =
                        entry.children_of("color-scale-entry").into_iter().map(ods_threshold).collect();
                    stops.filter(|stops| (2..=3).contains(&stops.len())).map(|thresholds| CondRule {
                        kind: "colorScale".into(),
                        thresholds,
                        ..Default::default()
                    })
                }
                "data-bar" => {
                    let bounds: Option<Vec<CondThreshold>> =
                        entry.children_of("formatting-entry").into_iter().map(ods_threshold).collect();
                    bounds.map(|bounds| {
                        // The automatic bounds are the default; only explicit ones are kept.
                        let thresholds = match bounds.as_slice() {
                            [low, high] if !(low.kind == "min" && high.kind == "max") => bounds.clone(),
                            _ => Vec::new(),
                        };
                        CondRule {
                            kind: "dataBar".into(),
                            fill: entry
                                .attr_any_ns("positive-color")
                                .and_then(ods_color)
                                .map(|color| color.to_ascii_uppercase()),
                            hide_value: entry.attr_any_ns("show-value") == Some("false"),
                            thresholds,
                            ..Default::default()
                        }
                    })
                }
                "icon-set" => {
                    let name = entry.attr_any_ns("icon-set-type").unwrap_or("3TrafficLights1").trim().to_string();
                    let thresholds: Option<Vec<CondThreshold>> =
                        entry.children_of("formatting-entry").into_iter().map(ods_threshold).collect();
                    crate::xlsx::icon_set_count(&name).zip(thresholds).filter(|(count, list)| list.len() == *count).map(
                        |(_, thresholds)| CondRule {
                            kind: "iconSet".into(),
                            icon_set: Some(name),
                            reverse_icons: entry.attr_any_ns("reverse") == Some("true"),
                            // LibreOffice says "hide the value" on the first entry.
                            hide_value: entry.attr_any_ns("show-value") == Some("false")
                                || entry
                                    .children_of("formatting-entry")
                                    .iter()
                                    .any(|threshold| threshold.attr_any_ns("show-value") == Some("false")),
                            thresholds,
                            ..Default::default()
                        },
                    )
                }
                _ => None,
            };
            match model {
                Some(mut rule) => {
                    rule.id = format!("cf{}", sheet.conditional.len() + 1);
                    rule.range = range.clone();
                    sheet.conditional.push(rule);
                }
                None => unsupported += 1,
            }
        }
    }
    unsupported
}

/// Whether an ODF row or column element is hidden: `collapse` is a hand-hidden
/// one, `filter` one an AutoFilter hid; both show as size 0 in the editor.
fn ods_collapsed(node: &XmlNode) -> bool {
    matches!(node.attr_any_ns("visibility"), Some("collapse" | "filter"))
}

/// `table:display="false"` on a row or column group: the group is folded.
fn ods_group_folded(group: &XmlNode) -> bool {
    group.attr_any_ns("display") == Some("false")
}

/// The rows of a table in document order with whether each is hidden. Rows may
/// sit in `table-header-rows`, `table-rows` and nested `table-row-group`s; a
/// folded group hides every row it holds.
fn collect_ods_rows<'a>(node: &'a XmlNode, hidden: bool, out: &mut Vec<(&'a XmlNode, bool)>) {
    for child in &node.children {
        match child.local_name() {
            "table-row" => out.push((child, hidden || ods_collapsed(child))),
            "table-rows" | "table-header-rows" => collect_ods_rows(child, hidden, out),
            "table-row-group" => collect_ods_rows(child, hidden || ods_group_folded(child), out),
            _ => {}
        }
    }
}

/// Runs of hidden columns as (first column, count), from the column elements of
/// a table, which may sit in `table-columns`, `table-header-columns` and groups.
fn ods_hidden_column_runs(table: &XmlNode) -> Vec<(u32, u32)> {
    fn walk(node: &XmlNode, hidden: bool, column: &mut u32, runs: &mut Vec<(u32, u32)>) {
        for child in &node.children {
            match child.local_name() {
                "table-column" => {
                    let repeat = child
                        .attr_any_ns("number-columns-repeated")
                        .and_then(|value| value.parse::<u32>().ok())
                        .unwrap_or(1)
                        .max(1);
                    if hidden || ods_collapsed(child) {
                        runs.push((*column, repeat));
                    }
                    *column = column.saturating_add(repeat);
                }
                "table-columns" | "table-header-columns" => walk(child, hidden, column, runs),
                "table-column-group" => walk(child, hidden || ods_group_folded(child), column, runs),
                _ => {}
            }
        }
    }
    let mut runs = Vec::new();
    walk(table, false, &mut 0, &mut runs);
    runs
}

pub fn read_ods(bytes: &[u8]) -> OfficeResult<SheetRead> {
    let reader = ZipReader::open(bytes.to_vec())?;
    if !reader.contains("content.xml") {
        return Err(OfficeError::corrupt("The package does not contain content.xml."));
    }
    let text = reader.read_text("content.xml")?;
    let root = parse_xml(&text)?;
    let mut workbook = Workbook::new_blank("Imported spreadsheet");
    workbook.sheets.clear();
    let mut warnings = Vec::new();
    let mut tables = Vec::new();
    root.find_all("table", &mut tables);
    let mut seen_objects = HashSet::new();
    // The named and automatic cell styles conditional formats point at.
    let styles_root = reader.read_text("styles.xml").ok().and_then(|text| parse_xml(&text).ok());
    let looks = ods_cell_looks(&[&root, styles_root.as_ref().unwrap_or(&root)]);
    let mut unsupported_formats = 0usize;
    let mut dropped_links = 0usize;
    for table in tables {
        // `table:name`: a plain `attr("name")` never matched the prefixed
        // attribute, so every imported sheet used to be called "Sheet".
        let name = table.attr_any_ns("name").unwrap_or("Sheet").to_string();
        let mut sheet = Sheet::new(&name);
        // Frames anchored to a cell, with that cell; charts are read once the
        // cells are in so a series label cell can be resolved.
        let mut frames: Vec<(String, &XmlNode)> = Vec::new();
        let mut row = 0u32;
        let mut last_row = 0u32;
        let mut cut_off = false;
        // Runs of hidden rows as (first row, count), applied once the grid is known.
        let mut hidden_row_runs: Vec<(u32, u32)> = Vec::new();
        let mut table_rows = Vec::new();
        collect_ods_rows(table, false, &mut table_rows);
        for (row_node, row_hidden) in table_rows {
            // The repeat only moves the position (a repeated row's cells are
            // set once), so it is taken as written: capping it shifted every
            // cell after a long empty gap up to the wrong row.
            let repeat_rows =
                row_node.attr_any_ns("number-rows-repeated").and_then(|value| value.parse::<u32>().ok()).unwrap_or(1);
            if row_hidden {
                hidden_row_runs.push((row, repeat_rows.max(1)));
            }
            let mut column = 0u32;
            for cell in row_node.children_named("table-cell") {
                let repeat = cell
                    .attr_any_ns("number-columns-repeated")
                    .and_then(|value| value.parse::<u32>().ok())
                    .unwrap_or(1)
                    .min(1024);
                // Past the imported area only charts are still picked up, so a
                // chart anchored far down or right is not lost on reopening.
                if row > ODS_MAX_READ_ROW || column > ODS_MAX_READ_COLUMN {
                    cut_off |= ods_cell_has_content(cell);
                    for frame in cell.children_named("frame") {
                        let anchor = crate::address::format(row.min(ODS_MAX_READ_ROW), column.min(ODS_MAX_READ_COLUMN));
                        frames.push((anchor, frame));
                    }
                    column = column.saturating_add(repeat);
                    continue;
                }
                let value_type = cell.attr_any_ns("value-type").unwrap_or("string").to_string();
                let formula = cell.attr_any_ns("formula").map(odf_formula_to_ours);
                let value = match value_type.as_str() {
                    "float" | "currency" | "percentage" => cell
                        .attr_any_ns("value")
                        .and_then(|value| value.parse::<f64>().ok())
                        .map(CellValue::Number)
                        .unwrap_or(CellValue::Empty),
                    "boolean" => cell
                        .attr_any_ns("boolean-value")
                        .map(|value| CellValue::Bool(value == "true"))
                        .unwrap_or(CellValue::Empty),
                    "date" => cell
                        .attr_any_ns("date-value")
                        .map(|value| CellValue::Text(value.to_string()))
                        .unwrap_or(CellValue::Empty),
                    _ => {
                        // Only the cell's own paragraphs: a note's text lives in
                        // `office:annotation`, which is not part of the value.
                        let text =
                            cell.children_of("p").iter().map(|node| node.deep_text()).collect::<Vec<_>>().join("\n");
                        if text.is_empty() {
                            CellValue::Empty
                        } else if cell.attr("calcext:value-type") == Some("error")
                            || (formula.is_some() && text.starts_with('#'))
                        {
                            CellValue::Error(text)
                        } else {
                            CellValue::Text(text)
                        }
                    }
                };
                let note = cell.child("annotation").and_then(read_ods_annotation);
                let link = read_ods_cell_link(cell, &mut dropped_links);
                if !matches!(value, CellValue::Empty) || formula.is_some() || note.is_some() || link.is_some() {
                    last_row = last_row.max(row);
                    for offset in 0..repeat.min(64) {
                        let address = crate::address::format(row, column + offset);
                        let mut model = Cell { value: value.clone(), formula: formula.clone(), ..Default::default() };
                        if let Some((target, tooltip)) = &link {
                            model.link = Some(target.clone());
                            model.link_tooltip = tooltip.clone();
                        }
                        // Only the first cell of a repeated run keeps the note.
                        if let (Some((text, author, visible)), 0) = (&note, offset) {
                            model.comment = Some(text.clone());
                            model.comment_author = author.clone();
                            model.comment_visible = *visible;
                        }
                        sheet.set(&address, model);
                    }
                }
                for frame in cell.children_named("frame") {
                    last_row = last_row.max(row);
                    frames.push((crate::address::format(row, column), frame));
                }
                column = column.saturating_add(repeat);
            }
            row = row.saturating_add(repeat_rows.max(1));
        }
        for shapes in table.children_named("shapes") {
            frames.extend(shapes.children_named("frame").map(|frame| (ods_shape_anchor(frame), frame)));
        }
        for formats in table.children_named("conditional-formats") {
            unsupported_formats += read_ods_conditional_formats(formats, &mut sheet, &looks);
        }
        for (anchor, frame) in frames {
            if sheet.charts.len() >= ODS_MAX_CHARTS_PER_SHEET {
                warnings.push(format!(
                    "Sheet \"{name}\" has more than {ODS_MAX_CHARTS_PER_SHEET} charts; the rest were skipped."
                ));
                break;
            }
            if let Some(placement) = read_ods_chart(&reader, frame, anchor, &sheet, &mut seen_objects, &mut warnings) {
                sheet.charts.push(placement);
            }
        }
        sheet.row_count = (last_row.min(ODS_MAX_READ_ROW) + 51).max(200);
        sheet.col_count = 26;
        // A hidden row or column is size 0 in the sparse tables. Only the
        // sheet's own grid is recorded: LibreOffice hides the unused rest of a
        // sheet with one run a million rows long.
        for (first, count) in hidden_row_runs {
            for hidden in first..first.saturating_add(count).min(sheet.row_count) {
                sheet.row_heights.insert(hidden, 0.0);
            }
        }
        let content_columns = sheet
            .cells
            .keys()
            .filter_map(|address| crate::address::parse(address).map(|(_, column)| column + 1))
            .max()
            .unwrap_or(0);
        let grid_columns = sheet.col_count.max(content_columns).min(ODS_MAX_READ_COLUMN + 1);
        for (first, count) in ods_hidden_column_runs(table) {
            for hidden in first..first.saturating_add(count).min(grid_columns) {
                sheet.col_widths.insert(hidden, 0.0);
            }
        }
        if cut_off {
            warnings.push(crate::xlsx::import_limit_warning(&name, ODS_MAX_READ_ROW, ODS_MAX_READ_COLUMN));
        }
        workbook.sheets.push(sheet);
    }
    if workbook.sheets.is_empty() {
        workbook.sheets.push(Sheet::new("Sheet1"));
    }
    if unsupported_formats > 0 {
        warnings.push(format!(
            "{unsupported_formats} conditional formatting rule(s) use types this editor cannot import and were dropped."
        ));
    }
    if dropped_links > 0 {
        warnings.push(format!(
            "{dropped_links} hyperlink(s) were dropped: only http, https, mailto and internal references are allowed."
        ));
    }
    warnings.push("Cell formatting from ODS files is imported with limited support.".into());
    Ok(SheetRead { workbook, warnings })
}

pub fn read_ods_file(path: &Path) -> OfficeResult<SheetRead> {
    let bytes = crate::io::read_bytes(path)?;
    let mut result = read_ods(&bytes)?;
    result.workbook.title = crate::io::file_stem(path);
    Ok(result)
}
