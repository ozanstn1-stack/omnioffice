//! Quad-point markup: highlights, underlines, strike-outs and squiggles.

use super::super::super::dests::resolve;
use super::super::super::links::number;
use super::{border_width, color, number_text, union, Synth};
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
    let width = if kind == QuadKind::Highlight { 0.0 } else { border_width(doc, annot)? };
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
        return Some(Synth { ops: ops.into_bytes(), bbox: bounds, multiply: true });
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
    Some(Synth { ops: ops.into_bytes(), bbox: bounds, multiply: false })
}

/// Splits the flat `/QuadPoints` array into normalized quads, skipping
/// degenerate ones.
fn quads_of(values: &[f64]) -> Vec<Quad> {
    let mut out = Vec::new();
    for chunk in values.chunks(8) {
        let p = |index: usize| (chunk[index], chunk[index + 1]);
        let (p1, p2, p3, p4) = (p(0), p(2), p(4), p(6));
        // Both orders in the wild list the upper edge first or second; the
        // edge with the higher midpoint is the top one. Text rotated on the
        // page is the documented limitation.
        let upper_first = p1.1 + p2.1 >= p3.1 + p4.1;
        let (mut tl, mut tr, mut bl, mut br) = if upper_first { (p1, p2, p3, p4) } else { (p3, p4, p1, p2) };
        if tl.0 > tr.0 {
            std::mem::swap(&mut tl, &mut tr);
            std::mem::swap(&mut bl, &mut br);
        }
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
