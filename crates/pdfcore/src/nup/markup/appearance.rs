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
//! The quad-point markup, the shapes, `FreeText` and widgets are covered;
//! `Stamp` and the text-note icon stay out of scope.

use super::super::dests::resolve;
use super::super::links::number;
use super::Synth;
use lopdf::{Dictionary, Document};

mod bevel;
mod border;
mod caption;
mod cidfont;
pub(crate) use cidfont::FontCache;
mod cloudy;
mod drfont;
mod ends;
mod face;
mod freetext;
mod paths;
mod quads;
mod shape;
pub(crate) mod std14;
mod textfit;

pub(crate) use std14::Std14;
pub(super) use textfit::{differences, extra_base};

/// Bézier constant for a quarter circle.
pub(super) const KAPPA: f64 = 0.552_284_749_8;
/// Builds the appearance of one annotation, or `None` when it cannot be drawn.
pub(super) fn synthesize(
    doc: &Document,
    annot: &Dictionary,
    subtype: &[u8],
    rect: [f64; 4],
    fonts: &mut cidfont::FontCache,
) -> Option<Synth> {
    match subtype {
        b"Highlight" => quads::synthesize(doc, annot, rect, quads::QuadKind::Highlight),
        b"Underline" => quads::synthesize(doc, annot, rect, quads::QuadKind::Underline),
        b"Squiggly" => quads::synthesize(doc, annot, rect, quads::QuadKind::Squiggly),
        b"StrikeOut" => quads::synthesize(doc, annot, rect, quads::QuadKind::StrikeOut),
        b"Square" => shape::shape(doc, annot, rect, shape::ShapeKind::Square),
        b"Circle" => shape::shape(doc, annot, rect, shape::ShapeKind::Circle),
        b"Line" => paths::line(doc, annot, rect),
        b"Ink" => paths::ink(doc, annot, rect),
        b"Polygon" => paths::polygon(doc, annot, rect, true),
        b"PolyLine" => paths::polygon(doc, annot, rect, false),
        b"FreeText" => freetext::synthesize(doc, annot, rect, fonts),
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
    use lopdf::Object;

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
    fn rectangles_union() {
        assert_eq!(union([0.0, 0.0, 10.0, 10.0], [5.0, -5.0, 20.0, 5.0]), [0.0, -5.0, 20.0, 10.0]);
    }
}
