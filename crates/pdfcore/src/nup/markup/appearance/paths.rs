//! Path-shaped annotations: lines, ink and the polygon kinds.
//!
//! All three are the same geometry with different bookkeeping - `/L` is one
//! segment, `/InkList` is a list of polylines, `/Vertices` one polygon or
//! polyline. The operators are emitted in page space, like every other
//! synthesized appearance.

use super::super::super::dests::resolve;
use super::border::stroke;
use super::{ends, number, number_text, union, Color, Synth};
use lopdf::{Dictionary, Document, Object};

/// A point in page space.
pub(super) type Point = (f64, f64);

/// Guards against absurd input rather than for a real document.
const MAX_POINTS: usize = 20_000;
const MAX_STROKES: usize = 1_000;

/// Which side a positive `/LL` moves the line to: the left normal, as the
/// viewers in the wild draw it rather than the specification's prose.
const LEADER_SIDE: f64 = 1.0;

/// The shifted line and its two leaders.
struct Leaders {
    start: Point,
    end: Point,
    legs: Option<[(Point, Point); 2]>,
}

/// A line with a leader is drawn beside its anchors: `/LL` shifts it sideways,
/// `/LLO` leaves a gap at each anchor and `/LLE` runs past the line.
fn leaders(start: Point, end: Point, offset: f64, extension: f64, gap: f64) -> Option<Leaders> {
    let (dx, dy) = (end.0 - start.0, end.1 - start.1);
    let length = dx.hypot(dy);
    if length < 1e-9 {
        return None;
    }
    let (ux, uy) = (dx / length, dy / length);
    let (nx, ny) = (LEADER_SIDE * -uy, LEADER_SIDE * ux);
    let side = if offset < 0.0 { -1.0 } else { 1.0 };
    let shift = offset.abs();
    if shift < 1e-9 {
        return Some(Leaders { start, end, legs: None });
    }
    let moved = |point: Point| (point.0 + nx * offset, point.1 + ny * offset);
    let leg = |point: Point| {
        let from = (point.0 + nx * side * gap, point.1 + ny * side * gap);
        let to = (point.0 + nx * side * (shift + extension), point.1 + ny * side * (shift + extension));
        let travelled = (to.0 - from.0).hypot(to.1 - from.1);
        (travelled > 1e-9).then_some((from, to))
    };
    let start_shifted = moved(start);
    let end_shifted = moved(end);
    let legs = match (leg(start), leg(end)) {
        (Some(first), Some(second)) => Some([first, second]),
        _ => None,
    };
    Some(Leaders { start: start_shifted, end: end_shifted, legs })
}

/// Builds a `Line` annotation (`/L`), honouring the strike width and leaders.
pub(super) fn line(doc: &Document, annot: &Dictionary, rect: [f64; 4]) -> Option<Synth> {
    let anchors = points(doc, annot.get(b"L").ok()?)?;
    if anchors.len() != 2 {
        return None;
    }
    let colour = super::color(doc, annot, b"C")??;
    let border = stroke(doc, annot)?;
    if border.width <= 0.0 {
        return None;
    }
    let read =
        |key: &[u8]| annot.get(key).ok().and_then(number).filter(|value| value.is_finite()).map(|value| value.max(0.0));
    let offset = annot.get(b"LL").ok().and_then(number).filter(|value| value.is_finite()).unwrap_or(0.0);
    let extension = read(b"LLE").unwrap_or(0.0);
    let gap = read(b"LLO").unwrap_or(0.0);
    let drawn = leaders(anchors[0], anchors[1], offset, extension, gap)?;
    let mut ops = colour.render(colour.stroke_op());
    border.ops(&mut ops);
    path_ops(&mut ops, &[drawn.start, drawn.end], false);
    if let Some(legs) = drawn.legs {
        for (from, to) in legs {
            path_ops(&mut ops, &[from, to], false);
        }
    }
    ops.push_str("S\n");
    let mut reach = vec![drawn.start, drawn.end];
    if let Some(legs) = drawn.legs {
        for (from, to) in legs {
            reach.push(from);
            reach.push(to);
        }
    }
    let pad = paint_ends(doc, annot, &mut ops, &[drawn.start, drawn.end], border.width)?;
    Some(Synth {
        ops: ops.into_bytes(),
        bbox: padded(rect, &reach, border.width / 2.0 + pad),
        multiply: false,
        font: None,
    })
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
    if closed && border.edge != super::border::Edge::Plain && has_stroke {
        // A styled polygon is painted edge by edge in two tones; the plain
        // path below is left untouched.
        if let Some(outline) = &outline {
            super::bevel::polygon_frame(&mut ops, &points, &border, outline);
        }
        return Some(Synth {
            ops: ops.into_bytes(),
            bbox: padded(rect, &points, border.width / 2.0),
            multiply: false,
            font: None,
        });
    }
    path_ops(&mut ops, &points, closed);
    let ends = if closed { (ends::Ending::None, ends::Ending::None) } else { ends::pair(doc, annot) };
    if ends.0 == ends::Ending::None && ends.1 == ends::Ending::None {
        // Nothing extra to paint.
    }
    ops.push_str(if has_stroke && fill.is_some() {
        "B\n"
    } else if has_stroke {
        "S\n"
    } else {
        "f\n"
    });
    let mut pad = 0.0;
    if !closed && has_stroke {
        pad = paint_ends(doc, annot, &mut ops, &points, border.width)?;
    }
    let boxed = padded(rect, &points, border.width / 2.0 + pad);
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

/// Paints the `/LE` marks of a stroked path and reports the padding they need.
fn paint_ends(doc: &Document, annot: &Dictionary, ops: &mut String, points: &[Point], width: f64) -> Option<f64> {
    let pair = ends::pair(doc, annot);
    if pair == (ends::Ending::None, ends::Ending::None) {
        return Some(0.0);
    }
    let fill = super::color(doc, annot, b"IC")?.map(|colour| colour.render(colour.fill_op()));
    Some(ends::draw(ops, points, pair, width, fill.as_deref()))
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
    fn a_positive_leader_moves_the_line_to_the_left_normal() {
        let drawn = leaders((100.0, 100.0), (200.0, 100.0), 20.0, 5.0, 3.0).expect("leaders");
        assert_eq!(drawn.start, (100.0, 120.0));
        assert_eq!(drawn.end, (200.0, 120.0));
        let legs = drawn.legs.expect("legs");
        assert_eq!(legs[0], ((100.0, 103.0), (100.0, 125.0)));
        assert_eq!(legs[1], ((200.0, 103.0), (200.0, 125.0)));
    }

    #[test]
    fn a_negative_leader_flips_the_side() {
        let drawn = leaders((100.0, 100.0), (200.0, 100.0), -20.0, 0.0, 0.0).expect("leaders");
        assert_eq!(drawn.start, (100.0, 80.0));
        let legs = drawn.legs.expect("legs");
        assert_eq!(legs[0], ((100.0, 100.0), (100.0, 80.0)));
    }

    #[test]
    fn no_offset_draws_the_line_where_it_is() {
        let drawn = leaders((0.0, 0.0), (0.0, 100.0), 0.0, 5.0, 5.0).expect("leaders");
        assert_eq!(drawn.start, (0.0, 0.0));
        assert_eq!(drawn.end, (0.0, 100.0));
        assert!(drawn.legs.is_none(), "an offset of zero has no leaders");
        assert!(leaders((5.0, 5.0), (5.0, 5.0), 10.0, 0.0, 0.0).is_none(), "no direction, no line");
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
