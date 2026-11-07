//! Line annotation captions: `/Cap`, `/CP` and `/CO`.
//!
//! A Line annotation may carry text that is "replicated as a caption in the
//! appearance of the line" (`/Cap true`, ISO 32000-1 §12.5.6.7 Table 175). When
//! the document has an appearance stream the caption is already inside it and
//! this module never runs; when the imposition synthesizes the appearance
//! because there is none, the caption would otherwise be lost with it.
//!
//! `/CP` picks the position - `/Inline` (the default) puts the text on the line
//! and breaks the stroke around it, `/Top` puts it above - and `/CO` moves it
//! from the line's midpoint, `h` along the line and `v` across it. A Line has no
//! `/DA`, so the caption is set in Helvetica at [`SIZE`] and filled with the
//! line's own colour, which is what viewers draw.
//!
//! Text the standard font cannot show means no caption at all: printing
//! replacement glyphs where the user wrote something else is worse than leaving
//! the line bare, which is the rule the FreeText slice set for synthesized text.

use std::collections::BTreeSet;

use lopdf::{Dictionary, Document, Object};

use crate::docutil::pdf_text_value;

use super::super::super::dests::resolve;
use super::super::FontSpec;
use super::paths::Point;
use super::std14::{Family, Std14};
use super::textfit::Std14Metrics;
use super::{number, number_text, textfit, union, Color};

/// The font a caption is set in. Helvetica is the reference implementation's
/// choice for annotation text and needs no embedded program.
pub(super) const HELVETICA: Std14 = Std14 { family: Family::Helvetica, bold: false, italic: false };

/// Caption size in points: the specification gives none for a Line, so this is
/// the size viewers in the wild use for annotation text.
const SIZE: f64 = 9.0;

/// AFM `Descender` magnitude of Helvetica, in 1/1000 em.
const DESCENT: f64 = 207.0;

/// Clear space between the caption and the broken stroke, in points.
const GAP: f64 = 1.0;

/// A caption is a caption, not a document: longer text is truncated.
const MAX_TEXT: usize = 1024;

/// Largest offset a `/CO` entry may carry (the extent of a user-space unit).
const MAX_OFFSET: f64 = 14_400.0;

/// What `/Cap`, `/CP` and `/CO` say, once the text is known to be printable.
struct Caption {
    codes: Codes,
    width: f64,
    top: bool,
    offset: (f64, f64),
}

/// How the caption's text is written: WinAnsi through a standard font, or the
/// bundled subset face (`<hex>` of two-byte glyph ids) when WinAnsi cannot
/// carry every character - Turkish `ğ`, `ı`, `ş`, `İ` among them.
enum Codes {
    WinAnsi(Vec<u8>),
    Subset { hex: String, chars: BTreeSet<char> },
}

/// The caption: operators, the box they cover, and where the stroke breaks.
pub(super) struct CaptionBox {
    pub(super) ops: String,
    pub(super) bbox: [f64; 4],
    pub(super) gap: Option<(Point, Point)>,
    /// The characters the form needs a subset font for, if any.
    chars: Option<BTreeSet<char>>,
}

impl CaptionBox {
    /// The font resource the form has to declare for these operators.
    pub(super) fn font(&self) -> FontSpec {
        match &self.chars {
            Some(chars) => FontSpec::Subset(chars.clone()),
            None => FontSpec::Standard(HELVETICA),
        }
    }
}

/// Builds the caption of a synthesized Line appearance, or `None` when the
/// annotation has none, its text cannot be shown, or the geometry is degenerate.
pub(super) fn synthesize(
    doc: &Document,
    annot: &Dictionary,
    start: Point,
    end: Point,
    stroke: f64,
    colour: &Color,
) -> Option<CaptionBox> {
    let caption = read(doc, annot)?;
    let (dx, dy) = (end.0 - start.0, end.1 - start.1);
    let length = dx.hypot(dy);
    if length < 1e-9 {
        return None;
    }
    let (ux, uy) = (dx / length, dy / length);
    let (nx, ny) = (-uy, ux);
    let mid = ((start.0 + end.0) / 2.0, (start.1 + end.1) / 2.0);
    let ascent = SIZE * f64::from(HELVETICA.ascent()) / 1000.0;
    let descent = SIZE * DESCENT / 1000.0;

    // The baseline origin starts at the midpoint, shifted so the text is
    // centred on the line (`/Inline`) or sitting just above it (`/Top`).
    let (mut ox, mut oy) = (mid.0 - ux * caption.width / 2.0, mid.1 - uy * caption.width / 2.0);
    let mut gap = None;
    if caption.top {
        let lift = stroke / 2.0 + descent + GAP;
        ox += nx * lift;
        oy += ny * lift;
    } else {
        ox -= nx * ascent / 2.0;
        oy -= ny * ascent / 2.0;
        let (from, to) = (length / 2.0 - caption.width / 2.0 - GAP, length / 2.0 + caption.width / 2.0 + GAP);
        if from > 0.0 && to < length {
            gap = Some(((start.0 + ux * from, start.1 + uy * from), (start.0 + ux * to, start.1 + uy * to)));
        }
    }
    ox += ux * caption.offset.0 + nx * caption.offset.1;
    oy += uy * caption.offset.0 + ny * caption.offset.1;

    // The glyph box: along the line for the advance, and from the descender to
    // the ascender so a tail ('p', 'y') stays inside the box too.
    let across = |distance: f64| (ox + nx * distance, oy + ny * distance);
    let along = |distance: f64| (distance * ux, distance * uy);
    let (ax, ay) = along(caption.width);
    let corners = [
        across(-descent),
        (across(-descent).0 + ax, across(-descent).1 + ay),
        (across(ascent).0 + ax, across(ascent).1 + ay),
        across(ascent),
    ];
    let bbox = corners.iter().fold([corners[0].0, corners[0].1, corners[0].0, corners[0].1], |box_, corner| {
        [box_[0].min(corner.0), box_[1].min(corner.1), box_[2].max(corner.0), box_[3].max(corner.1)]
    });

    let font = match caption.codes {
        Codes::WinAnsi(_) => "FOmni0",
        Codes::Subset { .. } => crate::textfont::RESOURCE_NAME,
    };
    let shown = match &caption.codes {
        Codes::WinAnsi(codes) => format!("<{}>", hex(codes)),
        Codes::Subset { hex, .. } => hex.clone(),
    };
    let mut ops = String::from("BT\n");
    ops.push_str(&format!("/{font} {} Tf\n", number_text(SIZE)));
    ops.push_str(&colour.render(colour.fill_op()));
    ops.push_str(&format!(
        "{} {} {} {} {} {} Tm\n{shown} Tj\nET\n",
        number_text(ux),
        number_text(uy),
        number_text(-uy),
        number_text(ux),
        number_text(ox),
        number_text(oy),
    ));
    let chars = match &caption.codes {
        Codes::Subset { chars, .. } => Some(chars.clone()),
        Codes::WinAnsi(_) => None,
    };
    Some(CaptionBox { ops, bbox, gap, chars })
}

/// Reads the caption a Line annotation describes, if it has one worth drawing.
fn read(doc: &Document, annot: &Dictionary) -> Option<Caption> {
    // `/Cap` is a boolean, and anything but `true` means "no caption".
    if !matches!(annot.get(b"Cap").ok(), Some(Object::Boolean(true))) {
        return None;
    }
    // `/Contents` is a text string; a name or a number there is not a caption.
    let contents = resolve(doc, annot.get(b"Contents").ok()?);
    if !matches!(contents, Object::String(_, _)) {
        return None;
    }
    let text = pdf_text_value(contents)?;
    let flat = text.chars().map(|symbol| if symbol.is_control() { ' ' } else { symbol }).collect::<String>();
    let flat = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.is_empty() {
        return None;
    }
    // WinAnsi first: a caption the standard font can show is unchanged. When it
    // cannot - Turkish letters, broader Latin - the bundled face is subset
    // instead, and only text that face cannot show either means no caption.
    let (codes, width) = match textfit::encode(&flat) {
        Some(mut codes) => {
            codes.truncate(MAX_TEXT);
            let width = f64::from(textfit::width_of(&Std14Metrics(HELVETICA), &codes)) * SIZE / 1000.0;
            (Codes::WinAnsi(codes), width)
        }
        None => {
            let text: String = flat.chars().take(MAX_TEXT).collect();
            let face = crate::textfont::SubsetFace::bundled()?;
            if !face.can_encode(&text) {
                return None;
            }
            let chars = text.chars().collect();
            (Codes::Subset { hex: face.encode(&text), chars }, face.measure(&text, SIZE))
        }
    };
    if !width.is_finite() || width <= 0.0 {
        return None;
    }
    let top =
        matches!(annot.get(b"CP").ok().map(|value| resolve(doc, value)), Some(Object::Name(name)) if name == b"Top");
    Some(Caption { codes, width, top, offset: offset_of(doc, annot) })
}

/// `/CO`, clamped: a missing, malformed or non-finite entry means the middle.
fn offset_of(doc: &Document, annot: &Dictionary) -> (f64, f64) {
    let items = annot.get(b"CO").ok().map(|value| resolve(doc, value)).and_then(|value| value.as_array().ok().cloned());
    let Some(items) = items.filter(|items| items.len() == 2) else { return (0.0, 0.0) };
    let read = |value: &Object| {
        number(value).filter(|number| number.is_finite()).unwrap_or(0.0).clamp(-MAX_OFFSET, MAX_OFFSET)
    };
    (read(&items[0]), read(&items[1]))
}

/// Hex, because a hex string needs no escaping for `(`, `)` or `\`.
fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02X}"));
    }
    out
}

/// The widest the caption makes the annotation's box: used by the caller to
/// keep `/BBox` around the text as well as the line.
pub(super) fn union_with(box_: [f64; 4], caption: &CaptionBox) -> [f64; 4] {
    union(box_, caption.bbox)
}

#[cfg(test)]
#[path = "caption_tests.rs"]
mod tests;
