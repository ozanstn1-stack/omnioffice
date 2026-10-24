//! ODP (Impress) import: shapes, text boxes and frames.

use super::*;
use crate::xml::XmlNode;

pub(super) fn read_odp_crop(image: &XmlNode, frame: &XmlNode, import: &OdpImport, w: f64, h: f64) -> Option<ImageCrop> {
    let clip = image
        .attr_any_ns("clip")
        .or_else(|| frame.attr_any_ns("clip"))
        .map(str::to_string)
        .or_else(|| frame.attr_any_ns("style-name").and_then(|name| import.clips.get(name)).cloned())?;
    let lengths = clip.trim().strip_prefix("rect(")?.strip_suffix(')')?;
    let lengths: Option<Vec<f64>> = lengths
        .split(|ch: char| ch.is_whitespace() || ch == ',')
        .filter(|part| !part.is_empty())
        .map(parse_cm)
        .collect();
    let lengths = lengths?;
    let [top, right, bottom, left] = lengths.as_slice() else { return None };
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let crop = ImageCrop {
        left: (left / w).abs().clamp(0.0, 0.95),
        top: (top / h).abs().clamp(0.0, 0.95),
        right: (right / w).abs().clamp(0.0, 0.95),
        bottom: (bottom / h).abs().clamp(0.0, 0.95),
    };
    (crop.left > 0.0 || crop.top > 0.0 || crop.right > 0.0 || crop.bottom > 0.0).then_some(crop)
}

/// A `draw:line` or `draw:connector` shape: geometry from the svg endpoints and
/// the glue references (`draw:start-shape` and friends) of a connector.
pub(super) fn read_odp_line(node: &XmlNode) -> Option<SlideObject> {
    let x1 = node.attr_any_ns("x1").and_then(parse_cm)?;
    let y1 = node.attr_any_ns("y1").and_then(parse_cm)?;
    let x2 = node.attr_any_ns("x2").and_then(parse_cm).unwrap_or(x1);
    let y2 = node.attr_any_ns("y2").and_then(parse_cm).unwrap_or(y1);
    let mut object = SlideObject::new("line", x1, y1, (x2 - x1).abs(), (y2 - y1).abs());
    object.line = Some(LineSpec {
        x2: x2 - x1,
        y2: y2 - y1,
        begin_arrow: node.attr_any_ns("marker-start").is_some(),
        end_arrow: node.attr_any_ns("marker-end").is_some(),
        dash: String::new(),
        begin_object: node.attr_any_ns("start-shape").map(str::to_string),
        end_object: node.attr_any_ns("end-shape").map(str::to_string),
        begin_site: node.attr_any_ns("start-glue-point").and_then(|value| value.parse::<u32>().ok()).unwrap_or(0),
        end_site: node.attr_any_ns("end-glue-point").and_then(|value| value.parse::<u32>().ok()).unwrap_or(0),
    });
    Some(object)
}

/// One `text:p` of a shape: runs with named styles resolved, alignment from
/// the paragraph style, and paragraph-level formatting aggregated from the
/// runs like the PPTX import does.
pub(super) fn read_odp_paragraph(paragraph: &XmlNode, level: u32, bullet: bool, styles: &ReadStyles) -> TextParagraph {
    let mut runs = Vec::new();
    let mut notes = NoteReadState::default();
    if !paragraph.text.is_empty() {
        runs.push(Run { text: paragraph.text.clone(), ..Default::default() });
    }
    for child in &paragraph.children {
        node_text_runs(child, &mut runs, &mut notes, styles);
    }
    let props = paragraph_props(paragraph, styles);
    let mut model = TextParagraph { level, bullet, align: props.align, ..Default::default() };
    for run in &runs {
        model.text.push_str(&run.text);
        model.bold |= run.bold;
        model.italic |= run.italic;
        model.underline |= run.underline;
        if model.size_pt.is_none() {
            model.size_pt = run.size_pt;
        }
        if model.color.is_none() {
            model.color = run.color.clone();
        }
    }
    model.lang = runs.iter().find_map(|run| run.lang.clone());
    model.runs = runs;
    model
}

/// The paragraphs of a `draw:text-box`: plain paragraphs and lists, the latter
/// nested so the nesting depth is the bullet level.
pub(super) fn read_odp_paragraphs(text_box: &XmlNode, styles: &ReadStyles) -> Vec<TextParagraph> {
    fn walk(node: &XmlNode, depth: u32, styles: &ReadStyles, out: &mut Vec<TextParagraph>) {
        for child in &node.children {
            match child.local_name() {
                "p" | "h" => out.push(read_odp_paragraph(child, depth.saturating_sub(1), depth > 0, styles)),
                "list" => {
                    for item in child.children_named("list-item") {
                        walk(item, depth + 1, styles, out);
                    }
                }
                _ => {}
            }
        }
    }
    let mut paragraphs = Vec::new();
    walk(text_box, 0, styles, &mut paragraphs);
    if paragraphs.is_empty() {
        paragraphs.push(TextParagraph::default());
    }
    paragraphs
}

/// One `draw:frame` of a slide or group: picture, embedded chart, text box,
/// table, connector, or a plain shape kept as a rectangle. Frames without a
/// size (lines) are skipped.
pub(super) fn read_odp_frame(
    frame: &XmlNode,
    zip: &ZipReader,
    import: &mut OdpImport,
    warnings: &mut Vec<String>,
) -> Option<SlideObject> {
    let x = frame.attr_any_ns("x").and_then(parse_cm).unwrap_or(40.0);
    let y = frame.attr_any_ns("y").and_then(parse_cm).unwrap_or(40.0);
    let w = frame.attr_any_ns("width").and_then(parse_cm).unwrap_or(320.0);
    let h = frame.attr_any_ns("height").and_then(parse_cm).unwrap_or(180.0);
    let mut images = Vec::new();
    frame.find_all("image", &mut images);
    if let Some(image_node) = images.first() {
        if let Some(href) = image_node.attr_any_ns("href") {
            let path = href.trim_start_matches("./");
            if let Ok(data) = zip.read(path) {
                let name = path.rsplit('/').next().unwrap_or("image.png").to_string();
                let mut object = SlideObject::new("image", x, y, w, h);
                let mut image = ImageData::from_bytes(&name, &data);
                image.crop = read_odp_crop(image_node, frame, import, w, h);
                object.image = Some(image);
                return Some(object);
            }
        }
    }
    // An embedded object is a chart when its sub-document parses as one; an
    // unrecognized one falls through to the rectangle fallback below.
    if frame.child("object").is_some() {
        let sheet = Sheet::new("Sheet1");
        if let Some(placement) = read_ods_chart(zip, frame, String::new(), &sheet, &mut import.seen_objects, warnings) {
            let mut object = SlideObject::new("chart", x, y, w, h);
            object.chart = Some(placement.chart);
            return Some(object);
        }
    }
    if let Some(line) = frame
        .children_of("connector")
        .into_iter()
        .next()
        .or_else(|| frame.children_of("line").into_iter().next())
        .and_then(read_odp_line)
    {
        return Some(line);
    }
    let mut text_boxes = Vec::new();
    frame.find_all("text-box", &mut text_boxes);
    if let Some(text_box) = text_boxes.first() {
        let paragraphs = read_odp_paragraphs(text_box, &import.styles);
        let mut object = SlideObject::new("text", x, y, w, h);
        if paragraphs.iter().any(|paragraph| !paragraph.text.trim().is_empty()) {
            object.text = Some(TextFrame { paragraphs, ..Default::default() });
        }
        return Some(object);
    }
    let mut tables = Vec::new();
    frame.find_all("table", &mut tables);
    if let Some(table) = tables.first() {
        let mut rows = Vec::new();
        for row_node in table.children_named("table-row") {
            let mut cells = Vec::new();
            for cell in row_node.children_named("table-cell") {
                let mut inner = Vec::new();
                cell.find_all("p", &mut inner);
                let cell_text = inner.iter().map(|node| node.deep_text()).collect::<Vec<_>>().join("\n");
                cells.push(TableCell { blocks: vec![Block::paragraph(&cell_text)], ..Default::default() });
            }
            rows.push(TableRow { cells, ..Default::default() });
        }
        let mut object = SlideObject::new("table", x, y, w, h);
        object.table = Some(TableData { rows, ..Default::default() });
        return Some(object);
    }
    // Shapes we cannot map precisely are preserved as rectangles.
    if frame.attr_any_ns("width").is_some() {
        let fill = match frame.attr_any_ns("fill") {
            Some("none") => None,
            _ => Some(frame.attr_any_ns("fill-color").unwrap_or("#E2E8F0").to_string()),
        };
        let mut object = SlideObject::new("rect", x, y, w, h);
        object.style = Some(ShapeStyle { fill, ..Default::default() });
        warnings.push("Some shapes were imported as simple rectangles.".into());
        return Some(object);
    }
    None
}

/// The shapes directly below `parent` (a `draw:page` or a `draw:g`) in
/// document order. A `draw:g` becomes a `group` object whose box is the
/// bounding box of its children, as the editor sizes groups; children keep
/// their absolute coordinates. Shape ids come from `xml:id`/`draw:id` so the
/// slide timing can find its targets.
pub(super) fn read_odp_shapes(
    parent: &XmlNode,
    zip: &ZipReader,
    import: &mut OdpImport,
    warnings: &mut Vec<String>,
) -> Vec<SlideObject> {
    let mut objects = Vec::new();
    for node in &parent.children {
        let object = match node.local_name() {
            "frame" => read_odp_frame(node, zip, import, warnings),
            // A line or connector may be a direct child of the page as well as
            // wrapped in a frame, as some producers write it.
            "line" | "connector" => read_odp_line(node),
            "g" => {
                let children = read_odp_shapes(node, zip, import, warnings);
                if children.is_empty() {
                    warnings.push("A group without importable shapes was skipped.".into());
                    None
                } else {
                    let left = children.iter().map(|child| child.x).fold(f64::INFINITY, f64::min);
                    let top = children.iter().map(|child| child.y).fold(f64::INFINITY, f64::min);
                    let right =
                        children.iter().map(|child| child.x + child.w.max(0.0)).fold(f64::NEG_INFINITY, f64::max);
                    let bottom =
                        children.iter().map(|child| child.y + child.h.max(0.0)).fold(f64::NEG_INFINITY, f64::max);
                    let mut group =
                        SlideObject::new("group", left, top, (right - left).max(1.0), (bottom - top).max(1.0));
                    group.children = children;
                    Some(group)
                }
            }
            _ => None,
        };
        let Some(mut object) = object else { continue };
        object.z = objects.len() as i32 + 1;
        if let Some(id) = node.attr("xml:id").or_else(|| node.attr("draw:id")).filter(|id| !id.trim().is_empty()) {
            object.id = id.to_string();
        }
        object.name = node.attr("draw:name").unwrap_or_default().to_string();
        objects.push(object);
    }
    objects
}

pub(super) fn collect_odp_ids<'a>(objects: &'a [SlideObject], out: &mut std::collections::HashSet<&'a str>) {
    for object in objects {
        out.insert(object.id.as_str());
        collect_odp_ids(&object.children, out);
    }
}

/// The effect `anim:par` nodes below a sequence, in document order. Effects are
/// the nodes that carry a trigger or a preset class; the click and "after
/// previous" containers around them are walked through.
pub(super) fn collect_odp_effects<'a>(node: &'a XmlNode, out: &mut Vec<&'a XmlNode>) {
    for child in &node.children {
        if !matches!(child.local_name(), "par" | "seq" | "iterate") {
            continue;
        }
        let trigger = matches!(child.attr_any_ns("node-type"), Some("on-click" | "with-previous" | "after-previous"));
        if trigger || child.attr_any_ns("preset-class").is_some() {
            out.push(child);
        } else {
            collect_odp_effects(child, out);
        }
    }
}
