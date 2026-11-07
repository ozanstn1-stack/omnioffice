//! Line endings (`/LE`) for lines, polylines and free-text callouts.
//!
//! A line ending is a small mark drawn where the line stops: an arrow head, a
//! square, a circle, a slash. The geometry is page space, like every other
//! synthesized appearance, and each mark is stroked or filled with the line's
//! own paint so it reads as part of the same stroke.

use super::super::super::dests::resolve;
use super::number_text;
use lopdf::{Dictionary, Document, Object};

/// A line ending kind; an unknown name is treated as `None`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Ending {
    None,
    Butt,
    Square,
    Circle,
    Diamond,
    OpenArrow,
    ClosedArrow,
    ROpenArrow,
    RClosedArrow,
    Slash,
}

impl Ending {
    /// Maps a `/LE` name onto a kind; anything unknown draws nothing.
    pub(super) fn from_name(name: &[u8]) -> Ending {
        match name {
            b"Square" => Ending::Square,
            b"Circle" => Ending::Circle,
            b"Diamond" => Ending::Diamond,
            b"OpenArrow" => Ending::OpenArrow,
            b"ClosedArrow" => Ending::ClosedArrow,
            b"ROpenArrow" => Ending::ROpenArrow,
            b"RClosedArrow" => Ending::RClosedArrow,
            b"Butt" => Ending::Butt,
            b"Slash" => Ending::Slash,
            _ => Ending::None,
        }
    }

    /// A closed shape is filled when the line carries an interior colour.
    pub(super) fn closed(self) -> bool {
        matches!(self, Ending::Square | Ending::Circle | Ending::Diamond | Ending::ClosedArrow | Ending::RClosedArrow)
    }
}

/// The two `/LE` entries of a line or polyline (`[start end]`).
pub(super) fn pair(doc: &Document, annot: &Dictionary) -> (Ending, Ending) {
    let Some(items) = annot.get(b"LE").ok().map(|value| resolve(doc, value)).and_then(|value| value.as_array().ok())
    else {
        return (Ending::None, Ending::None);
    };
    if items.len() != 2 {
        return (Ending::None, Ending::None);
    }
    let read = |value: &Object| value.as_name().map(Ending::from_name).unwrap_or(Ending::None);
    (read(&items[0]), read(&items[1]))
}

/// The single `/LE` name of a free-text callout; an array falls back to its
/// first entry, which some producers write where the specification says name.
pub(super) fn single(doc: &Document, annot: &Dictionary) -> Ending {
    let Ok(value) = annot.get(b"LE") else { return Ending::None };
    let value = resolve(doc, value);
    match value {
        Object::Name(name) => Ending::from_name(name),
        Object::Array(items) => {
            items.first().and_then(|item| item.as_name().ok()).map(Ending::from_name).unwrap_or(Ending::None)
        }
        _ => Ending::None,
    }
}

/// The length of one ending; it grows with the stroke so it stays visible.
pub(super) fn size(width: f64) -> f64 {
    3.0 * width.max(0.0).max(2.0)
}

/// The unit vector pointing away from the line at the given end, or `None`
/// when the points are too close to define a direction.
fn outward(points: &[(f64, f64)], at_end: bool) -> Option<(f64, f64)> {
    let (tip, previous) = if at_end {
        let tip = *points.last()?;
        let previous = points.iter().rev().copied().find(|point| distance(*point, tip) > 1e-9)?;
        (tip, previous)
    } else {
        let tip = *points.first()?;
        let previous = points.iter().copied().find(|point| distance(*point, tip) > 1e-9)?;
        (tip, previous)
    };
    let (dx, dy) = (tip.0 - previous.0, tip.1 - previous.1);
    let length = dx.hypot(dy);
    (length > 1e-9).then_some((dx / length, dy / length))
}

fn distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// Draws both ends of a path; returns the bounding-box padding they need.
pub(super) fn draw(
    out: &mut String,
    points: &[(f64, f64)],
    ends: (Ending, Ending),
    width: f64,
    fill: Option<&str>,
) -> f64 {
    if points.len() < 2 || (ends.0 == Ending::None && ends.1 == Ending::None) {
        return 0.0;
    }
    let size = size(width);
    let mut pad: f64 = 0.0;
    if let Some(direction) = outward(points, false) {
        if let Some(tip) = points.first() {
            end_ops(out, ends.0, *tip, direction, width, fill);
            pad = pad.max(size + width);
        }
    }
    if let Some(direction) = outward(points, true) {
        if let Some(tip) = points.last() {
            end_ops(out, ends.1, *tip, direction, width, fill);
            pad = pad.max(size + width);
        }
    }
    pad
}

/// One ending at `tip`, pointing along `direction` (away from the line).
pub(super) fn end_ops(
    out: &mut String,
    kind: Ending,
    tip: (f64, f64),
    direction: (f64, f64),
    width: f64,
    fill: Option<&str>,
) {
    if kind == Ending::None {
        return;
    }
    let reversed = matches!(kind, Ending::ROpenArrow | Ending::RClosedArrow);
    let kind = match kind {
        Ending::ROpenArrow => Ending::OpenArrow,
        Ending::RClosedArrow => Ending::ClosedArrow,
        other => other,
    };
    let length = size(width);
    let half = length / 2.0;
    let (ux, uy) = if reversed { (-direction.0, -direction.1) } else { direction };
    let (nx, ny) = (-uy, ux);
    let base = (tip.0 - ux * length, tip.1 - uy * length);
    let at = |s: f64, n: f64| (tip.0 + ux * s + nx * n, tip.1 + uy * s + ny * n);
    out.push_str("q\n[] 0 d\n");
    if width > 0.0 {
        out.push_str(&format!("{} w\n", number_text(width)));
    }
    if kind.closed() {
        if let Some(fill) = fill {
            out.push_str(fill);
        }
    }
    let point = |(x, y): (f64, f64)| format!("{} {} ", number_text(x), number_text(y));
    match kind {
        Ending::None => return,
        Ending::Butt => out.push_str(&format!("{}m\n{}l\n", point(at(0.0, half)), point(at(0.0, -half)))),
        Ending::Slash => {
            let (sx, sy) = (nx * 0.866_025_4 + ux * 0.5, ny * 0.866_025_4 + uy * 0.5);
            let start = (tip.0 + sx * half, tip.1 + sy * half);
            let end = (tip.0 - sx * half, tip.1 - sy * half);
            out.push_str(&format!("{}m\n{}l\n", point(start), point(end)));
        }
        Ending::Square => out.push_str(&format!(
            "{}m\n{}l\n{}l\n{}l\nh\n",
            point(at(half, half)),
            point(at(-half, half)),
            point(at(-half, -half)),
            point(at(half, -half))
        )),
        Ending::Diamond => out.push_str(&format!(
            "{}m\n{}l\n{}l\n{}l\nh\n",
            point(at(half, 0.0)),
            point(at(0.0, half)),
            point(at(-half, 0.0)),
            point(at(0.0, -half))
        )),
        Ending::Circle => out.push_str(&circle(tip, half)),
        Ending::OpenArrow => out.push_str(&format!(
            "{}m\n{}l\n{}l\n",
            point((base.0 + nx * half, base.1 + ny * half)),
            point(tip),
            point((base.0 - nx * half, base.1 - ny * half))
        )),
        Ending::ClosedArrow => out.push_str(&format!(
            "{}m\n{}l\n{}l\nh\n",
            point(tip),
            point((base.0 + nx * half, base.1 + ny * half)),
            point((base.0 - nx * half, base.1 - ny * half))
        )),
        Ending::ROpenArrow | Ending::RClosedArrow => return,
    }
    out.push_str(if kind.closed() && fill.is_some() {
        "b\n"
    } else if kind.closed() {
        "s\n"
    } else {
        "S\n"
    });
    out.push_str("Q\n");
}

/// Four Bézier curves around `center` with radius `r`.
fn circle(center: (f64, f64), r: f64) -> String {
    const KAPPA: f64 = 0.552_284_749_8;
    let (cx, cy) = center;
    let k = KAPPA * r;
    let point = |x: f64, y: f64| format!("{} {} ", number_text(x), number_text(y));
    let mut out = format!("{}m\n", point(cx + r, cy));
    out.push_str(&format!("{}{}{}c\n", point(cx + r, cy + k), point(cx + k, cy + r), point(cx, cy + r)));
    out.push_str(&format!("{}{}{}c\n", point(cx - k, cy + r), point(cx - r, cy + k), point(cx - r, cy)));
    out.push_str(&format!("{}{}{}c\n", point(cx - r, cy - k), point(cx - k, cy - r), point(cx, cy - r)));
    out.push_str(&format!("{}{}{}c\nh\n", point(cx + k, cy - r), point(cx + r, cy - k), point(cx + r, cy)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    fn doc() -> Document {
        Document::with_version("1.7")
    }

    #[test]
    fn names_map_onto_kinds() {
        assert_eq!(Ending::from_name(b"ClosedArrow"), Ending::ClosedArrow);
        assert_eq!(Ending::from_name(b"ROpenArrow"), Ending::ROpenArrow);
        assert_eq!(Ending::from_name(b"Butt"), Ending::Butt);
        assert_eq!(Ending::from_name(b"Whatever"), Ending::None);
        assert!(Ending::ClosedArrow.closed() && !Ending::OpenArrow.closed());
    }

    #[test]
    fn the_pair_is_read_from_the_annotation() {
        let doc = doc();
        let mut annot = Dictionary::new();
        annot
            .set("LE", Object::Array(vec![Object::Name(b"OpenArrow".to_vec()), Object::Name(b"ClosedArrow".to_vec())]));
        assert_eq!(pair(&doc, &annot), (Ending::OpenArrow, Ending::ClosedArrow));
        annot.set("LE", Object::Array(vec![Object::Name(b"OpenArrow".to_vec())]));
        assert_eq!(pair(&doc, &annot), (Ending::None, Ending::None), "a short array has no ends");
        annot.set("LE", Object::Name(b"Circle".to_vec()));
        assert_eq!(single(&doc, &annot), Ending::Circle);
        annot.set("LE", Object::Array(vec![Object::Name(b"Diamond".to_vec()), Object::Name(b"Square".to_vec())]));
        assert_eq!(single(&doc, &annot), Ending::Diamond);
        assert_eq!(single(&doc, &dictionary! {}), Ending::None);
    }

    #[test]
    fn the_size_follows_the_stroke() {
        assert_eq!(size(0.0), 6.0);
        assert_eq!(size(1.0), 6.0);
        assert_eq!(size(4.0), 12.0);
    }

    #[test]
    fn the_direction_points_away_from_the_line() {
        let points = [(10.0, 10.0), (20.0, 10.0)];
        assert_eq!(outward(&points, true), Some((1.0, 0.0)));
        assert_eq!(outward(&points, false), Some((-1.0, 0.0)));
        // Repeated points are stepped over.
        let repeated = [(10.0, 10.0), (10.0, 10.0), (10.0, 20.0)];
        assert_eq!(outward(&repeated, true), Some((0.0, 1.0)));
        let stuck = [(5.0, 5.0), (5.0, 5.0)];
        assert!(outward(&stuck, true).is_none(), "no direction without a span");
    }

    #[test]
    fn a_butt_end_is_a_perpendicular_line() {
        let mut out = String::new();
        end_ops(&mut out, Ending::Butt, (20.0, 10.0), (1.0, 0.0), 2.0, None);
        assert!(out.contains("20 13 m\n"), "{out:?}");
        assert!(out.contains("20 7 l\n"), "{out:?}");
        assert!(out.ends_with("S\nQ\n"), "{out:?}");
        assert!(out.contains("[] 0 d\n"), "an end is never dashed: {out:?}");
    }

    #[test]
    fn closed_ends_fill_when_a_colour_is_available() {
        let mut filled = String::new();
        end_ops(&mut filled, Ending::ClosedArrow, (20.0, 10.0), (1.0, 0.0), 2.0, Some("0 0 1 rg\n"));
        assert!(filled.contains("0 0 1 rg\n"), "the interior colour is applied: {filled:?}");
        assert!(filled.ends_with("b\nQ\n"), "{filled:?}");
        let mut outline = String::new();
        end_ops(&mut outline, Ending::ClosedArrow, (20.0, 10.0), (1.0, 0.0), 2.0, None);
        assert!(outline.ends_with("s\nQ\n"), "without a fill the arrow is stroked: {outline:?}");
    }

    #[test]
    fn a_reversed_arrow_points_the_other_way() {
        let mut forward = String::new();
        end_ops(&mut forward, Ending::ClosedArrow, (20.0, 10.0), (1.0, 0.0), 2.0, None);
        let mut reversed = String::new();
        end_ops(&mut reversed, Ending::RClosedArrow, (20.0, 10.0), (1.0, 0.0), 2.0, None);
        assert_ne!(forward, reversed, "the reversed arrow differs");
        // The reversed base sits on the far side (+x) of the tip.
        assert!(reversed.contains("26 "), "the base is past the tip: {reversed:?}");
    }

    #[test]
    fn a_zero_width_end_is_only_drawn_when_it_can_be_filled() {
        let mut filled = String::new();
        end_ops(&mut filled, Ending::Diamond, (10.0, 10.0), (1.0, 0.0), 0.0, Some("0 g\n"));
        assert!(filled.ends_with("b\nQ\n"), "a fill needs no stroke: {filled:?}");
        assert!(!filled.contains(" w\n"), "no line width is set: {filled:?}");
        let mut empty = String::new();
        end_ops(&mut empty, Ending::OpenArrow, (10.0, 10.0), (1.0, 0.0), 0.0, None);
        assert!(empty.ends_with("S\nQ\n"), "an open end still strokes");
    }

    #[test]
    fn no_ends_means_no_output() {
        let mut out = String::new();
        let pad = draw(&mut out, &[(0.0, 0.0), (10.0, 0.0)], (Ending::None, Ending::None), 2.0, None);
        assert!(out.is_empty());
        assert_eq!(pad, 0.0);
    }

    #[test]
    fn both_ends_report_the_padding() {
        let mut out = String::new();
        let pad = draw(&mut out, &[(0.0, 0.0), (10.0, 0.0)], (Ending::OpenArrow, Ending::ClosedArrow), 2.0, None);
        assert_eq!(out.matches("q\n").count(), 2, "one block per end");
        assert!(pad >= 6.0, "the padding covers the ending: {pad}");
    }
}
