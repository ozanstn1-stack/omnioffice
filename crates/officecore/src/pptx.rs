//! PresentationML (PPTX) import and export.
//!
//! The writer emits a complete, valid package (masters, layouts, themes,
//! slides, notes, media, ChartML and animation timing) so PowerPoint,
//! LibreOffice and OnlyOffice open it natively. The reader extracts text
//! boxes, pictures, shapes, groups, charts, tables, placeholders, animations,
//! notes and slide size, warning whenever something cannot be represented.

use crate::error::{OfficeError, OfficeResult};
use crate::model::*;
use crate::xml::{escape_attr, escape_text, parse_xml, XmlNode};
use crate::zip::{ZipReader, ZipWriter};
use std::collections::HashMap;
use std::path::Path;

const NS: &str = concat!(
    "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" ",
    "xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" ",
    "xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\""
);

fn emu(points: f64) -> i64 {
    (points * 12700.0).round() as i64
}

fn pt_from_emu(value: f64) -> f64 {
    value / 12700.0
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
        out.push_str(&format!("<p:sldId id=\"{next_id}\" r:id=\"{rid}\"/>"));
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

fn master_xml(theme: &Theme, background: &str) -> String {
    let _ = theme;
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<p:sldMaster {NS}><p:cSld><p:bg><p:bgPr><a:solidFill><a:srgbClr val=\"{background}\"/></a:solidFill><a:effectLst/></p:bgPr></p:bg><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/></p:spTree></p:cSld><p:clrMap bg1=\"lt1\" tx1=\"dk1\" bg2=\"lt2\" tx2=\"dk2\" accent1=\"accent1\" accent2=\"accent2\" accent3=\"accent3\" accent4=\"accent4\" accent5=\"accent5\" accent6=\"accent6\" hlink=\"hlink\" folHlink=\"folHlink\"/><p:sldLayoutIdLst><p:sldLayoutId id=\"2147483649\" r:id=\"rId1\"/></p:sldLayoutIdLst><p:txStyles><p:titleStyle/><p:bodyStyle/><p:otherStyle/></p:txStyles></p:sldMaster>"
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

fn part_stem(part: &str) -> String {
    file_name(part).split('.').next().unwrap_or(part).to_string()
}

fn placeholder_role(kind: &str) -> Option<&'static str> {
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

fn layout_kind(kind: &str) -> String {
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
}

impl SlideWriter {
    fn new() -> Self {
        Self { rels: RelSet::new(), next_shape: 1, shape_ids: HashMap::new() }
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
                    if let Some(color) = run.color.as_deref().or(paragraph.color.as_deref()).or(text.color.as_deref()) {
                        attributes.push_str(&format!(
                            "><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill>",
                            escape_attr(color.trim_start_matches('#'))
                        ));
                    } else {
                        attributes.push('>');
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
            format!(
                "<p:pic><p:nvPicPr><p:cNvPr id=\"{id}\" name=\"{}\"/><p:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed=\"{rid}\"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr>{}</p:spPr></p:pic>",
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
            format!(
                "<p:cxnSp><p:nvCxnSpPr><p:cNvPr id=\"{id}\" name=\"{}\"/><p:cNvCxnSpPr/><p:nvPr/></p:nvCxnSpPr><p:spPr><a:xfrm{flips}><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm><a:prstGeom prst=\"line\"><a:avLst/></a:prstGeom><a:ln w=\"{}\"><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill><a:prstDash val=\"{}\"/>{ends}</a:ln></p:spPr></p:cxnSp>",
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
    export: &mut ExportContext,
) -> String {
    let theme = theme_for(&planned.theme_name);
    let mut writer = SlideWriter::new();
    let mut shapes = String::new();
    let objects: Vec<&SlideObject> =
        planned.master.as_ref().map(|master| master.objects.iter().collect()).unwrap_or_default();
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
    let mut sorted = objects;
    sorted.sort_by_key(|object| object.z);
    for object in &sorted {
        if let Some(xml) = object_xml(object, &theme, &mut writer, export) {
            shapes.push_str(&xml);
        }
    }
    let mut footer_shapes = String::new();
    let bottom = (height - 44.0).max(0.0);
    if !has_date {
        let shape = placeholder_object("date", 40.0, bottom, 200.0, 28.0);
        if let Some(xml) = object_xml(&shape, &theme, &mut writer, export) {
            footer_shapes.push_str(&xml);
        }
    }
    if !has_footer {
        let shape = placeholder_object("footer", (width * 0.25).max(40.0), bottom, (width * 0.5).max(160.0), 28.0);
        if let Some(xml) = object_xml(&shape, &theme, &mut writer, export) {
            footer_shapes.push_str(&xml);
        }
    }
    if !has_slide_number {
        let shape = placeholder_object("slideNumber", (width - 120.0).max(40.0), bottom, 80.0, 28.0);
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
    let transition = match slide.transition.as_deref() {
        Some("fade") => "<p:transition spd=\"med\"><p:fade/></p:transition>".to_string(),
        Some("push") => "<p:transition spd=\"med\"><p:push dir=\"l\"/></p:transition>".to_string(),
        Some("wipe") => "<p:transition spd=\"med\"><p:wipe dir=\"l\"/></p:transition>".to_string(),
        Some("slide") => "<p:transition spd=\"med\"><p:slide dir=\"l\"/></p:transition>".to_string(),
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
            (None, true) => master_xml(&theme, background),
            _ => master_xml_planned(planned, &layout_rids, deck.size.width_pt, deck.size.height_pt, &mut export),
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
            object.image = Some(ImageData::from_bytes(&name, &data));
            object
        }
        "cxnSp" => {
            let shape_props = node.find_descendant("spPr")?;
            let transform = shape_props.find_descendant("xfrm");
            let (x, y, w, h, _) = read_transform(transform);
            let mut object = SlideObject::new("line", x, y, w, h);
            object.z = z;
            object.line = Some(LineSpec {
                x2: w,
                y2: h,
                end_arrow: node.find_descendant("tailEnd").is_some(),
                begin_arrow: node.find_descendant("headEnd").is_some(),
                dash: String::new(),
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
        let mut size: Option<f64> = None;
        let mut color: Option<String> = None;
        for child in &paragraph.children {
            if child.local_name() != "r" && child.local_name() != "fld" {
                continue;
            }
            let run_properties = child.child("rPr");
            let run_bold = run_properties.and_then(|node| node.attr("b")).map(|value| value == "1").unwrap_or(false);
            let run_italic = run_properties.and_then(|node| node.attr("i")).map(|value| value == "1").unwrap_or(false);
            let run_size = run_properties
                .and_then(|node| node.attr("sz"))
                .and_then(|value| value.parse::<f64>().ok())
                .map(|value| value / 100.0);
            let run_color = run_properties
                .and_then(|node| node.find_descendant("srgbClr"))
                .and_then(|color| color.attr("val"))
                .map(|value| format!("#{value}"));
            let run_lang = run_properties.and_then(|node| node.attr("lang")).and_then(known_lang);
            bold |= run_bold;
            italic |= run_italic;
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
                color: run_color,
                size_pt: run_size,
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
            underline: false,
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

fn read_masters(
    reader: &ZipReader,
    presentation_rels: &[(String, String, String)],
    warnings: &mut Vec<String>,
) -> (Vec<SlideMaster>, HashMap<String, (usize, String)>) {
    let mut masters: Vec<SlideMaster> = Vec::new();
    let mut layout_index: HashMap<String, (usize, String)> = HashMap::new();
    let mut failed = false;
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
        return (Vec::new(), HashMap::new());
    }
    (masters, layout_index)
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
    let (masters, layout_index) = read_masters(&reader, &typed_rels, &mut warnings);
    deck.masters = masters;
    deck.slides.clear();
    let mut slide_parts: Vec<String> = Vec::new();
    let mut slide_ids = Vec::new();
    root.find_all("sldId", &mut slide_ids);
    for node in slide_ids {
        if let Some(rid) = node.attr("r:id").or_else(|| node.attr_any_ns("id")) {
            if let Some(target) = rels.get(rid) {
                slide_parts.push(resolve_part("ppt/presentation.xml", target));
            }
        }
    }
    for part in &slide_parts {
        let Ok(text) = reader.read_text(part) else { continue };
        let Ok(slide_root) = parse_xml(&text) else { continue };
        let mut slide = Slide::default();
        slide.objects.clear();
        if let Some(background) = slide_root.find_descendant("bgPr") {
            if let Some(color) = background.find_descendant("srgbClr").and_then(|color| color.attr("val")) {
                slide.background = Some(format!("#{color}"));
            }
        }
        if let Some(transition) = slide_root.find_descendant("transition") {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_deck() -> Deck {
        let mut deck = Deck::new_blank("Sample deck");
        deck.theme = "business".into();
        let mut slide = Slide::default();
        let mut title = SlideObject::new("text", 60.0, 60.0, 600.0, 100.0);
        title.text = Some(TextFrame {
            paragraphs: vec![TextParagraph {
                text: "Başlık slaytı".into(),
                size_pt: Some(32.0),
                bold: true,
                ..Default::default()
            }],
            ..Default::default()
        });
        let mut rect = SlideObject::new("rect", 60.0, 200.0, 320.0, 160.0);
        rect.style = Some(ShapeStyle { fill: Some("#1D4ED8".into()), ..Default::default() });
        rect.text = Some(TextFrame {
            paragraphs: vec![TextParagraph {
                text: "Kutu".into(),
                color: Some("#FFFFFF".into()),
                ..Default::default()
            }],
            ..Default::default()
        });
        slide.objects = vec![title, rect];
        slide.notes = "Notlar burada".into();
        slide.transition = Some("fade".into());
        let mut second = Slide::default();
        let mut table = SlideObject::new("table", 60.0, 120.0, 500.0, 200.0);
        table.table = Some(TableData::simple(2, 3, 500.0));
        second.objects = vec![table];
        deck.slides = vec![slide, second];
        deck
    }

    #[test]
    fn pptx_package_structure() {
        let result = write_pptx_package(&sample_deck()).unwrap();
        let reader = ZipReader::open(result.bytes.clone()).unwrap();
        for part in [
            "[Content_Types].xml",
            "ppt/presentation.xml",
            "ppt/slideMasters/slideMaster1.xml",
            "ppt/slideLayouts/slideLayout1.xml",
            "ppt/theme/theme1.xml",
            "ppt/slides/slide1.xml",
            "ppt/slides/slide2.xml",
            "ppt/slides/_rels/slide1.xml.rels",
            "ppt/notesSlides/notesSlide1.xml",
        ] {
            assert!(reader.contains(part), "missing {part}");
        }
        let presentation = reader.read_text("ppt/presentation.xml").unwrap();
        assert!(presentation.contains("sldIdLst"));
        assert!(presentation.contains("sldSz"));
    }

    #[test]
    fn pptx_roundtrip() {
        let bytes = write_pptx(&sample_deck()).unwrap();
        let read = read_pptx(&bytes).unwrap();
        assert_eq!(read.deck.slides.len(), 2);
        let texts: Vec<String> = read.deck.slides[0]
            .objects
            .iter()
            .filter_map(|object| object.text.as_ref().map(TextFrame::plain))
            .collect();
        assert!(texts.iter().any(|text| text.contains("Başlık slaytı")), "texts: {texts:?}");
        assert!(read.deck.slides[0].notes.contains("Notlar"));
        assert_eq!(read.deck.size.width_pt.round() as i64, 960);
    }

    fn bullet_deck() -> Deck {
        let mut deck = Deck::new_blank("Bullets");
        let mut slide = Slide::default();
        let mut body = SlideObject::new("text", 60.0, 60.0, 600.0, 300.0);
        body.text = Some(TextFrame {
            paragraphs: vec![
                TextParagraph { text: "Top".into(), bullet: true, ..Default::default() },
                TextParagraph { text: "Nested".into(), bullet: true, level: 2, ..Default::default() },
                TextParagraph { text: "Plain".into(), ..Default::default() },
            ],
            ..Default::default()
        });
        slide.objects = vec![body];
        deck.slides = vec![slide];
        deck
    }

    #[test]
    fn pptx_bullet_is_a_real_bullet_and_level_roundtrips() {
        let result = write_pptx_package(&bullet_deck()).unwrap();
        let reader = ZipReader::open(result.bytes.clone()).unwrap();
        let slide_xml = reader.read_text("ppt/slides/slide1.xml").unwrap();
        assert!(slide_xml.contains("<a:buChar char=\"\u{2022}\"/>"), "bullet glyph: {slide_xml}");
        assert!(!slide_xml.contains('\u{00C3}') && !slide_xml.contains('\u{00E2}'), "mojibake in slide xml");

        let read = read_pptx(&result.bytes).unwrap();
        let paragraphs = &read.deck.slides[0].objects[0].text.as_ref().unwrap().paragraphs;
        assert_eq!(paragraphs.len(), 3);
        assert!(paragraphs[0].bullet && paragraphs[0].level == 0);
        assert!(paragraphs[1].bullet && paragraphs[1].level == 2);
        assert!(!paragraphs[2].bullet);
    }

    fn lang_deck(deck_lang: Option<&str>, run_lang: Option<&str>) -> Deck {
        let mut deck = Deck::new_blank("Lang");
        deck.lang = deck_lang.map(str::to_string);
        let mut slide = Slide::default();
        let mut body = SlideObject::new("text", 60.0, 60.0, 600.0, 300.0);
        body.text = Some(TextFrame {
            paragraphs: vec![
                TextParagraph {
                    text: "Hello".into(),
                    runs: vec![Run { text: "Hello".into(), lang: run_lang.map(str::to_string), ..Default::default() }],
                    ..Default::default()
                },
                TextParagraph { text: "Bare".into(), ..Default::default() },
            ],
            ..Default::default()
        });
        slide.objects = vec![body];
        slide.notes = "Note".into();
        let mut table = SlideObject::new("table", 60.0, 400.0, 400.0, 100.0);
        table.table = Some(TableData::simple(1, 1, 400.0));
        slide.objects.push(table);
        deck.slides = vec![slide];
        deck
    }

    fn lang_parts(deck: &Deck) -> (String, String, String) {
        let result = write_pptx_package(deck).unwrap();
        let reader = ZipReader::open(result.bytes).unwrap();
        (
            reader.read_text("ppt/slides/slide1.xml").unwrap(),
            reader.read_text("ppt/notesSlides/notesSlide1.xml").unwrap(),
            reader.read_text("ppt/presentation.xml").unwrap(),
        )
    }

    #[test]
    fn pptx_writes_no_language_when_unknown() {
        let (slide, notes, presentation) = lang_parts(&lang_deck(None, None));
        assert!(!slide.contains("lang="), "slide: {slide}");
        assert!(!notes.contains("lang="), "notes: {notes}");
        assert!(!presentation.contains("lang="), "presentation: {presentation}");
    }

    #[test]
    fn pptx_uses_run_and_deck_language() {
        let (slide, notes, presentation) = lang_parts(&lang_deck(Some("en-GB"), Some("de-DE")));
        // The run keeps its own language, the paragraph without runs, the
        // table cell and the notes fall back to the deck language.
        assert_eq!(slide.matches("lang=\"de-DE\"").count(), 1, "slide: {slide}");
        assert_eq!(slide.matches("lang=\"en-GB\"").count(), 2, "slide: {slide}");
        assert!(notes.contains("lang=\"en-GB\""));
        assert!(presentation.contains("<a:defRPr lang=\"en-GB\"/>"), "presentation: {presentation}");
        assert!(!slide.contains("tr-TR") && !notes.contains("tr-TR"));
    }

    #[test]
    fn pptx_language_roundtrips() {
        let bytes = write_pptx(&lang_deck(Some("en-GB"), Some("de-DE"))).unwrap();
        let read = read_pptx(&bytes).unwrap();
        assert_eq!(read.deck.lang.as_deref(), Some("en-GB"));
        let paragraphs = &read.deck.slides[0].objects[0].text.as_ref().unwrap().paragraphs;
        assert_eq!(paragraphs[0].runs[0].lang.as_deref(), Some("de-DE"));
        assert_eq!(paragraphs[0].lang.as_deref(), Some("de-DE"));
        // The paragraph without runs was written with the deck language.
        assert_eq!(paragraphs[1].lang.as_deref(), Some("en-GB"));

        let unknown = read_pptx(&write_pptx(&lang_deck(None, None)).unwrap()).unwrap();
        assert_eq!(unknown.deck.lang, None);
    }

    fn runs_deck(paragraphs: Vec<TextParagraph>) -> Deck {
        let mut deck = Deck::new_blank("Edited");
        let mut slide = Slide::default();
        let mut body = SlideObject::new("text", 60.0, 60.0, 600.0, 300.0);
        body.text = Some(TextFrame { paragraphs, ..Default::default() });
        slide.objects = vec![body];
        deck.slides = vec![slide];
        deck
    }

    fn formatted(text: &str, level: u32) -> TextParagraph {
        TextParagraph {
            text: text.into(),
            level,
            bullet: true,
            runs: vec![Run { text: text.into(), bold: true, ..Default::default() }],
            ..Default::default()
        }
    }

    fn exported_paragraphs(deck: &Deck) -> (String, Vec<TextParagraph>) {
        let bytes = write_pptx(deck).unwrap();
        let slide_xml = ZipReader::open(bytes.clone()).unwrap().read_text("ppt/slides/slide1.xml").unwrap();
        let read = read_pptx(&bytes).unwrap();
        (slide_xml, read.deck.slides[0].objects[0].text.clone().unwrap().paragraphs)
    }

    #[test]
    fn pptx_edited_paragraph_exports_its_new_text() {
        // An imported slide: both paragraphs carry runs. The editor changes
        // the second paragraph and clears its runs, the first one is kept.
        let (_, imported) = exported_paragraphs(&runs_deck(vec![formatted("Title", 0), formatted("Old detail", 1)]));
        let mut edited = imported.clone();
        edited[1].text = "New detail".into();
        edited[1].runs.clear();

        let (slide_xml, paragraphs) = exported_paragraphs(&runs_deck(edited));
        assert!(slide_xml.contains("New detail"), "slide: {slide_xml}");
        assert!(!slide_xml.contains("Old detail"), "slide: {slide_xml}");
        assert_eq!(paragraphs.len(), 2);
        assert_eq!(paragraphs[0].text, "Title");
        assert_eq!(paragraphs[0].runs.len(), 1);
        assert!(paragraphs[0].runs[0].bold);
        assert_eq!(paragraphs[1].text, "New detail");
        assert!(paragraphs[1].bullet && paragraphs[1].level == 1);
    }

    #[test]
    fn pptx_never_writes_runs_that_contradict_the_paragraph_text() {
        // Decks saved by 4.2.0 can hold a changed paragraph text next to the
        // runs of the old text; the paragraph text is what the user sees.
        let mut stale = formatted("Old detail", 1);
        stale.text = "New detail".into();
        let (slide_xml, paragraphs) = exported_paragraphs(&runs_deck(vec![stale]));
        assert!(slide_xml.contains("New detail"), "slide: {slide_xml}");
        assert!(!slide_xml.contains("Old detail"), "slide: {slide_xml}");
        assert_eq!(paragraphs[0].text, "New detail");
        assert!(paragraphs[0].bullet && paragraphs[0].level == 1);
    }

    #[test]
    fn pptx_roundtrip_image() {
        let mut buffer = image::RgbaImage::new(4, 4);
        for pixel in buffer.pixels_mut() {
            *pixel = image::Rgba([10, 200, 90, 255]);
        }
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(buffer).write_to(&mut png, image::ImageFormat::Png).unwrap();
        let mut deck = Deck::new_blank("Images");
        let mut slide = Slide::default();
        let mut object = SlideObject::new("image", 40.0, 40.0, 200.0, 150.0);
        object.image = Some(ImageData::from_bytes("pic.png", &png.into_inner()));
        slide.objects = vec![object];
        deck.slides = vec![slide];
        let bytes = write_pptx(&deck).unwrap();
        let read = read_pptx(&bytes).unwrap();
        let image = read.deck.slides[0].objects.iter().find_map(|object| object.image.clone());
        assert!(image.map(|image| !image.data_base64.is_empty()).unwrap_or(false));
    }

    #[test]
    fn malformed_inputs_are_errors() {
        assert!(read_pptx(b"").is_err());
        assert!(read_pptx(&[3u8; 40]).is_err());
    }
}
