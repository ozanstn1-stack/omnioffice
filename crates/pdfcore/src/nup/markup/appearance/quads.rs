//! Quad-point markup: highlights, underlines, strike-outs and squiggles.

use super::super::super::dests::resolve;
use super::super::super::links::number;
use super::{border, color, number_text, union, Synth};
use lopdf::{Dictionary, Document};

/// Guards against absurd inputs rather than for a real document.
const MAX_QUADS: usize = 2_000;
const MAX_SQUIGGLY_TEETH: usize = 4_000;

/// One `/QuadPoints` entry, normalized to top-left, top-right, bottom-right,
/// bottom-left order.
struct Quad {
    tl: (f64, f64),
    tr: (f64, f64),
    br: (f64, f64),
    bl: (f64, f64),
}

// ---------------------------------------------------------------------------
// Quad-point markup
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
pub(super) enum QuadKind {
    Highlight,
    Underline,
    Squiggly,
    StrikeOut,
}

pub(super) fn synthesize(doc: &Document, annot: &Dictionary, rect: [f64; 4], kind: QuadKind) -> Option<Synth> {
    let points = annot.get(b"QuadPoints").ok()?;
    let points = resolve(doc, points);
    let items = points.as_array().ok()?;
    if items.is_empty() || !items.len().is_multiple_of(8) || items.len() / 8 > MAX_QUADS {
        return None;
    }
    let mut values = Vec::with_capacity(items.len());
    for item in items {
        values.push(number(item)?);
    }
    let colour = color(doc, annot, b"C")??;
    let width = if kind == QuadKind::Highlight { 0.0 } else { border::stroke(doc, annot)?.width };
    let mut ops = String::new();
    let mut bounds = rect;
    if kind == QuadKind::Highlight {
        // One fill for every quad: overlapping marks must not darken twice.
        ops.push_str(&colour.render(colour.fill_op()));
        let mut any = false;
        for quad in quads_of(&values) {
            any = true;
            bounds = union(bounds, quad_bounds(&quad));
            let _ = write_polygon(&mut ops, &quad);
        }
        if !any {
            return None;
        }
        ops.push_str("f\n");
        return Some(Synth { ops: ops.into_bytes(), bbox: bounds, multiply: true, font: None });
    }
    ops.push_str(&colour.render(colour.stroke_op()));
    let mut any = false;
    for quad in quads_of(&values) {
        any = true;
        bounds = union(bounds, quad_bounds(&quad));
        match kind {
            QuadKind::Underline => underline(&mut ops, &quad, width),
            QuadKind::StrikeOut => strikeout(&mut ops, &quad, width),
            QuadKind::Squiggly => {
                if !squiggly(&mut ops, &quad, width) {
                    return None;
                }
            }
            QuadKind::Highlight => {}
        }
    }
    if !any {
        return None;
    }
    ops.push_str("S\n");
    Some(Synth { ops: ops.into_bytes(), bbox: bounds, multiply: false, font: None })
}

/// Splits the flat `/QuadPoints` array into normalized quads, skipping
/// degenerate ones.
fn quads_of(values: &[f64]) -> Vec<Quad> {
    let mut out = Vec::new();
    for chunk in values.chunks(8) {
        if chunk.len() != 8 {
            continue;
        }
        let p = |index: usize| (chunk[index], chunk[index + 1]);
        let (p1, p2, p3, p4) = (p(0), p(2), p(4), p(6));
        // Both orders in the wild list the upper edge first or second; the
        // edge with the higher midpoint is the top one. Text rotated on the
        // page is the documented limitation.
        let upper_first = p1.1 + p2.1 >= p3.1 + p4.1;
        let (top_a, top_b, bottom_a, bottom_b) = if upper_first { (p1, p2, p3, p4) } else { (p3, p4, p1, p2) };
        // The lower pair is sometimes stored in the opposite direction; pair
        // it corner-to-corner with the upper one before ordering the corners.
        let straight = (top_a.0 - bottom_a.0).abs() + (top_b.0 - bottom_b.0).abs();
        let crossed = (top_a.0 - bottom_b.0).abs() + (top_b.0 - bottom_a.0).abs();
        let (bottom_a, bottom_b) = if straight <= crossed { (bottom_a, bottom_b) } else { (bottom_b, bottom_a) };
        let (tl, tr, bl, br) =
            if top_a.0 <= top_b.0 { (top_a, top_b, bottom_a, bottom_b) } else { (top_b, top_a, bottom_b, bottom_a) };
        let quad = Quad { tl, tr, br, bl };
        if quad.bl != quad.br && quad.bl != quad.tl {
            out.push(quad);
        }
    }
    out
}

fn quad_bounds(quad: &Quad) -> [f64; 4] {
    let xs = [quad.tl.0, quad.tr.0, quad.br.0, quad.bl.0];
    let ys = [quad.tl.1, quad.tr.1, quad.br.1, quad.bl.1];
    [
        xs.iter().copied().fold(f64::MAX, f64::min),
        ys.iter().copied().fold(f64::MAX, f64::min),
        xs.iter().copied().fold(f64::MIN, f64::max),
        ys.iter().copied().fold(f64::MIN, f64::max),
    ]
}

/// Unit vector along the baseline and the unit normal pointing at the top edge.
fn axes(quad: &Quad) -> Option<((f64, f64), (f64, f64))> {
    let along = (quad.br.0 - quad.bl.0, quad.br.1 - quad.bl.1);
    let length = along.0.hypot(along.1);
    if length < 1e-6 {
        return None;
    }
    let u = (along.0 / length, along.1 / length);
    let mut normal = (-u.1, u.0);
    let up = (quad.tl.0 - quad.bl.0, quad.tl.1 - quad.bl.1);
    if up.0 * normal.0 + up.1 * normal.1 < 0.0 {
        normal = (u.1, -u.0);
    }
    Some((u, normal))
}

fn write_polygon(ops: &mut String, quad: &Quad) -> Option<()> {
    ops.push_str(&format!(
        "{} {} m\n{} {} l\n{} {} l\n{} {} l\nh\n",
        number_text(quad.tl.0),
        number_text(quad.tl.1),
        number_text(quad.tr.0),
        number_text(quad.tr.1),
        number_text(quad.br.0),
        number_text(quad.br.1),
        number_text(quad.bl.0),
        number_text(quad.bl.1)
    ));
    Some(())
}

fn underline(ops: &mut String, quad: &Quad, width: f64) {
    let Some((_, normal)) = axes(quad) else { return };
    let offset = width / 2.0;
    let start = (quad.bl.0 + normal.0 * offset, quad.bl.1 + normal.1 * offset);
    let end = (quad.br.0 + normal.0 * offset, quad.br.1 + normal.1 * offset);
    ops.push_str(&format!(
        "{w} w\n{} {} m\n{} {} l\n",
        number_text(start.0),
        number_text(start.1),
        number_text(end.0),
        number_text(end.1),
        w = number_text(width)
    ));
}

fn strikeout(ops: &mut String, quad: &Quad, width: f64) {
    let mid_left = ((quad.tl.0 + quad.bl.0) / 2.0, (quad.tl.1 + quad.bl.1) / 2.0);
    let mid_right = ((quad.tr.0 + quad.br.0) / 2.0, (quad.tr.1 + quad.br.1) / 2.0);
    ops.push_str(&format!(
        "{w} w\n{} {} m\n{} {} l\n",
        number_text(mid_left.0),
        number_text(mid_left.1),
        number_text(mid_right.0),
        number_text(mid_right.1),
        w = number_text(width)
    ));
}

fn squiggly(ops: &mut String, quad: &Quad, width: f64) -> bool {
    let Some((u, n)) = axes(quad) else { return false };
    let length = (quad.br.0 - quad.bl.0).hypot(quad.br.1 - quad.bl.1);
    let amplitude = 1.5_f64.min(length / 8.0).max(0.4);
    let step = amplitude;
    let teeth = (length / step).ceil() as usize;
    if teeth == 0 || teeth > MAX_SQUIGGLY_TEETH {
        return false;
    }
    let offset = width / 2.0;
    ops.push_str(&format!("{} w\n", number_text(width.max(0.3))));
    for index in 0..=teeth {
        let along = (index as f64 * step).min(length);
        let lift = if index % 2 == 0 { offset } else { offset + amplitude };
        let x = quad.bl.0 + u.0 * along + n.0 * lift;
        let y = quad.bl.1 + u.1 * along + n.1 * lift;
        ops.push_str(&format!("{} {} {} \n", number_text(x), number_text(y), if index == 0 { "m" } else { "l" }));
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quad_is_normalized_to_corners() {
        let quad = quads_of(&[100.0, 150.0, 200.0, 150.0, 100.0, 100.0, 200.0, 100.0]);
        assert_eq!(quad.len(), 1);
        assert_eq!(quad[0].tl, (100.0, 150.0));
        assert_eq!(quad[0].tr, (200.0, 150.0));
        assert_eq!(quad[0].bl, (100.0, 100.0));
        assert_eq!(quad[0].br, (200.0, 100.0));
        // The reversed order gives the same corners.
        let reversed = quads_of(&[100.0, 100.0, 200.0, 100.0, 200.0, 150.0, 100.0, 150.0]);
        assert_eq!(reversed[0].tl, (100.0, 150.0));
        assert_eq!(reversed[0].br, (200.0, 100.0));
        // A degenerate quad is skipped, a short array produces nothing.
        assert!(quads_of(&[0.0; 8]).is_empty());
        assert!(quads_of(&[0.0; 7]).is_empty());
        assert_eq!(quad_bounds(&quad[0]), [100.0, 100.0, 200.0, 150.0]);
    }

    #[test]
    fn the_baseline_and_normal_follow_the_quad() {
        let quad = quads_of(&[100.0, 150.0, 200.0, 150.0, 100.0, 100.0, 200.0, 100.0]).remove(0);
        let (along, normal) = axes(&quad).expect("axes");
        assert_eq!(along, (1.0, 0.0));
        assert_eq!(normal, (0.0, 1.0), "the normal points at the top edge");
        let flat = Quad { tl: (10.0, 10.0), tr: (10.0, 10.0), br: (10.0, 10.0), bl: (10.0, 10.0) };
        assert!(axes(&flat).is_none(), "a zero-length baseline has no axes");
    }
}
