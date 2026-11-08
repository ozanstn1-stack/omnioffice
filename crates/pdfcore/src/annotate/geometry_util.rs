//! Small coordinate helpers shared by the annotation paths.

use lopdf::Object;

/// A rectangle as `[x, y, w, h]` with a non-negative width and height.
pub(super) fn normalize_rect(rect: [f64; 4]) -> [f64; 4] {
    let (x0, x1) = if rect[2] >= 0.0 { (rect[0], rect[0] + rect[2]) } else { (rect[0] + rect[2], rect[0]) };
    let (y0, y1) = if rect[3] >= 0.0 { (rect[1], rect[1] + rect[3]) } else { (rect[1] + rect[3], rect[1]) };
    [x0, y0, x1 - x0, y1 - y0]
}

pub(super) fn to_page_space(page_h: f64, x: f64, y_top: f64, _w: f64, h: f64) -> (f64, f64) {
    // UI top-left -> display-space bottom-left.
    (x, page_h - y_top - h)
}

/// lopdf stores numbers as f32; every coordinate conversion lands here.
pub(super) fn real(value: f64) -> Object {
    Object::Real(value as f32)
}
