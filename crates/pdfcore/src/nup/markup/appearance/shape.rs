//! Square and Circle annotations: the plain rectangle and ellipse paths, the
//! styled frames the border style asks for, and the cloudy outline `/BE`
//! describes.
//!
//! The border style decides the frame: a plain one draws the path itself, a
//! bevel/inset/underline style is painted by `bevel`, and a cloudy one by
//! `cloudy`. A style that does not apply to the shape falls back to plain, so
//! the plain path stays byte-identical for every annotation that does not ask
//! for a frame.

use lopdf::{Dictionary, Document};

use super::super::Synth;
use super::{bevel, border, color, number_text, KAPPA};

#[derive(Clone, Copy, PartialEq)]
pub(super) enum ShapeKind {
    Square,
    Circle,
}

pub(super) fn shape(doc: &Document, annot: &Dictionary, rect: [f64; 4], kind: ShapeKind) -> Option<Synth> {
    let stroke = color(doc, annot, b"C")?;
    let fill = color(doc, annot, b"IC")?;
    let border = border::stroke(doc, annot)?;
    let width = border.width;
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
    // A styled frame (bevel, inset, underline) is painted by `bevel`; the
    // plain path below is left untouched so its output stays byte-identical.
    let edge = match kind {
        // An underline means nothing on an ellipse; a viewer shows a plain one.
        ShapeKind::Circle if border.edge == border::Edge::Underline => border::Edge::Plain,
        _ => border.edge,
    };
    if edge != border::Edge::Plain {
        let mut styled = String::new();
        if let Some(fill) = &fill {
            styled.push_str(&fill.render(fill.fill_op()));
            styled.push_str(&format!(
                "{} {} {} {} re f\n",
                number_text(x0),
                number_text(y0),
                number_text(x1 - x0),
                number_text(y1 - y0)
            ));
        }
        if let (Some(outline), true) = (&stroke, has_stroke) {
            match kind {
                ShapeKind::Square => bevel::rect_frame(&mut styled, rect, &border, outline),
                ShapeKind::Circle => bevel::ellipse_frame(&mut styled, rect, &border, outline),
            }
        }
        return Some(Synth { ops: styled.into_bytes(), bbox: rect, multiply: false, font: None });
    }
    let mut ops = String::new();
    if has_stroke {
        if let Some(stroke) = &stroke {
            ops.push_str(&stroke.render(stroke.stroke_op()));
        }
        border.ops(&mut ops);
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
    Some(Synth { ops: ops.into_bytes(), bbox: rect, multiply: false, font: None })
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
