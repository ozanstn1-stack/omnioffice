//! ODP (Impress) export: text frames, shapes and slide objects.

use super::slides_write::*;
use super::*;

/// Writes a `draw:text-box`'s paragraphs the way the ODT writer writes body
/// text: automatic paragraph and text styles shared through `AutoStyles`,
/// bullets collected into `text:list` elements, and `Standard` for a plain
/// paragraph. Runs that no longer add up to `paragraph.text` fall back to the
/// paragraph text itself, so an edit can never lose content.
fn write_odp_text_frame(writer: &mut XmlWriter, text: &TextFrame, styles: &mut AutoStyles) {
    let mut index = 0usize;
    while index < text.paragraphs.len() {
        let paragraph = &text.paragraphs[index];
        if !paragraph.bullet {
            write_odp_paragraph(writer, paragraph, styles);
            index += 1;
            continue;
        }
        // A run of consecutive bullets becomes one list, nested by level.
        let start = index;
        while index < text.paragraphs.len() && text.paragraphs[index].bullet {
            index += 1;
        }
        write_odp_list(writer, &text.paragraphs[start..index], styles, 0);
    }
    if text.paragraphs.is_empty() {
        writer.raw("<text:p/>");
    }
}

/// One `text:list` of a bullet run at `level`; items deeper than `level` become
/// nested lists inside their item, so the reader can recover the level.
pub(super) fn write_odp_list(writer: &mut XmlWriter, items: &[TextParagraph], styles: &mut AutoStyles, level: u32) {
    writer.raw("<text:list text:style-name=\"LB\">");
    let mut index = 0usize;
    while index < items.len() {
        writer.raw("<text:list-item>");
        write_odp_paragraph(writer, &items[index], styles);
        let mut end = index + 1;
        while end < items.len() && items[end].level > level {
            end += 1;
        }
        if end > index + 1 {
            write_odp_list(writer, &items[index + 1..end], styles, level + 1);
        }
        writer.raw("</text:list-item>");
        index = end;
    }
    writer.raw("</text:list>");
}

pub(super) fn write_odp_paragraph(writer: &mut XmlWriter, paragraph: &TextParagraph, styles: &mut AutoStyles) {
    let props = ParaProps {
        align: paragraph.align.clone(),
        list: paragraph.bullet.then(|| ListInfo {
            kind: "bullet".into(),
            level: paragraph.level,
            start: 1,
            marker: "•".into(),
        }),
        ..Default::default()
    };
    let properties = paragraph_style_xml(&props, false);
    let name = if properties == "<style:paragraph-properties/>" {
        "Standard".to_string()
    } else {
        styles.paragraph(properties)
    };
    writer.raw(&format!("<text:p text:style-name=\"{name}\">"));
    let runs_match = paragraph.runs.iter().map(|run| run.text.as_str()).collect::<String>() == paragraph.text;
    if paragraph.runs.is_empty() || !runs_match {
        let fallback = Run {
            text: paragraph.text.clone(),
            bold: paragraph.bold,
            italic: paragraph.italic,
            underline: paragraph.underline,
            size_pt: paragraph.size_pt,
            color: paragraph.color.clone(),
            lang: paragraph.lang.clone(),
            ..Default::default()
        };
        write_runs(writer, &[fallback], styles, &NoteContext::default());
    } else {
        write_runs(writer, &paragraph.runs, styles, &NoteContext::default());
    }
    writer.raw("</text:p>");
}

/// One shape of a slide as `draw:*` XML. `charts` maps a chart object's model
/// id to the `Object N` sub-document the writer stored for it.
pub(super) fn slide_object_xml(
    object: &SlideObject,
    ids: &mut OdpShapeIds,
    styles: &mut AutoStyles,
    charts: &HashMap<String, String>,
) -> String {
    let id = ids.assign(object);
    let id_attrs = format!(" draw:id=\"{0}\" xml:id=\"{0}\"", escape(&id));
    if object.kind == "group" {
        // Children keep their absolute page coordinates; draw:g has no box of
        // its own, which matches the editor's group = bounding box of children.
        let mut children: Vec<&SlideObject> = object.children.iter().collect();
        children.sort_by_key(|child| child.z);
        let inner: String = children.into_iter().map(|child| slide_object_xml(child, ids, styles, charts)).collect();
        return format!(
            "<draw:g draw:name=\"{}\"{id_attrs} draw:z-index=\"{}\">{inner}</draw:g>",
            escape(&object.name),
            object.z
        );
    }
    let frame_start = format!("<draw:frame draw:name=\"{}\"{id_attrs} text:anchor-type=\"page\"", escape(&object.name));
    let style = object.style.clone().unwrap_or_default();
    let mut inner = String::new();
    match object.kind.as_str() {
        "image" => {
            if let Some(image) = &object.image {
                // `fo:clip` trims the rendered frame; the fractions are of the
                // rendered size, spelled in cm with the same precision as every
                // other length in this writer.
                let clip = image
                    .crop
                    .as_ref()
                    .filter(|crop| crop.left > 0.0 || crop.top > 0.0 || crop.right > 0.0 || crop.bottom > 0.0)
                    .map(|crop| {
                        format!(
                            " fo:clip=\"rect({} {} {} {})\"",
                            cm(crop.top * object.h.max(1.0)),
                            cm(crop.right * object.w.max(1.0)),
                            cm(crop.bottom * object.h.max(1.0)),
                            cm(crop.left * object.w.max(1.0))
                        )
                    })
                    .unwrap_or_default();
                inner.push_str(&format!("<draw:image xlink:href=\"Pictures/{}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"{clip}/>", escape(&image.name)));
            }
        }
        "line" | "arrow" => {
            let line = object.line.clone().unwrap_or_default();
            let (x1, y1, x2, y2) = (object.x, object.y, object.x + line.x2, object.y + line.y2);
            if line.begin_object.is_some() || line.end_object.is_some() {
                // A glued line is a real connector: LibreOffice needs the
                // written shape ids (planned before writing) and glue points.
                let mut glue = String::new();
                if let Some(target) = line.begin_object.as_deref().and_then(|target| ids.slide.get(target)) {
                    glue.push_str(&format!(
                        " draw:start-shape=\"{}\" draw:start-glue-point=\"{}\"",
                        escape(target),
                        line.begin_site
                    ));
                }
                if let Some(target) = line.end_object.as_deref().and_then(|target| ids.slide.get(target)) {
                    glue.push_str(&format!(
                        " draw:end-shape=\"{}\" draw:end-glue-point=\"{}\"",
                        escape(target),
                        line.end_site
                    ));
                }
                let stroke = object.style.as_ref().and_then(|style| style.stroke.clone());
                let width = object.style.as_ref().map(|style| style.stroke_width_pt).unwrap_or(2.0).max(0.5);
                let properties = format!(
                    "<style:graphic-properties draw:stroke=\"solid\" svg:stroke-width=\"{}\"{} draw:fill=\"none\"/>",
                    cm(width),
                    stroke.map(|color| format!(" svg:stroke-color=\"{}\"", escape(&color))).unwrap_or_default()
                );
                let style_name = styles.graphic(properties);
                inner.push_str(&format!(
                    "<draw:connector svg:x1=\"{}\" svg:y1=\"{}\" svg:x2=\"{}\" svg:y2=\"{}\"{glue} draw:style-name=\"{style_name}\"/>",
                    cm(x1),
                    cm(y1),
                    cm(x2),
                    cm(y2)
                ));
            } else {
                inner.push_str(&format!(
                    "<draw:line svg:x1=\"{}\" svg:y1=\"{}\" svg:x2=\"{}\" svg:y2=\"{}\" draw:style-name=\"gr1\"/>",
                    cm(x1),
                    cm(y1),
                    cm(x2),
                    cm(y2)
                ));
            }
        }
        "table" => {
            if let Some(table) = &object.table {
                let mut writer = XmlWriter::new();
                let mut styles = AutoStyles::default();
                let mut media = Media::default();
                write_table(&mut writer, table, &mut styles, &mut media, &NoteContext::default());
                inner.push_str(&writer.finish());
            }
        }
        "chart" => match charts.get(&object.id) {
            Some(name) => inner.push_str(&format!(
                "<draw:object xlink:href=\"./{}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"/>",
                escape(name)
            )),
            None => {
                // A chart this writer cannot export keeps the old title
                // placeholder so nothing is dropped silently.
                if let Some(chart) = &object.chart {
                    let title =
                        if chart.title.trim().is_empty() { format!("{} chart", chart.kind) } else { chart.title.clone() };
                    inner.push_str(&format!(
                        "<draw:text-box><text:p text:style-name=\"Standard\">{}</text:p></draw:text-box>",
                        crate::xml::escape_text(&title)
                    ));
                }
            }
        },
        _ => {
            if let Some(text) = &object.text {
                let mut writer = XmlWriter::new();
                writer.raw("<draw:text-box>");
                write_odp_text_frame(&mut writer, text, styles);
                writer.raw("</draw:text-box>");
                inner.push_str(&writer.finish());
            }
        }
    }
    let shape = match object.kind.as_str() {
        "ellipse" => "draw:ellipse",
        "line" | "arrow" => return format!("{frame_start} draw:z-index=\"{}\">{inner}</draw:frame>", object.z),
        _ => "draw:frame",
    };
    if shape == "draw:frame" {
        let fill = style
            .fill
            .as_deref()
            .map(|fill| format!(" draw:fill=\"solid\" draw:fill-color=\"{}\"", escape(fill)))
            .unwrap_or_else(|| " draw:fill=\"none\"".into());
        format!(
            "{frame_start} svg:x=\"{}\" svg:y=\"{}\" svg:width=\"{}\" svg:height=\"{}\" draw:z-index=\"{}\"{}>{}<draw:glue-points/><draw:enhanced-geometry/></draw:frame>",
            cm(object.x),
            cm(object.y),
            cm(object.w.max(4.0)),
            cm(object.h.max(4.0)),
            object.z,
            fill,
            inner
        )
        .replace("<draw:glue-points/><draw:enhanced-geometry/>", "")
    } else {
        format!(
            "{frame_start} svg:x=\"{}\" svg:y=\"{}\" svg:width=\"{}\" svg:height=\"{}\" draw:z-index=\"{}\">{}</draw:frame>",
            cm(object.x),
            cm(object.y),
            cm(object.w.max(4.0)),
            cm(object.h.max(4.0)),
            object.z,
            inner
        )
    }
}

#[derive(Debug, Clone)]
pub struct DeckWrite {
    pub bytes: Vec<u8>,
    pub warnings: Vec<String>,
}

/// Assigns ids to the shapes of one slide in the order `slide_object_xml`
/// visits them, so a connector can name a shape written later in the z order.
pub(super) fn plan_odp_ids(objects: &[SlideObject], ids: &mut OdpShapeIds) {
    let mut ordered: Vec<&SlideObject> = objects.iter().collect();
    ordered.sort_by_key(|object| object.z);
    for object in ordered {
        if !object.id.is_empty() {
            ids.plan(object);
        }
        plan_odp_ids(&object.children, ids);
    }
}

/// The chart objects anywhere on a slide, in the order the writer visits them.
pub(super) fn collect_odp_charts<'a>(objects: &'a [SlideObject], charts: &mut Vec<&'a SlideObject>) {
    let mut ordered: Vec<&SlideObject> = objects.iter().collect();
    ordered.sort_by_key(|object| object.z);
    for object in ordered {
        if object.chart.is_some() {
            charts.push(object);
        }
        collect_odp_charts(&object.children, charts);
    }
}

pub(super) fn collect_odp_pictures(objects: &[SlideObject], pictures: &mut Vec<(String, Vec<u8>)>) {
    for object in objects {
        if let Some(image) = &object.image {
            if !image.is_empty() && !pictures.iter().any(|(name, _)| name == &image.name) {
                pictures.push((image.name.clone(), image.bytes()));
            }
        }
        collect_odp_pictures(&object.children, pictures);
    }
}
