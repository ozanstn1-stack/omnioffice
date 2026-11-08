//! Synthesizes the appearance of a `FreeText` annotation (slice A: the
//! standard 14 fonts).
//!
//! A free-text annotation carries its text in `/Contents`, its styling in `/DA`
//! (`/AcroForm /DA` is the fallback), and its box in `/Rect` (narrowed by
//! `/RD`). Nothing of that is drawn when the document ships no `/AP`, so an
//! imposed sheet has to lay the text out itself.
//!
//! The output references a standard font by name - Helvetica, Times or Courier
//! with its bold/italic member - and declares the Turkish and Latin-A glyphs
//! WinAnsi lacks through `/Differences`, so no font program is embedded and the
//! file stays small.

use super::super::super::dests::resolve;
use super::super::super::links::number;
use super::face::{Face, HELVETICA};
use super::std14::{Family, Std14};
use super::textfit::{decode, encode, width_of, wrap_with};
use super::{cidfont::FontCache, drfont};
use super::{color, Color, Synth};
use lopdf::{Dictionary, Document};

/// Space between the border and the text, in points.
const PADDING: f64 = 2.0;
/// Never lay out more than this many lines, whatever the box says.
const MAX_LINES: usize = 512;
/// Line spacing as a multiple of the font size.
const LEADING: f64 = 1.2;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Align {
    Left,
    Center,
    Right,
}

/// What `/DA` says about the text: font, size and colour.
struct Style {
    face: Face,
    /// The `/DA` font name, which a composite font is declared under.
    name: String,
    size: f64,
    colour: Color,
}

/// Builds the appearance of one `FreeText` annotation.
pub(super) fn synthesize(doc: &Document, annot: &Dictionary, rect: [f64; 4], fonts: &mut FontCache) -> Option<Synth> {
    let style = style_of(doc, annot)?;
    let align = match annot.get(b"Q").ok().and_then(|value| value.as_i64().ok()) {
        Some(1) => Align::Center,
        Some(2) => Align::Right,
        _ => Align::Left,
    };
    let stroke = super::border::stroke(doc, annot)?;
    let border = stroke.width;
    let [x0, y0, x1, y1] = inset(doc, annot, rect)?;
    let mut ops = String::from("q\n");
    if let Some(background) = color(doc, annot, b"C")? {
        ops.push_str(&background.render(background.fill_op()));
        ops.push_str(&format!("{} {} {} {} re f\n", n(x0), n(y0), n(x1 - x0), n(y1 - y0)));
    }
    if border > 0.0 {
        if stroke.edge == super::border::Edge::Plain {
            let half = border / 2.0;
            ops.push_str(&style.colour.render(style.colour.stroke_op()));
            stroke.ops(&mut ops);
            ops.push_str(&format!(
                "{} {} {} {} re S\n",
                n(x0 + half),
                n(y0 + half),
                n(x1 - x0 - border),
                n(y1 - y0 - border)
            ));
        } else {
            super::bevel::rect_frame(&mut ops, [x0, y0, x1, y1], &stroke, &style.colour);
        }
    }
    callout(doc, annot, &style, &stroke, &mut ops);

    let text = annot
        .get(b"Contents")
        .ok()
        .map(|value| resolve(doc, value))
        .and_then(|value| value.as_str().ok())
        .and_then(decode);
    let mut style = style;
    if let Some(text) = text.filter(|text| !text.trim().is_empty()) {
        // A document font is used only when it can show every character; a
        // missing glyph would print as a blank, so the note as a whole falls
        // back to a standard face.
        if !style.face.can_show(&text) {
            style.face = Face::Standard(HELVETICA);
        }
        // WinAnsi cannot carry every script: when it falls short the bundled
        // face is tried through its slots, then the document's own composite
        // font, and a character no face can show still drops the note rather
        // than printing a guess.
        if matches!(style.face, Face::Standard(_)) && encode(&text).is_none() {
            style.face = Face::script_for(doc, annot, &style.name, &text, fonts)?;
        }
        layout(&style, align, border, [x0, y0, x1, y1], &text, &mut ops);
    }
    let spec = style.face.spec();
    ops.push_str("Q\n");
    Some(Synth { ops: ops.into_bytes(), bbox: rect, multiply: false, font: Some(spec) })
}

/// The box after `/RD` and the border are taken off.
fn inset(doc: &Document, annot: &Dictionary, rect: [f64; 4]) -> Option<[f64; 4]> {
    let [mut x0, mut y0, mut x1, mut y1] = rect;
    if let Ok(value) = annot.get(b"RD") {
        let value = resolve(doc, value);
        if let Ok(items) = value.as_array() {
            if items.len() == 4 {
                let read: Vec<f64> = items.iter().map(|item| number(item).unwrap_or(0.0)).collect();
                let (left, top, right, bottom) = (read[0], read[1], read[2], read[3]);
                let (nx0, ny0, nx1, ny1) = (x0 + left, y0 + bottom, x1 - right, y1 - top);
                if nx1 > nx0 && ny1 > ny0 {
                    x0 = nx0;
                    y0 = ny0;
                    x1 = nx1;
                    y1 = ny1;
                }
            }
        }
    }
    (x1 > x0 && y1 > y0).then_some([x0, y0, x1, y1])
}

/// A callout line from `/CL` (four or six numbers).
fn callout(doc: &Document, annot: &Dictionary, style: &Style, stroke: &super::border::Stroke, ops: &mut String) {
    let Ok(value) = annot.get(b"CL") else { return };
    let value = resolve(doc, value);
    let Ok(items) = value.as_array() else { return };
    if !matches!(items.len(), 4 | 6) {
        return;
    }
    let Some(points) = items.iter().map(number).collect::<Option<Vec<f64>>>() else { return };
    ops.push_str(&style.colour.render(style.colour.stroke_op()));
    stroke.ops(ops);
    for pair in points.chunks(2) {
        let command = if pair.as_ptr() == points.as_ptr() { "m" } else { "l" };
        ops.push_str(&format!("{} {} {command}\n", n(pair[0]), n(pair[1])));
    }
    ops.push_str("S\n");
    let ending = super::ends::single(doc, annot);
    if ending != super::ends::Ending::None && points.len() >= 2 {
        let (ux, uy) = (points[0] - points[2], points[1] - points[3]);
        let length = ux.hypot(uy);
        if length > 1e-9 {
            let fill = super::color(doc, annot, b"IC").flatten().map(|colour| colour.render(colour.fill_op()));
            super::ends::end_ops(
                ops,
                ending,
                (points[0], points[1]),
                (ux / length, uy / length),
                stroke.width.max(1.0),
                fill.as_deref(),
            );
        }
    }
}

/// Lays the text out in the box and appends the `BT … ET` block.
fn layout(style: &Style, align: Align, border: f64, box_: [f64; 4], text: &str, ops: &mut String) {
    let size = style.size;
    let pad = border + PADDING;
    let tx0 = box_[0] + pad;
    let tx1 = box_[2] - pad;
    let ty0 = box_[1] + pad;
    let ty1 = box_[3] - pad;
    let max_width = tx1 - tx0;
    ops.push_str(&format!("{} {} {} {} re W n\n", n(tx0), n(ty0), n(max_width.max(0.0)), n((ty1 - ty0).max(0.0))));
    if max_width <= size || ty1 - ty0 < size {
        return;
    }
    style.face.with_metrics(|metrics| {
        let lines = wrap_with(metrics, size as f32, text, max_width as f32, MAX_LINES);
        let lead = LEADING * size;
        let first = ty1 - size * f64::from(metrics.ascent()) / 1000.0;
        ops.push_str(&format!(
            "BT\n/{} {} Tf\n{}\n",
            style.face.resource_key(),
            n(size),
            style.colour.render(style.colour.fill_op()).trim_end()
        ));
        for (index, line) in lines.iter().enumerate() {
            let baseline = first - index as f64 * lead;
            if baseline < ty0 - size {
                break;
            }
            let line_width = f64::from(width_of(metrics, line)) * size / 1000.0;
            let x = match align {
                Align::Left => tx0,
                Align::Center => tx0 + (max_width - line_width) / 2.0,
                Align::Right => tx1 - line_width,
            };
            ops.push_str(&format!("1 0 0 1 {} {} Tm\n{} Tj\n", n(x), n(baseline), style.face.operand(line)));
        }
        ops.push_str("ET\n");
    });
}
/// The `/DA` chain: the annotation's own, then the form's, then a default.
fn style_of(doc: &Document, annot: &Dictionary) -> Option<Style> {
    let own = annot
        .get(b"DA")
        .ok()
        .map(|value| resolve(doc, value))
        .and_then(|value| value.as_str().ok().map(<[u8]>::to_vec));
    let form = doc
        .catalog()
        .ok()
        .and_then(|catalog| catalog.get(b"AcroForm").ok().cloned())
        .and_then(|form| resolve(doc, &form).as_dict().ok().cloned())
        .and_then(|form| form.get(b"DA").ok().and_then(|value| value.as_str().ok()).map(<[u8]>::to_vec));
    let parsed = own.as_deref().and_then(parse_da).or_else(|| form.as_deref().and_then(parse_da)).unwrap_or_default();
    let face = match drfont::resolve_font(doc, annot, &parsed.font_name) {
        Some(metrics) => Face::Document { key: parsed.font_name.clone(), id: metrics.id, metrics },
        None => Face::Standard(font_of(doc, &parsed.font_name)),
    };
    let size = parsed.size.unwrap_or(12.0);
    if !size.is_finite() || size <= 0.0 || size > 1000.0 {
        return None;
    }
    let colour = parsed.colour.unwrap_or(Color { values: vec![0.0] });
    Some(Style { face, name: parsed.font_name.clone(), size, colour })
}

#[derive(Default)]
struct ParsedDa {
    font_name: String,
    size: Option<f64>,
    colour: Option<Color>,
}

/// Reads the last `Tf`, the last colour operator and a sane size from `/DA`.
fn parse_da(raw: &[u8]) -> Option<ParsedDa> {
    let text = std::str::from_utf8(raw).ok()?;
    let mut out = ParsedDa::default();
    let mut tokens: Vec<String> = Vec::new();
    for token in text.split([' ', '\n', '\r', '\t']) {
        if token.is_empty() {
            continue;
        }
        tokens.push(token.to_string());
        let op = token;
        match op {
            "Tf" if tokens.len() >= 3 => {
                let name = tokens[tokens.len() - 3].clone();
                let size = tokens[tokens.len() - 2].parse::<f64>().ok();
                out.font_name = name.trim_start_matches('/').to_string();
                out.size = size;
            }
            "g" | "rg" | "k" => {
                let wanted = if op == "g" {
                    1
                } else if op == "rg" {
                    3
                } else {
                    4
                };
                if tokens.len() > wanted {
                    let start = tokens.len() - wanted - 1;
                    let values: Option<Vec<f64>> =
                        tokens[start..start + wanted].iter().map(|t| t.parse().ok()).collect();
                    if let Some(values) = values {
                        out.colour = Some(Color { values: values.into_iter().map(|v| v.clamp(0.0, 1.0)).collect() });
                    }
                }
            }
            _ => {}
        }
        if tokens.len() > 64 {
            tokens.drain(..32);
        }
    }
    Some(out)
}

/// Chooses a standard font from a resource name, then from `/DR`.
fn font_of(doc: &Document, resource: &str) -> Std14 {
    let mut base = resource.to_string();
    let dr_base = doc
        .catalog()
        .ok()
        .and_then(|catalog| catalog.get(b"AcroForm").ok().cloned())
        .and_then(|form| resolve(doc, &form).as_dict().ok().cloned())
        .and_then(|form| form.get(b"DR").ok().cloned())
        .and_then(|dr| resolve(doc, &dr).as_dict().ok().cloned())
        .and_then(|dr| dr.get(b"Font").ok().cloned())
        .and_then(|fonts| resolve(doc, &fonts).as_dict().ok().cloned())
        .and_then(|fonts| fonts.get(resource.as_bytes()).ok().cloned())
        .and_then(|font| resolve(doc, &font).as_dict().ok().cloned())
        .and_then(|font| font.get(b"BaseFont").ok().and_then(|value| value.as_name().ok()).map(<[u8]>::to_vec));
    if let Some(name) = dr_base {
        base = String::from_utf8_lossy(&name).to_string();
    }
    let lower = base.to_ascii_lowercase();
    // Both full names and the four-letter abbreviations Acrobat writes
    // (`TiRo`, `HeBo`, `TiIt`, `TiBI`, `Cour`).
    let times = lower.contains("times")
        || lower.contains("tiro")
        || lower.contains("tibo")
        || lower.contains("tiit")
        || lower.contains("tibi")
        || (lower.contains("serif") && !lower.contains("sans"));
    let family = if lower.contains("cour") || lower.contains("mono") {
        Family::Courier
    } else if times {
        Family::Times
    } else {
        Family::Helvetica
    };
    Std14 {
        family,
        bold: lower.contains("bold") || lower.contains("hebo") || lower.contains("tibo"),
        italic: lower.contains("italic") || lower.contains("oblique") || lower.contains("it"),
    }
}

fn n(value: f64) -> String {
    super::number_text(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_appearance_sets_the_style() {
        let parsed = parse_da(b"/Helv 12 Tf 0 g").expect("parsed");
        assert_eq!(parsed.font_name, "Helv");
        assert_eq!(parsed.size, Some(12.0));
        assert_eq!(parsed.colour.expect("colour").values, vec![0.0]);
        // The last colour operator wins.
        let parsed = parse_da(b"0 0 1 rg /TiRo 9 Tf 1 0 0 rg").expect("parsed");
        assert_eq!(parsed.font_name, "TiRo");
        assert_eq!(parsed.size, Some(9.0));
        assert_eq!(parsed.colour.expect("colour").values, vec![1.0, 0.0, 0.0]);
        assert!(parse_da(b"0 Tf").is_some());
        assert!(parse_da(b"\xff\xfe").is_none());
    }

    #[test]
    fn the_family_follows_the_resource_name() {
        let doc = Document::with_version("1.7");
        assert_eq!(font_of(&doc, "Cour").family, Family::Courier);
        assert_eq!(font_of(&doc, "TiRo").family, Family::Times);
        assert_eq!(font_of(&doc, "Helv").family, Family::Helvetica);
        assert!(!font_of(&doc, "Helv").bold);
    }
}
