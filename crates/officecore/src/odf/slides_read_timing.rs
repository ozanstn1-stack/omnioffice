//! ODP (Impress) import: timing, footers and the package reader.

use super::slides_read::*;
use super::*;

/// Reads the slide's main sequence back into animations: trigger from the
/// effect's node type, class and preset to kind and editor effect (a preset
/// without an editor twin maps to the closest effect with a warning), delay
/// from its begin and duration from its longest child.
fn read_odp_timing(page: &XmlNode, objects: &[SlideObject], warnings: &mut Vec<String>) -> Vec<Animation> {
    let Some(root) = page.children.iter().find(|child| child.local_name() == "par") else { return Vec::new() };
    let mut effects = Vec::new();
    for sequence in root.children_named("seq") {
        match sequence.attr_any_ns("node-type") {
            Some("main-sequence") => collect_odp_effects(sequence, &mut effects),
            Some("interactive-sequence") => {
                warnings.push("Animations triggered by clicking a shape were not imported.".into());
            }
            _ => {}
        }
    }
    let mut ids = std::collections::HashSet::new();
    collect_odp_ids(objects, &mut ids);
    let mut animations = Vec::new();
    for effect in effects {
        let mut nodes = Vec::new();
        effect.walk(&mut nodes);
        let nodes = &nodes[1..];
        let preset = effect.attr_any_ns("preset-id").unwrap_or("");
        let kind = match effect.attr_any_ns("preset-class") {
            Some(kind @ ("entrance" | "exit" | "emphasis")) => kind,
            Some(other) => {
                warnings.push(format!("A \"{other}\" animation has no editor equivalent and was skipped."));
                continue;
            }
            None if preset.starts_with("ooo-entrance-") => "entrance",
            None if preset.starts_with("ooo-exit-") => "exit",
            None => "emphasis",
        };
        let Some(target) = nodes.iter().find_map(|node| node.attr_any_ns("targetElement")) else {
            warnings.push("An animation without a target object was skipped.".into());
            continue;
        };
        if !ids.contains(target) {
            warnings.push("An animation whose target shape was not imported was skipped.".into());
            continue;
        }
        let scale = nodes
            .iter()
            .find(|node| node.local_name() == "animateTransform" && node.attr_any_ns("type") == Some("scale"))
            .and_then(|node| node.attr_any_ns("by").or_else(|| node.attr_any_ns("to")))
            .and_then(|value| value.split(',').next()?.trim().parse::<f64>().ok());
        let exact = ODP_PRESETS.iter().find(|entry| {
            entry.0 == kind
                && entry.2 == preset
                && match entry.1 {
                    "grow" => scale.map(|scale| scale >= 1.0).unwrap_or(true),
                    "shrink" => scale.map(|scale| scale < 1.0).unwrap_or(false),
                    _ => true,
                }
        });
        let effect_name = match exact {
            Some(entry) => entry.1,
            None => {
                // A custom effect has no preset; its transition type is the best hint.
                let hint = if preset.is_empty() {
                    nodes
                        .iter()
                        .find_map(|node| node.attr_any_ns("type").filter(|_| node.local_name() == "transitionFilter"))
                        .unwrap_or("")
                } else {
                    preset
                };
                let closest = closest_odp_effect(kind, hint.trim_start_matches(&format!("ooo-{kind}-")));
                let label = if preset.is_empty() { "without a preset" } else { preset };
                warnings.push(format!(
                    "The {kind} effect \"{label}\" was imported as the closest editor effect \"{closest}\"."
                ));
                closest
            }
        };
        let trigger = match effect.attr_any_ns("node-type") {
            Some("with-previous") => "withPrevious",
            Some("after-previous") => "afterPrevious",
            _ => "onClick",
        };
        let duration = nodes
            .iter()
            .filter_map(|node| {
                let duration = node.attr_any_ns("dur").and_then(parse_smil_seconds)?;
                Some(node.attr_any_ns("begin").and_then(parse_smil_seconds).unwrap_or(0.0) + duration)
            })
            .fold(0.0, f64::max);
        animations.push(Animation {
            id: uuid::Uuid::new_v4().to_string(),
            object_id: target.to_string(),
            kind: kind.to_string(),
            effect: effect_name.to_string(),
            trigger: trigger.to_string(),
            duration_ms: seconds_to_ms(duration),
            delay_ms: effect.attr_any_ns("begin").and_then(parse_smil_seconds).map(seconds_to_ms).unwrap_or(0),
            order: animations.len() as u32,
        });
    }
    animations
}

/// The names of the drawing-page styles that mark a slide hidden
/// (`presentation:visibility="hidden"` on their properties), LibreOffice's
/// convention for a hidden slide.
fn odp_hidden_page_styles(root: &XmlNode) -> HashSet<String> {
    let mut hidden = HashSet::new();
    let mut styles = Vec::new();
    root.find_all("style", &mut styles);
    for style in styles {
        if style.attr_any_ns("family") != Some("drawing-page") {
            continue;
        }
        let Some(name) = style.attr_any_ns("name") else { continue };
        let mut properties = Vec::new();
        style.find_all("drawing-page-properties", &mut properties);
        if properties.iter().any(|node| node.attr_any_ns("visibility") == Some("hidden")) {
            hidden.insert(name.to_string());
        }
    }
    hidden
}

/// The deck footer settings a master page declares: the
/// `presentation:display-*` flags on its drawing-page properties, plus the
/// placeholder frames (`presentation:class`) carrying the footer and date
/// text. `None` when the master page says nothing about a footer.
fn read_odp_footer(root: &XmlNode) -> Option<SlideFooter> {
    let mut footer = SlideFooter::default();
    let mut found = false;
    let mut properties = Vec::new();
    root.find_all("drawing-page-properties", &mut properties);
    for node in properties {
        if node.attr_any_ns("display-footer") == Some("true") {
            footer.show_text = true;
            found = true;
        }
        if node.attr_any_ns("display-page-number") == Some("true") {
            footer.show_slide_number = true;
            found = true;
        }
        if node.attr_any_ns("display-date-time") == Some("true") {
            footer.show_date = true;
            found = true;
        }
    }
    let mut frames = Vec::new();
    root.find_all("frame", &mut frames);
    for frame in frames {
        match frame.attr_any_ns("class") {
            Some("footer") => {
                footer.show_text = true;
                found = true;
                let text = frame.deep_text().trim().to_string();
                if footer.text.is_empty() {
                    footer.text = text;
                }
            }
            Some("page-number") => {
                footer.show_slide_number = true;
                found = true;
            }
            Some("date-time") => {
                footer.show_date = true;
                found = true;
                let text = frame.deep_text().trim().to_string();
                if footer.date_text.is_empty() {
                    footer.date_text = text;
                }
            }
            _ => {}
        }
    }
    found.then(|| {
        footer.enabled = true;
        footer
    })
}

pub fn read_odp(bytes: &[u8]) -> OfficeResult<DeckRead> {
    let reader = ZipReader::open(bytes.to_vec())?;
    if !reader.contains("content.xml") {
        return Err(OfficeError::corrupt("The package does not contain content.xml."));
    }
    let text = reader.read_text("content.xml")?;
    // Trailing text after an inline element is wrapped, so run order survives
    // the parse exactly like the ODT reader's parts.
    let root = parse_xml(&wrap_trailing_text(&text))?;
    let mut deck = Deck::new_blank("Imported presentation");
    deck.slides.clear();
    let mut warnings = Vec::new();
    // Slide size and master page (footer flags) from styles.xml when available.
    let styles_root =
        reader.read_text("styles.xml").ok().and_then(|styles| parse_xml(&wrap_trailing_text(&styles)).ok());
    if let Some(styles_root) = &styles_root {
        let mut layouts = Vec::new();
        styles_root.find_all("page-layout-properties", &mut layouts);
        if let Some(properties) = layouts.first() {
            if let Some(width) = properties.attr_any_ns("page-width").and_then(parse_cm) {
                deck.size.width_pt = width;
            }
            if let Some(height) = properties.attr_any_ns("page-height").and_then(parse_cm) {
                deck.size.height_pt = height;
            }
        }
        if let Some(footer) = read_odp_footer(styles_root) {
            deck.footer = Some(footer);
        }
    }
    let mut import = OdpImport::default();
    collect_styles(&root, &mut import.styles);
    let mut roots = vec![&root];
    if let Some(styles_root) = &styles_root {
        collect_styles(styles_root, &mut import.styles);
        roots.push(styles_root);
    }
    import.clips = odp_graphic_clips(&roots);
    let hidden_pages = odp_hidden_page_styles(&root);
    let mut pages = Vec::new();
    root.find_all("page", &mut pages);
    for page in pages {
        let objects = read_odp_shapes(page, &reader, &mut import, &mut warnings);
        let animations = read_odp_timing(page, &objects, &mut warnings);
        let hidden = page.attr_any_ns("visibility") == Some("hidden")
            || page.attr_any_ns("style-name").map(|name| hidden_pages.contains(name)).unwrap_or(false);
        let mut slide = Slide { objects, animations, hidden, ..Default::default() };
        let mut notes = Vec::new();
        page.find_all("notes", &mut notes);
        if let Some(notes) = notes.first() {
            slide.notes = notes.deep_text().trim().to_string();
        }
        deck.slides.push(slide);
    }
    if deck.slides.is_empty() {
        deck.slides.push(Slide::default());
    }
    warnings.sort();
    warnings.dedup();
    Ok(DeckRead { deck, warnings })
}

pub fn read_odp_file(path: &Path) -> OfficeResult<DeckRead> {
    let bytes = crate::io::read_bytes(path)?;
    let mut result = read_odp(&bytes)?;
    result.deck.title = crate::io::file_stem(path);
    Ok(result)
}
