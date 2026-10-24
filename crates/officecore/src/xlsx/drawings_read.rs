//! Import of drawing parts: cell anchors, charts and sheet pictures.

use super::*;

// ---------------------------------------------------------------------------
// Import: drawings (charts and pictures)
// ---------------------------------------------------------------------------

/// First descendant with a matching local name. `XmlNode::find_all` also
/// matches the local name, which is what chart/drawing XML needs.
pub(super) fn descendant<'a>(node: &'a XmlNode, name: &str) -> Option<&'a XmlNode> {
    let mut found: Vec<&XmlNode> = Vec::new();
    node.find_all(name, &mut found);
    found.into_iter().next()
}

/// The text of a `c:title`/`c:tx` block: rich text, a cached string reference
/// or a literal `c:v`, whichever the writer used.
fn chart_text_block(node: &XmlNode) -> String {
    if let Some(value) = node.child("v") {
        return value.deep_text();
    }
    if let Some(reference) = node.child("strRef") {
        if let Some(value) = reference.child("v") {
            return value.deep_text();
        }
        if let Some(value) = descendant(reference, "strCache")
            .and_then(|cache| cache.children_of("pt").first().and_then(|point| point.child("v")))
            .map(XmlNode::deep_text)
        {
            if !value.is_empty() {
                return value;
            }
        }
        return reference.child("f").map(XmlNode::deep_text).unwrap_or_default();
    }
    if let Some(rich) = node.child("rich") {
        let mut out = String::new();
        for paragraph in rich.children_of("p") {
            if let Some(value) = paragraph.child("t") {
                out.push_str(&value.deep_text());
            }
            for run in paragraph.children_of("r") {
                if let Some(value) = run.child("t") {
                    out.push_str(&value.deep_text());
                }
            }
        }
        if !out.is_empty() {
            return out;
        }
    }
    node.deep_text()
}

/// `'My Sheet'!$A$1:$B$5` to the model's relative `A1:B5`.
fn relative_ref(formula: &str) -> String {
    let text = formula.trim();
    let text = match text.rfind('!') {
        Some(index) => &text[index + 1..],
        None => text,
    };
    text.trim().replace('$', "")
}

/// Cached `c:strCache`/`c:numCache` points as text, ordered by `c:pt/@idx`.
fn cache_text_values(reference: &XmlNode) -> Vec<String> {
    let Some(cache) = descendant(reference, "strCache").or_else(|| descendant(reference, "numCache")) else {
        return Vec::new();
    };
    let mut points: Vec<(u32, String)> = Vec::new();
    for point in cache.children_of("pt") {
        let index = parse_u32_attr(point, "idx").unwrap_or(points.len() as u32);
        let value = point.child("v").map(XmlNode::deep_text).unwrap_or_default();
        points.push((index, value));
    }
    points.sort_by_key(|(index, _)| *index);
    points.into_iter().map(|(_, value)| value).take(MAX_CHART_CACHE_POINTS).collect()
}

/// Cached `c:numCache` points as numbers, ordered by `c:pt/@idx`.
fn cache_number_values(reference: &XmlNode) -> Vec<f64> {
    let Some(cache) = descendant(reference, "numCache") else { return Vec::new() };
    let mut points: Vec<(u32, f64)> = Vec::new();
    for point in cache.children_of("pt") {
        let index = parse_u32_attr(point, "idx").unwrap_or(points.len() as u32);
        let value = point.child("v").and_then(|value| value.deep_text().trim().parse::<f64>().ok());
        if let Some(value) = value {
            points.push((index, value));
        }
    }
    points.sort_by_key(|(index, _)| *index);
    points.into_iter().map(|(_, value)| value).take(MAX_CHART_CACHE_POINTS).collect()
}

/// Reads one ChartML part back into the model. This is the inverse of
/// [`chart_xml`]: the exporter writes one cache-free `c:ser` per series with a
/// literal `c:tx`, so the round trip through our own files is exact.
fn read_xlsx_chart(xml: &str, warnings: &mut Vec<String>) -> Option<ChartData> {
    let root = parse_xml(xml).ok()?;
    let plot = descendant(&root, "plotArea")?;
    let plot_child = plot.children.iter().find(|child| child.local_name().ends_with("Chart"))?;
    let kind = match plot_child.local_name() {
        "barChart" => {
            if descendant(plot_child, "barDir").and_then(|node| node.attr("val")) == Some("bar") {
                "bar".to_string()
            } else {
                "column".to_string()
            }
        }
        "lineChart" => "line".to_string(),
        "pieChart" => "pie".to_string(),
        "areaChart" => "area".to_string(),
        "scatterChart" => "scatter".to_string(),
        "doughnutChart" => "doughnut".to_string(),
        other => {
            let raw = other.trim_end_matches("Chart").to_ascii_lowercase();
            if !raw.is_empty() {
                warnings.push(format!(
                    "A \"{raw}\" chart was imported with limited support; it is kept in the native .oswk file."
                ));
            }
            raw
        }
    };
    let chart_node = root.child("chart");
    let title = chart_node.and_then(|node| node.child("title")).map(chart_text_block).unwrap_or_default();
    let legend_node = chart_node.and_then(|node| node.child("legend"));
    let legend = legend_node.is_some();
    // The model stores only whether a legend exists; a foreign position cannot
    // be kept, and the exporter always writes a bottom legend.
    let legend_position = legend_node
        .and_then(|node| descendant(node, "legendPos"))
        .and_then(|node| node.attr("val"))
        .map(str::to_string);
    if let Some(position) = legend_position.as_deref().filter(|position| !position.is_empty() && *position != "b") {
        warnings.push(format!(
            "An imported chart has its legend at \"{position}\"; the editor renders charts with a bottom legend."
        ));
    }
    let scatter = kind == "scatter";
    let mut series = Vec::new();
    let mut categories = String::new();
    let mut categories_cache = Vec::new();
    let mut series_values_cache = Vec::new();
    for ser in plot_child.children_named("ser") {
        let name = ser.child("tx").map(chart_text_block).unwrap_or_default();
        // A scatter series carries its X values in `xVal` and its Y values in
        // `yVal`; every other kind uses `cat` and `val`.
        let (value_node, category_node) =
            if scatter { (ser.child("yVal"), ser.child("xVal")) } else { (ser.child("val"), ser.child("cat")) };
        let range = value_node
            .and_then(|val| descendant(val, "f"))
            .map(XmlNode::deep_text)
            .map(|value| relative_ref(&value))
            .unwrap_or_default();
        // A markers-only scatter series puts its colour on the marker, not on
        // the (hidden) line.
        let colored = |props: Option<&XmlNode>| {
            props
                .and_then(|props| descendant(props, "srgbClr"))
                .and_then(|color| color.attr("val"))
                .map(|value| format!("#{}", value.trim_start_matches('#').to_ascii_uppercase()))
        };
        let color = if scatter {
            colored(ser.child("spPr")).or_else(|| colored(ser.child("marker").and_then(|marker| marker.child("spPr"))))
        } else {
            colored(descendant(ser, "spPr"))
        };
        if let Some(cat) = category_node {
            let reference =
                descendant(cat, "f").map(XmlNode::deep_text).map(|value| relative_ref(&value)).unwrap_or_default();
            if categories.is_empty() {
                categories = reference;
                categories_cache = cache_text_values(cat);
            } else if scatter && !reference.is_empty() && reference != categories {
                // The model has one X range per chart; a series with its own X
                // values keeps the first range and reports the difference.
                warnings.push(format!(
                    "Scatter series \"{name}\" has its own X values ({reference}); the chart uses {categories} for every series."
                ));
            }
        }
        series_values_cache.push(value_node.map(cache_number_values).unwrap_or_default());
        series.push(ChartSeries { name, range, color });
    }
    let stacked = descendant(plot_child, "grouping")
        .and_then(|node| node.attr("val"))
        .map(|value| value == "stacked")
        .unwrap_or(false);
    // Empty caches are dropped so an export/import cycle of cache-free charts
    // stays byte-identical; a chart with at least one cache keeps the aligned
    // per-series vectors the model documents.
    if series_values_cache.iter().all(Vec::is_empty) {
        series_values_cache.clear();
    }
    let show_labels = descendant(&root, "dLbls")
        .and_then(|labels| descendant(labels, "showVal"))
        .and_then(|node| node.attr("val"))
        .map(|value| value == "1")
        .unwrap_or(false);
    let axis_title =
        |axis: Option<&XmlNode>| axis.and_then(|axis| axis.child("title")).map(chart_text_block).unwrap_or_default();
    // A scatter chart has two value axes: the one along the bottom (or top) is
    // X, the other is Y.
    let (x_title, y_title) = if scatter {
        let axes = plot.children_of("valAx");
        let horizontal =
            |axis: &XmlNode| matches!(axis.child("axPos").and_then(|node| node.attr("val")), Some("b") | Some("t"));
        let x_index = axes.iter().position(|axis| horizontal(axis)).unwrap_or(0);
        let y_index = usize::from(x_index == 0);
        (axis_title(axes.get(x_index).copied()), axis_title(axes.get(y_index).copied()))
    } else {
        (axis_title(descendant(plot, "catAx")), axis_title(descendant(plot, "valAx")))
    };
    let hole_size = if kind == "doughnut" {
        descendant(plot_child, "holeSize")
            .and_then(|node| parse_u32_attr(node, "val"))
            .map(|hole| hole.clamp(10, 90))
            .filter(|hole| *hole != DOUGHNUT_DEFAULT_HOLE)
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

/// The scatter flavour of a `c:scatterChart`, in the model's spelling. Excel
/// writes `lineMarker` for plain scatter too, so the look comes from what the
/// series draw: a hidden line (`a:ln/a:noFill`) or a `none` marker symbol.
/// Markers only is the default and reads as `None`.
fn read_scatter_style(plot_child: &XmlNode) -> Option<String> {
    let declared = plot_child.child("scatterStyle").and_then(|node| node.attr("val")).unwrap_or("lineMarker");
    let series = plot_child.children_named("ser").collect::<Vec<_>>();
    let line_hidden = |ser: &XmlNode| {
        ser.child("spPr")
            .and_then(|props| props.child("ln"))
            .map(|line| line.child("noFill").is_some())
            .unwrap_or(false)
    };
    let marker_hidden = |ser: &XmlNode| {
        ser.child("marker").and_then(|marker| marker.child("symbol")).and_then(|symbol| symbol.attr("val"))
            == Some("none")
    };
    // `c:smooth` without a value means on (CT_Boolean defaults to true).
    let smooth = declared.starts_with("smooth")
        || series
            .iter()
            .filter_map(|ser| ser.child("smooth"))
            .any(|node| node.attr("val").map(|value| matches!(value.trim(), "1" | "true")).unwrap_or(true));
    let lines = !series.iter().all(|ser| line_hidden(ser)) && !matches!(declared, "marker" | "none");
    let markers =
        !(!series.is_empty() && series.iter().all(|ser| marker_hidden(ser))) && !matches!(declared, "line" | "smooth");
    match (lines, markers, smooth) {
        (false, ..) => None,
        (true, true, false) => Some("lineMarker".into()),
        (true, false, false) => Some("line".into()),
        (true, true, true) => Some("smoothMarker".into()),
        (true, false, true) => Some("smooth".into()),
    }
}

/// The cell anchor of a `oneCellAnchor`/`twoCellAnchor`, in the model's terms.
struct DrawingAnchor {
    address: String,
    col_off_emu: i64,
    row_off_emu: i64,
    to_address: Option<String>,
    to_col_off_emu: i64,
    to_row_off_emu: i64,
    width_px: f64,
    height_px: f64,
}

/// 96 dpi pixels per EMU, the same factor the exporter uses.
const EMU_PER_PX: f64 = 9525.0;

fn parse_drawing_anchor(anchor: &XmlNode) -> Option<DrawingAnchor> {
    let from = anchor.child("from")?;
    let child_number = |node: &XmlNode, name: &str| -> Option<u32> {
        node.child(name).map(XmlNode::deep_text).and_then(|value| value.trim().parse::<u32>().ok())
    };
    let column = child_number(from, "col")?;
    let row = child_number(from, "row")?;
    if row > MAX_IMPORT_ROWS || column > MAX_IMPORT_COLS {
        return None;
    }
    let offset = |node: Option<&XmlNode>, name: &str| -> i64 {
        node.and_then(|node| node.child(name))
            .map(XmlNode::deep_text)
            .and_then(|value| value.trim().parse::<i64>().ok())
            .unwrap_or(0)
    };
    let col_off_emu = offset(Some(from), "colOff");
    let row_off_emu = offset(Some(from), "rowOff");
    let to = anchor.child("to");
    let to_address = to.and_then(|to| {
        let to_column = child_number(to, "col")?;
        let to_row = child_number(to, "row")?;
        Some(crate::address::format(to_row, to_column))
    });
    let to_col_off_emu = offset(to, "colOff");
    let to_row_off_emu = offset(to, "rowOff");
    // A one-cell anchor carries `ext`; a two-cell anchor's extent is derived
    // from its corners using Excel's default column (64 px) and row (20 px).
    let (width_px, height_px) = if let Some(ext) = anchor.child("ext") {
        let cx = ext.attr("cx").and_then(|value| value.trim().parse::<f64>().ok()).unwrap_or(0.0);
        let cy = ext.attr("cy").and_then(|value| value.trim().parse::<f64>().ok()).unwrap_or(0.0);
        (cx / EMU_PER_PX, cy / EMU_PER_PX)
    } else {
        let to = to?;
        let to_column = child_number(to, "col").unwrap_or(column);
        let to_row = child_number(to, "row").unwrap_or(row);
        let width = to_column.saturating_sub(column) as f64 * 64.0 + (to_col_off_emu - col_off_emu) as f64 / EMU_PER_PX;
        let height = to_row.saturating_sub(row) as f64 * 20.0 + (to_row_off_emu - row_off_emu) as f64 / EMU_PER_PX;
        (width, height)
    };
    if !width_px.is_finite() || !height_px.is_finite() || width_px <= 0.0 || height_px <= 0.0 {
        return None;
    }
    Some(DrawingAnchor {
        address: crate::address::format(row, column),
        col_off_emu,
        row_off_emu,
        to_address,
        to_col_off_emu,
        to_row_off_emu,
        width_px,
        height_px,
    })
}

/// Reads the sheet's drawing part and maps charts and pictures onto the model.
///
/// A drawing is optional: a sheet without one simply keeps no floating
/// objects. Every failure is a warning; the calamine values are never touched.
pub(super) fn apply_drawings(
    zip: &crate::zip::ZipReader,
    part: &str,
    root: &XmlNode,
    sheet: &mut Sheet,
    warnings: &mut Vec<String>,
) {
    let Some(drawing) = root.child("drawing") else { return };
    let Some(relationship_id) = drawing.attr_any_ns("id") else { return };
    let relationships = read_relationships(zip, part);
    let Some(relationship) =
        relationships.get(relationship_id).filter(|relationship| relationship.kind.ends_with("/drawing"))
    else {
        return;
    };
    let target = resolve_part(part, &relationship.target);
    let text = match zip.read_text(&target) {
        Ok(text) => text,
        Err(error) => {
            warnings.push(format!("The drawing of sheet \"{}\" could not be read ({error}).", sheet.name));
            return;
        }
    };
    if text.len() > MAX_DETAIL_PART_BYTES {
        warnings.push(format!(
            "The drawing of sheet \"{}\" is too large to inspect; charts and images were skipped.",
            sheet.name
        ));
        return;
    }
    let parsed = match parse_xml(&text) {
        Ok(parsed) => parsed,
        Err(_) => {
            warnings.push(format!(
                "The drawing of sheet \"{}\" could not be parsed; charts and images were skipped.",
                sheet.name
            ));
            return;
        }
    };
    let drawing_relationships = read_relationships(zip, &target);
    for anchor in parsed.children.iter().filter(|child| matches!(child.local_name(), "oneCellAnchor" | "twoCellAnchor"))
    {
        let Some(geometry) = parse_drawing_anchor(anchor) else { continue };
        // Charts: `xdr:graphicFrame` wrapping a `c:chart` relationship.
        if let Some(frame) = anchor.child("graphicFrame") {
            let Some(chart_ref) = descendant(frame, "chart") else {
                warnings.push(format!(
                    "A non-chart graphic frame in the drawing of sheet \"{}\" was not imported.",
                    sheet.name
                ));
                continue;
            };
            let Some(embed) = chart_ref.attr_any_ns("id") else {
                warnings.push(format!("A chart on sheet \"{}\" has no relationship and was skipped.", sheet.name));
                continue;
            };
            let Some(relationship) =
                drawing_relationships.get(embed).filter(|relationship| relationship.kind.ends_with("/chart"))
            else {
                warnings.push(format!(
                    "A chart relationship on sheet \"{}\" could not be resolved and was skipped.",
                    sheet.name
                ));
                continue;
            };
            let chart_part = resolve_part(&target, &relationship.target);
            let chart_text = match zip.read_text(&chart_part) {
                Ok(text) => text,
                Err(_) => {
                    warnings.push(format!("Chart part {chart_part} could not be read and was skipped."));
                    continue;
                }
            };
            if chart_text.len() > MAX_DETAIL_PART_BYTES {
                warnings.push(format!("Chart part {chart_part} is too large to inspect and was skipped."));
                continue;
            }
            match read_xlsx_chart(&chart_text, warnings) {
                Some(chart) => sheet.charts.push(ChartPlacement {
                    id: format!("chart{}", sheet.charts.len() + 1),
                    chart,
                    anchor: geometry.address,
                    width_px: geometry.width_px,
                    height_px: geometry.height_px,
                }),
                None => warnings.push(format!("Chart part {chart_part} could not be parsed and was skipped.")),
            }
            continue;
        }
        // Pictures: `xdr:pic` with an `a:blip/@r:embed` relationship.
        let Some(picture) = anchor.child("pic") else { continue };
        let Some(embed) = descendant(picture, "blip").and_then(|blip| blip.attr_any_ns("embed")) else {
            warnings.push(format!("An image on sheet \"{}\" has no relationship and was skipped.", sheet.name));
            continue;
        };
        let Some(relationship) =
            drawing_relationships.get(embed).filter(|relationship| relationship.kind.ends_with("/image"))
        else {
            warnings.push(format!(
                "An image relationship on sheet \"{}\" could not be resolved and was skipped.",
                sheet.name
            ));
            continue;
        };
        let media_part = resolve_part(&target, &relationship.target);
        let bytes = match zip.read(&media_part) {
            Ok(bytes) if !bytes.is_empty() => bytes,
            _ => {
                warnings.push(format!("Image part {media_part} could not be read and was skipped."));
                continue;
            }
        };
        let name = media_part.rsplit('/').next().unwrap_or("image.png").to_string();
        let mut image = ImageData::from_bytes(&name, &bytes);
        image.alt = descendant(picture, "cNvPr").and_then(|props| props.attr("descr")).unwrap_or("").to_string();
        if descendant(picture, "srcRect").is_some() {
            warnings.push(format!(
                "A cropped image on sheet \"{}\" was imported without its crop; the full picture is shown.",
                sheet.name
            ));
        }
        let rotation_deg = picture
            .child("spPr")
            .and_then(|props| props.child("xfrm"))
            .and_then(|xfrm| xfrm.attr("rot"))
            .and_then(|value| value.trim().parse::<f64>().ok())
            .map(|value| value / 60_000.0)
            .unwrap_or(0.0);
        sheet.images.push(SheetImage {
            image,
            anchor: CellAnchor {
                address: geometry.address,
                col_off_emu: geometry.col_off_emu,
                row_off_emu: geometry.row_off_emu,
                to_address: geometry.to_address,
                to_col_off_emu: geometry.to_col_off_emu,
                to_row_off_emu: geometry.to_row_off_emu,
            },
            width_px: geometry.width_px,
            height_px: geometry.height_px,
            rotation_deg,
        });
    }
    let absolute = parsed.children_of("absoluteAnchor").len();
    if absolute > 0 {
        warnings.push(format!(
            "{absolute} absolutely anchored drawing(s) on sheet \"{}\" have no cell anchor and were not imported.",
            sheet.name
        ));
    }
    let shapes = parsed.children.iter().filter(|child| matches!(child.local_name(), "sp" | "grpSp")).count();
    if shapes > 0 {
        warnings.push(format!(
            "{shapes} shape(s) in the drawing of sheet \"{}\" were not imported; the sheet model has no floating shapes.",
            sheet.name
        ));
    }
}
