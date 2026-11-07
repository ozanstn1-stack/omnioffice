//! Border styles: bevel, inset and underline frames.
//!
//! A plain frame is one stroked rectangle; a beveled one carries two tones so
//! it looks raised, an inset one swaps them so it looks sunk, and an underline
//! keeps only the bottom edge. The bands are drawn **inside** the frame, so the
//! form's box does not grow and the neighbouring cell is never touched.
//!
//! The tones are derived from the stroke colour (`lighter`/`darker`,
//! a quarter of the way to white or black) rather than from the viewer's
//! theme, and a shape too small for a band falls back to the plain frame.

use super::border::{darker, lighter, Edge, Stroke};
use super::{number_text, Color};

/// The rectangle bands need at least this many widths on each side.
const MIN_SPAN: f64 = 4.0;

fn inset(rect: [f64; 4], by: f64) -> [f64; 4] {
    [rect[0] + by, rect[1] + by, rect[2] - by, rect[3] - by]
}

fn rect_ops(ops: &mut String, rect: [f64; 4], fill: &Color) {
    ops.push_str(&fill.render(fill.fill_op()));
    ops.push_str(&format!(
        "{} {} {} {} re f\n",
        number_text(rect[0]),
        number_text(rect[1]),
        number_text(rect[2] - rect[0]),
        number_text(rect[3] - rect[1])
    ));
}

fn line_ops(ops: &mut String, from: (f64, f64), to: (f64, f64)) {
    ops.push_str(&format!(
        "{} {} m\n{} {} l\nS\n",
        number_text(from.0),
        number_text(from.1),
        number_text(to.0),
        number_text(to.1)
    ));
}

/// Draws the frame of a rectangle in the given style; the plain frame is the
/// caller's job so its output stays byte-identical.
pub(super) fn rect_frame(ops: &mut String, rect: [f64; 4], stroke: &Stroke, colour: &Color) {
    let width = stroke.width;
    if width <= 0.0 {
        return;
    }
    let half = width / 2.0;
    let [x0, y0, x1, y1] = rect;
    let outer = inset(rect, half);
    ops.push_str(&colour.render(colour.stroke_op()));
    stroke.ops(ops);
    if stroke.edge == Edge::Underline {
        line_ops(ops, (x0 + half, y0 + half), (x1 - half, y0 + half));
        return;
    }
    // The base frame first, then the bands between `outer` and `inner`.
    ops.push_str(&format!(
        "{} {} {} {} re S\n",
        number_text(outer[0]),
        number_text(outer[1]),
        number_text(outer[2] - outer[0]),
        number_text(outer[3] - outer[1])
    ));
    if x1 - x0 <= MIN_SPAN * width || y1 - y0 <= MIN_SPAN * width {
        return;
    }
    let inner = inset(rect, 2.0 * width);
    let (top_left, bottom_right) = match stroke.edge {
        Edge::Inset => (darker(colour), lighter(colour)),
        _ => (lighter(colour), darker(colour)),
    };
    // Top and left share one tone, bottom and right the other; a corner is
    // painted by whichever band comes last, which is the documented order.
    rect_ops(ops, [outer[0], inner[3], outer[2], outer[3]], &top_left);
    rect_ops(ops, [outer[0], outer[1], inner[0], outer[3]], &top_left);
    rect_ops(ops, [outer[0], outer[1], outer[2], inner[1]], &bottom_right);
    rect_ops(ops, [inner[2], outer[1], outer[2], outer[3]], &bottom_right);
}

/// Draws the frame of an ellipse; the bands are two half-ellipse strokes
/// approximated by short segments (a documented simplification).
pub(super) fn ellipse_frame(ops: &mut String, rect: [f64; 4], stroke: &Stroke, colour: &Color) {
    let width = stroke.width;
    if width <= 0.0 {
        return;
    }
    let (cx, cy) = ((rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0);
    let (rx, ry) = ((rect[2] - rect[0]) / 2.0, (rect[3] - rect[1]) / 2.0);
    if rx <= 2.0 * width || ry <= 2.0 * width {
        // Too small for a band: a plain outline is all a viewer would show.
        ops.push_str(&colour.render(colour.stroke_op()));
        stroke.ops(ops);
        arc(ops, cx, cy, rx, ry, 0.0, 360.0);
        return;
    }
    ops.push_str(&colour.render(colour.stroke_op()));
    stroke.ops(ops);
    arc(ops, cx, cy, rx, ry, 0.0, 360.0);
    let band = (rx - 1.5 * width, ry - 1.5 * width);
    let (light, dark) = match stroke.edge {
        Edge::Inset => (darker(colour), lighter(colour)),
        _ => (lighter(colour), darker(colour)),
    };
    ops.push_str(&light.render(light.stroke_op()));
    stroke.ops(ops);
    arc(ops, cx, cy, band.0, band.1, 45.0, 225.0);
    ops.push_str(&dark.render(dark.stroke_op()));
    stroke.ops(ops);
    arc(ops, cx, cy, band.0, band.1, 225.0, 405.0);
}

/// A stroke along an elliptical arc, one segment every fifteen degrees.
fn arc(ops: &mut String, cx: f64, cy: f64, rx: f64, ry: f64, from: f64, to: f64) {
    let steps = (((to - from).abs() / 15.0).ceil() as usize).max(1);
    for index in 0..=steps {
        let angle = (from + (to - from) * index as f64 / steps as f64).to_radians();
        let (x, y) = (cx + rx * angle.cos(), cy + ry * angle.sin());
        let command = if index == 0 { "m" } else { "l" };
        ops.push_str(&format!("{} {} {command}\n", number_text(x), number_text(y)));
    }
    ops.push_str("S\n");
}

/// Draws a polygon's edges each in one of the two tones, which is all a closed
/// outline can express without an inner band.
pub(super) fn polygon_frame(ops: &mut String, points: &[(f64, f64)], stroke: &Stroke, colour: &Color) {
    let width = stroke.width;
    if width <= 0.0 || points.len() < 3 {
        return;
    }
    let area = signed_area(points);
    if area.abs() < 1e-9 {
        return;
    }
    let (light, dark) = match stroke.edge {
        Edge::Inset => (darker(colour), lighter(colour)),
        _ => (lighter(colour), darker(colour)),
    };
    ops.push_str("2 J\n");
    for (index, from) in points.iter().enumerate() {
        let to = points[(index + 1) % points.len()];
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        // The outward normal for a counter-clockwise outline points left.
        let (nx, ny) = if area > 0.0 { (dy, -dx) } else { (-dy, dx) };
        let tone = if ny - nx > 0.0 { &light } else { &dark };
        ops.push_str(&tone.render(tone.stroke_op()));
        stroke.ops(ops);
        line_ops(ops, *from, to);
    }
}

/// Twice the signed area of the polygon; its sign says the winding direction.
fn signed_area(points: &[(f64, f64)]) -> f64 {
    let mut sum = 0.0;
    for (index, (x, y)) in points.iter().enumerate() {
        let (nx, ny) = points[(index + 1) % points.len()];
        sum += x * ny - nx * y;
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nup::markup::appearance::Color;

    fn stroke(edge: Edge, width: f64) -> Stroke {
        Stroke { width, dash: None, edge }
    }

    fn colour() -> Color {
        Color { values: vec![0.0, 0.0, 0.0] }
    }

    #[test]
    fn a_beveled_rectangle_paints_four_bands() {
        let mut ops = String::new();
        rect_frame(&mut ops, [100.0, 100.0, 200.0, 150.0], &stroke(Edge::Beveled, 2.0), &colour());
        assert!(ops.contains("re S\n"), "the base frame is stroked: {ops:?}");
        assert_eq!(ops.matches("re f\n").count(), 4, "four bands: {ops:?}");
        assert!(ops.contains("0.25 0.25 0.25 rg"), "the light tone: {ops:?}");
        assert!(ops.contains("0 0 0 rg"), "the dark tone: {ops:?}");
    }

    #[test]
    fn an_inset_rectangle_swaps_the_tones() {
        let mut beveled = String::new();
        rect_frame(&mut beveled, [0.0, 0.0, 100.0, 100.0], &stroke(Edge::Beveled, 2.0), &colour());
        let mut sunk = String::new();
        rect_frame(&mut sunk, [0.0, 0.0, 100.0, 100.0], &stroke(Edge::Inset, 2.0), &colour());
        let light = "0.25 0.25 0.25 rg";
        assert!(beveled.find(light) < beveled.find("0 0 0 rg"), "beveled starts light");
        assert!(sunk.find(light) > sunk.find("0 0 0 rg"), "inset starts dark");
    }

    #[test]
    fn an_underline_keeps_only_the_bottom_edge() {
        let mut ops = String::new();
        rect_frame(&mut ops, [100.0, 100.0, 200.0, 150.0], &stroke(Edge::Underline, 2.0), &colour());
        assert!(!ops.contains("re"), "no frame is drawn: {ops:?}");
        assert!(ops.contains("101 101 m\n199 101 l\n"), "{ops:?}");
    }

    #[test]
    fn a_small_shape_falls_back_to_the_plain_frame() {
        let mut ops = String::new();
        rect_frame(&mut ops, [0.0, 0.0, 6.0, 100.0], &stroke(Edge::Beveled, 2.0), &colour());
        assert_eq!(ops.matches("re f\n").count(), 0, "no band fits: {ops:?}");
        assert!(ops.contains("re S\n"), "the plain frame is still drawn: {ops:?}");
    }

    #[test]
    fn an_ellipse_splits_into_two_half_arcs() {
        let mut ops = String::new();
        ellipse_frame(&mut ops, [0.0, 0.0, 100.0, 60.0], &stroke(Edge::Beveled, 2.0), &colour());
        assert_eq!(ops.matches("S\n").count(), 3, "the outline and two bands: {ops:?}");
        assert!(ops.contains("0.25 0.25 0.25 RG"), "the light band: {ops:?}");
    }

    #[test]
    fn a_polygon_colours_each_edge() {
        let mut ops = String::new();
        let points = [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)];
        polygon_frame(&mut ops, &points, &stroke(Edge::Beveled, 1.0), &colour());
        assert!(ops.contains("2 J"), "the edges meet at their corners: {ops:?}");
        assert!(ops.contains("0.25 0.25 0.25 RG") && ops.contains("0 0 0 RG"), "both tones: {ops:?}");
        assert_eq!(ops.matches(" l\n").count(), 4, "one line per edge: {ops:?}");
    }

    #[test]
    fn the_signed_area_says_the_winding() {
        let square = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        assert!(signed_area(&square) > 0.0);
        let reversed: Vec<(f64, f64)> = square.iter().rev().copied().collect();
        assert!(signed_area(&reversed) < 0.0);
    }
}
