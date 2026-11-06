//! Synthesizes an appearance for annotations that carry none.
//!
//! Many producers ship a highlight or a square without an `/AP` stream and let
//! the viewer paint it from the annotation's geometry. An imposed sheet has no
//! viewer to do that, so this module turns the geometry into operators. The
//! operators are emitted in **page space** and wrapped in a Form XObject whose
//! box is the annotation rectangle (expanded by half the border width for the
//! line kinds), so the placement matrix that already maps page space onto the
//! sheet applies unchanged.
//!
//! Slice one covers the kinds whose appearance needs no font: the quad-point
//! markup (`Highlight`, `Underline`, `Squiggly`, `StrikeOut`) and the shapes
//! (`Square`, `Circle`, `Line`, `Ink`). `FreeText`, `Stamp` and the text-note
//! icon stay out of scope; `/LE`, `/BE`, dashed borders and ink smoothing are
//! documented as known simplifications.

use super::super::dests::resolve;
use super::super::links::number;
use super::Synth;
use lopdf::{Dictionary, Document};

mod quads;

/// A Bézier constant for a quarter circle.
const KAPPA: f64 = 0.552_284_749_8;
/// Guards against absurd inputs rather than for a real document.
const MAX_INK_POINTS: usize = 20_000;
const MAX_INK_STROKES: usize = 1_000;

/// Builds the appearance of one annotation, or `None` when it cannot be drawn.
pub(super) fn synthesize(doc: &Document, annot: &Dictionary, subtype: &[u8], rect: [f64; 4]) -> Option<Synth> {
    match subtype {
        b"Highlight" => quads::synthesize(doc, annot, rect, quads::QuadKind::Highlight),
        b"Underline" => quads::synthesize(doc, annot, rect, quads::QuadKind::Underline),
        b"Squiggly" => quads::synthesize(doc, annot, rect, quads::QuadKind::Squiggly),
        b"StrikeOut" => quads::synthesize(doc, annot, rect, quads::QuadKind::StrikeOut),
        b"Square" => shape(doc, annot, rect, ShapeKind::Square),
        b"Circle" => shape(doc, annot, rect, ShapeKind::Circle),
        b"Line" => line(doc, annot, rect),
        b"Ink" => ink(doc, annot, rect),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Colors and borders
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub(super) struct Color {
    values: Vec<f64>,
}

impl Color {
    pub(super) fn fill_op(&self) -> &'static str {
        match self.values.len() {
            1 => "g",
            3 => "rg",
            _ => "k",
        }
    }

    pub(super) fn stroke_op(&self) -> &'static str {
        match self.values.len() {
            1 => "G",
            3 => "RG",
            _ => "K",
        }
    }

    pub(super) fn render(&self, op: &str) -> String {
        let mut out = String::new();
        for value in &self.values {
            out.push_str(&number_text(*value));
            out.push(' ');
        }
        out.push_str(op);
        out.push('\n');
        out
    }
}

/// `Some(None)` means the key is absent (transparent), `Some(Some(color))` a
/// usable color and `None` a malformed one that invalidates the annotation.
pub(super) fn color(doc: &Document, annot: &Dictionary, key: &[u8]) -> Option<Option<Color>> {
    let Ok(value) = annot.get(key) else { return Some(None) };
    let value = resolve(doc, value);
    let items = value.as_array().ok()?;
    if items.is_empty() {
        return Some(None);
    }
    if !matches!(items.len(), 1 | 3 | 4) {
        return None;
    }
    let mut values = Vec::with_capacity(items.len());
    for item in items {
        let value = number(item)?;
        values.push(value.clamp(0.0, 1.0));
    }
    Some(Some(Color { values }))
}

/// `/BS /W`, then the legacy `/Border` third entry, then a hairline.
pub(super) fn border_width(doc: &Document, annot: &Dictionary) -> Option<f64> {
    let width = annot
        .get(b"BS")
        .ok()
        .map(|value| resolve(doc, value))
        .and_then(|value| value.as_dict().ok().and_then(|dict| dict.get(b"W").ok().and_then(number)))
        .or_else(|| {
            annot.get(b"Border").ok().and_then(|value| {
                let value = resolve(doc, value);
                let items = value.as_array().ok()?;
                items.get(2).and_then(number)
            })
        })
        .unwrap_or(1.0);
    (width.is_finite() && (0.0..=1_000.0).contains(&width)).then_some(width)
}

// ---------------------------------------------------------------------------
// Shapes
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum ShapeKind {
    Square,
    Circle,
}

fn shape(doc: &Document, annot: &Dictionary, rect: [f64; 4], kind: ShapeKind) -> Option<Synth> {
    let stroke = color(doc, annot, b"C")?;
    let fill = color(doc, annot, b"IC")?;
    let width = border_width(doc, annot)?;
    let has_stroke = stroke.is_some() && width > 0.0;
    if stroke.is_none() && fill.is_none() {
        return None;
    }
    let [x0, y0, x1, y1] = rect;
    let inner = width / 2.0;
    let (left, bottom, right, top) = (x0 + inner, y0 + inner, x1 - inner, y1 - inner);
    if right - left <= 1e-6 || top - bottom <= 1e-6 {
        return None;
    }
    let mut ops = String::new();
    if has_stroke {
        if let Some(stroke) = &stroke {
            ops.push_str(&stroke.render(stroke.stroke_op()));
        }
        ops.push_str(&format!("{} w\n", number_text(width)));
    }
    if let Some(fill) = &fill {
        ops.push_str(&fill.render(fill.fill_op()));
    }
    match kind {
        ShapeKind::Square => ops.push_str(&format!(
            "{} {} {} {} re\n",
            number_text(left),
            number_text(bottom),
            number_text(right - left),
            number_text(top - bottom)
        )),
        ShapeKind::Circle => ellipse(&mut ops, left, bottom, right, top),
    }
    ops.push_str(if has_stroke && fill.is_some() {
        "B\n"
    } else if has_stroke {
        "S\n"
    } else {
        "f\n"
    });
    Some(Synth { ops: ops.into_bytes(), bbox: rect, multiply: false })
}

fn ellipse(ops: &mut String, left: f64, bottom: f64, right: f64, top: f64) {
    let (cx, cy) = ((left + right) / 2.0, (bottom + top) / 2.0);
    let (rx, ry) = ((right - left) / 2.0, (top - bottom) / 2.0);
    let (kx, ky) = (KAPPA * rx, KAPPA * ry);
    let point = |x: f64, y: f64| format!("{} {} ", number_text(x), number_text(y));
    ops.push_str(&format!("{}m\n", point(cx + rx, cy)));
    ops.push_str(&format!("{}{}{}c\n", point(cx + rx, cy + ky), point(cx + kx, cy + ry), point(cx, cy + ry)));
    ops.push_str(&format!("{}{}{}c\n", point(cx - kx, cy + ry), point(cx - rx, cy + ky), point(cx - rx, cy)));
    ops.push_str(&format!("{}{}{}c\n", point(cx - rx, cy - ky), point(cx - kx, cy - ry), point(cx, cy - ry)));
    ops.push_str(&format!("{}{}{}c\nh\n", point(cx + kx, cy - ry), point(cx + rx, cy - ky), point(cx + rx, cy)));
}

fn line(doc: &Document, annot: &Dictionary, rect: [f64; 4]) -> Option<Synth> {
    // A leader line shifts the stroke sideways; ignoring it would draw the
    // line in the wrong place, so the annotation is skipped instead.
    if annot.get(b"LL").ok().and_then(number).is_some_and(|value| value != 0.0) {
        return None;
    }
    let points = resolve(doc, annot.get(b"L").ok()?);
    let items = points.as_array().ok()?;
    if items.len() != 4 {
        return None;
    }
    let values: Vec<f64> = items.iter().map(number).collect::<Option<_>>()?;
    let colour = color(doc, annot, b"C")??;
    let width = border_width(doc, annot)?;
    if width <= 0.0 {
        return None;
    }
    let mut ops = colour.render(colour.stroke_op());
    ops.push_str(&format!("{} w\n", number_text(width)));
    ops.push_str(&format!(
        "{} {} m\n{} {} l\nS\n",
        number_text(values[0]),
        number_text(values[1]),
        number_text(values[2]),
        number_text(values[3])
    ));
    let half = width / 2.0;
    let bounds = union(
        rect,
        [
            values[0].min(values[2]) - half,
            values[1].min(values[3]) - half,
            values[0].max(values[2]) + half,
            values[1].max(values[3]) + half,
        ],
    );
    Some(Synth { ops: ops.into_bytes(), bbox: bounds, multiply: false })
}

fn ink(doc: &Document, annot: &Dictionary, rect: [f64; 4]) -> Option<Synth> {
    let list = resolve(doc, annot.get(b"InkList").ok()?);
    let strokes = list.as_array().ok()?;
    if strokes.is_empty() || strokes.len() > MAX_INK_STROKES {
        return None;
    }
    let colour = color(doc, annot, b"C")??;
    let width = border_width(doc, annot)?;
    if width <= 0.0 {
        return None;
    }
    let mut ops = String::from("1 J\n1 j\n");
    ops.push_str(&colour.render(colour.stroke_op()));
    ops.push_str(&format!("{} w\n", number_text(width)));
    let mut total = 0usize;
    let mut bounds = rect;
    for stroke in strokes {
        let points = resolve(doc, stroke);
        let items = points.as_array().ok()?;
        if items.len() < 2 || !items.len().is_multiple_of(2) {
            return None;
        }
        total += items.len() / 2;
        if total > MAX_INK_POINTS {
            return None;
        }
        let mut coordinates = Vec::with_capacity(items.len());
        for item in items {
            coordinates.push(number(item)?);
        }
        for (index, pair) in coordinates.chunks(2).enumerate() {
            let command = if index == 0 { "m" } else { "l" };
            ops.push_str(&format!("{} {} {command}\n", number_text(pair[0]), number_text(pair[1])));
        }
        for pair in coordinates.chunks(2) {
            bounds = union(bounds, [pair[0], pair[1], pair[0], pair[1]]);
        }
    }
    ops.push_str("S\n");
    let half = width / 2.0;
    Some(Synth {
        ops: ops.into_bytes(),
        bbox: [bounds[0] - half, bounds[1] - half, bounds[2] + half, bounds[3] + half],
        multiply: false,
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub(super) fn union(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]
}

pub(super) fn number_text(value: f64) -> String {
    if !value.is_finite() {
        return "0".into();
    }
    let text = format!("{value:.4}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-0" {
        "0".into()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Object};

    fn doc() -> Document {
        Document::with_version("1.7")
    }

    fn annot(key: &str, value: Object) -> Dictionary {
        let mut dict = Dictionary::new();
        dict.set(key, value);
        dict
    }

    fn real(value: f64) -> Object {
        Object::Real(value as f32)
    }

    #[test]
    fn numbers_are_written_without_trailing_zeros() {
        assert_eq!(number_text(100.0), "100");
        assert_eq!(number_text(0.5), "0.5");
        assert_eq!(number_text(-0.0), "0");
        assert_eq!(number_text(f64::NAN), "0");
        assert_eq!(number_text(1.25), "1.25");
    }

    #[test]
    fn colors_are_read_by_length() {
        let doc = doc();
        let missing = Dictionary::new();
        assert!(matches!(color(&doc, &missing, b"C"), Some(None)));
        let empty = annot("C", Object::Array(Vec::new()));
        assert!(matches!(color(&doc, &empty, b"C"), Some(None)));
        let gray = annot("C", Object::Array(vec![real(0.5)]));
        assert_eq!(color(&doc, &gray, b"C").unwrap().unwrap().fill_op(), "g");
        let rgb = annot("C", Object::Array(vec![real(1.0), real(0.0), real(0.0)]));
        assert_eq!(color(&doc, &rgb, b"C").unwrap().unwrap().stroke_op(), "RG");
        let cmyk = annot("C", Object::Array(vec![real(0.0); 4]));
        assert_eq!(color(&doc, &cmyk, b"C").unwrap().unwrap().fill_op(), "k");
        let wrong = annot("C", Object::Array(vec![real(1.0), real(0.0)]));
        assert!(color(&doc, &wrong, b"C").is_none());
        let text = annot("C", Object::Array(vec![Object::string_literal("red")]));
        assert!(color(&doc, &text, b"C").is_none());
        // Out-of-range values are clamped rather than refused.
        let loud = annot("C", Object::Array(vec![real(5.0), real(-2.0), real(0.0)]));
        let loud = color(&doc, &loud, b"C").unwrap().unwrap();
        assert_eq!(loud.values, vec![1.0, 0.0, 0.0]);
    }

    #[test]
    fn the_border_width_falls_back_through_bs_and_border() {
        let doc = doc();
        assert_eq!(border_width(&doc, &Dictionary::new()), Some(1.0));
        let bs = annot("BS", Object::Dictionary(dictionary! { "W" => 2i64 }));
        assert_eq!(border_width(&doc, &bs), Some(2.0));
        let legacy = annot("Border", Object::Array(vec![real(0.0), real(0.0), real(3.0)]));
        assert_eq!(border_width(&doc, &legacy), Some(3.0));
        let negative = annot("BS", Object::Dictionary(dictionary! { "W" => -1i64 }));
        assert_eq!(border_width(&doc, &negative), None);
        let absurd = annot("BS", Object::Dictionary(dictionary! { "W" => 5_000i64 }));
        assert_eq!(border_width(&doc, &absurd), None);
    }

    #[test]
    fn rectangles_union() {
        assert_eq!(union([0.0, 0.0, 10.0, 10.0], [5.0, -5.0, 20.0, 5.0]), [0.0, -5.0, 20.0, 10.0]);
    }
}
