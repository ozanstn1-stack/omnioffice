//! DOCX reader: relationships, styles, tables, notes, fields and warnings.
//! The writer lives in `writer`; the public entry points are re-exported by `mod.rs`.

use super::*;

#[derive(Debug, Clone)]
pub struct DocxRead {
    pub document: TextDocument,
    pub warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Numbering {
    /// numId -> abstractNumId
    num_to_abstract: HashMap<u32, u32>,
    /// (abstractNumId, level) -> (kind, start)
    levels: HashMap<(u32, u32), (String, u32)>,
}

#[derive(Clone)]
struct RelLink {
    target: String,
    external: bool,
}

struct PartContext {
    rels: HashMap<String, RelLink>,
    warnings: Vec<String>,
    /// True while the cached result of a PAGE/NUMPAGES field is being skipped.
    field_skip: bool,
    /// Numeric revision counter while `w:ins` / `w:del` wrappers are read.
    revision_seq: u32,
    /// Revision applied to runs read inside the current `w:ins` / `w:del`.
    pending_revision: Option<RevisionMark>,
    /// Comment id opened by the current `w:commentRangeStart`.
    pending_comment: Option<String>,
    /// A non page-number field being captured (`REF`, `DATE`, ...).
    field_capture: Option<FieldRef>,
}

impl PartContext {
    fn new(rels: HashMap<String, RelLink>) -> Self {
        Self {
            rels,
            warnings: Vec::new(),
            field_skip: false,
            revision_seq: 0,
            pending_revision: None,
            pending_comment: None,
            field_capture: None,
        }
    }
}

fn read_relationships(reader: &ZipReader, part: &str) -> HashMap<String, RelLink> {
    let (dir, file) = match part.rsplit_once('/') {
        Some((dir, file)) => (format!("{dir}/"), file.to_string()),
        None => (String::new(), part.to_string()),
    };
    let rels_name = format!("{dir}_rels/{file}.rels");
    let mut map = HashMap::new();
    let Ok(text) = reader.read_text(&rels_name) else {
        return map;
    };
    let Ok(root) = parse_xml(&text) else {
        return map;
    };
    for node in root.children_named("Relationship") {
        let Some(id) = node.attr("Id") else { continue };
        let Some(target) = node.attr("Target") else { continue };
        let external = node.attr("TargetMode").map(|value| value.eq_ignore_ascii_case("external")).unwrap_or(false);
        map.insert(id.to_string(), RelLink { target: target.to_string(), external });
    }
    map
}

fn resolve_media_path(target: &str) -> String {
    if target.starts_with('/') {
        return target.trim_start_matches('/').to_string();
    }
    if target.contains("://") {
        return String::new();
    }
    format!("word/{}", target.trim_start_matches("./"))
}

fn read_numbering(reader: &ZipReader) -> Numbering {
    let mut numbering = Numbering::default();
    let Ok(text) = reader.read_text("word/numbering.xml") else {
        return numbering;
    };
    let Ok(root) = parse_xml(&text) else {
        return numbering;
    };
    for node in root.children_named("abstractNum") {
        let Some(abstract_id) = node.attr_any_ns("abstractNumId").and_then(|value| value.parse::<u32>().ok()) else {
            continue;
        };
        for level in node.children_named("lvl") {
            let lvl = level.attr_any_ns("ilvl").and_then(|value| value.parse::<u32>().ok()).unwrap_or(0);
            let format = level.child("numFmt").and_then(|node| node.attr_any_ns("val")).unwrap_or("bullet").to_string();
            let start = level
                .child("start")
                .and_then(|node| node.attr_any_ns("val"))
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(1);
            let kind = if format == "bullet" { "bullet" } else { "number" };
            numbering.levels.insert((abstract_id, lvl), (kind.to_string(), start));
        }
    }
    for node in root.children_named("num") {
        let Some(num_id) = node.attr_any_ns("numId").and_then(|value| value.parse::<u32>().ok()) else { continue };
        if let Some(abstract_id) = node
            .child("abstractNumId")
            .and_then(|node| node.attr_any_ns("val"))
            .and_then(|value| value.parse::<u32>().ok())
        {
            numbering.num_to_abstract.insert(num_id, abstract_id);
        }
    }
    numbering
}

fn read_styles(reader: &ZipReader) -> Vec<ParaStyle> {
    let mut styles = default_styles();
    let Ok(text) = reader.read_text("word/styles.xml") else {
        return styles;
    };
    let Ok(root) = parse_xml(&text) else {
        return styles;
    };
    for node in root.children_named("style") {
        if node.attr_any_ns("type").map(|value| value != "paragraph").unwrap_or(false) {
            continue;
        }
        let Some(id) = node.attr_any_ns("styleId") else { continue };
        let existing = styles.iter().position(|style| style.id == id);
        let mut style = existing.map(|index| styles[index].clone()).unwrap_or_default();
        style.id = id.to_string();
        style.name = node.child("name").and_then(|node| node.attr_any_ns("val")).unwrap_or(id).to_string();
        style.based_on = node.child("basedOn").and_then(|node| node.attr_any_ns("val")).map(str::to_string);
        style.next = node.child("next").and_then(|node| node.attr_any_ns("val")).map(str::to_string);
        if let Some(properties) = node.child("pPr") {
            if let Some(value) = properties.child("jc").and_then(|node| node.attr_any_ns("val")) {
                style.align = Some(match value {
                    "center" => "center".into(),
                    "right" => "right".into(),
                    "both" => "justify".into(),
                    _ => "left".into(),
                });
            }
            if let Some(spacing) = properties.child("spacing") {
                if let Some(value) = spacing.attr_any_ns("line").and_then(parse_f64) {
                    if value > 1.0 {
                        style.line_spacing = Some(value / 240.0);
                    }
                }
                if let Some(value) = spacing.attr_any_ns("before").and_then(parse_f64) {
                    style.space_before_pt = Some(value / 20.0);
                }
                if let Some(value) = spacing.attr_any_ns("after").and_then(parse_f64) {
                    style.space_after_pt = Some(value / 20.0);
                }
            }
            if let Some(value) = properties
                .child("outlineLvl")
                .and_then(|node| node.attr_any_ns("val"))
                .and_then(|value| value.parse::<u32>().ok())
            {
                style.outline_level = Some(value);
            }
            if properties.child("keepNext").is_some() {
                style.keep_with_next = Some(true);
            }
        }
        if let Some(properties) = node.child("rPr") {
            if let Some(font) = properties.child("rFonts").and_then(|node| node.attr_any_ns("ascii")) {
                style.font = Some(font.to_string());
            }
            if properties.child("b").is_some() {
                style.bold = Some(true);
            }
            if properties.child("i").is_some() {
                style.italic = Some(true);
            }
            if properties.child("u").is_some() {
                style.underline = Some(true);
            }
            if let Some(color) = properties.child("color").and_then(|node| node.attr_any_ns("val")) {
                if !color.eq_ignore_ascii_case("auto") {
                    style.color = normalize_hex(color);
                }
            }
            if let Some(size) = properties.child("sz").and_then(|node| node.attr_any_ns("val")).and_then(parse_f64) {
                style.size_pt = Some(size / 2.0);
            }
        }
        match existing {
            Some(index) => styles[index] = style,
            None => styles.push(style),
        }
    }
    styles
}

fn parse_f64(value: &str) -> Option<f64> {
    value.trim().parse::<f64>().ok()
}

fn parse_bool_flag(node: Option<&XmlNode>) -> bool {
    match node {
        Some(node) => !matches!(node.attr_any_ns("val"), Some("0") | Some("false") | Some("off")),
        None => false,
    }
}

pub(crate) fn read_run_properties(node: Option<&XmlNode>) -> Run {
    let mut run = Run::default();
    let Some(properties) = node else {
        return run;
    };
    if let Some(fonts) = properties.child("rFonts") {
        if let Some(font) = fonts.attr_any_ns("ascii") {
            run.font = Some(font.to_string());
        }
    }
    // `w:lang w:val="tr-TR"` carries the run's language; the Writer uses it to
    // hyphenate Turkish text.
    if let Some(lang) = properties.child("lang").and_then(|node| node.attr_any_ns("val")) {
        if !lang.is_empty() {
            run.lang = Some(lang.to_string());
        }
    }
    run.bold = parse_bool_flag(properties.child("b"));
    run.italic = parse_bool_flag(properties.child("i"));
    run.underline = properties.child("u").is_some();
    run.strike = parse_bool_flag(properties.child("strike"));
    if let Some(color) = properties.child("color").and_then(|node| node.attr_any_ns("val")) {
        if !color.eq_ignore_ascii_case("auto") {
            run.color = normalize_hex(color);
        }
    }
    if let Some(highlight) = properties.child("highlight").and_then(|node| node.attr_any_ns("val")) {
        let normalized = match highlight {
            "yellow" => "#FFFF00",
            "green" => "#00FF00",
            "cyan" => "#00FFFF",
            "magenta" => "#FF00FF",
            "blue" => "#0000FF",
            "red" => "#FF0000",
            "darkBlue" => "#000080",
            "darkGreen" => "#008000",
            "darkRed" => "#800000",
            "darkYellow" => "#808000",
            "gray" => "#808080",
            "lightGray" => "#C0C0C0",
            "black" => "#000000",
            "white" => "#FFFFFF",
            "darkGray" => "#A9A9A9",
            _ => "",
        };
        if !normalized.is_empty() {
            run.highlight = Some(normalized.into());
        }
    }
    if let Some(size) = properties.child("sz").and_then(|node| node.attr_any_ns("val")).and_then(parse_f64) {
        run.size_pt = Some(size / 2.0);
    }
    if let Some(align) = properties.child("vertAlign").and_then(|node| node.attr_any_ns("val")) {
        if align == "superscript" {
            run.superscript = true;
        } else if align == "subscript" {
            run.subscript = true;
        }
    }
    // Tracked formatting change: `w:rPrChange` carries the formatting the run
    // had before the change.
    if let Some(change) = properties.child("rPrChange") {
        let mut original = Run::default();
        if let Some(inner) = change.child("rPr") {
            original = read_run_properties(Some(inner));
        }
        let author = change.attr_any_ns("author").unwrap_or("Unknown").to_string();
        let date = change.attr_any_ns("date").unwrap_or_default().to_string();
        run.revision = Some(RevisionMark {
            id: format!("format-{author}-{date}"),
            kind: "format".into(),
            author,
            date,
            original: Some(original.format_snapshot()),
        });
    }
    run
}

fn read_image_block(
    node: &XmlNode,
    rels: &HashMap<String, RelLink>,
    reader: &ZipReader,
    warnings: &mut Vec<String>,
) -> Option<Block> {
    let mut blips = Vec::new();
    node.find_all("a:blip", &mut blips);
    if blips.is_empty() {
        node.find_all("blip", &mut blips);
    }
    let blip = blips.first()?;
    let Some(embed) = blip.attr_any_ns("embed") else {
        warnings.push("An embedded object (chart or SmartArt) was not imported.".into());
        return None;
    };
    let Some(link) = rels.get(embed) else {
        warnings.push("An image could not be located in the document package.".into());
        return None;
    };
    let path = resolve_media_path(&link.target);
    let Ok(data) = reader.read(&path) else {
        warnings.push("An image could not be read from the document package.".into());
        return None;
    };
    let name = path.rsplit('/').next().unwrap_or("image").to_string();
    let image = ImageData::from_bytes(&name, &data);
    let mut width_pt = 320.0;
    let mut height_pt = 200.0;
    let mut extents = Vec::new();
    node.find_all("wp:extent", &mut extents);
    if let Some(extent) = extents.first() {
        if let Some(cx) = extent.attr("cx").and_then(parse_f64) {
            width_pt = cx / 12700.0;
        }
        if let Some(cy) = extent.attr("cy").and_then(parse_f64) {
            height_pt = cy / 12700.0;
        }
    }
    let mut align = "center".to_string();
    let mut wrap = "inline".to_string();
    let mut anchors = Vec::new();
    node.find_all("wp:anchor", &mut anchors);
    if let Some(anchor) = anchors.first() {
        let mut positions = Vec::new();
        anchor.find_all("wp:positionH", &mut positions);
        if let Some(position) = positions.first().and_then(|position| position.child("align")) {
            match position.deep_text().trim() {
                "left" => align = "left".into(),
                "right" => align = "right".into(),
                "center" => align = "center".into(),
                _ => {}
            }
        }
        // wrapSquare and wrapTopAndBottom are the two kinds the model carries;
        // every other wrap element is approximated as square. An anchored
        // image with no wrap element at all stays inline.
        if anchor.child("wrapSquare").is_some() {
            wrap = "square".into();
        } else if anchor.child("wrapTopAndBottom").is_some() {
            wrap = "topBottom".into();
        } else if anchor.child("wrapNone").is_some()
            || anchor.child("wrapTight").is_some()
            || anchor.child("wrapThrough").is_some()
        {
            wrap = "square".into();
        }
    }
    Some(Block::Image { image, width_pt, height_pt, align, caption: String::new(), wrap })
}

/// Maps a Word field instruction onto the model field kinds we understand.
fn map_field_instruction(instruction: &str, cached: &str) -> Option<FieldRef> {
    let trimmed = instruction.trim().trim_start_matches('=').trim();
    let upper = trimmed.to_uppercase();
    let kind = |kind: &str, target: String| FieldRef { kind: kind.to_string(), target, cached: cached.to_string() };
    if upper.starts_with("NUMPAGES") {
        return Some(kind("pages", String::new()));
    }
    if upper.starts_with("PAGEREF ") {
        return Some(kind("refPage", field_argument(trimmed, 8)));
    }
    if upper.starts_with("PAGE") {
        return Some(kind("page", String::new()));
    }
    if upper.starts_with("NOTEREF ") {
        return Some(kind("footnote", field_argument(trimmed, 9)));
    }
    if upper.starts_with("REF ") {
        return Some(kind("ref", field_argument(trimmed, 4)));
    }
    if upper.starts_with("DATE") {
        return Some(kind("date", String::new()));
    }
    if upper.starts_with("TIME") {
        return Some(kind("time", String::new()));
    }
    if upper.starts_with("TITLE") {
        return Some(kind("title", String::new()));
    }
    if upper.starts_with("AUTHOR") {
        return Some(kind("author", String::new()));
    }
    None
}

fn field_argument(instruction: &str, prefix_len: usize) -> String {
    instruction[prefix_len.min(instruction.len())..].split_whitespace().next().unwrap_or("").to_string()
}

fn read_runs(
    node: &XmlNode,
    context: &mut PartContext,
    reader: &ZipReader,
    runs: &mut Vec<Run>,
    blocks: &mut Vec<Block>,
    first_break: &mut bool,
) {
    for child in &node.children {
        match child.local_name() {
            "r" => {
                let mut run = read_run_properties(child.child("rPr"));
                let mut has_text = false;
                for part in &child.children {
                    match part.local_name() {
                        "t" => {
                            if let Some(field) = &mut context.field_capture {
                                field.cached.push_str(&part.deep_text());
                                continue;
                            }
                            if context.field_skip {
                                continue;
                            }
                            run.text.push_str(&part.deep_text());
                            has_text = true;
                        }
                        "delText" => {
                            if !context.field_skip {
                                run.text.push_str(&part.deep_text());
                                has_text = true;
                            }
                        }
                        "tab" => {
                            run.text.push('\t');
                            has_text = true;
                        }
                        "br" => {
                            if part.attr_any_ns("type") == Some("page") {
                                if !run.text.is_empty() {
                                    runs.push(run.clone());
                                    run.text.clear();
                                }
                                flush_paragraph(runs, blocks, Some(ParaProps::default()));
                                blocks.push(Block::PageBreak);
                                *first_break = false;
                            } else {
                                run.text.push('\n');
                            }
                            has_text = true;
                        }
                        "drawing" | "pict" => {
                            if !run.text.is_empty() {
                                runs.push(run.clone());
                                run.text.clear();
                            }
                            if let Some(block) = read_image_block(part, &context.rels, reader, &mut context.warnings) {
                                flush_paragraph(runs, blocks, None);
                                blocks.push(block);
                            }
                        }
                        "instrText" => {
                            let instruction = part.deep_text();
                            match map_field_instruction(&instruction, "") {
                                Some(field) if field.kind == "page" || field.kind == "pages" => {
                                    // Keep the token the editors already render.
                                    runs.push(Run {
                                        text: if field.kind == "pages" {
                                            "{{pages}}".into()
                                        } else {
                                            "{{page}}".into()
                                        },
                                        ..run.clone()
                                    });
                                    context.field_skip = true;
                                }
                                Some(field) => {
                                    context.field_capture = Some(field);
                                    context.field_skip = true;
                                }
                                None => {
                                    context.warnings.push(
                                        "A field this build does not understand was imported as plain text.".into(),
                                    );
                                    context.field_skip = true;
                                }
                            }
                        }
                        "fldChar" => {
                            let kind = part.attr_any_ns("fldCharType");
                            if kind == Some("end") {
                                if let Some(field) = context.field_capture.take() {
                                    runs.push(Run { field: Some(field), ..run.clone() });
                                }
                                context.field_skip = false;
                            }
                        }
                        "footnoteReference" | "endnoteReference" => {
                            let is_endnote = part.local_name() == "endnoteReference";
                            if let Some(id) = part.attr_any_ns("id").and_then(|value| value.parse::<u32>().ok()) {
                                if id > 0 {
                                    if !run.text.is_empty() {
                                        runs.push(run.clone());
                                        run.text.clear();
                                    }
                                    let mut note_run = run.clone();
                                    if is_endnote {
                                        note_run.endnote = Some(format!("en{id}"));
                                    } else {
                                        note_run.footnote = Some(format!("fn{id}"));
                                    }
                                    runs.push(note_run);
                                    has_text = true;
                                }
                            }
                        }
                        "commentReference" => {}
                        _ => {}
                    }
                }
                if has_text || !run.text.is_empty() {
                    if run.revision.is_none() {
                        run.revision = context.pending_revision.clone();
                    }
                    if run.comment.is_none() {
                        if let Some(comment) = context.pending_comment.take() {
                            run.comment = Some(comment);
                        }
                    }
                    runs.push(run);
                }
            }
            "hyperlink" => {
                let link = child
                    .attr_any_ns("id")
                    .and_then(|id| context.rels.get(id))
                    .filter(|link| link.external)
                    .map(|link| link.target.clone());
                let mut inner_runs = Vec::new();
                let mut ignored = Vec::new();
                read_runs(child, context, reader, &mut inner_runs, &mut ignored, first_break);
                for mut run in inner_runs {
                    if run.link.is_none() {
                        run.link = link.clone();
                    }
                    runs.push(run);
                }
            }
            "smartTag" | "sdt" | "sdtContent" => {
                read_runs(child, context, reader, runs, blocks, first_break);
            }
            "ins" | "del" => {
                let kind = if child.local_name() == "del" { "delete" } else { "insert" };
                let previous = context.pending_revision.take();
                context.revision_seq += 1;
                let revision = RevisionMark {
                    id: format!("rev{}", context.revision_seq),
                    kind: kind.into(),
                    author: child.attr_any_ns("author").unwrap_or("Unknown").to_string(),
                    date: child.attr_any_ns("date").unwrap_or_default().to_string(),
                    original: None,
                };
                context.pending_revision = Some(revision);
                read_runs(child, context, reader, runs, blocks, first_break);
                context.pending_revision = previous;
            }
            "commentRangeStart" => {
                if let Some(id) = child.attr_any_ns("id") {
                    context.pending_comment = Some(id.to_string());
                }
            }
            "commentRangeEnd" | "commentReference" => {}
            "fldSimple" => {
                let instruction = child.attr_any_ns("instr").unwrap_or_default().to_string();
                match map_field_instruction(&instruction, "") {
                    Some(field) if field.kind == "page" || field.kind == "pages" => {
                        runs.push(Run {
                            text: if field.kind == "pages" { "{{pages}}".into() } else { "{{page}}".into() },
                            ..Default::default()
                        });
                    }
                    Some(mut field) => {
                        let mut inner_runs = Vec::new();
                        let saved_skip = context.field_skip;
                        context.field_skip = false;
                        read_runs(child, context, reader, &mut inner_runs, blocks, first_break);
                        context.field_skip = saved_skip;
                        field.cached = inner_runs.iter().map(|run| run.text.clone()).collect::<Vec<_>>().join("");
                        runs.push(Run { field: Some(field), ..Default::default() });
                    }
                    None => {
                        let mut inner_runs = Vec::new();
                        read_runs(child, context, reader, &mut inner_runs, blocks, first_break);
                        runs.extend(inner_runs);
                    }
                }
            }
            _ => {}
        }
    }
}

fn flush_paragraph(runs: &mut Vec<Run>, blocks: &mut Vec<Block>, props: Option<ParaProps>) {
    if runs.is_empty() {
        return;
    }
    blocks.push(Block::Paragraph { props: props.unwrap_or_default(), runs: std::mem::take(runs) });
}

fn read_paragraph(node: &XmlNode, context: &mut PartContext, reader: &ZipReader, numbering: &Numbering) -> Vec<Block> {
    let mut props = ParaProps::default();
    if let Some(properties) = node.child("pPr") {
        if let Some(style) = properties.child("pStyle").and_then(|node| node.attr_any_ns("val")) {
            props.style = style.to_string();
        }
        if let Some(align) = properties.child("jc").and_then(|node| node.attr_any_ns("val")) {
            props.align = match align {
                "center" => "center",
                "right" => "right",
                "both" => "justify",
                _ => "left",
            }
            .into();
        }
        if let Some(spacing) = properties.child("spacing") {
            if let Some(value) = spacing.attr_any_ns("before").and_then(parse_f64) {
                props.space_before_pt = value / 20.0;
            }
            if let Some(value) = spacing.attr_any_ns("after").and_then(parse_f64) {
                props.space_after_pt = value / 20.0;
            }
            if let Some(value) = spacing.attr_any_ns("line").and_then(parse_f64) {
                if value > 1.0 {
                    props.line_spacing = value / 240.0;
                }
            }
        }
        if let Some(indent) = properties.child("ind") {
            if let Some(value) = indent.attr_any_ns("left").and_then(parse_f64) {
                props.indent_left_pt = value / 20.0;
            }
            if let Some(value) = indent.attr_any_ns("right").and_then(parse_f64) {
                props.indent_right_pt = value / 20.0;
            }
            if let Some(value) = indent.attr_any_ns("firstLine").and_then(parse_f64) {
                props.first_line_pt = value / 20.0;
            }
            if let Some(value) = indent.attr_any_ns("hanging").and_then(parse_f64) {
                props.first_line_pt = -value / 20.0;
            }
        }
        props.page_break_before = properties.child("pageBreakBefore").is_some();
        if let Some(tabs) = properties.child("tabs") {
            for tab in tabs.children_named("tab") {
                let align = match tab.attr_any_ns("val").unwrap_or("left") {
                    value @ ("left" | "center" | "right" | "decimal") => value.to_string(),
                    _ => continue,
                };
                let Some(position) = tab.attr_any_ns("pos").and_then(parse_f64) else { continue };
                props.tabs.push(TabStop { pos_pt: position / 20.0, align });
            }
        }
        if let Some(numbering_properties) = properties.child("numPr") {
            let num_id = numbering_properties
                .child("numId")
                .and_then(|node| node.attr_any_ns("val"))
                .and_then(|value| value.parse::<u32>().ok());
            let level = numbering_properties
                .child("ilvl")
                .and_then(|node| node.attr_any_ns("val"))
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(0);
            if let Some(num_id) = num_id {
                let abstract_id = numbering.num_to_abstract.get(&num_id).copied().unwrap_or(0);
                let (kind, start) =
                    numbering.levels.get(&(abstract_id, level)).cloned().unwrap_or_else(|| ("bullet".to_string(), 1));
                props.list = Some(ListInfo { kind, level, start, marker: String::new() });
            }
        }
    }
    let mut runs = Vec::new();
    let mut blocks = Vec::new();
    let mut first_break = true;
    read_runs(node, context, reader, &mut runs, &mut blocks, &mut first_break);
    flush_paragraph(&mut runs, &mut blocks, Some(props));
    if blocks.is_empty() {
        blocks.push(Block::Paragraph { props: ParaProps::default(), runs: vec![Run::default()] });
    }
    blocks
}

fn read_table(
    node: &XmlNode,
    context: &mut PartContext,
    reader: &ZipReader,
    numbering: &Numbering,
    warnings: &mut Vec<String>,
) -> Block {
    let mut widths = Vec::new();
    if let Some(grid) = node.child("tblGrid") {
        for column in grid.children_named("gridCol") {
            let width = column.attr_any_ns("w").and_then(parse_f64).unwrap_or(1200.0) / 20.0;
            widths.push(width);
        }
    }
    let mut rows: Vec<TableRow> = Vec::new();
    // Per grid column, the cell an open `w:vMerge` from the row above continues
    // (row index in `rows`, cell index in that row).
    let mut open_origins: Vec<Option<(usize, usize)>> = Vec::new();
    for row_node in node.children_named("tr") {
        let mut row = TableRow::default();
        if let Some(properties) = row_node.child("trPr") {
            row.header = properties.child("tblHeader").is_some();
            if let Some(height) =
                properties.child("trHeight").and_then(|node| node.attr_any_ns("val")).and_then(parse_f64)
            {
                if height > 0.0 {
                    row.height_pt = Some(height / 20.0);
                }
            }
        }
        let previous_origins = std::mem::take(&mut open_origins);
        // Origins started in this row, applied once the row is stored.
        let mut pending_origins: Vec<(usize, usize, usize)> = Vec::new();
        // Origins this row already extended; a colspan above two columns writes
        // one continuation cell per grid column, but that is still one row.
        let mut extended: Vec<(usize, usize)> = Vec::new();
        let mut column = 0usize;
        for cell_node in row_node.children_named("tc") {
            let mut cell = TableCell::default();
            // `Some(true)` is a `restart` vMerge, `Some(false)` a continuation.
            let mut merge: Option<bool> = None;
            if let Some(properties) = cell_node.child("tcPr") {
                if let Some(span) = properties
                    .child("gridSpan")
                    .and_then(|node| node.attr_any_ns("val"))
                    .and_then(|value| value.parse::<u32>().ok())
                {
                    cell.colspan = span.max(1);
                }
                if let Some(node) = properties.child("vMerge") {
                    cell.rowspan = 1;
                    merge = Some(
                        node.attr_any_ns("val").map(|value| value.eq_ignore_ascii_case("restart")).unwrap_or(false),
                    );
                }
                if let Some(fill) = properties.child("shd").and_then(|node| node.attr_any_ns("fill")) {
                    if !fill.eq_ignore_ascii_case("auto") {
                        cell.background = normalize_hex(fill);
                    }
                }
                if let Some(valign) = properties.child("vAlign").and_then(|node| node.attr_any_ns("val")) {
                    cell.valign = valign.to_string();
                }
                if let Some(width) = properties.child("tcW").and_then(|node| node.attr_any_ns("w")).and_then(parse_f64)
                {
                    cell.width_pt = Some(width / 20.0);
                }
            }
            let span = cell.colspan.max(1) as usize;
            if merge == Some(false) {
                if let Some(origin) = previous_origins.get(column).copied().flatten() {
                    if !extended.contains(&origin) {
                        extended.push(origin);
                        if let Some(target) =
                            rows.get_mut(origin.0).and_then(|origin_row| origin_row.cells.get_mut(origin.1))
                        {
                            target.rowspan = target.rowspan.saturating_add(1);
                        }
                    }
                    while open_origins.len() < column + span {
                        open_origins.push(None);
                    }
                    for slot in open_origins.iter_mut().skip(column).take(span) {
                        *slot = Some(origin);
                    }
                    column += span;
                    continue;
                }
                // A continuation without a prior restart is kept as an empty
                // cell rather than dropped.
            }
            for child in &cell_node.children {
                match child.local_name() {
                    "p" => cell.blocks.extend(read_paragraph(child, context, reader, numbering)),
                    "tbl" => {
                        warnings.push("Nested tables are imported as plain content.".into());
                        cell.blocks.push(read_table(child, context, reader, numbering, warnings));
                    }
                    _ => {}
                }
            }
            if cell.blocks.is_empty() {
                cell.blocks.push(Block::paragraph(""));
            }
            let cell_index = row.cells.len();
            row.cells.push(cell);
            if merge == Some(true) {
                pending_origins.push((column, span, cell_index));
            }
            column += span;
        }
        if !row.cells.is_empty() {
            let row_index = rows.len();
            rows.push(row);
            for (column, span, cell_index) in pending_origins {
                while open_origins.len() < column + span {
                    open_origins.push(None);
                }
                for slot in open_origins.iter_mut().skip(column).take(span) {
                    *slot = Some((row_index, cell_index));
                }
            }
        }
    }
    Block::Table {
        table: TableData {
            rows,
            column_widths_pt: widths,
            borders: true,
            border_color: "#94A3B8".into(),
            align: "left".into(),
        },
    }
}

/// One `name:value` entry of a VML `style` attribute (`rotation:315`).
fn vml_style_value(style: &str, name: &str) -> Option<String> {
    for part in style.split(';') {
        let part = part.trim();
        if let Some(value) = part.strip_prefix(name).and_then(|value| value.strip_prefix(':')) {
            return Some(value.trim().to_string());
        }
    }
    None
}

fn parse_pt(value: &str) -> Option<f64> {
    value.trim().trim_end_matches("pt").trim().parse::<f64>().ok()
}

/// A VML `opacity`, which may be a fraction (`0.18`) or a percentage (`18%`).
fn parse_opacity(value: &str) -> f64 {
    let value = value.trim();
    if let Some(percent) = value.strip_suffix('%') {
        return percent.trim().parse::<f64>().map(|part| (part / 100.0).clamp(0.0, 1.0)).unwrap_or(1.0);
    }
    value
        .parse::<f64>()
        .map(|part| if part > 1.0 { (part / 100.0).clamp(0.0, 1.0) } else { part.clamp(0.0, 1.0) })
        .unwrap_or(1.0)
}

/// Detects a VML text watermark: a `v:textpath` with a `string` attribute,
/// as Word writes into the default header of a watermarked document.
fn parse_watermark(root: &XmlNode) -> Option<Watermark> {
    let mut textpaths = Vec::new();
    root.find_all("v:textpath", &mut textpaths);
    let textpath = textpaths.iter().find(|node| node.attr("string").is_some())?;
    let text = textpath.attr("string").unwrap_or_default().to_string();
    let font_style = textpath.attr("style").unwrap_or_default();
    let font_pt = vml_style_value(font_style, "font-size").and_then(|value| parse_pt(&value)).unwrap_or(72.0);
    let bold = vml_style_value(font_style, "font-weight")
        .map(|value| value.eq_ignore_ascii_case("bold") || value == "700")
        .unwrap_or(false);
    let mut shapes = Vec::new();
    root.find_all("v:shape", &mut shapes);
    let shape = shapes.iter().find(|node| node.attr("fillcolor").is_some());
    let color = shape.and_then(|node| node.attr("fillcolor")).and_then(|value| {
        if value.eq_ignore_ascii_case("auto") {
            None
        } else {
            normalize_hex(value)
        }
    });
    let rotation = shape
        .and_then(|node| node.attr("style"))
        .and_then(|style| vml_style_value(style, "rotation"))
        .and_then(|value| parse_f64(&value))
        .unwrap_or(0.0);
    let opacity = shape
        .and_then(|node| {
            let mut fills = Vec::new();
            node.find_all("v:fill", &mut fills);
            fills.first().and_then(|fill| fill.attr("opacity")).map(parse_opacity)
        })
        .unwrap_or(1.0);
    Some(Watermark { text, color, opacity, rotation, font_pt, bold })
}

/// Reads a header part purely to look for a watermark paragraph.
fn read_watermark_part(reader: &ZipReader, part: &str) -> Option<Watermark> {
    let text = reader.read_text(part).ok()?;
    let root = parse_xml(&text).ok()?;
    parse_watermark(&root)
}

/// Parses a `w:sectPr` into the model's section properties, resolving the
/// header/footer parts it references. `watermark` is filled from the default
/// header part when one is present.
fn parse_section_props(
    node: &XmlNode,
    rels: &HashMap<String, RelLink>,
    reader: &ZipReader,
    numbering: &Numbering,
    part_dir: &str,
    warnings: &mut Vec<String>,
    watermark: &mut Option<Watermark>,
) -> SectionProps {
    let mut section = SectionProps::default();
    let page = &mut section.page;
    if let Some(size) = node.child("pgSz") {
        let width = size.attr_any_ns("w").and_then(parse_f64).unwrap_or(11906.0) / 20.0;
        let height = size.attr_any_ns("h").and_then(parse_f64).unwrap_or(16838.0) / 20.0;
        page.width_pt = width;
        page.height_pt = height;
        page.orientation = if width > height { "landscape".into() } else { "portrait".into() };
        page.size = match ((width.round() as i64), (height.round() as i64)) {
            (595, 842) | (842, 595) => "a4",
            (420, 595) | (595, 420) => "a5",
            (612, 792) | (792, 612) => "letter",
            (612, 1008) | (1008, 612) => "legal",
            _ => "custom",
        }
        .into();
    }
    if let Some(margins) = node.child("pgMar") {
        page.margin_top_pt = margins.attr_any_ns("top").and_then(parse_f64).unwrap_or(1440.0) / 20.0;
        page.margin_right_pt = margins.attr_any_ns("right").and_then(parse_f64).unwrap_or(1440.0) / 20.0;
        page.margin_bottom_pt = margins.attr_any_ns("bottom").and_then(parse_f64).unwrap_or(1440.0) / 20.0;
        page.margin_left_pt = margins.attr_any_ns("left").and_then(parse_f64).unwrap_or(1440.0) / 20.0;
        page.header_distance_pt = margins.attr_any_ns("header").and_then(parse_f64).unwrap_or(720.0) / 20.0;
        page.footer_distance_pt = margins.attr_any_ns("footer").and_then(parse_f64).unwrap_or(720.0) / 20.0;
    }
    if let Some(columns) = node.child("cols") {
        page.columns = columns.attr_any_ns("num").and_then(|value| value.parse::<u32>().ok()).unwrap_or(1).max(1);
        page.column_spacing_pt = columns.attr_any_ns("space").and_then(parse_f64).unwrap_or(708.0) / 20.0;
    }
    page.different_first_page = node.child("titlePg").is_some();
    section.different_first_page = page.different_first_page;
    section.different_odd_even = node.child("evenAndOddHeaders").is_some();
    section.columns = page.columns;
    section.start = match node.child("type").and_then(|node| node.attr_any_ns("val")) {
        Some("continuous") => "continuous".into(),
        Some("oddPage") => "oddPage".into(),
        Some("evenPage") => "evenPage".into(),
        _ => "newPage".into(),
    };
    for key in ["headerReference", "footerReference"] {
        for reference in node.children_named(key) {
            let Some(id) = reference.attr_any_ns("id") else { continue };
            let Some(link) = rels.get(id) else { continue };
            let kind = reference.attr_any_ns("type").unwrap_or("default");
            let path = if link.target.trim_start_matches('/').starts_with("word/") {
                link.target.trim_start_matches('/').to_string()
            } else {
                format!("{part_dir}/{}", link.target.trim_start_matches("./"))
            };
            let blocks = read_part_blocks(reader, &path, numbering, warnings);
            let is_header = key.starts_with("header");
            if is_header && kind == "default" && watermark.is_none() {
                *watermark = read_watermark_part(reader, &path);
            }
            let slot = match (is_header, kind) {
                (true, "first") => &mut section.first_header,
                (true, "even") => &mut section.even_header,
                (true, _) => &mut section.header,
                (false, "first") => &mut section.first_footer,
                (false, "even") => &mut section.even_footer,
                (false, _) => &mut section.footer,
            };
            if slot.is_empty() {
                *slot = blocks;
            }
        }
    }
    section
}

fn apply_section_to_document(document: &mut TextDocument, section: SectionProps) {
    document.page = section.page;
    document.header = section.header;
    document.footer = section.footer;
}

fn read_part_blocks(reader: &ZipReader, part: &str, numbering: &Numbering, warnings: &mut Vec<String>) -> Vec<Block> {
    let Ok(text) = reader.read_text(part) else {
        return Vec::new();
    };
    let Ok(root) = parse_xml(&text) else {
        return Vec::new();
    };
    let mut context = PartContext::new(read_relationships(reader, part));
    let mut blocks = Vec::new();
    let container = root.child("body").unwrap_or(&root);
    for child in &container.children {
        match child.local_name() {
            "p" => blocks.extend(read_paragraph(child, &mut context, reader, numbering)),
            "tbl" => blocks.push(read_table(child, &mut context, reader, numbering, warnings)),
            "sdt" => {
                let mut inner = Vec::new();
                child.find_all("sdtContent", &mut inner);
                for content in inner {
                    for node in &content.children {
                        if node.local_name() == "p" {
                            blocks.extend(read_paragraph(node, &mut context, reader, numbering));
                        }
                    }
                }
            }
            "bookmarkStart" | "bookmarkEnd" | "proofErr" | "commentRangeStart" | "commentRangeEnd" => {}
            _ => {}
        }
    }
    warnings.extend(context.warnings);
    blocks
}

struct SectionPlan {
    /// `(block index after the paragraph, properties of that section)` for every
    /// paragraph-level `w:sectPr`.
    paragraph_sections: Vec<(usize, SectionProps)>,
    /// Watermark found in each paragraph-level section's default header,
    /// aligned with `paragraph_sections`.
    paragraph_watermarks: Vec<Option<Watermark>>,
    body: Option<SectionProps>,
    body_watermark: Option<Watermark>,
}

/// Reads the main document part, returning blocks plus the section plan.
fn read_body(
    reader: &ZipReader,
    part: &str,
    numbering: &Numbering,
    warnings: &mut Vec<String>,
) -> (Vec<Block>, SectionPlan) {
    let mut plan = SectionPlan {
        paragraph_sections: Vec::new(),
        paragraph_watermarks: Vec::new(),
        body: None,
        body_watermark: None,
    };
    let Ok(text) = reader.read_text(part) else {
        return (Vec::new(), plan);
    };
    let Ok(root) = parse_xml(&text) else {
        return (Vec::new(), plan);
    };
    let rels = read_relationships(reader, part);
    let part_dir = part.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("word");
    let mut context = PartContext::new(rels.clone());
    let mut blocks = Vec::new();
    let container = root.child("body").unwrap_or(&root);
    for child in &container.children {
        match child.local_name() {
            "p" => {
                blocks.extend(read_paragraph(child, &mut context, reader, numbering));
                if let Some(sect) = child.child("pPr").and_then(|properties| properties.child("sectPr")) {
                    let mut watermark = None;
                    let props = parse_section_props(sect, &rels, reader, numbering, part_dir, warnings, &mut watermark);
                    plan.paragraph_sections.push((blocks.len(), props));
                    plan.paragraph_watermarks.push(watermark);
                }
            }
            "tbl" => blocks.push(read_table(child, &mut context, reader, numbering, warnings)),
            "sectPr" => {
                let mut watermark = None;
                plan.body =
                    Some(parse_section_props(child, &rels, reader, numbering, part_dir, warnings, &mut watermark));
                plan.body_watermark = watermark;
            }
            "sdt" => {
                let mut inner = Vec::new();
                child.find_all("sdtContent", &mut inner);
                for content in inner {
                    for node in &content.children {
                        if node.local_name() == "p" {
                            blocks.extend(read_paragraph(node, &mut context, reader, numbering));
                        }
                    }
                }
            }
            "bookmarkStart" | "bookmarkEnd" | "proofErr" | "commentRangeStart" | "commentRangeEnd" => {}
            _ => {}
        }
    }
    warnings.extend(context.warnings);
    (blocks, plan)
}

/// Rebuilds the model's section list from the DOCX section plan.
fn apply_section_plan(document: &mut TextDocument, mut blocks: Vec<Block>, plan: SectionPlan) {
    // The watermark lives in the first section's default header; later
    // sections do not override the document-level watermark.
    document.watermark = if plan.paragraph_sections.is_empty() {
        plan.body_watermark.clone()
    } else {
        plan.paragraph_watermarks.first().cloned().flatten()
    };
    if plan.paragraph_sections.is_empty() {
        if let Some(body) = plan.body {
            apply_section_to_document(document, body);
        }
        document.blocks = blocks;
        return;
    }
    let first = plan.paragraph_sections[0].1.clone();
    apply_section_to_document(document, first);
    // Sections after the first: each paragraph sectPr defines the section that
    // ends there; the body sectPr defines the final section. Without a body
    // sectPr the last paragraph section doubles as the final one.
    let mut sections_after: Vec<SectionProps> =
        plan.paragraph_sections.iter().skip(1).map(|(_, props)| props.clone()).collect();
    match plan.body.clone() {
        Some(body) => sections_after.push(body),
        None => {
            if let Some(last) = plan.paragraph_sections.last() {
                sections_after.push(last.1.clone());
            }
        }
    }
    let mut insertions: Vec<(usize, SectionProps)> = Vec::new();
    for (index, (position, _)) in plan.paragraph_sections.iter().enumerate() {
        if let Some(props) = sections_after.get(index) {
            insertions.push((*position, props.clone()));
        }
    }
    for (position, props) in insertions.into_iter().rev() {
        let position = position.min(blocks.len());
        blocks.insert(position, Block::SectionBreak { section: props });
    }
    document.blocks = blocks;
}

/// Reads `word/footnotes.xml` / `word/endnotes.xml` into model notes.
fn read_notes(
    reader: &ZipReader,
    part: &str,
    endnotes: bool,
    numbering: &Numbering,
    warnings: &mut Vec<String>,
) -> Vec<Footnote> {
    let mut notes = Vec::new();
    let Ok(text) = reader.read_text(part) else {
        return notes;
    };
    let Ok(root) = parse_xml(&text) else {
        return notes;
    };
    let item = if endnotes { "endnote" } else { "footnote" };
    let prefix = if endnotes { "en" } else { "fn" };
    let mut context = PartContext::new(read_relationships(reader, part));
    for node in root.children_named(item) {
        let Some(id) = node.attr_any_ns("id").and_then(|value| value.parse::<i64>().ok()) else { continue };
        if id <= 0 {
            continue;
        }
        let mut runs = Vec::new();
        for paragraph in node.children_named("p") {
            for block in read_paragraph(paragraph, &mut context, reader, numbering) {
                if let Block::Paragraph { runs: paragraph_runs, .. } = block {
                    runs.extend(paragraph_runs);
                }
            }
        }
        while runs.first().map(|run| run.text.trim().is_empty()).unwrap_or(false) {
            runs.remove(0);
        }
        if runs.is_empty() {
            runs.push(Run::default());
        }
        notes.push(Footnote { id: format!("{prefix}{id}"), runs, marker: String::new() });
    }
    warnings.extend(context.warnings);
    notes
}

/// Reads `word/comments.xml`; exported replies (which DOCX stores as extra
/// comment paragraphs) become `replies` again.
fn read_comments(reader: &ZipReader) -> Vec<Comment> {
    let mut comments = Vec::new();
    let Ok(text) = reader.read_text("word/comments.xml") else {
        return comments;
    };
    let Ok(root) = parse_xml(&text) else {
        return comments;
    };
    for node in root.children_named("comment") {
        let Some(id) = node.attr_any_ns("id") else { continue };
        let author = node.attr_any_ns("author").unwrap_or("Unknown").to_string();
        let created = node.attr_any_ns("date").unwrap_or_default().to_string();
        let mut paragraphs: Vec<String> = Vec::new();
        for paragraph in node.children_named("p") {
            let text = paragraph.deep_text();
            let cleaned = text.trim().to_string();
            if !cleaned.is_empty() {
                paragraphs.push(cleaned);
            }
        }
        let mut comment = Comment {
            id: id.to_string(),
            author: author.clone(),
            text: String::new(),
            created: created.clone(),
            resolved: false,
            modified: created,
            replies: Vec::new(),
        };
        for paragraph in paragraphs {
            if let Some(rest) = paragraph.strip_prefix("Re: ") {
                if let Some((reply_author, reply_text)) = rest.split_once(": ") {
                    comment.replies.push(CommentReply {
                        author: reply_author.to_string(),
                        text: reply_text.to_string(),
                        created: String::new(),
                    });
                    continue;
                }
            }
            if comment.text.is_empty() {
                comment.text = paragraph;
            } else {
                comment.text.push('\n');
                comment.text.push_str(&paragraph);
            }
        }
        comments.push(comment);
    }
    comments
}

pub fn read_docx(bytes: &[u8]) -> OfficeResult<DocxRead> {
    let reader = ZipReader::open(bytes.to_vec())?;
    let mut warnings = Vec::new();
    let mut document = TextDocument::default();
    document.blocks.clear();

    let document_part = if reader.contains("word/document.xml") {
        "word/document.xml".to_string()
    } else {
        let root = reader
            .read_text("_rels/.rels")
            .ok()
            .and_then(|text| parse_xml(&text).ok())
            .ok_or_else(|| OfficeError::corrupt("The package does not contain a main document part."))?;
        let mut found = None;
        for node in root.children_named("Relationship") {
            let kind = node.attr("Type").unwrap_or_default();
            if kind.ends_with("/officeDocument") {
                if let Some(target) = node.attr("Target") {
                    found = Some(target.trim_start_matches('/').to_string());
                }
            }
        }
        found.ok_or_else(|| OfficeError::corrupt("The package does not declare a main document."))?
    };

    document.styles = read_styles(&reader);
    let numbering = read_numbering(&reader);
    let (blocks, plan) = read_body(&reader, &document_part, &numbering, &mut warnings);
    apply_section_plan(&mut document, blocks, plan);

    document.footnotes = read_notes(&reader, "word/footnotes.xml", false, &numbering, &mut warnings);
    document.endnotes = read_notes(&reader, "word/endnotes.xml", true, &numbering, &mut warnings);
    document.comments = read_comments(&reader);
    if let Ok(text) = reader.read_text("word/settings.xml") {
        if let Ok(root) = parse_xml(&text) {
            document.track_changes = root.child("trackRevisions").is_some();
        }
    }

    if reader.contains("word/vbaProject.bin") {
        warnings.push("Macros were not loaded. Documents always open with macros disabled.".into());
    }
    warnings.sort();
    warnings.dedup();

    if document.blocks.is_empty() {
        document.blocks.push(Block::paragraph(""));
    }
    document.metadata.title = document.title.clone();
    if let Ok(text) = reader.read_text("docProps/core.xml") {
        if let Ok(root) = parse_xml(&text) {
            if let Some(value) = root.child("dc:title").map(XmlNode::deep_text) {
                if !value.is_empty() {
                    document.title = value;
                }
            }
            if let Some(value) = root.child("dc:creator").map(XmlNode::deep_text) {
                document.metadata.author = value;
            }
            if let Some(value) = root.child("dc:subject").map(XmlNode::deep_text) {
                document.metadata.subject = value;
            }
            if let Some(value) =
                root.attr_any_ns("keywords").or_else(|| root.child("cp:keywords").map(|node| node.text.as_str()))
            {
                document.metadata.keywords = value.to_string();
            }
            if let Some(value) = root.child("dcterms:created").map(XmlNode::deep_text) {
                document.metadata.created = value;
            }
        }
    }
    if document.metadata.title.is_empty() {
        document.metadata.title = document.title.clone();
    }
    Ok(DocxRead { document, warnings })
}

pub fn read_docx_file(path: &Path) -> OfficeResult<DocxRead> {
    let bytes = crate::io::read_bytes(path)?;
    let mut result = read_docx(&bytes)?;
    if result.document.title.is_empty() || result.document.title == "Untitled document" {
        result.document.title = crate::io::file_stem(path);
    }
    Ok(result)
}
