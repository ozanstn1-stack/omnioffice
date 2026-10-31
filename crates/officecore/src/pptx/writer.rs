//! PPTX writer: themes, masters, layouts, slides, notes, charts, media and timing.
//! The reader lives in `reader`; the public entry points are re-exported by `mod.rs`.

use super::*;

const NS: &str = concat!(
    "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" ",
    "xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" ",
    "xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\""
);

/// Fixed field ids keep footer, date and slide-number fields deterministic, so
/// re-exporting the same deck produces the same bytes.
const SLIDE_NUMBER_FIELD_ID: &str = "{5F2F7F04-8F5A-4D1C-9A9E-2B4C6D8E0A11}";
const DATE_FIELD_ID: &str = "{8B1D4C6E-3A2F-4E7B-B5C1-0D9F3A7E5B42}";

fn emu(points: f64) -> i64 {
    (points * 12700.0).round() as i64
}

pub(super) fn pt_from_emu(value: f64) -> f64 {
    value / 12700.0
}

/// A crop fraction as the 0..100000 DrawingML percentage units.
fn crop_percent(fraction: f64) -> i64 {
    (fraction.clamp(0.0, 1.0) * 100000.0).round() as i64
}

#[derive(Debug, Clone)]
pub struct DeckRead {
    pub deck: Deck,
    pub warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// Theme catalogue
// ---------------------------------------------------------------------------

struct Theme {
    name: &'static str,
    dk1: &'static str,
    lt1: &'static str,
    dk2: &'static str,
    lt2: &'static str,
    accents: [&'static str; 6],
    heading_font: &'static str,
    body_font: &'static str,
}

fn theme_for(name: &str) -> Theme {
    match name {
        "business" => Theme {
            name: "Business",
            dk1: "1F2937",
            lt1: "FFFFFF",
            dk2: "111827",
            lt2: "F3F4F6",
            accents: ["1D4ED8", "0E7490", "B45309", "15803D", "7C3AED", "BE123C"],
            heading_font: "Segoe UI",
            body_font: "Segoe UI",
        },
        "dark" => Theme {
            name: "Dark",
            dk1: "F8FAFC",
            lt1: "0F172A",
            dk2: "E2E8F0",
            lt2: "1E293B",
            accents: ["60A5FA", "34D399", "FBBF24", "F472B6", "A78BFA", "22D3EE"],
            heading_font: "Segoe UI",
            body_font: "Segoe UI",
        },
        "modern" => Theme {
            name: "Modern",
            dk1: "0B1220",
            lt1: "FFFFFF",
            dk2: "334155",
            lt2: "E2E8F0",
            accents: ["2563EB", "14B8A6", "F97316", "8B5CF6", "EC4899", "10B981"],
            heading_font: "Segoe UI",
            body_font: "Segoe UI",
        },
        "education" => Theme {
            name: "Education",
            dk1: "1E293B",
            lt1: "FFFDF5",
            dk2: "334155",
            lt2: "FEF3C7",
            accents: ["B45309", "0369A1", "15803D", "7C2D12", "6D28D9", "0F766E"],
            heading_font: "Georgia",
            body_font: "Georgia",
        },
        "simple" => Theme {
            name: "Simple",
            dk1: "111111",
            lt1: "FFFFFF",
            dk2: "444444",
            lt2: "F2F2F2",
            accents: ["444444", "666666", "888888", "B0B0B0", "D0D0D0", "9A9A9A"],
            heading_font: "Arial",
            body_font: "Arial",
        },
        _ => Theme {
            name: "Minimal",
            dk1: "111827",
            lt1: "FFFFFF",
            dk2: "374151",
            lt2: "F9FAFB",
            accents: ["2563EB", "059669", "D97706", "DC2626", "7C3AED", "0891B2"],
            heading_font: "Segoe UI",
            body_font: "Segoe UI",
        },
    }
}

fn theme_xml(theme: &Theme) -> String {
    let accents = theme.accents;
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="{name}"><a:themeElements>
<a:clrScheme name="{name}"><a:dk1><a:srgbClr val="{dk1}"/></a:dk1><a:lt1><a:srgbClr val="{lt1}"/></a:lt1><a:dk2><a:srgbClr val="{dk2}"/></a:dk2><a:lt2><a:srgbClr val="{lt2}"/></a:lt2>
<a:accent1><a:srgbClr val="{a1}"/></a:accent1><a:accent2><a:srgbClr val="{a2}"/></a:accent2><a:accent3><a:srgbClr val="{a3}"/></a:accent3><a:accent4><a:srgbClr val="{a4}"/></a:accent4><a:accent5><a:srgbClr val="{a5}"/></a:accent5><a:accent6><a:srgbClr val="{a6}"/></a:accent6>
<a:hlink><a:srgbClr val="{a1}"/></a:hlink><a:folHlink><a:srgbClr val="{a5}"/></a:folHlink></a:clrScheme>
<a:fontScheme name="{name}"><a:majorFont><a:latin typeface="{heading}"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont><a:minorFont><a:latin typeface="{body}"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme>
<a:fmtScheme name="{name}"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:gradFill rotWithShape="1"><a:gsLst><a:gs pos="0"><a:schemeClr val="phClr"><a:lumMod val="110000"/><a:satMod val="105000"/><a:tint val="67000"/></a:schemeClr></a:gs><a:gs pos="50000"><a:schemeClr val="phClr"><a:lumMod val="105000"/><a:satMod val="103000"/><a:tint val="73000"/></a:schemeClr></a:gs><a:gs pos="100000"><a:schemeClr val="phClr"><a:lumMod val="105000"/><a:satMod val="109000"/><a:tint val="81000"/></a:schemeClr></a:gs></a:gsLst><a:lin ang="5400000" scaled="0"/></a:gradFill><a:gradFill rotWithShape="1"><a:gsLst><a:gs pos="0"><a:schemeClr val="phClr"><a:satMod val="103000"/><a:lumMod val="102000"/><a:tint val="94000"/></a:schemeClr></a:gs><a:gs pos="50000"><a:schemeClr val="phClr"><a:satMod val="110000"/><a:lumMod val="100000"/><a:shade val="100000"/></a:schemeClr></a:gs><a:gs pos="100000"><a:schemeClr val="phClr"><a:lumMod val="99000"/><a:satMod val="120000"/><a:shade val="78000"/></a:schemeClr></a:gs></a:gsLst><a:lin ang="5400000" scaled="0"/></a:gradFill></a:fillStyleLst>
<a:lnStyleLst><a:ln w="6350" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="solid"/><a:miter lim="800000"/></a:ln><a:ln w="12700" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="solid"/><a:miter lim="800000"/></a:ln><a:ln w="19050" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="solid"/><a:miter lim="800000"/></a:ln></a:lnStyleLst>
<a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst>
<a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"><a:tint val="95000"/><a:satMod val="170000"/></a:schemeClr></a:solidFill><a:gradFill rotWithShape="1"><a:gsLst><a:gs pos="0"><a:schemeClr val="phClr"><a:tint val="93000"/><a:satMod val="150000"/><a:shade val="98000"/><a:lumMod val="102000"/></a:schemeClr></a:gs><a:gs pos="50000"><a:schemeClr val="phClr"><a:tint val="98000"/><a:satMod val="130000"/><a:shade val="90000"/><a:lumMod val="103000"/></a:schemeClr></a:gs><a:gs pos="100000"><a:schemeClr val="phClr"><a:shade val="63000"/><a:satMod val="120000"/></a:schemeClr></a:gs></a:gsLst><a:lin ang="5400000" scaled="0"/></a:gradFill></a:bgFillStyleLst></a:fmtScheme></a:themeElements><a:objectDefaults/><a:extraClrSchemeLst/></a:theme>"#,
        name = theme.name,
        dk1 = theme.dk1,
        lt1 = theme.lt1,
        dk2 = theme.dk2,
        lt2 = theme.lt2,
        a1 = accents[0],
        a2 = accents[1],
        a3 = accents[2],
        a4 = accents[3],
        a5 = accents[4],
        a6 = accents[5],
        heading = theme.heading_font,
        body = theme.body_font
    )
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

struct RelSet {
    next: usize,
    entries: Vec<(String, String, String)>,
}

impl RelSet {
    fn new() -> Self {
        Self { next: 1, entries: Vec::new() }
    }

    fn add(&mut self, kind: &str, target: &str) -> String {
        let id = format!("rId{}", self.next);
        self.next += 1;
        self.entries.push((id.clone(), kind.to_string(), target.to_string()));
        id
    }

    fn xml(&self) -> String {
        let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">");
        for (id, kind, target) in &self.entries {
            out.push_str(&format!("<Relationship Id=\"{id}\" Type=\"{kind}\" Target=\"{}\"/>", escape_attr(target)));
        }
        out.push_str("</Relationships>");
        out
    }
}

const REL_SLIDE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide";
const REL_LAYOUT: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout";
const REL_IMAGE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
const REL_NOTES: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesSlide";
const REL_MASTER: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster";
const REL_THEME: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme";
const REL_CHART: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart";
const REL_PACKAGE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/package";

fn presentation_xml(deck: &Deck, master_ids: &[String], slide_ids: &[String]) -> String {
    let mut out = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<p:presentation {NS} saveSubsetFonts=\"1\"><p:sldMasterIdLst>"
    );
    for (offset, rid) in master_ids.iter().enumerate() {
        let master_id = 2147483648u32 + offset as u32;
        out.push_str(&format!("<p:sldMasterId id=\"{master_id}\" r:id=\"{rid}\"/>"));
    }
    out.push_str("</p:sldMasterIdLst><p:sldIdLst>");
    for (offset, rid) in slide_ids.iter().enumerate() {
        let next_id = 256u32 + offset as u32;
        if deck.slides.get(offset).map(|slide| slide.hidden).unwrap_or(false) {
            out.push_str(&format!("<p:sldId id=\"{next_id}\" r:id=\"{rid}\" show=\"0\"/>"));
        } else {
            out.push_str(&format!("<p:sldId id=\"{next_id}\" r:id=\"{rid}\"/>"));
        }
    }
    out.push_str("</p:sldIdLst>");
    out.push_str(&format!(
        "<p:sldSz cx=\"{}\" cy=\"{}\"/><p:notesSz cx=\"6858000\" cy=\"9144000\"/>",
        emu(deck.size.width_pt.max(200.0)),
        emu(deck.size.height_pt.max(150.0))
    ));
    let lang = lang_attr(deck.lang.as_deref());
    if !lang.is_empty() {
        out.push_str(&format!("<p:defaultTextStyle><a:defPPr><a:defRPr{lang}/></a:defPPr></p:defaultTextStyle>"));
    }
    out.push_str("</p:presentation>");
    out
}

fn master_xml(
    theme: &Theme,
    background: &str,
    footer: Option<&SlideFooter>,
    width: f64,
    height: f64,
    export: &mut ExportContext,
) -> String {
    let _ = theme;
    let mut shapes = String::new();
    if let Some(footer) = footer.filter(|footer| footer.enabled) {
        let mut writer = SlideWriter::new();
        for (role, x, y, w, h) in default_footer_placeholders(width, height) {
            let mut shape = placeholder_object(role, x, y, w, h);
            shape.text = footer_placeholder_text(role, footer);
            if let Some(xml) = object_xml(&shape, theme, &mut writer, export) {
                shapes.push_str(&xml);
            }
        }
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<p:sldMaster {NS}><p:cSld><p:bg><p:bgPr><a:solidFill><a:srgbClr val=\"{background}\"/></a:solidFill><a:effectLst/></p:bgPr></p:bg><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>{shapes}</p:spTree></p:cSld><p:clrMap bg1=\"lt1\" tx1=\"dk1\" bg2=\"lt2\" tx2=\"dk2\" accent1=\"accent1\" accent2=\"accent2\" accent3=\"accent3\" accent4=\"accent4\" accent5=\"accent5\" accent6=\"accent6\" hlink=\"hlink\" folHlink=\"folHlink\"/><p:sldLayoutIdLst><p:sldLayoutId id=\"2147483649\" r:id=\"rId1\"/></p:sldLayoutIdLst><p:txStyles><p:titleStyle/><p:bodyStyle/><p:otherStyle/></p:txStyles></p:sldMaster>"
    )
}

fn layout_xml(background: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<p:sldLayout {NS} type=\"blank\" preserve=\"1\"><p:cSld name=\"Blank\"><p:bg><p:bgPr><a:solidFill><a:srgbClr val=\"{background}\"/></a:solidFill><a:effectLst/></p:bgPr></p:bg><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/></p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>"
    )
}

struct PlannedLayout {
    id: String,
    part: String,
    model: Option<SlideLayout>,
    theme_name: String,
}

struct PlannedMaster {
    master: Option<SlideMaster>,
    part: String,
    theme_part: String,
    theme_name: String,
    background: String,
    layouts: Vec<PlannedLayout>,
}

fn plan_masters(deck: &Deck) -> Vec<PlannedMaster> {
    let deck_theme = deck.theme.as_str();
    if deck.masters.is_empty() {
        let background = match deck_theme {
            "dark" => theme_for(deck_theme).lt1.to_string(),
            _ => "FFFFFF".to_string(),
        };
        return vec![PlannedMaster {
            master: None,
            part: "ppt/slideMasters/slideMaster1.xml".into(),
            theme_part: "ppt/theme/theme1.xml".into(),
            theme_name: deck_theme.to_string(),
            background,
            layouts: vec![PlannedLayout {
                id: String::new(),
                part: "ppt/slideLayouts/slideLayout1.xml".into(),
                model: None,
                theme_name: deck_theme.to_string(),
            }],
        }];
    }
    let mut planned = Vec::new();
    let mut layout_number = 1usize;
    for (index, master) in deck.masters.iter().enumerate() {
        let theme_name = if master.theme.trim().is_empty() { deck_theme } else { master.theme.as_str() };
        let theme = theme_for(theme_name);
        let background = master.background.as_deref().unwrap_or(theme.lt1).trim_start_matches('#').to_string();
        let mut layouts = Vec::new();
        for layout in &master.layouts {
            layouts.push(PlannedLayout {
                id: layout.id.clone(),
                part: format!("ppt/slideLayouts/slideLayout{layout_number}.xml"),
                model: Some(layout.clone()),
                theme_name: theme_name.to_string(),
            });
            layout_number += 1;
        }
        if layouts.is_empty() {
            layouts.push(PlannedLayout {
                id: String::new(),
                part: format!("ppt/slideLayouts/slideLayout{layout_number}.xml"),
                model: None,
                theme_name: theme_name.to_string(),
            });
            layout_number += 1;
        }
        planned.push(PlannedMaster {
            master: Some(master.clone()),
            part: format!("ppt/slideMasters/slideMaster{}.xml", index + 1),
            theme_part: format!("ppt/theme/theme{}.xml", index + 1),
            theme_name: theme_name.to_string(),
            background,
            layouts,
        });
    }
    planned
}

fn layout_for_slide<'a>(slide: &Slide, masters: &'a [PlannedMaster]) -> Option<&'a PlannedLayout> {
    if let Some(layout_id) = slide.layout_id.as_deref() {
        for master in masters {
            if let Some(layout) = master.layouts.iter().find(|layout| layout.id == layout_id) {
                return Some(layout);
            }
        }
    }
    if let Some(master_id) = slide.master_id.as_deref() {
        if let Some(master) =
            masters.iter().find(|master| master.master.as_ref().map(|model| model.id.as_str()) == Some(master_id))
        {
            if let Some(layout) = master.layouts.first() {
                return Some(layout);
            }
        }
    }
    masters.first().and_then(|master| master.layouts.first())
}

fn planned_theme_name(layout: Option<&PlannedLayout>, deck: &Deck) -> String {
    layout.map(|layout| layout.theme_name.clone()).unwrap_or_else(|| deck.theme.clone())
}

fn file_name(part: &str) -> &str {
    part.rsplit('/').next().unwrap_or(part)
}

fn rels_name(part: &str) -> String {
    let (dir, file) = part.rsplit_once('/').unwrap_or(("", part));
    format!("{dir}/_rels/{file}.rels")
}

pub(super) fn part_stem(part: &str) -> String {
    file_name(part).split('.').next().unwrap_or(part).to_string()
}

pub(super) fn placeholder_role(kind: &str) -> Option<&'static str> {
    match kind {
        "title" | "ctrTitle" => Some("title"),
        "body" => Some("body"),
        "subTitle" => Some("subtitle"),
        "ftr" => Some("footer"),
        "sldNum" => Some("slideNumber"),
        "dt" => Some("date"),
        _ => None,
    }
}

fn placeholder_xml(placeholder: &Option<String>) -> String {
    match placeholder.as_deref() {
        Some("title") => "<p:ph type=\"title\"/>".into(),
        Some("body") => "<p:ph type=\"body\" idx=\"1\"/>".into(),
        Some("subtitle") => "<p:ph type=\"subTitle\" idx=\"1\"/>".into(),
        Some("footer") => "<p:ph type=\"ftr\" idx=\"11\"/>".into(),
        Some("slideNumber") => "<p:ph type=\"sldNum\" idx=\"12\"/>".into(),
        Some("date") => "<p:ph type=\"dt\" idx=\"10\"/>".into(),
        _ => String::new(),
    }
}

fn layout_type(kind: &str) -> &str {
    match kind {
        "title" => "titleOnly",
        "titleContent" => "obj",
        "twoContent" => "twoColTx",
        "section" => "secHead",
        "blank" => "blank",
        other if !other.is_empty() => other,
        _ => "blank",
    }
}

pub(super) fn layout_kind(kind: &str) -> String {
    match kind {
        "title" | "titleOnly" => "title".into(),
        "obj" => "titleContent".into(),
        "twoColTx" => "twoContent".into(),
        "secHead" => "section".into(),
        other => other.to_string(),
    }
}

struct SlideWriter {
    rels: RelSet,
    next_shape: usize,
    shape_ids: HashMap<String, usize>,
    planned_ids: HashMap<String, usize>,
}

impl SlideWriter {
    fn new() -> Self {
        Self { rels: RelSet::new(), next_shape: 1, shape_ids: HashMap::new(), planned_ids: HashMap::new() }
    }

    fn shape_id(&mut self) -> usize {
        self.next_shape += 1;
        self.next_shape
    }

    fn transform(x: f64, y: f64, w: f64, h: f64, rotation: f64) -> String {
        let rotation_attr = if rotation.abs() > 0.01 {
            format!(" rot=\"{}\"", (rotation * 60000.0).round() as i64)
        } else {
            String::new()
        };
        format!(
            "<a:xfrm{rotation_attr}><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm>",
            emu(x.max(-100000.0)),
            emu(y.max(-100000.0)),
            emu(w.max(4.0)),
            emu(h.max(4.0))
        )
    }

    fn fill(stroke: &Option<(String, f64)>, fill: &Option<String>) -> String {
        let mut out = String::new();
        match fill {
            Some(color) => out.push_str(&format!(
                "<a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill>",
                escape_attr(color.trim_start_matches('#'))
            )),
            None => out.push_str("<a:noFill/>"),
        }
        if let Some((color, width)) = stroke {
            out.push_str(&format!(
                "<a:ln w=\"{}\"><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill><a:prstDash val=\"solid\"/></a:ln>",
                (width * 12700.0).round() as i64,
                escape_attr(color.trim_start_matches('#'))
            ))
        }
        out
    }

    fn text_body(text: &TextFrame, theme: &Theme, deck_lang: Option<&str>) -> String {
        let default_size = text.size_pt.unwrap_or(18.0);
        let mut out =
            String::from("<p:txBody><a:bodyPr wrap=\"square\" rtlCol=\"0\"><a:normAutofit/></a:bodyPr><a:lstStyle/>");
        if text.paragraphs.is_empty() {
            out.push_str("<a:p/>");
        }
        for paragraph in &text.paragraphs {
            let size = paragraph.size_pt.unwrap_or(default_size);
            let bullet = if paragraph.bullet { "<a:buChar char=\"\u{2022}\"/>" } else { "<a:buNone/>" };
            out.push_str(&format!(
                "<a:p><a:pPr lvl=\"{}\" algn=\"{}\">{bullet}</a:pPr>",
                paragraph.level.min(8),
                alignment(if paragraph.align.is_empty() { &text.align } else { &paragraph.align })
            ));
            // Runs describe the paragraph text; when they no longer add up to
            // it (an edit that kept the old runs) the paragraph text wins.
            let runs_match = paragraph.runs.iter().map(|run| run.text.as_str()).collect::<String>() == paragraph.text;
            if paragraph.runs.is_empty() || !runs_match {
                let mut attributes = format!(
                    "{} sz=\"{}\"",
                    lang_attr(paragraph.lang.as_deref().or(deck_lang)),
                    (size * 100.0).round() as i64
                );
                if paragraph.bold {
                    attributes.push_str(" b=\"1\"");
                }
                if paragraph.italic {
                    attributes.push_str(" i=\"1\"");
                }
                if paragraph.underline {
                    attributes.push_str(" u=\"sng\"");
                }
                if let Some(color) = paragraph.color.as_deref().or(text.color.as_deref()) {
                    attributes.push_str(&format!(
                        "><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill>",
                        escape_attr(color.trim_start_matches('#'))
                    ));
                } else {
                    attributes.push('>');
                }
                out.push_str(&format!(
                    "<a:r><a:rPr{attributes}<a:latin typeface=\"{}\"/><a:cs typeface=\"{}\"/></a:rPr><a:t>{}</a:t></a:r>",
                    escape_attr(text.font.as_deref().unwrap_or(theme.body_font)),
                    escape_attr(theme.body_font),
                    escape_text(&paragraph.text)
                ));
            } else {
                for run in &paragraph.runs {
                    // Slide-number and date placeholders are real fields with a
                    // cached value instead of plain text.
                    if let Some(field) = run.field.as_ref().and_then(field_xml) {
                        out.push_str(&field);
                        continue;
                    }
                    let mut attributes = format!(
                        "{} sz=\"{}\"",
                        lang_attr(run.lang.as_deref().or(paragraph.lang.as_deref()).or(deck_lang)),
                        (run.size_pt.unwrap_or(size) * 100.0).round() as i64
                    );
                    if run.bold || paragraph.bold {
                        attributes.push_str(" b=\"1\"");
                    }
                    if run.italic || paragraph.italic {
                        attributes.push_str(" i=\"1\"");
                    }
                    if run.underline || paragraph.underline {
                        attributes.push_str(" u=\"sng\"");
                    }
                    if run.strike {
                        attributes.push_str(" strike=\"sngStrike\"");
                    }
                    if run.superscript {
                        attributes.push_str(" baseline=\"30000\"");
                    } else if run.subscript {
                        attributes.push_str(" baseline=\"-25000\"");
                    }
                    if let Some(color) = run.color.as_deref().or(paragraph.color.as_deref()).or(text.color.as_deref()) {
                        attributes.push_str(&format!(
                            "><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill>",
                            escape_attr(color.trim_start_matches('#'))
                        ));
                    } else {
                        attributes.push('>');
                    }
                    if let Some(highlight) = run.highlight.as_deref() {
                        attributes.push_str(&format!(
                            "<a:highlight><a:srgbClr val=\"{}\"/></a:highlight>",
                            escape_attr(highlight.trim_start_matches('#'))
                        ));
                    }
                    out.push_str(&format!("<a:r><a:rPr{attributes}<a:latin typeface=\"{}\"/><a:cs typeface=\"{}\"/></a:rPr><a:t>{}</a:t></a:r>", escape_attr(run.font.as_deref().or(text.font.as_deref()).unwrap_or(theme.body_font)), escape_attr(theme.body_font), escape_text(&run.text)));
                }
            }
            out.push_str("</a:p>");
        }
        out.push_str("</p:txBody>");
        out
    }
}

/// ` lang="..."` for a known language tag, nothing when it is unknown so the
/// consumer falls back to its own proofing language instead of ours.
fn lang_attr(lang: Option<&str>) -> String {
    match lang.map(str::trim).filter(|tag| !tag.is_empty()) {
        Some(tag) => format!(" lang=\"{}\"", escape_attr(tag)),
        None => String::new(),
    }
}

fn alignment(value: &str) -> &'static str {
    match value {
        "center" => "ctr",
        "right" => "r",
        _ => "l",
    }
}

/// One exported chart part plus, when the chart carries cached values, the
/// embedded workbook that backs them (`c:externalData`).
struct ChartPart {
    part: String,
    xml: String,
    rels: String,
    embedding: Option<(String, Vec<u8>)>,
}

struct ExportContext {
    media: Vec<(String, Vec<u8>)>,
    charts: Vec<ChartPart>,
    next_chart: usize,
    next_embedding: usize,
    warnings: Vec<String>,
    /// Deck default language, the fallback for text that carries none.
    lang: Option<String>,
}

impl ExportContext {
    fn new(lang: Option<String>) -> Self {
        Self { media: Vec::new(), charts: Vec::new(), next_chart: 1, next_embedding: 1, warnings: Vec::new(), lang }
    }

    fn warn(&mut self, message: &str) {
        self.warnings.push(message.to_string());
    }
}

fn chart_kind_supported(kind: &str) -> bool {
    matches!(kind, "column" | "bar" | "line" | "pie" | "area")
}

const CHART_CATEGORY_AXIS: u64 = 111_111_111;
const CHART_VALUE_AXIS: u64 = 222_222_222;

fn chart_title_xml(text: &str) -> String {
    format!(
        "<c:title><c:tx><c:rich><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang=\"en-US\"/><a:t>{}</a:t></a:r></a:p></c:rich></c:tx><c:overlay val=\"0\"/></c:title>",
        escape_text(text)
    )
}

fn chart_axes_xml(chart: &ChartData) -> String {
    let category_title = if chart.x_title.is_empty() { String::new() } else { chart_title_xml(&chart.x_title) };
    let value_title = if chart.y_title.is_empty() { String::new() } else { chart_title_xml(&chart.y_title) };
    format!(
        "<c:catAx><c:axId val=\"{cat}\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"b\"/>{category_title}<c:crossAx val=\"{val}\"/></c:catAx><c:valAx><c:axId val=\"{val}\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"l\"/>{value_title}<c:crossAx val=\"{cat}\"/></c:valAx>",
        cat = CHART_CATEGORY_AXIS,
        val = CHART_VALUE_AXIS,
    )
}

fn chart_ref(value: &str) -> String {
    escape_text(value.trim())
}

/// True when the chart carries cached values: those are written as ChartML
/// caches and backed by an embedded workbook so Excel/LibreOffice can render
/// (and re-edit) the chart even when the original range is not in the deck.
fn chart_has_cache(chart: &ChartData) -> bool {
    !chart.categories_cache.is_empty() || chart.series_values_cache.iter().any(|values| !values.is_empty())
}

/// Formats one cached value the way Excel writes `c:v`: plain decimal, no
/// exponent for ordinary magnitudes; non-finite values become 0 because XML
/// numbers cannot be NaN/Infinity.
fn chart_cache_number(value: f64) -> String {
    if !value.is_finite() {
        return "0".into();
    }
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

fn chart_str_cache_xml(labels: &[String]) -> String {
    let mut out = format!("<c:strCache><c:ptCount val=\"{}\"/>", labels.len());
    for (index, label) in labels.iter().enumerate() {
        out.push_str(&format!("<c:pt idx=\"{index}\"><c:v>{}</c:v></c:pt>", escape_text(label)));
    }
    out.push_str("</c:strCache>");
    out
}

fn chart_num_cache_xml(values: &[f64]) -> String {
    let mut out = format!("<c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"{}\"/>", values.len());
    for (index, value) in values.iter().enumerate() {
        out.push_str(&format!("<c:pt idx=\"{index}\"><c:v>{}</c:v></c:pt>", chart_cache_number(*value)));
    }
    out.push_str("</c:numCache>");
    out
}

/// Builds the embedded workbook for a chart whose caches carry values. The
/// package is produced with the same xlsx writer Calc uses, so the reader in
/// this crate (and Excel/LibreOffice) sees an ordinary readable spreadsheet.
fn embedded_workbook(chart: &ChartData) -> OfficeResult<Vec<u8>> {
    let mut workbook = Workbook::new_blank("Chart data");
    workbook.sheets.clear();
    let mut sheet = Sheet::new("Sheet1");
    sheet.set("A1", Cell { value: CellValue::Text("Category".into()), ..Default::default() });
    for (index, series) in chart.series.iter().enumerate() {
        let address = crate::address::format(0, index as u32 + 1);
        sheet.set(&address, Cell { value: CellValue::Text(series.name.clone()), ..Default::default() });
    }
    for (row, label) in chart.categories_cache.iter().enumerate() {
        let address = crate::address::format(row as u32 + 1, 0);
        sheet.set(&address, Cell { value: CellValue::Text(label.clone()), ..Default::default() });
    }
    for (series_index, values) in chart.series_values_cache.iter().enumerate() {
        for (row, value) in values.iter().enumerate() {
            let address = crate::address::format(row as u32 + 1, series_index as u32 + 1);
            sheet.set(&address, Cell { value: CellValue::Number(*value), ..Default::default() });
        }
    }
    workbook.sheets.push(sheet);
    crate::xlsx::write_xlsx(&workbook)
}

fn chart_part_xml(chart: &ChartData, external_rid: Option<&str>) -> Result<String, String> {
    let kind = chart.kind.as_str();
    if !chart_kind_supported(kind) {
        return Err(format!("the chart type \"{kind}\" is not representable yet"));
    }
    if chart.series.is_empty() {
        return Err("the chart has no data series".into());
    }
    if chart.categories.trim().is_empty() {
        return Err("the chart has no category range".into());
    }

    let mut series_xml = String::new();
    let label_cache =
        if chart.categories_cache.is_empty() { String::new() } else { chart_str_cache_xml(&chart.categories_cache) };
    for (index, series) in chart.series.iter().enumerate() {
        series_xml.push_str(&format!(
            "<c:ser><c:idx val=\"{index}\"/><c:order val=\"{index}\"/><c:tx><c:v>{}</c:v></c:tx>",
            escape_text(&series.name)
        ));
        if let Some(color) = series.color.as_deref() {
            series_xml.push_str(&format!(
                "<c:spPr><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill></c:spPr>",
                escape_attr(color.trim_start_matches('#'))
            ));
        }
        // Only series that actually carry cached values get a c:numCache; the
        // rest keep the range-only shape the existing decks rely on.
        let value_cache = chart
            .series_values_cache
            .get(index)
            .filter(|values| !values.is_empty())
            .map(|values| chart_num_cache_xml(values))
            .unwrap_or_default();
        series_xml.push_str(&format!(
            "<c:cat><c:strRef><c:f>{}</c:f>{}</c:strRef></c:cat><c:val><c:numRef><c:f>{}</c:f>{}</c:numRef></c:val></c:ser>",
            chart_ref(&chart.categories),
            label_cache,
            chart_ref(&series.range),
            value_cache
        ));
    }

    let labels = if chart.show_labels {
        "<c:dLbls><c:showLegendKey val=\"0\"/><c:showVal val=\"1\"/><c:showCatName val=\"0\"/><c:showSerName val=\"0\"/><c:showPercent val=\"0\"/><c:showBubbleSize val=\"0\"/></c:dLbls>"
    } else {
        ""
    };

    let mut plot = String::new();
    let mut axes = String::new();
    match kind {
        "column" | "bar" => {
            let direction = if kind == "bar" { "bar" } else { "col" };
            let grouping = if chart.stacked { "stacked" } else { "clustered" };
            let overlap = if chart.stacked { "<c:overlap val=\"100\"/>" } else { "" };
            plot.push_str(&format!(
                "<c:barChart><c:barDir val=\"{direction}\"/><c:grouping val=\"{grouping}\"/><c:varyColors val=\"0\"/>{series_xml}{labels}<c:gapWidth val=\"150\"/>{overlap}<c:axId val=\"{cat}\"/><c:axId val=\"{val}\"/></c:barChart>",
                cat = CHART_CATEGORY_AXIS,
                val = CHART_VALUE_AXIS,
            ));
            axes = chart_axes_xml(chart);
        }
        "line" => {
            plot.push_str(&format!(
                "<c:lineChart><c:grouping val=\"standard\"/><c:varyColors val=\"0\"/>{series_xml}{labels}<c:marker val=\"1\"/><c:axId val=\"{cat}\"/><c:axId val=\"{val}\"/></c:lineChart>",
                cat = CHART_CATEGORY_AXIS,
                val = CHART_VALUE_AXIS,
            ));
            axes = chart_axes_xml(chart);
        }
        "area" => {
            let grouping = if chart.stacked { "stacked" } else { "standard" };
            plot.push_str(&format!(
                "<c:areaChart><c:grouping val=\"{grouping}\"/><c:varyColors val=\"0\"/>{series_xml}{labels}<c:axId val=\"{cat}\"/><c:axId val=\"{val}\"/></c:areaChart>",
                cat = CHART_CATEGORY_AXIS,
                val = CHART_VALUE_AXIS,
            ));
            axes = chart_axes_xml(chart);
        }
        "pie" => {
            plot.push_str(&format!(
                "<c:pieChart><c:varyColors val=\"1\"/>{series_xml}{labels}<c:firstSliceAng val=\"0\"/></c:pieChart>"
            ));
        }
        _ => return Err(format!("the chart type \"{kind}\" is not representable yet")),
    }

    let title = if chart.title.is_empty() { String::new() } else { chart_title_xml(&chart.title) };
    let legend = if chart.legend { "<c:legend><c:legendPos val=\"b\"/><c:overlay val=\"0\"/></c:legend>" } else { "" };
    // c:externalData points at the embedded workbook part so Word/Excel treat
    // the cached values as an editable data source instead of a dead cache.
    let external = external_rid
        .map(|rid| format!("<c:externalData r:id=\"{}\"><c:autoUpdate val=\"0\"/></c:externalData>", escape_attr(rid)))
        .unwrap_or_default();
    Ok(format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><c:roundedCorners val=\"0\"/><c:chart>{title}<c:plotArea><c:layout/>{plot}{axes}</c:plotArea>{legend}<c:plotVisOnly val=\"1\"/><c:dispBlanksAs val=\"gap\"/></c:chart>{external}<c:printSettings><c:headerFooter/><c:pageMargins b=\"0.75\" l=\"0.7\" r=\"0.7\" t=\"0.75\" header=\"0.3\" footer=\"0.3\"/><c:pageSetup/></c:printSettings></c:chartSpace>"
    ))
}

fn children_bounds(children: &[SlideObject]) -> Option<(f64, f64, f64, f64)> {
    if children.is_empty() {
        return None;
    }
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;
    for child in children {
        min_x = min_x.min(child.x);
        min_y = min_y.min(child.y);
        max_x = max_x.max(child.x + child.w.max(0.0));
        max_y = max_y.max(child.y + child.h.max(0.0));
    }
    Some((min_x, min_y, (max_x - min_x).max(1.0), (max_y - min_y).max(1.0)))
}

fn map_object_into_group(
    object: &SlideObject,
    outer: (f64, f64, f64, f64),
    inner: (f64, f64, f64, f64),
) -> SlideObject {
    let (ox, oy, ow, oh) = outer;
    let (cx, cy, cw, ch) = inner;
    let sx = if ow.abs() > 0.01 { cw / ow } else { 1.0 };
    let sy = if oh.abs() > 0.01 { ch / oh } else { 1.0 };
    let mut mapped = object.clone();
    mapped.x = cx + (object.x - ox) * sx;
    mapped.y = cy + (object.y - oy) * sy;
    mapped.w = object.w * sx;
    mapped.h = object.h * sy;
    if let (Some(line), Some(source)) = (mapped.line.as_mut(), object.line.as_ref()) {
        line.x2 = source.x2 * sx;
        line.y2 = source.y2 * sy;
    }
    mapped.children = object.children.iter().map(|child| map_object_into_group(child, outer, inner)).collect();
    mapped
}

fn group_transform(x: f64, y: f64, w: f64, h: f64, rotation: f64, inner: (f64, f64, f64, f64)) -> String {
    let rotation_attr =
        if rotation.abs() > 0.01 { format!(" rot=\"{}\"", (rotation * 60000.0).round() as i64) } else { String::new() };
    format!(
        "<a:xfrm{rotation_attr}><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/><a:chOff x=\"{}\" y=\"{}\"/><a:chExt cx=\"{}\" cy=\"{}\"/></a:xfrm>",
        emu(x.max(-100000.0)),
        emu(y.max(-100000.0)),
        emu(w.max(4.0)),
        emu(h.max(4.0)),
        emu(inner.0.max(-100000.0)),
        emu(inner.1.max(-100000.0)),
        emu(inner.2.max(4.0)),
        emu(inner.3.max(4.0))
    )
}

/// Maps every object of a slide (or master/layout) to the OOXML shape id the
/// writer will assign it. The traversal mirrors `object_xml` exactly (objects
/// in z-order, group children in z-order), so connectors can point at objects
/// that are written later. Ids are deterministic for the same deck.
fn planned_shape_ids(objects: &[&SlideObject]) -> HashMap<String, usize> {
    let mut map = HashMap::new();
    let mut next = 1usize;
    for object in objects {
        plan_shape_id(object, &mut next, &mut map);
    }
    map
}

fn plan_shape_id(object: &SlideObject, next: &mut usize, map: &mut HashMap<String, usize>) {
    *next += 1;
    if !object.id.is_empty() {
        map.insert(object.id.clone(), *next);
    }
    if object.kind == "group" {
        let mut children: Vec<&SlideObject> = object.children.iter().collect();
        children.sort_by_key(|child| child.z);
        for child in children {
            plan_shape_id(child, next, map);
        }
    }
}

/// Default master positions for the footer, date and slide-number
/// placeholders, matching the slots the writer has always used.
fn default_footer_placeholders(width: f64, height: f64) -> [(&'static str, f64, f64, f64, f64); 3] {
    let bottom = (height - 44.0).max(0.0);
    [
        ("date", 40.0, bottom, 200.0, 28.0),
        ("footer", (width * 0.25).max(40.0), bottom, (width * 0.5).max(160.0), 28.0),
        ("slideNumber", (width - 120.0).max(40.0), bottom, 80.0, 28.0),
    ]
}

/// The placeholder content for a footer slot, or `None` when the deck turned
/// the slot off. The date and slide-number slots become real DrawingML fields
/// with a cached value so every viewer shows the same text.
fn footer_placeholder_text(role: &str, footer: &SlideFooter) -> Option<TextFrame> {
    match role {
        "footer" if footer.show_text => Some(TextFrame {
            paragraphs: vec![TextParagraph { text: footer.text.clone(), ..Default::default() }],
            ..Default::default()
        }),
        "slideNumber" if footer.show_slide_number => Some(TextFrame {
            paragraphs: vec![TextParagraph {
                text: "1".into(),
                runs: vec![Run {
                    text: "1".into(),
                    field: Some(FieldRef { kind: "slideNumber".into(), cached: "1".into(), ..Default::default() }),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        }),
        "date" if footer.show_date => Some(TextFrame {
            paragraphs: vec![TextParagraph {
                text: footer.date_text.clone(),
                runs: vec![Run {
                    text: footer.date_text.clone(),
                    field: Some(FieldRef {
                        kind: "date".into(),
                        cached: footer.date_text.clone(),
                        ..Default::default()
                    }),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        }),
        _ => None,
    }
}

/// `<a:fld>` rendering for the fields the writer knows; fixed ids and cached
/// text make the output deterministic.
fn field_xml(field: &FieldRef) -> Option<String> {
    match field.kind.as_str() {
        "slideNumber" => Some(format!(
            "<a:fld id=\"{SLIDE_NUMBER_FIELD_ID}\" type=\"slidenum\"><a:rPr lang=\"en-US\" sz=\"1200\"/><a:t>{}</a:t></a:fld>",
            escape_text(&field.cached)
        )),
        "date" => Some(format!(
            "<a:fld id=\"{DATE_FIELD_ID}\" type=\"datetime1\"><a:t>{}</a:t></a:fld>",
            escape_text(&field.cached)
        )),
        _ => None,
    }
}

fn object_xml(
    object: &SlideObject,
    theme: &Theme,
    writer: &mut SlideWriter,
    export: &mut ExportContext,
) -> Option<String> {
    let style = object.style.clone().unwrap_or_default();
    let fill = style.fill.as_deref().map(str::to_string);
    let stroke = style.stroke.as_deref().map(|color| (color.to_string(), style.stroke_width_pt.max(0.5)));
    let name = if object.name.is_empty() { format!("{} {}", object.kind, object.id) } else { object.name.clone() };
    let id = writer.shape_id();
    let xml = match object.kind.as_str() {
        "image" => {
            let Some(image) = &object.image else {
                export.warn("An image object has no image data and was kept in the native .oswk file.");
                return None;
            };
            if image.is_empty() {
                export.warn("An image object has no image data and was kept in the native .oswk file.");
                return None;
            }
            let mut resolved = None;
            for (media_name, _) in &export.media {
                if media_name == &image.name {
                    resolved = Some(media_name.clone());
                }
            }
            let part_name = resolved.unwrap_or_else(|| {
                let part = format!("image{}.{}", export.media.len() + 1, image.extension());
                export.media.push((part.clone(), image.bytes()));
                part
            });
            let rid = writer.rels.add(REL_IMAGE, &format!("../media/{part_name}"));
            let src_rect = image
                .crop
                .as_ref()
                .filter(|crop| crop.left > 0.0 || crop.top > 0.0 || crop.right > 0.0 || crop.bottom > 0.0)
                .map(|crop| {
                    format!(
                        "<a:srcRect l=\"{}\" t=\"{}\" r=\"{}\" b=\"{}\"/>",
                        crop_percent(crop.left),
                        crop_percent(crop.top),
                        crop_percent(crop.right),
                        crop_percent(crop.bottom)
                    )
                })
                .unwrap_or_default();
            format!(
                "<p:pic><p:nvPicPr><p:cNvPr id=\"{id}\" name=\"{}\"/><p:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed=\"{rid}\"/>{src_rect}<a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr>{}</p:spPr></p:pic>",
                escape_attr(&name),
                SlideWriter::transform(object.x, object.y, object.w, object.h, object.rotation)
            )
        }
        "chart" => {
            let Some(chart) = &object.chart else {
                export.warn("A chart object has no chart data and was kept in the native .oswk file.");
                return None;
            };
            // A cached chart gets an embedded workbook part plus the
            // relationship that c:externalData points at. If the workbook
            // cannot be built the chart is still exported with its ranges.
            let mut embedding: Option<(String, Vec<u8>)> = None;
            if chart_has_cache(chart) {
                match embedded_workbook(chart) {
                    Ok(bytes) => {
                        let number = export.next_embedding;
                        export.next_embedding += 1;
                        embedding = Some((format!("ppt/embeddings/Microsoft_Excel_Worksheet{number}.xlsx"), bytes));
                    }
                    Err(error) => export.warn(&format!(
                        "A chart cache could not be embedded and was kept in the native .oswk file: {error}."
                    )),
                }
            }
            let mut chart_rels = RelSet::new();
            let external_rid = embedding.as_ref().map(|(target, _)| {
                let target = target.strip_prefix("ppt/").unwrap_or(target);
                chart_rels.add(REL_PACKAGE, &format!("../{target}"))
            });
            match chart_part_xml(chart, external_rid.as_deref()) {
                Ok(chart_xml) => {
                    let part = format!("ppt/charts/chart{}.xml", export.next_chart);
                    export.next_chart += 1;
                    let rid = writer.rels.add(REL_CHART, &format!("../charts/{}", file_name(&part)));
                    export.charts.push(ChartPart {
                        part,
                        xml: chart_xml,
                        rels: if embedding.is_some() { chart_rels.xml() } else { String::new() },
                        embedding,
                    });
                    format!(
                        "<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id=\"{id}\" name=\"{}\"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm>{}</p:xfrm><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" r:id=\"{rid}\"/></a:graphicData></a:graphic></p:graphicFrame>",
                        escape_attr(&name),
                        SlideWriter::transform(object.x, object.y, object.w, object.h, object.rotation)
                    )
                }
                Err(reason) => {
                    export.warn(&format!(
                        "A chart was kept in the native .oswk file and not embedded into PPTX: {reason}."
                    ));
                    return None;
                }
            }
        }
        "group" => {
            let inner =
                children_bounds(&object.children).unwrap_or((object.x, object.y, object.w.max(4.0), object.h.max(4.0)));
            let mut sorted: Vec<&SlideObject> = object.children.iter().collect();
            sorted.sort_by_key(|child| child.z);
            let mut children = String::new();
            for child in sorted {
                let mapped = map_object_into_group(child, (object.x, object.y, object.w, object.h), inner);
                if let Some(child_xml) = object_xml(&mapped, theme, writer, export) {
                    children.push_str(&child_xml);
                }
            }
            format!(
                "<p:grpSp><p:nvGrpSpPr><p:cNvPr id=\"{id}\" name=\"{}\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr>{}</p:grpSpPr>{children}</p:grpSp>",
                escape_attr(&name),
                group_transform(object.x, object.y, object.w, object.h, object.rotation, inner)
            )
        }
        "line" | "arrow" => {
            let line = object.line.clone().unwrap_or_default();
            let color = fill
                .clone()
                .or_else(|| stroke.as_ref().map(|(color, _)| color.clone()))
                .unwrap_or_else(|| theme.accents[0].to_string());
            let width = if style.stroke_width_pt > 0.0 { style.stroke_width_pt } else { 2.0 };
            let mut ends = String::new();
            if line.begin_arrow {
                ends.push_str("<a:headEnd type=\"triangle\"/>");
            }
            if line.end_arrow {
                ends.push_str("<a:tailEnd type=\"triangle\"/>");
            }
            let (dx, dy) = (line.x2, line.y2);
            let off_x = if dx >= 0.0 { object.x } else { object.x + dx };
            let off_y = if dy >= 0.0 { object.y } else { object.y + dy };
            let flips =
                format!("{}{}", if dx < 0.0 { " flipH=\"1\"" } else { "" }, if dy < 0.0 { " flipV=\"1\"" } else { "" });
            // Endpoints glued to other slide objects become stCxn/endCxn
            // references; the site index is the OOXML rect connection site
            // (0 = top, 1 = left, 2 = bottom, 3 = right).
            let mut connections = String::new();
            if let Some(begin) = line.begin_object.as_deref().and_then(|target| writer.planned_ids.get(target)) {
                connections.push_str(&format!("<a:stCxn id=\"{begin}\" idx=\"{}\"/>", line.begin_site));
            }
            if let Some(end) = line.end_object.as_deref().and_then(|target| writer.planned_ids.get(target)) {
                connections.push_str(&format!("<a:endCxn id=\"{end}\" idx=\"{}\"/>", line.end_site));
            }
            format!(
                "<p:cxnSp><p:nvCxnSpPr><p:cNvPr id=\"{id}\" name=\"{}\"/><p:cNvCxnSpPr>{connections}</p:cNvCxnSpPr><p:nvPr/></p:nvCxnSpPr><p:spPr><a:xfrm{flips}><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm><a:prstGeom prst=\"line\"><a:avLst/></a:prstGeom><a:ln w=\"{}\"><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill><a:prstDash val=\"{}\"/>{ends}</a:ln></p:spPr></p:cxnSp>",
                escape_attr(&name),
                emu(off_x),
                emu(off_y),
                emu(dx.abs().max(1.0)),
                emu(dy.abs().max(1.0)),
                (width * 12700.0).round() as i64,
                escape_attr(color.trim_start_matches('#')),
                if line.dash.is_empty() || line.dash == "solid" { "solid" } else { &line.dash }
            )
        }
        "table" => {
            let Some(table) = &object.table else {
                export.warn("A table object has no table data and was kept in the native .oswk file.");
                return None;
            };
            let rows = table.rows.len().max(1);
            let columns = table.rows.iter().map(|row| row.cells.len()).max().unwrap_or(1).max(1);
            let cell_width = emu(object.w / columns as f64);
            let cell_height = emu(object.h / rows as f64);
            let mut grid = String::new();
            for _ in 0..columns {
                grid.push_str(&format!("<a:gridCol w=\"{cell_width}\"/>"));
            }
            let mut table_rows = String::new();
            for row in &table.rows {
                table_rows.push_str(&format!("<a:tr h=\"{cell_height}\">"));
                for cell in &row.cells {
                    let text = cell.blocks.iter().map(Block::plain_text).collect::<Vec<_>>().join(" ");
                    let fill_xml = cell
                        .background
                        .as_deref()
                        .map(|color| {
                            format!(
                                "<a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill>",
                                escape_attr(color.trim_start_matches('#'))
                            )
                        })
                        .unwrap_or_else(|| "<a:solidFill><a:srgbClr val=\"FFFFFF\"/></a:solidFill>".into());
                    table_rows.push_str(&format!(
                        "<a:tc><a:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr{} sz=\"1200\"/><a:t>{}</a:t></a:r></a:p></a:txBody><a:tcPr>{fill_xml}<a:lnL w=\"6350\"><a:solidFill><a:srgbClr val=\"94A3B8\"/></a:solidFill></a:lnL><a:lnR w=\"6350\"><a:solidFill><a:srgbClr val=\"94A3B8\"/></a:solidFill></a:lnR><a:lnT w=\"6350\"><a:solidFill><a:srgbClr val=\"94A3B8\"/></a:solidFill></a:lnT><a:lnB w=\"6350\"><a:solidFill><a:srgbClr val=\"94A3B8\"/></a:solidFill></a:lnB></a:tcPr></a:tc>",
                        lang_attr(export.lang.as_deref()),
                        escape_text(&text)
                    ));
                }
                table_rows.push_str("</a:tr>");
            }
            format!(
                "<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id=\"{id}\" name=\"{}\"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm>{}</p:xfrm><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/table\"><a:tbl><a:tblPr firstRow=\"1\" bandRow=\"1\"/><a:tblGrid>{grid}</a:tblGrid>{table_rows}</a:tbl></a:graphicData></a:graphic></p:graphicFrame>",
                escape_attr(&name),
                SlideWriter::transform(object.x, object.y, object.w, object.h, object.rotation)
            )
        }
        _ => {
            let preset = match object.kind.as_str() {
                "ellipse" => "ellipse",
                "roundRect" => "roundRect",
                _ => "rect",
            };
            let mut body = String::new();
            let placeholder = placeholder_xml(&object.placeholder);
            if placeholder.is_empty() {
                body.push_str(&format!(
                    "<p:nvSpPr><p:cNvPr id=\"{id}\" name=\"{}\"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>",
                    escape_attr(&name)
                ));
            } else {
                body.push_str(&format!(
                    "<p:nvSpPr><p:cNvPr id=\"{id}\" name=\"{}\"/><p:cNvSpPr/><p:nvPr>{placeholder}</p:nvPr></p:nvSpPr>",
                    escape_attr(&name)
                ));
            }
            body.push_str(&format!(
                "<p:spPr>{}<a:prstGeom prst=\"{preset}\"><a:avLst/></a:prstGeom>{}</p:spPr>",
                SlideWriter::transform(object.x, object.y, object.w, object.h, object.rotation),
                SlideWriter::fill(&stroke, &fill)
            ));
            if let Some(text) = &object.text {
                body.push_str(&SlideWriter::text_body(text, theme, export.lang.as_deref()));
            } else {
                body.push_str("<p:txBody><a:bodyPr/><a:lstStyle/><a:p/></p:txBody>");
            }
            format!("<p:sp>{body}</p:sp>")
        }
    };
    if !object.id.is_empty() {
        writer.shape_ids.insert(object.id.clone(), id);
    }
    Some(xml)
}

fn trigger_node_types(trigger: &str) -> (&'static str, &'static str) {
    match trigger {
        "withPrevious" => ("withGroup", "withEffect"),
        "afterPrevious" => ("afterGroup", "afterEffect"),
        _ => ("clickPar", "clickEffect"),
    }
}

fn timing_set_xml(next_id: &mut usize, spid: usize, visible: bool, duration: i64) -> String {
    let id = *next_id;
    *next_id += 1;
    format!(
        "<p:set><p:cBhvr><p:cTn id=\"{id}\" dur=\"{duration}\" fill=\"hold\"><p:stCondLst><p:cond delay=\"0\"/></p:stCondLst></p:cTn><p:tgtEl><p:spTgt spid=\"{spid}\"/></p:tgtEl><p:attrNameLst><p:attrName>style.visibility</p:attrName></p:attrNameLst></p:cBhvr><p:to><p:strVal val=\"{}\"/></p:to></p:set>",
        if visible { "visible" } else { "hidden" }
    )
}

fn timing_effect_xml(
    next_id: &mut usize,
    spid: usize,
    transition: Option<&str>,
    filter: &str,
    duration: i64,
) -> String {
    let id = *next_id;
    *next_id += 1;
    let transition_attr = transition.map(|value| format!(" transition=\"{value}\"")).unwrap_or_default();
    format!(
        "<p:animEffect{transition_attr} filter=\"{}\"><p:cBhvr><p:cTn id=\"{id}\" dur=\"{duration}\" fill=\"hold\"><p:stCondLst><p:cond delay=\"0\"/></p:stCondLst></p:cTn><p:tgtEl><p:spTgt spid=\"{spid}\"/></p:tgtEl></p:cBhvr></p:animEffect>",
        escape_attr(filter)
    )
}

fn timing_xml(slide: &Slide, shape_ids: &HashMap<String, usize>, warnings: &mut Vec<String>) -> String {
    let mut animations: Vec<&Animation> = slide.animations.iter().collect();
    animations.sort_by_key(|animation| animation.order);
    if animations.is_empty() {
        return String::new();
    }
    let mut next_id = 3usize;
    let mut pars = String::new();
    for animation in animations {
        let Some(spid) = shape_ids.get(&animation.object_id).copied() else {
            warnings.push(format!(
                "An animation targeting \"{}\" was not written because the object is not on the exported slide.",
                animation.object_id
            ));
            continue;
        };
        let (par_node, effect_node) = trigger_node_types(&animation.trigger);
        let duration = ((animation.duration_ms as f64) / 10.0).round().max(0.0) as i64;
        let delay = ((animation.delay_ms as f64) / 10.0).round().max(0.0) as i64;
        let mut effects = String::new();
        match animation.kind.as_str() {
            "entrance" => {
                effects.push_str(&timing_set_xml(&mut next_id, spid, true, duration));
                if animation.effect.is_empty() {
                    warnings.push("An entrance animation without an effect was written as \"appear\".".into());
                } else if animation.effect != "appear" {
                    effects.push_str(&timing_effect_xml(&mut next_id, spid, Some("in"), &animation.effect, duration));
                }
            }
            "exit" => {
                effects.push_str(&timing_set_xml(&mut next_id, spid, false, duration));
                let filter = if animation.effect.is_empty() {
                    warnings.push("An exit animation without an effect was written as \"fade\".".into());
                    "fade"
                } else {
                    animation.effect.as_str()
                };
                effects.push_str(&timing_effect_xml(&mut next_id, spid, Some("out"), filter, duration));
            }
            "emphasis" => {
                let filter = if animation.effect.is_empty() {
                    warnings.push("An emphasis animation without an effect was written as \"pulse\".".into());
                    "pulse"
                } else {
                    animation.effect.as_str()
                };
                effects.push_str(&timing_effect_xml(&mut next_id, spid, None, filter, duration));
            }
            other => {
                warnings.push(format!("An animation of kind \"{other}\" was kept in the native .oswk file and not written to the PPTX timing."));
                continue;
            }
        }
        let par_id = next_id;
        next_id += 1;
        let effect_id = next_id;
        next_id += 1;
        pars.push_str(&format!(
            "<p:par><p:cTn id=\"{par_id}\" fill=\"hold\" nodeType=\"{par_node}\"><p:stCondLst><p:cond delay=\"{delay}\"/></p:stCondLst><p:childTnLst><p:par><p:cTn id=\"{effect_id}\" fill=\"hold\" nodeType=\"{effect_node}\"><p:childTnLst>{effects}</p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:par>"
        ));
    }
    if pars.is_empty() {
        return String::new();
    }
    format!(
        "<p:timing><p:tnLst><p:par><p:cTn id=\"1\" dur=\"indefinite\" restart=\"never\" nodeType=\"tmRoot\"><p:childTnLst><p:seq concurrent=\"1\" nextAc=\"seek\"><p:cTn id=\"2\" dur=\"indefinite\" nodeType=\"mainSeq\"><p:childTnLst>{pars}</p:childTnLst></p:cTn><p:prevCondLst><p:cond evt=\"onPrev\" delay=\"0\"><p:tgtEl><p:sldTgt/></p:tgtEl></p:cond></p:prevCondLst><p:nextCondLst><p:cond evt=\"onNext\" delay=\"0\"><p:tgtEl><p:sldTgt/></p:tgtEl></p:cond></p:nextCondLst></p:seq></p:childTnLst></p:cTn></p:par></p:tnLst></p:timing>"
    )
}

fn master_xml_planned(
    planned: &PlannedMaster,
    layout_rids: &[String],
    width: f64,
    height: f64,
    footer: Option<&SlideFooter>,
    export: &mut ExportContext,
) -> String {
    let theme = theme_for(&planned.theme_name);
    let mut writer = SlideWriter::new();
    let mut shapes = String::new();
    let mut objects: Vec<SlideObject> =
        planned.master.as_ref().map(|master| master.objects.clone()).unwrap_or_default();
    // An enabled deck footer takes over the content of the footer, date and
    // slide-number placeholders; the placeholders themselves are created below
    // when the master does not carry them yet.
    let enabled_footer = footer.filter(|footer| footer.enabled);
    if let Some(footer) = enabled_footer {
        for object in &mut objects {
            if let Some(role @ ("footer" | "slideNumber" | "date")) = object.placeholder.as_deref() {
                object.text = footer_placeholder_text(role, footer);
            }
        }
    }
    let mut has_footer = false;
    let mut has_slide_number = false;
    let mut has_date = false;
    for object in &objects {
        match object.placeholder.as_deref() {
            Some("footer") => has_footer = true,
            Some("slideNumber") => has_slide_number = true,
            Some("date") => has_date = true,
            _ => {}
        }
    }
    let mut sorted: Vec<&SlideObject> = objects.iter().collect();
    sorted.sort_by_key(|object| object.z);
    writer.planned_ids = planned_shape_ids(&sorted);
    for object in &sorted {
        if let Some(xml) = object_xml(object, &theme, &mut writer, export) {
            shapes.push_str(&xml);
        }
    }
    let mut footer_shapes = String::new();
    for (role, x, y, w, h) in default_footer_placeholders(width, height) {
        let present = match role {
            "date" => has_date,
            "footer" => has_footer,
            _ => has_slide_number,
        };
        if present {
            continue;
        }
        let mut shape = placeholder_object(role, x, y, w, h);
        if let Some(footer) = enabled_footer {
            shape.text = footer_placeholder_text(role, footer);
        }
        if let Some(xml) = object_xml(&shape, &theme, &mut writer, export) {
            footer_shapes.push_str(&xml);
        }
    }
    let layout_list: String = layout_rids
        .iter()
        .enumerate()
        .map(|(index, rid)| format!("<p:sldLayoutId id=\"{}\" r:id=\"{rid}\"/>", 2147483649u64 + index as u64))
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<p:sldMaster {NS}><p:cSld><p:bg><p:bgPr><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill><a:effectLst/></p:bgPr></p:bg><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>{shapes}{footer_shapes}</p:spTree></p:cSld><p:clrMap bg1=\"lt1\" tx1=\"dk1\" bg2=\"lt2\" tx2=\"dk2\" accent1=\"accent1\" accent2=\"accent2\" accent3=\"accent3\" accent4=\"accent4\" accent5=\"accent5\" accent6=\"accent6\" hlink=\"hlink\" folHlink=\"folHlink\"/><p:sldLayoutIdLst>{layout_list}</p:sldLayoutIdLst><p:txStyles><p:titleStyle/><p:bodyStyle/><p:otherStyle/></p:txStyles></p:sldMaster>",
        escape_attr(&planned.background)
    )
}

fn placeholder_object(kind: &str, x: f64, y: f64, w: f64, h: f64) -> SlideObject {
    let mut object = SlideObject::new("text", x, y, w, h);
    object.placeholder = Some(kind.to_string());
    object
}

fn layout_xml_planned(layout: &SlideLayout, theme_name: &str, export: &mut ExportContext) -> String {
    let theme = theme_for(theme_name);
    let mut writer = SlideWriter::new();
    let mut shapes = String::new();
    let mut objects: Vec<&SlideObject> = layout.objects.iter().collect();
    objects.sort_by_key(|object| object.z);
    writer.planned_ids = planned_shape_ids(&objects);
    for object in &objects {
        if let Some(xml) = object_xml(object, &theme, &mut writer, export) {
            shapes.push_str(&xml);
        }
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<p:sldLayout {NS} type=\"{}\" preserve=\"1\"><p:cSld name=\"{}\"><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>{shapes}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>",
        escape_attr(layout_type(&layout.kind)),
        escape_attr(&layout.name)
    )
}

fn slide_xml(slide: &Slide, theme: &Theme, writer: &mut SlideWriter, export: &mut ExportContext) -> String {
    let background = slide.background.as_deref().unwrap_or(theme.lt1);
    let mut shapes = String::new();
    let mut objects: Vec<&SlideObject> = slide.objects.iter().collect();
    objects.sort_by_key(|object| object.z);
    writer.planned_ids = planned_shape_ids(&objects);
    for object in objects {
        if let Some(xml) = object_xml(object, theme, writer, export) {
            shapes.push_str(&xml);
        }
    }
    let mut background_xml = String::new();
    if slide.background.is_some() {
        background_xml = format!(
            "<p:bg><p:bgPr><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill><a:effectLst/></p:bgPr></p:bg>",
            escape_attr(background.trim_start_matches('#'))
        );
    }
    let advance = if slide.transition_ms > 0 { format!(" advTm=\"{}\"", slide.transition_ms) } else { String::new() };
    let transition = match slide.transition.as_deref() {
        Some("fade") => format!("<p:transition spd=\"med\"{advance}><p:fade/></p:transition>"),
        Some("push") => format!("<p:transition spd=\"med\"{advance}><p:push dir=\"l\"/></p:transition>"),
        Some("wipe") => format!("<p:transition spd=\"med\"{advance}><p:wipe dir=\"l\"/></p:transition>"),
        Some("slide") => format!("<p:transition spd=\"med\"{advance}><p:slide dir=\"l\"/></p:transition>"),
        _ => String::new(),
    };
    let timing = timing_xml(slide, &writer.shape_ids, &mut export.warnings);
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<p:sld {NS}><p:cSld>{background_xml}<p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>{shapes}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>{transition}{timing}</p:sld>"
    )
}

fn notes_xml(slide: &Slide, deck_lang: Option<&str>) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<p:notes {NS}><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/><p:sp><p:nvSpPr><p:cNvPr id=\"2\" name=\"Notes Placeholder\"/><p:cNvSpPr/><p:nvPr><p:ph type=\"body\" idx=\"1\"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr{}/><a:t>{}</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:notes>",
        lang_attr(deck_lang),
        escape_text(&slide.notes)
    )
}

#[derive(Debug, Clone)]
pub struct DeckWrite {
    pub bytes: Vec<u8>,
    pub warnings: Vec<String>,
}

pub fn write_pptx_package(deck: &Deck) -> OfficeResult<DeckWrite> {
    let theme = theme_for(&deck.theme);
    let empty_masters = deck.masters.is_empty();
    let background = match deck.theme.as_str() {
        "dark" => theme.lt1,
        _ => "FFFFFF",
    };
    let planned_masters = plan_masters(deck);
    let mut export = ExportContext::new(deck.lang.clone());

    let mut theme_parts: Vec<(String, String)> = Vec::new();
    if empty_masters {
        theme_parts.push(("ppt/theme/theme1.xml".into(), theme_xml(&theme)));
    } else {
        for planned in &planned_masters {
            theme_parts.push((planned.theme_part.clone(), theme_xml(&theme_for(&planned.theme_name))));
        }
    }

    let mut master_parts: Vec<(String, String, String)> = Vec::new();
    let mut layout_parts: Vec<(String, String, String)> = Vec::new();
    for planned in &planned_masters {
        let mut rels = RelSet::new();
        let mut layout_rids = Vec::new();
        for layout in &planned.layouts {
            layout_rids.push(rels.add(REL_LAYOUT, &format!("../slideLayouts/{}", file_name(&layout.part))));
        }
        rels.add(REL_THEME, &format!("../theme/{}", file_name(&planned.theme_part)));
        let master_content = match (&planned.master, empty_masters) {
            (None, true) => master_xml(
                &theme,
                background,
                deck.footer.as_ref(),
                deck.size.width_pt,
                deck.size.height_pt,
                &mut export,
            ),
            _ => master_xml_planned(
                planned,
                &layout_rids,
                deck.size.width_pt,
                deck.size.height_pt,
                deck.footer.as_ref(),
                &mut export,
            ),
        };
        master_parts.push((planned.part.clone(), master_content, rels.xml()));
        for layout in &planned.layouts {
            let layout_content = match (&layout.model, empty_masters) {
                (Some(model), false) => layout_xml_planned(model, &planned.theme_name, &mut export),
                _ => layout_xml(background),
            };
            let mut layout_rels = RelSet::new();
            layout_rels.add(REL_MASTER, &format!("../slideMasters/{}", file_name(&planned.part)));
            layout_parts.push((layout.part.clone(), layout_content, layout_rels.xml()));
        }
    }

    let mut parts: Vec<(String, String, Option<String>)> = Vec::new();
    for (index, slide) in deck.slides.iter().enumerate() {
        let mut writer = SlideWriter::new();
        let planned_layout = layout_for_slide(slide, &planned_masters);
        if let Some(layout) = planned_layout {
            writer.rels.add(REL_LAYOUT, &format!("../slideLayouts/{}", file_name(&layout.part)));
        }
        let slide_theme = theme_for(&planned_theme_name(planned_layout, deck));
        let xml = slide_xml(slide, &slide_theme, &mut writer, &mut export);
        let mut rels = writer.rels;
        let mut notes_part = None;
        if !slide.notes.trim().is_empty() {
            rels.add(REL_NOTES, &format!("../notesSlides/notesSlide{}.xml", index + 1));
            notes_part = Some(notes_xml(slide, deck.lang.as_deref()));
        }
        parts.push((xml, rels.xml(), notes_part));
    }

    let (presentation_rels, slide_rids, master_rids) = {
        let mut rels = RelSet::new();
        let mut master_rids = Vec::new();
        for planned in &planned_masters {
            let target = planned.part.strip_prefix("ppt/").unwrap_or(&planned.part);
            master_rids.push(rels.add(REL_MASTER, target));
        }
        let mut slide_rids = Vec::new();
        for index in 0..parts.len() {
            slide_rids.push(rels.add(REL_SLIDE, &format!("slides/slide{}.xml", index + 1)));
        }
        rels.add("http://schemas.openxmlformats.org/officeDocument/2006/relationships/presProps", "presProps.xml");
        rels.add("http://schemas.openxmlformats.org/officeDocument/2006/relationships/viewProps", "viewProps.xml");
        rels.add("http://schemas.openxmlformats.org/officeDocument/2006/relationships/tableStyles", "tableStyles.xml");
        if let Some((first_theme, _)) = theme_parts.first() {
            rels.add(REL_THEME, &format!("theme/{}", file_name(first_theme)));
        }
        (rels.xml(), slide_rids, master_rids)
    };

    let mut zip = ZipWriter::new();
    let mut content_types = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/>",
    );
    for (extension, mime) in [
        ("png", "image/png"),
        ("jpg", "image/jpeg"),
        ("jpeg", "image/jpeg"),
        ("gif", "image/gif"),
        ("bmp", "image/bmp"),
        ("webp", "image/webp"),
    ] {
        content_types.push_str(&format!("<Default Extension=\"{extension}\" ContentType=\"{mime}\"/>"));
    }
    if export.charts.iter().any(|chart| chart.embedding.is_some()) {
        content_types.push_str("<Default Extension=\"xlsx\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet\"/>");
    }
    content_types.push_str("<Override PartName=\"/ppt/presentation.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml\"/>");
    for (part, _) in &theme_parts {
        content_types.push_str(&format!(
            "<Override PartName=\"/{part}\" ContentType=\"application/vnd.openxmlformats-officedocument.theme+xml\"/>"
        ));
    }
    for (part, _, _) in &master_parts {
        content_types.push_str(&format!("<Override PartName=\"/{part}\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml\"/>"));
    }
    for (part, _, _) in &layout_parts {
        content_types.push_str(&format!("<Override PartName=\"/{part}\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml\"/>"));
    }
    content_types.push_str("<Override PartName=\"/ppt/presProps.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.presProps+xml\"/>");
    content_types.push_str("<Override PartName=\"/ppt/viewProps.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.viewProps+xml\"/>");
    content_types.push_str("<Override PartName=\"/ppt/tableStyles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.tableStyles+xml\"/>");
    for (index, part) in parts.iter().enumerate() {
        content_types.push_str(&format!("<Override PartName=\"/ppt/slides/slide{}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/>", index + 1));
        if part.2.is_some() {
            content_types.push_str(&format!("<Override PartName=\"/ppt/notesSlides/notesSlide{}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml\"/>", index + 1));
        }
    }
    for chart in &export.charts {
        content_types.push_str(&format!(
            "<Override PartName=\"/{}\" ContentType=\"application/vnd.openxmlformats-officedocument.drawingml.chart+xml\"/>",
            chart.part
        ));
    }
    content_types.push_str("<Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/><Override PartName=\"/docProps/app.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.extended-properties+xml\"/></Types>");

    let mut root_rels = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">");
    root_rels.push_str("<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"ppt/presentation.xml\"/>");
    root_rels.push_str("<Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/>");
    root_rels.push_str("<Relationship Id=\"rId3\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties\" Target=\"docProps/app.xml\"/>");
    root_rels.push_str("</Relationships>");

    zip.add_text("[Content_Types].xml", &content_types);
    zip.add_text("_rels/.rels", &root_rels);
    zip.add_text("docProps/core.xml", &core_properties(deck));
    zip.add_text("docProps/app.xml", "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\"><Application>OmniOffice</Application></Properties>");
    zip.add_text("ppt/presentation.xml", &presentation_xml(deck, &master_rids, &slide_rids));
    zip.add_text("ppt/_rels/presentation.xml.rels", &presentation_rels);
    zip.add_text("ppt/presProps.xml", "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<p:presentationPr xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"/>");
    zip.add_text("ppt/viewProps.xml", "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<p:viewPr xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"/>");
    zip.add_text("ppt/tableStyles.xml", "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<a:tblStyleLst xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" def=\"mediumStyle2Accent1\"/>");
    for (part, xml) in &theme_parts {
        zip.add_text(part, xml);
    }
    for (part, xml, rels) in &master_parts {
        zip.add_text(part, xml);
        zip.add_text(&rels_name(part), rels);
    }
    for (part, xml, rels) in &layout_parts {
        zip.add_text(part, xml);
        zip.add_text(&rels_name(part), rels);
    }
    for (index, (xml, rels, notes)) in parts.iter().enumerate() {
        zip.add_text(&format!("ppt/slides/slide{}.xml", index + 1), xml);
        zip.add_text(&format!("ppt/slides/_rels/slide{}.xml.rels", index + 1), rels);
        if let Some(notes) = notes {
            zip.add_text(&format!("ppt/notesSlides/notesSlide{}.xml", index + 1), notes);
            zip.add_text(&format!("ppt/notesSlides/_rels/notesSlide{}.xml.rels", index + 1), &format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide\" Target=\"../slides/slide{}.xml\"/></Relationships>",
                index + 1
            ));
        }
    }
    for (name, data) in &export.media {
        zip.add(&format!("ppt/media/{name}"), data);
    }
    for chart in &export.charts {
        zip.add_text(&chart.part, &chart.xml);
        if !chart.rels.is_empty() {
            zip.add_text(&rels_name(&chart.part), &chart.rels);
        }
        if let Some((path, bytes)) = &chart.embedding {
            zip.add(path, bytes);
        }
    }
    let mut warnings = export.warnings;
    warnings.sort();
    warnings.dedup();
    Ok(DeckWrite { bytes: zip.finish(), warnings })
}

fn core_properties(deck: &Deck) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:title>{}</dc:title><dc:creator>{}</dc:creator></cp:coreProperties>",
        escape_text(&deck.title),
        escape_text(&deck.metadata.author)
    )
}

pub fn write_pptx(deck: &Deck) -> OfficeResult<Vec<u8>> {
    Ok(write_pptx_package(deck)?.bytes)
}

pub fn write_pptx_file(path: &Path, deck: &Deck) -> OfficeResult<()> {
    crate::io::write_atomic(path, &write_pptx(deck)?)
}
