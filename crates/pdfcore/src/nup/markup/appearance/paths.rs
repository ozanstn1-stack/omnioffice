//! Path-shaped annotations: lines, ink and the polygon kinds.
//!
//! All three are the same geometry with different bookkeeping - `/L` is one
//! segment, `/InkList` is a list of polylines, `/Vertices` one polygon or
//! polyline. The operators are emitted in page space, like every other
//! synthesized appearance.

use super::super::super::dests::resolve;
use super::border::stroke;
use super::{number, number_text, union, Color, Synth};
use lopdf::{Dictionary, Document, Object};

/// A point in page space.
pub(super) type Point = (f64, f64);

/// Guards against absurd input rather than for a real document.
const MAX_POINTS: usize = 20_000;
const MAX_STROKES: usize = 1_000;

/// Builds a `Line` annotation (`/L`), honouring the strike width.
pub(super) fn line(doc: &Document, annot: &Dictionary, rect: [f64; 4]) -> Option<Synth> {
    // A leader line shifts the stroke sideways; ignoring it would put the line
    // in the wrong place.
    if annot.get(b"LL").ok().and_then(number).is_some_and(|value| value != 0.0) {
        return None;
    }
    let points = points(doc, annot.get(b"L").ok()?)?;
    if points.len() != 2 {
        return None;
    }
    let colour = super::color(doc, annot, b"C")??;
    let border = stroke(doc, annot)?;
    if border.width <= 0.0 {
        return None;
    }
    let mut ops = colour.render(colour.stroke_op());
    border.ops(&mut ops);
    path_ops(&mut ops, &points, false);
    ops.push_str("S\n");
    Some(Synth { ops: ops.into_bytes(), bbox: padded(rect, &points, border.width / 2.0), multiply: false, font: None })
}

/// Builds an `Ink` annotation (`/InkList`).
pub(super) fn ink(doc: &Document, annot: &Dictionary, rect: [f64; 4]) -> Option<Synth> {
    let list = resolve(doc, annot.get(b"InkList").ok()?);
    let strokes = list.as_array().ok()?;
    if strokes.is_empty() || strokes.len() > MAX_STROKES {
        return None;
    }
    let colour = super::color(doc, annot, b"C")??;
    let border = stroke(doc, annot)?;
    if border.width <= 0.0 {
        return None;
    }
    let mut ops = String::from("1 J\n1 j\n");
    ops.push_str(&colour.render(colour.stroke_op()));
    border.ops(&mut ops);
    let mut bounds = rect;
    let mut total = 0usize;
    for stroke_items in strokes {
        let points = points(doc, stroke_items)?;
        total += points.len();
        if total > MAX_POINTS {
            return None;
        }
        path_ops(&mut ops, &points, false);
        bounds = padded(bounds, &points, 0.0);
    }
    ops.push_str("S\n");
    let half = border.width / 2.0;
    Some(Synth {
        ops: ops.into_bytes(),
        bbox: [bounds[0] - half, bounds[1] - half, bounds[2] + half, bounds[3] + half],
        multiply: false,
        font: None,
    })
}

/// Builds a `Polygon` (`closed`) or `PolyLine` from `/Vertices`.
pub(super) fn polygon(doc: &Document, annot: &Dictionary, rect: [f64; 4], closed: bool) -> Option<Synth> {
    let points = points(doc, annot.get(b"Vertices").ok()?)?;
    if points.len() < 2 {
        return None;
    }
    let border = stroke(doc, annot)?;
    // A polygon may be filled; a polyline only carries a fill for its line
    // endings, so the path itself is never filled.
    let fill = if closed { super::color(doc, annot, b"IC")? } else { None };
    let outline = super::color(doc, annot, b"C")?;
    let has_stroke = outline.is_some() && border.width > 0.0;
    if !has_stroke && fill.is_none() {
        return None;
    }
    let mut ops = String::new();
    if has_stroke {
        if let Some(outline) = &outline {
            ops.push_str(&outline.render(outline.stroke_op()));
        }
        border.ops(&mut ops);
    }
    if let Some(fill) = &fill {
        ops.push_str(&fill.render(fill.fill_op()));
    }
    path_ops(&mut ops, &points, closed);
    ops.push_str(if has_stroke && fill.is_some() {
        "B\n"
    } else if has_stroke {
        "S\n"
    } else {
        "f\n"
    });
    let boxed = padded(rect, &points, border.width / 2.0);
    Some(Synth { ops: ops.into_bytes(), bbox: boxed, multiply: false, font: None })
}

/// Parses a flat coordinate array into points; `None` when it is unusable.
fn points(doc: &Document, value: &Object) -> Option<Vec<Point>> {
    let value = resolve(doc, value);
    let items = value.as_array().ok()?;
    if items.len() < 4 || !items.len().is_multiple_of(2) || items.len() / 2 > MAX_POINTS {
        return None;
    }
    let mut numbers = Vec::with_capacity(items.len());
    for item in items {
        numbers.push(number(item)?);
    }
    Some(numbers.chunks(2).map(|pair| (pair[0], pair[1])).collect())
}

/// Emits `m`/`l` operators, closing the path when asked.
pub(super) fn path_ops(ops: &mut String, points: &[Point], close: bool) {
    for (index, (x, y)) in points.iter().enumerate() {
        let command = if index == 0 { "m" } else { "l" };
        ops.push_str(&format!("{} {} {command}\n", number_text(*x), number_text(*y)));
    }
    if close {
        ops.push_str("h\n");
    }
}

/// Grows `rect` so a stroke of half-width `half` around the points stays inside
/// the form's box.
fn padded(rect: [f64; 4], points: &[Point], half: f64) -> [f64; 4] {
    let mut bounds = rect;
    for (x, y) in points {
        bounds = union(bounds, [*x - half, *y - half, *x + half, *y + half]);
    }
    bounds
}

/// The fill colour of a closed path, when the annotation carries one.
#[allow(dead_code)]
fn fill_colour(doc: &Document, annot: &Dictionary) -> Option<Option<Color>> {
    super::color(doc, annot, b"IC")
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    fn doc() -> Document {
        Document::with_version("1.7")
    }

    fn array(values: &[f64]) -> Object {
        Object::Array(values.iter().map(|value| Object::Real(*value as f32)).collect())
    }

    #[test]
    fn a_coordinate_array_becomes_points() {
        let doc = doc();
        let parsed = points(&doc, &array(&[1.0, 2.0, 3.0, 4.0])).expect("points");
        assert_eq!(parsed, vec![(1.0, 2.0), (3.0, 4.0)]);
        assert!(points(&doc, &array(&[1.0, 2.0])).is_none(), "a short array is unusable");
        assert!(points(&doc, &array(&[1.0, 2.0, 3.0])).is_none(), "an odd count is unusable");
        assert!(points(&doc, &Object::Name(b"L".to_vec())).is_none());
    }

    #[test]
    fn the_path_operators_differ_only_in_the_close() {
        let points = [(10.0, 20.0), (30.0, 40.0)];
        let mut open = String::new();
        path_ops(&mut open, &points, false);
        assert_eq!(open, "10 20 m\n30 40 l\n");
        let mut closed = String::new();
        path_ops(&mut closed, &points, true);
        assert_eq!(closed, "10 20 m\n30 40 l\nh\n");
    }

    #[test]
    fn the_padding_covers_the_stroke() {
        let grown = padded([0.0, 0.0, 10.0, 10.0], &[(50.0, 50.0)], 2.0);
        assert_eq!(grown, [0.0, 0.0, 52.0, 52.0]);
        let inside = padded([0.0, 0.0, 100.0, 100.0], &[(50.0, 50.0)], 2.0);
        assert_eq!(inside, [0.0, 0.0, 100.0, 100.0], "a point inside the rect does not shrink it");
    }

    #[test]
    fn a_fill_colour_is_optional() {
        let doc = doc();
        let mut annot = Dictionary::new();
        annot.set("IC", array(&[0.0, 0.0, 1.0]));
        let fill = fill_colour(&doc, &annot).expect("colour");
        assert_eq!(fill.expect("coloured").values.len(), 3);
        assert!(fill_colour(&doc, &dictionary! { "T" => Object::string_literal("x") }).expect("none").is_none());
    }
}
