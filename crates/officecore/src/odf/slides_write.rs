//! ODP (Impress) export helpers: effects, timing and ids.

use super::*;
use std::collections::{BTreeMap, HashMap};

pub(super) fn is_ncname(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(first) if first.is_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_alphanumeric() || matches!(ch, '.' | '-' | '_'))
}

/// `xml:id`/`draw:id` allocation for ODP shapes, which animations target.
/// Ids must be unique in the whole content.xml: a model id that is an NCName
/// and still free is kept, so a re-import restores it; anything else (a UUID
/// starting with a digit, an id repeated on another slide) gets `idN`.
#[derive(Default)]
pub(super) struct OdpShapeIds {
    pub(super) used: std::collections::HashSet<String>,
    /// Model object id -> written id, for the slide being written.
    pub(super) slide: HashMap<String, String>,
    /// Model ids whose planned id has already been handed out on this slide.
    pub(super) written: std::collections::HashSet<String>,
    pub(super) next: usize,
}

impl OdpShapeIds {
    /// A fresh id that is unique in the whole content.xml, preferring an NCName
    /// model id that is still free.
    pub(super) fn unique(&mut self, object: &SlideObject) -> String {
        let id = if is_ncname(&object.id) && !self.used.contains(&object.id) {
            object.id.clone()
        } else {
            loop {
                self.next += 1;
                let candidate = format!("id{}", self.next);
                if !self.used.contains(&candidate) {
                    break candidate;
                }
            }
        };
        self.used.insert(id.clone());
        id
    }

    /// Plans the id of one shape without consuming it, so a connector written
    /// before its target already knows the target's id. The first object with a
    /// given model id owns the mapping.
    pub(super) fn plan(&mut self, object: &SlideObject) -> String {
        let id = self.unique(object);
        if !object.id.is_empty() {
            self.slide.entry(object.id.clone()).or_insert_with(|| id.clone());
        }
        id
    }

    /// The id to write for `object`: its planned id when there is one, otherwise
    /// a fresh one. A repeated model id falls back to a fresh id so every shape
    /// stays addressable.
    pub(super) fn assign(&mut self, object: &SlideObject) -> String {
        if !object.id.is_empty() {
            if let Some(planned) = self.slide.get(&object.id).cloned() {
                if self.written.insert(object.id.clone()) {
                    return planned;
                }
            }
        }
        let id = self.unique(object);
        if !object.id.is_empty() {
            self.slide.entry(object.id.clone()).or_insert_with(|| id.clone());
        }
        id
    }

    /// Starts a new slide; the planned and written maps are per page.
    pub(super) fn reset_slide(&mut self) {
        self.slide.clear();
        self.written.clear();
    }
}

/// The editor's animation effects and the LibreOffice presets they are written
/// as (LibreOffice's `simpress/effects.xml`): kind, effect, preset id and
/// preset sub-type. Grow and shrink share a preset and differ by scale.
pub(super) const ODP_PRESETS: [(&str, &str, &str, &str); 11] = [
    ("entrance", "appear", "ooo-entrance-appear", ""),
    ("entrance", "fade", "ooo-entrance-fade-in", ""),
    ("entrance", "flyIn", "ooo-entrance-fly-in", "from-bottom"),
    ("entrance", "zoom", "ooo-entrance-zoom", "in"),
    ("emphasis", "pulse", "ooo-emphasis-flash-bulb", ""),
    ("emphasis", "spin", "ooo-emphasis-spin", ""),
    ("emphasis", "grow", "ooo-emphasis-grow-and-shrink", ""),
    ("emphasis", "shrink", "ooo-emphasis-grow-and-shrink", ""),
    ("exit", "disappear", "ooo-exit-disappear", ""),
    ("exit", "fadeOut", "ooo-exit-fade-out", ""),
    ("exit", "flyOut", "ooo-exit-fly-out", "to-top"),
];

/// The editor effect closest to a foreign effect or preset name (a PPTX filter
/// such as `wipe(down)`, or a LibreOffice preset without an editor twin).
pub(super) fn closest_odp_effect(kind: &str, name: &str) -> &'static str {
    let name = name.to_ascii_lowercase();
    let has = |words: &[&str]| words.iter().any(|word| name.contains(word));
    let flies = has(&["fly", "peek", "rise", "ascend", "descend", "float", "glide", "sling", "credits", "sink"]);
    match kind {
        "entrance" if flies => "flyIn",
        "entrance" if has(&["zoom", "magnify", "expand", "grow", "stretch", "compress", "unfold"]) => "zoom",
        "entrance" if name.is_empty() || has(&["appear", "flash"]) => "appear",
        "entrance" => "fade",
        "exit" if flies => "flyOut",
        "exit" if has(&["disappear", "flash"]) => "disappear",
        "exit" => "fadeOut",
        _ if has(&["shrink", "compress"]) => "shrink",
        _ if has(&["grow", "magnify", "zoom", "scale", "size", "expand"]) => "grow",
        _ if has(&["spin", "rotate", "teeter", "swivel", "wave"]) => "spin",
        _ => "pulse",
    }
}

/// The editor effect an animation is written as, and whether that is exact.
pub(super) fn odp_export_effect(kind: &str, effect: &str) -> (&'static str, bool) {
    let exact = match (kind, effect) {
        ("entrance", "fadeIn") => Some("fade"),
        // PPTX imports name the exit fade `fade`.
        ("exit", "fade") => Some("fadeOut"),
        _ => ODP_PRESETS.iter().find(|preset| preset.0 == kind && preset.1 == effect).map(|preset| preset.1),
    };
    match exact {
        Some(effect) => (effect, true),
        None => (closest_odp_effect(kind, effect), false),
    }
}

/// A SMIL clock value in seconds (`0.5s`).
pub(super) fn smil_time(ms: u64) -> String {
    format!("{}s", ms as f64 / 1000.0)
}

/// Seconds from a SMIL clock value (`0.5s`, `500ms`, `2`, `00:00:01.5`);
/// `None` for event values such as `next` or `indefinite`.
pub(super) fn parse_smil_seconds(value: &str) -> Option<f64> {
    let value = value.trim();
    let seconds = if let Some(ms) = value.strip_suffix("ms") {
        ms.trim().parse::<f64>().ok()? / 1000.0
    } else if let Some(minutes) = value.strip_suffix("min") {
        minutes.trim().parse::<f64>().ok()? * 60.0
    } else if let Some(hours) = value.strip_suffix('h') {
        hours.trim().parse::<f64>().ok()? * 3600.0
    } else if let Some(seconds) = value.strip_suffix('s') {
        seconds.trim().parse::<f64>().ok()?
    } else if value.contains(':') {
        let mut total = 0.0;
        for part in value.split(':') {
            total = total * 60.0 + part.trim().parse::<f64>().ok()?;
        }
        total
    } else {
        value.parse::<f64>().ok()?
    };
    seconds.is_finite().then_some(seconds.max(0.0))
}

pub(super) fn seconds_to_ms(seconds: f64) -> u32 {
    (seconds * 1000.0).round().clamp(0.0, u32::MAX as f64) as u32
}

/// The animation nodes of one effect, following the LibreOffice preset of the
/// same name with its durations scaled to `duration_ms`.
pub(super) fn odp_effect_nodes(effect: &str, target: &str, duration_ms: u64) -> String {
    let target = escape(target);
    let dur = smil_time(duration_ms);
    let visibility = |visible: bool, begin_ms: u64, dur: &str| {
        format!(
            "<anim:set smil:begin=\"{}\" smil:dur=\"{dur}\" smil:fill=\"hold\" smil:targetElement=\"{target}\" smil:attributeName=\"visibility\" smil:to=\"{}\"/>",
            smil_time(begin_ms),
            if visible { "visible" } else { "hidden" }
        )
    };
    let show = visibility(true, 0, "0.001s");
    let hide_at_end = visibility(false, duration_ms.saturating_sub(1), "0.001s");
    let animate = |attribute: &str, values: &str| {
        format!(
            "<anim:animate smil:dur=\"{dur}\" smil:fill=\"hold\" smil:targetElement=\"{target}\" smil:attributeName=\"{attribute}\" smil:values=\"{values}\" smil:keyTimes=\"0;1\"/>"
        )
    };
    let fade = |mode: &str| {
        format!(
            "<anim:transitionFilter smil:dur=\"{dur}\" smil:targetElement=\"{target}\" smil:type=\"fade\" smil:subtype=\"crossfade\"{mode}/>"
        )
    };
    let transform = |kind: &str, by: &str| {
        format!(
            "<anim:animateTransform smil:dur=\"{dur}\" smil:fill=\"hold\" smil:targetElement=\"{target}\" smil:by=\"{by}\" svg:type=\"{kind}\"/>"
        )
    };
    match effect {
        "fade" => format!("{show}{}", fade("")),
        "flyIn" => format!("{show}{}{}", animate("x", "x;x"), animate("y", "1+height/2;y")),
        "zoom" => format!("{show}{}{}", animate("width", "0;width"), animate("height", "0;height")),
        "disappear" => visibility(false, 0, &dur),
        "fadeOut" => format!("{}{hide_at_end}", fade(" smil:mode=\"out\"")),
        "flyOut" => format!("{}{}{hide_at_end}", animate("x", "x;x"), animate("y", "y;0-height/2")),
        "spin" => transform("rotate", "360"),
        "grow" => transform("scale", "1.5,1.5"),
        "shrink" => transform("scale", "0.5,0.5"),
        "pulse" => format!(
            "<anim:transitionFilter smil:dur=\"{dur}\" smil:targetElement=\"{target}\" smil:keySplines=\"0,0;0.2,0.5;0.8,0.5;1,0\" smil:type=\"fade\" smil:subtype=\"crossfade\" smil:mode=\"out\"/><anim:animateTransform smil:dur=\"{}\" smil:fill=\"hold\" smil:autoReverse=\"true\" smil:targetElement=\"{target}\" smil:by=\"1.05,1.05\" svg:type=\"scale\"/>",
            smil_time(duration_ms / 2)
        ),
        // "appear": visible for the whole effect so its duration survives.
        _ => visibility(true, 0, &dur),
    }
}

/// One "after previous" step of a click group in the main sequence.
pub(super) struct OdpTimingStep {
    begin_ms: u64,
    end_ms: u64,
    effects: String,
}

/// The slide's main sequence as ODF SMIL timing, the structure LibreOffice
/// writes: timing root -> main sequence -> one `anim:par` per click -> one per
/// "after previous" step -> one per effect. The effect `anim:par` carries the
/// trigger (`presentation:node-type`), class, preset and delay; its children
/// carry the duration and the target shape.
pub(super) fn odp_timing_xml(slide: &Slide, ids: &HashMap<String, String>, warnings: &mut Vec<String>) -> String {
    let mut animations: Vec<&Animation> = slide.animations.iter().collect();
    animations.sort_by_key(|animation| animation.order);
    let mut clicks: Vec<(&str, Vec<OdpTimingStep>)> = Vec::new();
    for animation in animations {
        let Some(target) = ids.get(&animation.object_id) else {
            warnings.push(format!(
                "An animation targeting \"{}\" was not written because the object is not on the exported slide.",
                animation.object_id
            ));
            continue;
        };
        if !matches!(animation.kind.as_str(), "entrance" | "exit" | "emphasis") {
            warnings.push(format!(
                "An animation of kind \"{}\" was kept in the native .oswk file and not written to the ODP timing.",
                animation.kind
            ));
            continue;
        }
        let (effect, exact) = odp_export_effect(&animation.kind, &animation.effect);
        let Some(&(_, _, preset_id, sub_type)) =
            ODP_PRESETS.iter().find(|preset| preset.0 == animation.kind && preset.1 == effect)
        else {
            continue;
        };
        if !exact {
            warnings.push(format!(
                "The {} effect \"{}\" has no LibreOffice preset and was written as \"{preset_id}\".",
                animation.kind, animation.effect
            ));
        }
        let node_type = match animation.trigger.as_str() {
            "withPrevious" => "with-previous",
            "afterPrevious" => "after-previous",
            _ => "on-click",
        };
        let first_step = || vec![OdpTimingStep { begin_ms: 0, end_ms: 0, effects: String::new() }];
        match (node_type, clicks.last_mut()) {
            ("on-click", _) => clicks.push(("indefinite", first_step())),
            // Effects before the first click start with the slide.
            (_, None) => clicks.push(("0s", first_step())),
            ("after-previous", Some((_, steps))) => {
                let begin_ms = steps.last().map(|step| step.end_ms).unwrap_or(0);
                steps.push(OdpTimingStep { begin_ms, end_ms: begin_ms, effects: String::new() });
            }
            _ => {}
        }
        let Some(step) = clicks.last_mut().and_then(|(_, steps)| steps.last_mut()) else { continue };
        let duration = u64::from(animation.duration_ms);
        let delay = u64::from(animation.delay_ms);
        let sub_type =
            if sub_type.is_empty() { String::new() } else { format!(" presentation:preset-sub-type=\"{sub_type}\"") };
        step.effects.push_str(&format!(
            "<anim:par smil:begin=\"{}\" smil:fill=\"hold\" presentation:node-type=\"{node_type}\" presentation:preset-class=\"{}\" presentation:preset-id=\"{preset_id}\"{sub_type}>{}</anim:par>",
            smil_time(delay),
            animation.kind,
            odp_effect_nodes(effect, target, duration)
        ));
        step.end_ms = step.end_ms.max(step.begin_ms + delay + duration);
    }
    if clicks.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "<anim:par presentation:node-type=\"timing-root\"><anim:seq presentation:node-type=\"main-sequence\">",
    );
    for (begin, steps) in clicks {
        out.push_str(&format!("<anim:par smil:begin=\"{begin}\">"));
        for step in steps {
            out.push_str(&format!("<anim:par smil:begin=\"{}\">{}</anim:par>", smil_time(step.begin_ms), step.effects));
        }
        out.push_str("</anim:par>");
    }
    out.push_str("</anim:seq></anim:par>");
    out
}
