//! ODP (Impress) export: the package writer.

use super::slides_write::*;
use super::slides_write_objects::*;
use super::*;
use std::path::Path;

pub fn write_odp_package(deck: &Deck) -> OfficeResult<DeckWrite> {
    let mut warnings = Vec::new();
    let bytes = write_odp_bytes(deck, &mut warnings)?;
    let mut seen = std::collections::HashSet::new();
    warnings.retain(|warning| seen.insert(warning.clone()));
    Ok(DeckWrite { bytes, warnings })
}

pub fn write_odp(deck: &Deck) -> OfficeResult<Vec<u8>> {
    Ok(write_odp_package(deck)?.bytes)
}

/// The `text:list` style the ODP content declares for the bullet lists the
/// writer emits (`LB`, the same name the ODT writer uses).
fn odp_list_style_xml() -> String {
    let mut out = String::from("<text:list-style style:name=\"LB\">");
    for level in 1..=9 {
        out.push_str(&format!("<text:list-level-style-bullet text:level=\"{level}\" text:bullet-char=\"•\"><style:list-level-properties text:space-before=\"{}cm\" text:min-label-width=\"0.6cm\"/></text:list-level-style-bullet>", (level as f64 - 1.0) * 0.6));
    }
    out.push_str("</text:list-style>");
    out
}

/// One placeholder frame of the ODF master page for the footer, date and slide
/// number; `body` is already escaped (or a field element).
fn odp_master_frame(class: &str, x: f64, y: f64, w: f64, h: f64, body: &str) -> String {
    format!(
        "<draw:frame presentation:class=\"{class}\" svg:x=\"{}\" svg:y=\"{}\" svg:width=\"{}\" svg:height=\"{}\"><draw:text-box><text:p>{body}</text:p></draw:text-box></draw:frame>",
        cm(x),
        cm(y),
        cm(w),
        cm(h)
    )
}

/// The placeholder frames of the default master page for an enabled footer,
/// laid out along the bottom of the slide like the PPTX writer lays them out.
fn odp_footer_shapes(deck: &Deck, footer: &SlideFooter) -> String {
    let width = deck.size.width_pt;
    let bottom = (deck.size.height_pt - 34.0).max(0.0);
    let mut out = String::new();
    if footer.show_date {
        out.push_str(&odp_master_frame(
            "date-time",
            width * 0.06,
            bottom,
            width * 0.25,
            24.0,
            &crate::xml::escape_text(&footer.date_text),
        ));
    }
    if footer.show_text {
        out.push_str(&odp_master_frame(
            "footer",
            width * 0.35,
            bottom,
            width * 0.30,
            24.0,
            &crate::xml::escape_text(&footer.text),
        ));
    }
    if footer.show_slide_number {
        out.push_str(&odp_master_frame(
            "page-number",
            width * 0.88,
            bottom,
            width * 0.06,
            24.0,
            "<text:page-number>1</text:page-number>",
        ));
    }
    out
}

fn write_odp_bytes(deck: &Deck, warnings: &mut Vec<String>) -> OfficeResult<Vec<u8>> {
    let mut body = String::new();
    let mut pictures: Vec<(String, Vec<u8>)> = Vec::new();
    let mut ids = OdpShapeIds::default();
    // Charts become `Object N` sub-documents numbered across the deck.
    let mut chart_objects: Vec<OdsChartObject> = Vec::new();
    let mut auto = AutoStyles::default();
    for (index, slide) in deck.slides.iter().enumerate() {
        let style_attr = if slide.hidden { " draw:style-name=\"dp-hidden\"" } else { "" };
        body.push_str(&format!(
            "<draw:page draw:name=\"Slide{}\" draw:master-page-name=\"Default\"{style_attr}>",
            index + 1
        ));
        ids.reset_slide();
        plan_odp_ids(&slide.objects, &mut ids);
        collect_odp_pictures(&slide.objects, &mut pictures);
        let mut chart_map: HashMap<String, String> = HashMap::new();
        let mut charts = Vec::new();
        collect_odp_charts(&slide.objects, &mut charts);
        for chart_object in charts {
            let Some(chart) = &chart_object.chart else { continue };
            match ods_chart_problem(chart) {
                Some(reason) => warnings.push(format!("Chart data is kept in the native .oswk file: {reason}.")),
                None => {
                    let name = format!("Object {}", chart_objects.len() + 1);
                    let placement = ChartPlacement {
                        id: chart_object.id.clone(),
                        chart: chart.clone(),
                        anchor: String::new(),
                        width_px: chart_object.w.max(48.0) / 0.75,
                        height_px: chart_object.h.max(48.0) / 0.75,
                    };
                    chart_objects.push(OdsChartObject {
                        content: ods_chart_content(&placement, "Sheet1", &BTreeMap::new()),
                        name: name.clone(),
                    });
                    if !chart_object.id.is_empty() {
                        chart_map.insert(chart_object.id.clone(), name);
                    }
                }
            }
        }
        let mut objects: Vec<&SlideObject> = slide.objects.iter().collect();
        objects.sort_by_key(|object| object.z);
        for object in objects {
            body.push_str(&slide_object_xml(object, &mut ids, &mut auto, &chart_map));
        }
        // draw:page content order: shapes, then the timing root, then notes.
        body.push_str(&odp_timing_xml(slide, &ids.slide, warnings));
        if !slide.notes.is_empty() {
            body.push_str(&format!("<presentation:notes><draw:frame presentation:class=\"notes\"><draw:text-box><text:p>{}</text:p></draw:text-box></draw:frame></presentation:notes>", crate::xml::escape_text(&slide.notes)));
        }
        body.push_str("</draw:page>");
    }
    let page_layout = format!(
        "<style:page-layout style:name=\"pl1\"><style:page-layout-properties fo:page-width=\"{}\" fo:page-height=\"{}\"/></style:page-layout>",
        cm(deck.size.width_pt),
        cm(deck.size.height_pt)
    );
    // A hidden slide is a drawing-page style with `visibility="hidden"`
    // (LibreOffice's convention); visible-only output stays exactly as before.
    let hidden_style = if deck.slides.iter().any(|slide| slide.hidden) {
        "<style:style style:name=\"dp-hidden\" style:family=\"drawing-page\"><style:drawing-page-properties presentation:visibility=\"hidden\"/></style:style>"
    } else {
        ""
    };
    let automatic = auto.xml();
    let automatic =
        automatic.trim_start_matches("<office:automatic-styles>").trim_end_matches("</office:automatic-styles>");
    let content = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-content {NS} {ANIM_NS} office:version=\"1.2\"><office:styles>{}</office:styles><office:automatic-styles>{page_layout}{hidden_style}{automatic}</office:automatic-styles><office:body><office:presentation>{body}</office:presentation></office:body></office:document-content>",
        odp_list_style_xml()
    );
    let background = match deck.theme.as_str() {
        "dark" => "#0F172A",
        "business" => "#F8FAFC",
        "education" => "#FEFCE8",
        "modern" => "#FFFFFF",
        _ => "#FFFFFF",
    };
    let mut drawing_props = format!("draw:fill=\"solid\" draw:fill-color=\"{background}\"");
    let mut master_shapes = String::new();
    if let Some(footer) = deck.footer.as_ref().filter(|footer| footer.enabled) {
        if footer.show_text {
            drawing_props.push_str(" presentation:display-footer=\"true\"");
        }
        if footer.show_slide_number {
            drawing_props.push_str(" presentation:display-page-number=\"true\"");
        }
        if footer.show_date {
            drawing_props.push_str(" presentation:display-date-time=\"true\"");
        }
        master_shapes = odp_footer_shapes(deck, footer);
    }
    let styles = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-styles {NS} office:version=\"1.2\"><office:styles/><office:automatic-styles>{page_layout}</office:automatic-styles><office:master-styles><style:master-page style:name=\"Default\" style:page-layout-name=\"pl1\"><style:drawing-page-properties {drawing_props}/>{master_shapes}</style:master-page></office:master-styles></office:document-styles>"
    );
    // Every chart object is a sub-document with its own manifest entries.
    let mut files = vec!["content.xml".to_string(), "styles.xml".to_string(), "meta.xml".to_string()];
    for object in &chart_objects {
        files.push(format!("{}/", object.name));
        files.push(format!("{}/content.xml", object.name));
        files.push(format!("{}/styles.xml", object.name));
    }
    for (name, _) in &pictures {
        files.push(format!("Pictures/{name}"));
    }
    let manifest = manifest_for("application/vnd.oasis.opendocument.presentation", &files);
    let mut zip = ZipWriter::new();
    zip.add_text("mimetype", "application/vnd.oasis.opendocument.presentation");
    zip.add_text("META-INF/manifest.xml", &manifest);
    zip.add_text("content.xml", &content);
    zip.add_text("styles.xml", &styles);
    zip.add_text("meta.xml", &meta_xml(&deck.title, "OmniOffice", None));
    for object in &chart_objects {
        zip.add_text(&format!("{}/content.xml", object.name), &object.content);
        zip.add_text(
            &format!("{}/styles.xml", object.name),
            &format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-styles {NS} {CHART_NS} office:version=\"1.2\"><office:styles/></office:document-styles>"
            ),
        );
    }
    for (name, data) in &pictures {
        zip.add(&format!("Pictures/{name}"), data);
    }
    Ok(zip.finish())
}

pub fn write_odp_file(path: &Path, deck: &Deck) -> OfficeResult<()> {
    crate::io::write_atomic(path, &write_odp(deck)?)
}

/// Read-side state of one ODP package: named styles, crops declared by graphic
/// styles and the embedded objects already read.
#[derive(Default)]
pub(super) struct OdpImport {
    pub(super) styles: ReadStyles,
    pub(super) clips: HashMap<String, String>,
    pub(super) seen_objects: HashSet<String>,
}

/// `style:graphic-properties/fo:clip` by style name, for images that carry
/// their crop on a graphic style instead of the `draw:image` element.
pub(super) fn odp_graphic_clips(roots: &[&XmlNode]) -> HashMap<String, String> {
    let mut clips = HashMap::new();
    for root in roots {
        let mut styles = Vec::new();
        root.find_all("style", &mut styles);
        for style in styles {
            let Some(name) = style.attr_any_ns("name") else { continue };
            let mut properties = Vec::new();
            style.find_all("graphic-properties", &mut properties);
            if let Some(clip) = properties.iter().find_map(|node| node.attr_any_ns("clip")) {
                clips.insert(name.to_string(), clip.to_string());
            }
        }
    }
    clips
}
