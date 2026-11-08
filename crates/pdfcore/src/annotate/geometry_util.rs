//! Small coordinate helpers shared by the annotation paths.

use lopdf::{Document, Object, ObjectId};

use crate::error::{PdfError, PdfResult};

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

/// The visible region of a page as `(x0, y0, width, height)`.
///
/// A viewer shows the crop box clipped to the media box (ISO 32000-1 14.11.2),
/// which is what the canvas reports coordinates against, and either box may
/// have its corners written in any order (7.9.5). A missing, empty or disjoint
/// crop box leaves the media box as the visible region, which is what a viewer
/// shows in that case too.
pub(super) fn visible_box(media: [f64; 4], crop: Option<[f64; 4]>) -> (f64, f64, f64, f64) {
    let (mx0, my0, mx1, my1) = corners(media);
    let Some(crop) = crop else { return (mx0, my0, mx1 - mx0, my1 - my0) };
    let (cx0, cy0, cx1, cy1) = corners(crop);
    let (x0, y0) = (cx0.max(mx0), cy0.max(my0));
    let (x1, y1) = (cx1.min(mx1), cy1.min(my1));
    if x1 <= x0 || y1 <= y0 {
        return (mx0, my0, mx1 - mx0, my1 - my0);
    }
    (x0, y0, x1 - x0, y1 - y0)
}

fn corners(rect: [f64; 4]) -> (f64, f64, f64, f64) {
    (rect[0].min(rect[2]), rect[1].min(rect[3]), rect[0].max(rect[2]), rect[1].max(rect[3]))
}

/// A page-tree inherited numeric attribute (`/Rotate`).
pub(super) fn page_rotation_or_inherited(doc: &Document, page_id: ObjectId) -> i32 {
    let mut current = Some(page_id);
    let mut hops = 0;
    while let Some(id) = current {
        hops += 1;
        if hops > 64 {
            break;
        }
        let page = match doc.get_dictionary(id) {
            Ok(page) => page,
            Err(_) => break,
        };
        let rotation = page.get(b"Rotate").ok().and_then(|value| value.as_i64().ok());
        if let Some(rotation) = rotation {
            return rotation.rem_euclid(360) as i32;
        }
        current = page.get(b"Parent").ok().and_then(|value| value.as_reference().ok());
    }
    0
}

/// A page box lookup that follows the page tree, so reading a document does not
/// have to materialize inherited attributes onto its pages.
pub(super) fn inherited_box(
    doc: &Document,
    page_id: ObjectId,
    read: impl Fn(&Document, ObjectId) -> Option<[f64; 4]>,
) -> Option<[f64; 4]> {
    let mut current = Some(page_id);
    let mut hops = 0;
    while let Some(id) = current {
        hops += 1;
        if hops > 64 {
            break;
        }
        if let Some(found) = read(doc, id) {
            return Some(found);
        }
        current = doc
            .get_dictionary(id)
            .ok()
            .and_then(|page| page.get(b"Parent").ok().and_then(|value| value.as_reference().ok()));
    }
    None
}

/// `/MediaBox` through the page tree, where a page may inherit it.
pub(super) fn page_mediabox_or_inherited(doc: &Document, page_id: ObjectId) -> PdfResult<[f64; 4]> {
    inherited_box(doc, page_id, |doc, id| crate::docutil::page_mediabox(doc, id).ok())
        .ok_or_else(|| PdfError::CorruptPdf("page has no MediaBox".into()))
}
