//! PPTX reader: relationships, shapes, text, charts, tables, placeholders, animations and warnings.
//! The writer lives in `writer`; the public entry points are re-exported by `mod.rs`.

use super::*;

// ---------------------------------------------------------------------------
// Import
// ---------------------------------------------------------------------------

fn part_rels_typed(reader: &ZipReader, part: &str) -> Vec<(String, String, String)> {
    let (dir, file) = match part.rsplit_once('/') {
        Some((dir, file)) => (format!("{dir}/"), file.to_string()),
        None => (String::new(), part.to_string()),
    };
    let mut entries = Vec::new();
    let Ok(text) = reader.read_text(&format!("{dir}_rels/{file}.rels")) else {
        return entries;
    };
    let Ok(root) = parse_xml(&text) else { return entries };
    for node in root.children_named("Relationship") {
        if let (Some(id), Some(kind), Some(target)) = (node.attr("Id"), node.attr("Type"), node.attr("Target")) {
            entries.push((id.to_string(), kind.to_string(), target.to_string()));
        }
    }
    entries
}

fn part_rels(reader: &ZipReader, part: &str) -> HashMap<String, String> {
    part_rels_typed(reader, part).into_iter().map(|(id, _, target)| (id, target)).collect()
}

fn resolve_part(current: &str, target: &str) -> String {
    if target.starts_with('/') {
        return target.trim_start_matches('/').to_string();
    }
    let base = current.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
    let mut segments: Vec<&str> = if base.is_empty() { Vec::new() } else { base.split('/').collect() };
    for segment in target.split('/') {
        match segment {
            "." | "" => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other),
        }
    }
    segments.join("/")
}

fn map_object_outside(object: &SlideObject, outer: (f64, f64, f64, f64), inner: (f64, f64, f64, f64)) -> SlideObject {
    let (ox, oy, ow, oh) = outer;
    let (cx, cy, cw, ch) = inner;
    let sx = if cw.abs() > 0.01 { ow / cw } else { 1.0 };
    let sy = if ch.abs() > 0.01 { oh / ch } else { 1.0 };
    let mut mapped = object.clone();
    mapped.x = ox + (object.x - cx) * sx;
    mapped.y = oy + (object.y - cy) * sy;
    mapped.w = object.w * sx;
    mapped.h = object.h * sy;
    if let (Some(line), Some(source)) = (mapped.line.as_mut(), object.line.as_ref()) {
        line.x2 = source.x2 * sx;
        line.y2 = source.y2 * sy;
    }
    mapped.children = object.children.iter().map(|child| map_object_outside(child, outer, inner)).collect();
    mapped
}

fn read_group_transform(transform: Option<&XmlNode>) -> Option<(f64, f64, f64, f64)> {
    let transform = transform?;
    let offset = transform.child("chOff")?;
    let extent = transform.child("chExt")?;
    let x = offset.attr("x").and_then(|value| value.parse::<f64>().ok()).map(pt_from_emu)?;
    let y = offset.attr("y").and_then(|value| value.parse::<f64>().ok()).map(pt_from_emu)?;
    let w = extent.attr("cx").and_then(|value| value.parse::<f64>().ok()).map(pt_from_emu)?;
    let h = extent.attr("cy").and_then(|value| value.parse::<f64>().ok()).map(pt_from_emu)?;
    Some((x, y, w, h))
}

fn read_shape_id(node: &XmlNode) -> Option<usize> {
    node.find_descendant("cNvPr").and_then(|props| props.attr("id")).and_then(|value| value.parse::<usize>().ok())
}

/// Reads a `c:strCache` below `parent` (a category/value reference node).
/// Points are placed by their `idx`, so a sparse cache stays aligned instead
/// of shifting values; missing labels become empty strings.
fn read_str_cache(parent: &XmlNode) -> Vec<String> {
    let Some(cache) = parent.find_descendant("strCache") else {
        return Vec::new();
    };
    let mut points = Vec::new();
    cache.find_all("pt", &mut points);
    let mut out: Vec<String> = Vec::new();
    for point in points {
        let index = point.attr("idx").and_then(|value| value.parse::<usize>().ok()).unwrap_or(out.len());
        let value = point.find_descendant("v").map(XmlNode::deep_text).unwrap_or_default();
        while out.len() <= index {
            out.push(String::new());
        }
        out[index] = value;
    }
    out
}

/// Reads a `c:numCache` below `parent` (a series value reference node).
fn read_num_cache(parent: &XmlNode) -> Vec<f64> {
    let Some(cache) = parent.find_descendant("numCache") else {
        return Vec::new();
    };
    let mut points = Vec::new();
    cache.find_all("pt", &mut points);
    let mut out: Vec<f64> = Vec::new();
    for point in points {
        let index = point.attr("idx").and_then(|value| value.parse::<usize>().ok()).unwrap_or(out.len());
        let value = point
            .find_descendant("v")
            .map(XmlNode::deep_text)
            .and_then(|text| text.trim().parse::<f64>().ok())
            .unwrap_or(0.0);
        while out.len() <= index {
            out.push(0.0);
        }
        out[index] = value;
    }
    out
}

fn read_chart_xml(xml: &str, warnings: &mut Vec<String>) -> Option<ChartData> {
    let root = parse_xml(xml).ok()?;
    let plot = root.find_descendant("plotArea")?;
    let plot_child = plot.children.iter().find(|child| child.local_name().ends_with("Chart"))?;
    let kind = match plot_child.local_name() {
        "barChart" => {
            if plot_child.find_descendant("barDir").and_then(|node| node.attr("val")) == Some("bar") {
                "bar".to_string()
            } else {
                "column".to_string()
            }
        }
        "lineChart" => "line".to_string(),
        "pieChart" => "pie".to_string(),
        "areaChart" => "area".to_string(),
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
    let title = chart_node.and_then(|node| node.child("title")).map(|node| node.deep_text()).unwrap_or_default();
    let legend = chart_node.and_then(|node| node.child("legend")).is_some();
    let mut series = Vec::new();
    let mut series_values_cache = Vec::new();
    for ser in plot_child.children_named("ser") {
        let name = ser
            .child("tx")
            .map(|tx| tx.find_descendant("v").map(XmlNode::deep_text).unwrap_or_else(|| tx.deep_text()))
            .unwrap_or_default();
        let range =
            ser.child("val").and_then(|val| val.find_descendant("f")).map(XmlNode::deep_text).unwrap_or_default();
        let color = ser
            .find_descendant("spPr")
            .and_then(|props| props.find_descendant("srgbClr"))
            .and_then(|color| color.attr("val"))
            .map(|value| format!("#{value}"));
        series_values_cache.push(ser.child("val").map(read_num_cache).unwrap_or_default());
        series.push(ChartSeries { name, range, color });
    }
    let categories = plot_child
        .children_named("ser")
        .next()
        .and_then(|ser| ser.child("cat"))
        .and_then(|cat| cat.find_descendant("f"))
        .map(XmlNode::deep_text)
        .unwrap_or_default();
    // The caches are what makes a deck render with values even when the source
    // workbook is gone; empty vectors mean the chart only carries ranges.
    let categories_cache = plot_child
        .children_named("ser")
        .next()
        .and_then(|ser| ser.child("cat"))
        .map(read_str_cache)
        .unwrap_or_default();
    let stacked = plot_child
        .find_descendant("grouping")
        .and_then(|node| node.attr("val"))
        .map(|value| value == "stacked")
        .unwrap_or(false);
    let show_labels = root
        .find_descendant("dLbls")
        .and_then(|labels| labels.find_descendant("showVal"))
        .and_then(|node| node.attr("val"))
        .map(|value| value == "1")
        .unwrap_or(false);
    let x_title = plot
        .find_descendant("catAx")
        .and_then(|axis| axis.find_descendant("title"))
        .map(|node| node.deep_text())
        .unwrap_or_default();
    let y_title = plot
        .find_descendant("valAx")
        .and_then(|axis| axis.find_descendant("title"))
        .map(|node| node.deep_text())
        .unwrap_or_default();
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
        ..Default::default()
    })
}

fn read_shape(
    node: &XmlNode,
    reader: &ZipReader,
    rels: &HashMap<String, String>,
    base: &str,
    z: i32,
    warnings: &mut Vec<String>,
) -> Option<SlideObject> {
    let mut object = match node.local_name() {
        "sp" => {
            let shape_props = node.find_descendant("spPr")?;
            let transform = shape_props.find_descendant("xfrm");
            let (x, y, w, h, rotation) = read_transform(transform);
            let preset = shape_props
                .find_descendant("prstGeom")
                .and_then(|geometry| geometry.attr("prst"))
                .unwrap_or("rect")
                .to_string();
            let mut object = SlideObject::new(
                match preset.as_str() {
                    "ellipse" => "ellipse",
                    "roundRect" => "roundRect",
                    "line" => "line",
                    _ => "rect",
                },
                x,
                y,
                w,
                h,
            );
            object.z = z;
            object.rotation = rotation;
            object.placeholder = node
                .child("nvSpPr")
                .and_then(|props| props.child("nvPr"))
                .and_then(|props| props.child("ph"))
                .and_then(|placeholder| placeholder.attr("type"))
                .and_then(placeholder_role)
                .map(str::to_string);
            let fill = shape_props
                .find_descendant("solidFill")
                .and_then(|fill| fill.find_descendant("srgbClr"))
                .and_then(|color| color.attr("val"))
                .map(|value| format!("#{value}"));
            let stroke = shape_props
                .find_descendant("ln")
                .and_then(|line| line.find_descendant("srgbClr"))
                .and_then(|color| color.attr("val"))
                .map(|value| format!("#{value}"));
            if fill.is_some() || stroke.is_some() {
                object.style = Some(ShapeStyle {
                    fill,
                    stroke,
                    stroke_width_pt: 1.5,
                    opacity: 1.0,
                    corner_radius_pt: 0.0,
                    shadow: false,
                });
            }
            let text_body = node.find_descendant("txBody");
            if let Some(text_body) = text_body {
                let paragraphs = read_paragraphs(text_body);
                if paragraphs.iter().any(|paragraph| !paragraph.text.trim().is_empty()) {
                    object.text = Some(TextFrame { paragraphs, ..Default::default() });
                }
            }
            object
        }
        "pic" => {
            let shape_props = node.find_descendant("spPr")?;
            let transform = shape_props.find_descendant("xfrm");
            let (x, y, w, h, rotation) = read_transform(transform);
            let Some(embed) = node.find_descendant("blip").and_then(|blip| blip.attr_any_ns("embed")) else {
                warnings.push("An image without a relationship was skipped.".into());
                return None;
            };
            let Some(target) = rels.get(embed) else {
                warnings.push("An image relationship could not be resolved and was skipped.".into());
                return None;
            };
            let part = resolve_part(base, target);
            let data = match reader.read(&part) {
                Ok(data) => data,
                Err(_) => {
                    warnings.push(format!("Image part {part} could not be read and was skipped."));
                    return None;
                }
            };
            if data.is_empty() {
                return None;
            }
            let name = part.rsplit('/').next().unwrap_or("image.png").to_string();
            let mut object = SlideObject::new("image", x, y, w, h);
            object.z = z;
            object.rotation = rotation;
            let mut image = ImageData::from_bytes(&name, &data);
            if let Some(rect) = node.find_descendant("srcRect") {
                let fraction =
                    |side: &str| rect.attr(side).and_then(|value| value.parse::<f64>().ok()).unwrap_or(0.0) / 100000.0;
                let crop = ImageCrop {
                    left: fraction("l").clamp(0.0, 0.95),
                    top: fraction("t").clamp(0.0, 0.95),
                    right: fraction("r").clamp(0.0, 0.95),
                    bottom: fraction("b").clamp(0.0, 0.95),
                };
                if crop.left > 0.0 || crop.top > 0.0 || crop.right > 0.0 || crop.bottom > 0.0 {
                    image.crop = Some(crop);
                }
            }
            object.image = Some(image);
            object
        }
        "cxnSp" => {
            let shape_props = node.find_descendant("spPr")?;
            let transform = shape_props.find_descendant("xfrm");
            let (x, y, w, h, _) = read_transform(transform);
            let mut object = SlideObject::new("line", x, y, w, h);
            object.z = z;
            // An endpoint's `id` is the OOXML shape id of the object it is
            // glued to; `read_shape` gives every imported shape the
            // deterministic model id `shape{id}`, so the reference maps back
            // one-to-one without a second pass over the slide.
            let begin = node.find_descendant("stCxn");
            let end = node.find_descendant("endCxn");
            object.line = Some(LineSpec {
                x2: w,
                y2: h,
                end_arrow: node.find_descendant("tailEnd").is_some(),
                begin_arrow: node.find_descendant("headEnd").is_some(),
                dash: String::new(),
                begin_object: begin.and_then(|cxn| cxn.attr("id")).map(|id| format!("shape{id}")),
                end_object: end.and_then(|cxn| cxn.attr("id")).map(|id| format!("shape{id}")),
                begin_site: begin
                    .and_then(|cxn| cxn.attr("idx"))
                    .and_then(|value| value.parse::<u32>().ok())
                    .unwrap_or(0),
                end_site: end.and_then(|cxn| cxn.attr("idx")).and_then(|value| value.parse::<u32>().ok()).unwrap_or(0),
            });
            let color = node
                .find_descendant("ln")
                .and_then(|line| line.find_descendant("srgbClr"))
                .and_then(|color| color.attr("val"))
                .map(|value| format!("#{value}"));
            object.style = Some(ShapeStyle {
                fill: color.clone(),
                stroke: color,
                stroke_width_pt: 2.0,
                opacity: 1.0,
                corner_radius_pt: 0.0,
                shadow: false,
            });
            object
        }
        "graphicFrame" => {
            let transform = node.find_descendant("xfrm");
            let (x, y, w, h, rotation) = read_transform(transform);
            if let Some(table) = node.find_descendant("tbl") {
                let mut rows = Vec::new();
                for row_node in table.children_named("tr") {
                    let mut cells = Vec::new();
                    for cell in row_node.children_named("tc") {
                        let inner = vec![Block::paragraph(cell.deep_text().trim())];
                        cells.push(TableCell { blocks: inner, ..Default::default() });
                    }
                    rows.push(TableRow { cells, ..Default::default() });
                }
                let mut object = SlideObject::new("table", x, y, w, h);
                object.z = z;
                object.rotation = rotation;
                object.table = Some(TableData { rows, ..Default::default() });
                object
            } else {
                let chart_ref = node.find_descendant("chart")?;
                let Some(embed) = chart_ref.attr_any_ns("id") else {
                    warnings.push("A chart without a relationship was skipped.".into());
                    return None;
                };
                let Some(target) = rels.get(embed) else {
                    warnings.push("A chart relationship could not be resolved and was skipped.".into());
                    return None;
                };
                let part = resolve_part(base, target);
                let text = match reader.read_text(&part) {
                    Ok(text) => text,
                    Err(_) => {
                        warnings.push(format!("Chart part {part} could not be read and was skipped."));
                        return None;
                    }
                };
                let chart = match read_chart_xml(&text, warnings) {
                    Some(chart) => chart,
                    None => {
                        warnings.push(format!("Chart part {part} could not be parsed and was skipped."));
                        return None;
                    }
                };
                let mut object = SlideObject::new("chart", x, y, w, h);
                object.z = z;
                object.rotation = rotation;
                object.chart = Some(chart);
                object
            }
        }
        "grpSp" => {
            let group_props = node.child("grpSpPr");
            let transform = group_props.and_then(|props| props.child("xfrm"));
            let (x, y, w, h, rotation) = read_transform(transform);
            let mut object = SlideObject::new("group", x, y, w, h);
            object.z = z;
            object.rotation = rotation;
            let child_transform = read_group_transform(transform);
            let mut child_z = 1i32;
            for child in node
                .children
                .iter()
                .filter(|child| child.local_name() != "nvGrpSpPr" && child.local_name() != "grpSpPr")
            {
                if let Some(child_object) = read_shape(child, reader, rels, base, child_z, warnings) {
                    let mapped = match child_transform {
                        Some(inner) => map_object_outside(&child_object, (x, y, w, h), inner),
                        None => child_object,
                    };
                    object.children.push(mapped);
                    child_z += 1;
                } else if !matches!(child.local_name(), "extLst" | "contentPart") {
                    warnings.push("A shape inside a group could not be imported.".into());
                }
            }
            if let Some(props) = group_props {
                if props.find_descendant("effectLst").map(|effects| !effects.children.is_empty()).unwrap_or(false) {
                    warnings.push("Effects on grouped shapes were not imported.".into());
                }
            }
            object
        }
        _ => return None,
    };
    if let Some(shape_id) = read_shape_id(node) {
        object.id = format!("shape{shape_id}");
    }
    Some(object)
}

fn read_transform(transform: Option<&XmlNode>) -> (f64, f64, f64, f64, f64) {
    let Some(transform) = transform else { return (0.0, 0.0, 300.0, 120.0, 0.0) };
    let offset = transform.child("off");
    let extent = transform.child("ext");
    let x = offset
        .and_then(|node| node.attr("x"))
        .and_then(|value| value.parse::<f64>().ok())
        .map(pt_from_emu)
        .unwrap_or(0.0);
    let y = offset
        .and_then(|node| node.attr("y"))
        .and_then(|value| value.parse::<f64>().ok())
        .map(pt_from_emu)
        .unwrap_or(0.0);
    let w = extent
        .and_then(|node| node.attr("cx"))
        .and_then(|value| value.parse::<f64>().ok())
        .map(pt_from_emu)
        .unwrap_or(120.0);
    let h = extent
        .and_then(|node| node.attr("cy"))
        .and_then(|value| value.parse::<f64>().ok())
        .map(pt_from_emu)
        .unwrap_or(60.0);
    let rotation =
        transform.attr("rot").and_then(|value| value.parse::<f64>().ok()).map(|value| value / 60000.0).unwrap_or(0.0);
    (x, y, w, h, rotation)
}

/// A declared language tag, ignoring the "no proofing" placeholders.
fn known_lang(tag: &str) -> Option<String> {
    let tag = tag.trim();
    if tag.is_empty() || tag.eq_ignore_ascii_case("x-none") || tag.eq_ignore_ascii_case("zxx") {
        None
    } else {
        Some(tag.to_string())
    }
}

/// Reads a run toggle that OOXML stores either as an `a:rPr` attribute
/// (`u="sng"`, `strike="sngStrike"`) or, in some producers, as an element;
/// `present` is the value to assume for a valueless element.
fn run_toggle_value(properties: Option<&XmlNode>, attribute: &str, element: &str, present: &str) -> Option<String> {
    properties.and_then(|node| {
        node.attr(attribute)
            .map(str::to_string)
            .or_else(|| node.child(element).map(|child| child.attr("val").unwrap_or(present).to_string()))
    })
}

fn read_paragraphs(text_body: &XmlNode) -> Vec<TextParagraph> {
    let mut paragraphs = Vec::new();
    for paragraph in text_body.children_named("p") {
        let properties = paragraph.child("pPr");
        let level =
            properties.and_then(|node| node.attr("lvl")).and_then(|value| value.parse::<u32>().ok()).unwrap_or(0);
        let align = properties
            .and_then(|node| node.attr("algn"))
            .map(|value| match value {
                "ctr" => "center".to_string(),
                "r" => "right".to_string(),
                "just" => "justify".to_string(),
                _ => "left".to_string(),
            })
            .unwrap_or_default();
        let bullet = properties
            .map(|node| node.find_descendant("buChar").is_some() || node.find_descendant("buAutoNum").is_some())
            .unwrap_or(false);
        let mut text = String::new();
        let mut runs = Vec::new();
        let mut bold = false;
        let mut italic = false;
        let mut underline = false;
        let mut size: Option<f64> = None;
        let mut color: Option<String> = None;
        for child in &paragraph.children {
            if child.local_name() != "r" && child.local_name() != "fld" {
                continue;
            }
            let run_properties = child.child("rPr");
            let run_bold = run_properties.and_then(|node| node.attr("b")).map(|value| value == "1").unwrap_or(false);
            let run_italic = run_properties.and_then(|node| node.attr("i")).map(|value| value == "1").unwrap_or(false);
            let run_underline = run_toggle_value(run_properties, "u", "u", "sng")
                .map(|value| !value.eq_ignore_ascii_case("none"))
                .unwrap_or(false);
            let run_strike = run_toggle_value(run_properties, "strike", "strike", "sngStrike")
                .map(|value| !value.eq_ignore_ascii_case("noStrike"))
                .unwrap_or(false);
            let run_baseline = run_properties
                .and_then(|node| node.attr("baseline"))
                .and_then(|value| value.parse::<f64>().ok())
                .unwrap_or(0.0);
            let run_size = run_properties
                .and_then(|node| node.attr("sz"))
                .and_then(|value| value.parse::<f64>().ok())
                .map(|value| value / 100.0);
            // A run fill is a direct child; looking only there keeps the
            // highlight color from being mistaken for the text color.
            let run_color = run_properties
                .and_then(|node| node.child("solidFill"))
                .and_then(|fill| fill.find_descendant("srgbClr"))
                .and_then(|color| color.attr("val"))
                .map(|value| format!("#{value}"));
            let run_highlight = run_properties
                .and_then(|node| node.child("highlight"))
                .and_then(|highlight| highlight.find_descendant("srgbClr"))
                .and_then(|color| color.attr("val"))
                .map(|value| format!("#{value}"));
            let run_font = run_properties
                .and_then(|node| node.child("latin"))
                .and_then(|latin| latin.attr("typeface"))
                .map(str::trim)
                .filter(|face| !face.is_empty())
                .map(str::to_string);
            let run_lang = run_properties.and_then(|node| node.attr("lang")).and_then(known_lang);
            bold |= run_bold;
            italic |= run_italic;
            underline |= run_underline;
            if size.is_none() {
                size = run_size;
            }
            if color.is_none() {
                color = run_color.clone();
            }
            let run_text = child.find_descendant("t").map(XmlNode::deep_text).unwrap_or_default();
            if run_text.is_empty() {
                continue;
            }
            text.push_str(&run_text);
            runs.push(Run {
                text: run_text,
                bold: run_bold,
                italic: run_italic,
                underline: run_underline,
                strike: run_strike,
                color: run_color,
                highlight: run_highlight,
                font: run_font,
                size_pt: run_size,
                superscript: run_baseline > 0.0,
                subscript: run_baseline < 0.0,
                lang: run_lang,
                ..Default::default()
            });
        }
        // A paragraph without text still declares its language on endParaRPr.
        let lang = runs
            .iter()
            .find_map(|run| run.lang.clone())
            .or_else(|| paragraph.child("endParaRPr").and_then(|node| node.attr("lang")).and_then(known_lang));
        paragraphs.push(TextParagraph {
            text,
            level,
            bold,
            italic,
            underline,
            size_pt: size,
            color,
            align,
            bullet,
            runs,
            lang,
        });
    }
    paragraphs
}

trait FindDescendant {
    fn find_descendant(&self, name: &str) -> Option<&XmlNode>;
}

impl FindDescendant for XmlNode {
    fn find_descendant<'a>(&'a self, name: &str) -> Option<&'a XmlNode> {
        let mut found: Vec<&XmlNode> = Vec::new();
        self.find_all(name, &mut found);
        found.into_iter().next()
    }
}

fn timing_trigger(node_type: &str) -> &'static str {
    match node_type {
        "withEffect" | "withGroup" => "withPrevious",
        "afterEffect" | "afterGroup" => "afterPrevious",
        _ => "onClick",
    }
}

fn parse_duration(value: &str) -> Option<u32> {
    let hundredths = value.parse::<f64>().ok()?;
    Some((hundredths * 10.0).round().max(0.0) as u32)
}

fn read_timing_par(par: &XmlNode, order: u32, warnings: &mut Vec<String>) -> Option<Animation> {
    let mut effect_nodes = Vec::new();
    par.find_all("animEffect", &mut effect_nodes);
    let mut set_nodes = Vec::new();
    par.find_all("set", &mut set_nodes);
    if effect_nodes.is_empty() && set_nodes.is_empty() {
        return None;
    }
    if effect_nodes.len() > 1 {
        warnings.push("Multiple effects inside one animation node were simplified.".into());
    }
    let mut trigger = None;
    for node in effect_nodes.iter().chain(set_nodes.iter()) {
        if let Some(node_type) = node.find_descendant("cTn").and_then(|ctn| ctn.attr("nodeType")) {
            if matches!(node_type, "clickEffect" | "withEffect" | "afterEffect") {
                trigger = Some(timing_trigger(node_type));
                break;
            }
        }
    }
    let trigger = trigger
        .or_else(|| par.child("cTn").and_then(|ctn| ctn.attr("nodeType")).map(timing_trigger))
        .unwrap_or("onClick");
    let delay = par
        .child("cTn")
        .and_then(|ctn| ctn.child("stCondLst"))
        .and_then(|conditions| conditions.find_descendant("cond"))
        .and_then(|condition| condition.attr("delay"))
        .and_then(parse_duration)
        .unwrap_or(0);
    let mut kind = "entrance";
    let mut effect = "appear".to_string();
    let mut duration = 0u32;
    let mut spid = None;
    if let Some(effect_node) = effect_nodes.first() {
        kind = match effect_node.attr("transition") {
            Some("in") => "entrance",
            Some("out") => "exit",
            _ => "emphasis",
        };
        effect = match effect_node.attr("filter") {
            Some(filter) => filter.to_string(),
            None => {
                let fallback = match kind {
                    "entrance" => "appear",
                    "exit" => "fade",
                    _ => "pulse",
                };
                warnings.push(format!("An animation effect without a filter was imported as \"{fallback}\"."));
                fallback.to_string()
            }
        };
        duration =
            effect_node.find_descendant("cTn").and_then(|ctn| ctn.attr("dur")).and_then(parse_duration).unwrap_or(0);
        spid = effect_node
            .find_descendant("spTgt")
            .and_then(|target| target.attr("spid"))
            .and_then(|value| value.parse::<usize>().ok());
    } else if let Some(set_node) = set_nodes.first() {
        let visible = set_node.find_descendant("strVal").and_then(|value| value.attr("val")) == Some("visible");
        kind = if visible { "entrance" } else { "exit" };
        duration =
            set_node.find_descendant("cTn").and_then(|ctn| ctn.attr("dur")).and_then(parse_duration).unwrap_or(0);
        spid = set_node
            .find_descendant("spTgt")
            .and_then(|target| target.attr("spid"))
            .and_then(|value| value.parse::<usize>().ok());
    }
    let Some(spid) = spid else {
        warnings.push("An animation without a target object was skipped.".into());
        return None;
    };
    Some(Animation {
        id: uuid::Uuid::new_v4().to_string(),
        object_id: format!("shape{spid}"),
        kind: kind.to_string(),
        effect,
        trigger: trigger.to_string(),
        duration_ms: duration,
        delay_ms: delay,
        order,
    })
}

fn read_timing(slide_root: &XmlNode, warnings: &mut Vec<String>) -> Vec<Animation> {
    let Some(timing) = slide_root.find_descendant("timing") else { return Vec::new() };
    let mut sequences = Vec::new();
    timing.find_all("seq", &mut sequences);
    let Some(sequence) = sequences.first() else {
        warnings.push("The slide timing could not be imported.".into());
        return Vec::new();
    };
    let main_seq = sequence
        .children_named("cTn")
        .find(|ctn| ctn.attr("nodeType") == Some("mainSeq"))
        .and_then(|ctn| ctn.child("childTnLst"));
    let Some(container) = main_seq else {
        warnings.push("The slide timing could not be imported.".into());
        return Vec::new();
    };
    let mut animations = Vec::new();
    for (index, par) in container.children_named("par").enumerate() {
        if let Some(animation) = read_timing_par(par, index as u32, warnings) {
            animations.push(animation);
        }
    }
    if animations.is_empty() {
        warnings.push("Some animation effects could not be imported.".into());
    }
    animations
}

fn theme_key(name: &str) -> Option<&'static str> {
    match name.to_ascii_lowercase().as_str() {
        "business" => Some("business"),
        "dark" => Some("dark"),
        "modern" => Some("modern"),
        "education" => Some("education"),
        "simple" => Some("simple"),
        "minimal" => Some("minimal"),
        _ => None,
    }
}

/// Placeholder role of a master/layout shape, when it declares one.
fn shape_placeholder_role(shape: &XmlNode) -> Option<&'static str> {
    shape
        .child("nvSpPr")
        .and_then(|props| props.child("nvPr"))
        .and_then(|props| props.child("ph"))
        .and_then(|placeholder| placeholder.attr("type"))
        .and_then(placeholder_role)
}

/// Finds the first DrawingML field with the given type below a shape.
fn shape_field<'a>(shape: &'a XmlNode, field_type: &str) -> Option<&'a XmlNode> {
    let mut fields = Vec::new();
    shape.find_all("fld", &mut fields);
    fields.into_iter().find(|field| field.attr("type") == Some(field_type))
}

/// Finds the first datetime field below a shape (`datetime1`, `datetime2`, ...).
fn shape_datetime_field(shape: &XmlNode) -> Option<&XmlNode> {
    let mut fields = Vec::new();
    shape.find_all("fld", &mut fields);
    fields.into_iter().find(|field| field.attr("type").map(|value| value.starts_with("datetime")).unwrap_or(false))
}

/// Reads the footer, date and slide-number content of a master or layout
/// shape into the deck-wide footer settings. An empty placeholder (the writer
/// injects those into masters by default) does not turn anything on.
fn scan_footer_shape(shape: &XmlNode, footer: &mut SlideFooter) -> bool {
    let Some(role) = shape_placeholder_role(shape) else { return false };
    match role {
        "footer" => {
            let text = shape
                .find_descendant("txBody")
                .map(|body| {
                    read_paragraphs(body).iter().map(|paragraph| paragraph.text.clone()).collect::<Vec<_>>().join("\n")
                })
                .unwrap_or_default();
            let text = text.trim();
            if text.is_empty() {
                return false;
            }
            footer.enabled = true;
            footer.show_text = true;
            if footer.text.is_empty() {
                footer.text = text.to_string();
            }
            true
        }
        "slideNumber" => {
            if shape_field(shape, "slidenum").is_none() {
                return false;
            }
            footer.enabled = true;
            footer.show_slide_number = true;
            true
        }
        "date" => {
            let Some(field) = shape_datetime_field(shape) else { return false };
            footer.enabled = true;
            footer.show_date = true;
            if footer.date_text.is_empty() {
                footer.date_text = field.find_descendant("t").map(XmlNode::deep_text).unwrap_or_default();
            }
            true
        }
        _ => false,
    }
}

fn read_masters(
    reader: &ZipReader,
    presentation_rels: &[(String, String, String)],
    warnings: &mut Vec<String>,
) -> (Vec<SlideMaster>, HashMap<String, (usize, String)>, Option<SlideFooter>) {
    let mut masters: Vec<SlideMaster> = Vec::new();
    let mut layout_index: HashMap<String, (usize, String)> = HashMap::new();
    let mut failed = false;
    let mut footer = SlideFooter::default();
    let mut footer_found = false;
    for (_, kind, target) in presentation_rels {
        if !kind.ends_with("slideMaster") {
            continue;
        }
        let master_part = resolve_part("ppt/presentation.xml", target);
        let master_text = match reader.read_text(&master_part) {
            Ok(text) => text,
            Err(_) => {
                failed = true;
                warnings.push(format!("Slide master part {master_part} could not be read."));
                continue;
            }
        };
        let master_root = match parse_xml(&master_text) {
            Ok(root) if root.local_name() == "sldMaster" => root,
            _ => {
                failed = true;
                warnings.push(format!("Slide master part {master_part} is malformed."));
                continue;
            }
        };
        let mut master = SlideMaster {
            id: part_stem(&master_part),
            name: format!("Master {}", masters.len() + 1),
            ..Default::default()
        };
        if let Some(background) = master_root.find_descendant("bgPr") {
            master.background = background
                .find_descendant("srgbClr")
                .and_then(|color| color.attr("val"))
                .map(|value| format!("#{value}"));
        }
        let typed = part_rels_typed(reader, &master_part);
        let rels: HashMap<String, String> = typed.iter().map(|(id, _, target)| (id.clone(), target.clone())).collect();
        for (_, rel_kind, rel_target) in &typed {
            if !rel_kind.ends_with("theme") {
                continue;
            }
            let theme_part = resolve_part(&master_part, rel_target);
            if let Ok(theme_text) = reader.read_text(&theme_part) {
                if let Ok(theme_root) = parse_xml(&theme_text) {
                    master.theme = theme_root.attr("name").and_then(theme_key).unwrap_or_default().to_string();
                }
            }
        }
        if let Some(tree) = master_root.find_descendant("spTree") {
            let mut z = 1i32;
            for shape in tree
                .children
                .iter()
                .filter(|child| child.local_name() != "nvGrpSpPr" && child.local_name() != "grpSpPr")
            {
                footer_found |= scan_footer_shape(shape, &mut footer);
                if let Some(object) = read_shape(shape, reader, &rels, &master_part, z, warnings) {
                    master.objects.push(object);
                    z += 1;
                }
            }
        }
        for (_, rel_kind, rel_target) in &typed {
            if !rel_kind.ends_with("slideLayout") {
                continue;
            }
            let layout_part = resolve_part(&master_part, rel_target);
            let layout_id = part_stem(&layout_part);
            let layout_text = match reader.read_text(&layout_part) {
                Ok(text) => text,
                Err(_) => {
                    failed = true;
                    warnings.push(format!("Slide layout part {layout_part} could not be read."));
                    continue;
                }
            };
            let layout_root = match parse_xml(&layout_text) {
                Ok(root) if root.local_name() == "sldLayout" => root,
                _ => {
                    failed = true;
                    warnings.push(format!("Slide layout part {layout_part} is malformed."));
                    continue;
                }
            };
            let mut layout = SlideLayout { id: layout_id.clone(), ..Default::default() };
            layout.name = layout_root.child("cSld").and_then(|cld| cld.attr("name")).unwrap_or("Layout").to_string();
            layout.kind = layout_root.attr("type").map(layout_kind).unwrap_or_else(|| "blank".to_string());
            if let Some(tree) = layout_root.find_descendant("spTree") {
                let mut z = 1i32;
                for shape in tree
                    .children
                    .iter()
                    .filter(|child| child.local_name() != "nvGrpSpPr" && child.local_name() != "grpSpPr")
                {
                    footer_found |= scan_footer_shape(shape, &mut footer);
                    if let Some(object) = read_shape(shape, reader, &rels, &layout_part, z, warnings) {
                        layout.objects.push(object);
                        z += 1;
                    }
                }
            }
            layout_index.insert(layout_part, (masters.len(), layout.id.clone()));
            master.layouts.push(layout);
        }
        masters.push(master);
    }
    if failed {
        warnings.push("Slide masters or layouts could not be read; the deck opens without master inheritance.".into());
        return (Vec::new(), HashMap::new(), None);
    }
    (masters, layout_index, if footer_found { Some(footer) } else { None })
}

pub fn read_pptx(bytes: &[u8]) -> OfficeResult<DeckRead> {
    let reader = ZipReader::open(bytes.to_vec())?;
    if !reader.contains("ppt/presentation.xml") {
        return Err(OfficeError::corrupt("The package does not contain a presentation."));
    }
    let mut warnings = Vec::new();
    let presentation = reader.read_text("ppt/presentation.xml")?;
    let root = parse_xml(&presentation)?;
    let typed_rels = part_rels_typed(&reader, "ppt/presentation.xml");
    let rels: HashMap<String, String> = typed_rels.iter().map(|(id, _, target)| (id.clone(), target.clone())).collect();
    let mut deck = Deck::new_blank("Imported presentation");
    if let Some(size) = root.child("sldSz") {
        if let Some(cx) = size.attr("cx").and_then(|value| value.parse::<f64>().ok()) {
            deck.size.width_pt = pt_from_emu(cx);
        }
        if let Some(cy) = size.attr("cy").and_then(|value| value.parse::<f64>().ok()) {
            deck.size.height_pt = pt_from_emu(cy);
        }
    }
    if let Some(defaults) = root.child("defaultTextStyle") {
        let mut default_runs = Vec::new();
        defaults.find_all("defRPr", &mut default_runs);
        deck.lang = default_runs.iter().find_map(|node| node.attr("lang").and_then(known_lang));
    }
    let (masters, layout_index, footer) = read_masters(&reader, &typed_rels, &mut warnings);
    deck.masters = masters;
    deck.footer = footer;
    deck.slides.clear();
    let mut slide_parts: Vec<(String, bool)> = Vec::new();
    let mut slide_ids = Vec::new();
    root.find_all("sldId", &mut slide_ids);
    for node in slide_ids {
        if let Some(rid) = node.attr("r:id").or_else(|| node.attr_any_ns("id")) {
            if let Some(target) = rels.get(rid) {
                let hidden =
                    node.attr("show").map(|value| value == "0" || value.eq_ignore_ascii_case("false")).unwrap_or(false);
                slide_parts.push((resolve_part("ppt/presentation.xml", target), hidden));
            }
        }
    }
    for (part, hidden) in &slide_parts {
        let Ok(text) = reader.read_text(part) else { continue };
        let Ok(slide_root) = parse_xml(&text) else { continue };
        let mut slide = Slide::default();
        slide.objects.clear();
        slide.hidden = *hidden;
        if let Some(background) = slide_root.find_descendant("bgPr") {
            if let Some(color) = background.find_descendant("srgbClr").and_then(|color| color.attr("val")) {
                slide.background = Some(format!("#{color}"));
            }
        }
        if let Some(transition) = slide_root.find_descendant("transition") {
            if let Some(advance) = transition.attr_any_ns("advTm").and_then(|value| value.trim().parse::<u32>().ok()) {
                slide.transition_ms = advance;
            }
            if transition.find_descendant("fade").is_some() {
                slide.transition = Some("fade".into());
            } else if transition.find_descendant("push").is_some() {
                slide.transition = Some("push".into());
            } else if transition.find_descendant("wipe").is_some() {
                slide.transition = Some("wipe".into());
            } else if transition.find_descendant("slide").is_some() {
                slide.transition = Some("slide".into());
            }
        }
        let slide_rels = part_rels(&reader, part);
        for (_, rel_kind, target) in part_rels_typed(&reader, part) {
            if !rel_kind.ends_with("slideLayout") {
                continue;
            }
            let layout_part = resolve_part(part, &target);
            if let Some((master_index, layout_id)) = layout_index.get(&layout_part) {
                slide.layout_id = Some(layout_id.clone());
                slide.master_id = deck.masters.get(*master_index).map(|master| master.id.clone());
            } else {
                slide.layout_id = Some(part_stem(&layout_part));
            }
        }
        let mut z = 1i32;
        let shapes: Vec<&XmlNode> = slide_root
            .find_descendant("spTree")
            .map(|tree| {
                tree.children
                    .iter()
                    .filter(|child| child.local_name() != "nvGrpSpPr" && child.local_name() != "grpSpPr")
                    .collect()
            })
            .unwrap_or_default();
        for shape in shapes {
            if let Some(object) = read_shape(shape, &reader, &slide_rels, part, z, &mut warnings) {
                slide.objects.push(object);
                z += 1;
            } else if shape.local_name() == "graphicFrame" && shape.find_descendant("chart").is_none() {
                warnings.push("An embedded diagram or object was not imported.".into());
            }
        }
        slide.animations = read_timing(&slide_root, &mut warnings);
        // Notes.
        for (rid, target) in &slide_rels {
            if target.contains("notesSlide") {
                let notes_part = resolve_part(part, target);
                if let Ok(notes_text) = reader.read_text(&notes_part) {
                    if let Ok(notes_root) = parse_xml(&notes_text) {
                        let mut body = Vec::new();
                        notes_root.find_all("bodyPr", &mut body);
                        slide.notes = notes_root
                            .children_named("cSld")
                            .flat_map(|cld| cld.children_named("spTree"))
                            .flat_map(|tree| tree.children.iter())
                            .filter_map(|shape| shape.find_descendant("txBody"))
                            .map(|body| {
                                read_paragraphs(body)
                                    .iter()
                                    .map(|paragraph| paragraph.text.clone())
                                    .collect::<Vec<_>>()
                                    .join("\n")
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                            .trim()
                            .to_string();
                    }
                }
                let _ = rid;
            }
        }
        deck.slides.push(slide);
    }
    if deck.slides.is_empty() {
        deck.slides.push(Slide::default());
    }
    if reader.names().any(|name| name.contains("vbaProject")) {
        warnings.push("Macros were not loaded. Presentations always open with macros disabled.".into());
    }
    warnings.sort();
    warnings.dedup();
    Ok(DeckRead { deck, warnings })
}

pub fn read_pptx_file(path: &Path) -> OfficeResult<DeckRead> {
    let bytes = crate::io::read_bytes(path)?;
    let mut result = read_pptx(&bytes)?;
    if result.deck.title.starts_with("Imported") || result.deck.title.is_empty() {
        result.deck.title = crate::io::file_stem(path);
    }
    Ok(result)
}
