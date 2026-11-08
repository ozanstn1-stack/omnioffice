//! The box a viewer actually shows, and the page-tree lookup that finds it.
//!
//! A PDF page carries up to five boxes. What a renderer draws is the **crop
//! box clipped to the media box** (ISO 32000-1 14.11.2), and both boxes may be
//! written with their corners in any order (7.9.5) or inherited from the page
//! tree (7.7.3.4). Every feature that takes a rectangle in *display space* -
//! annotations, watermarks, page-number stamps, exhibit labels - has to agree
//! with the viewer on that region, so the rule lives here once.

use lopdf::{Document, ObjectId};

use crate::docutil::{page_cropbox, page_mediabox, Matrix};

/// The visible region of a page as `(x0, y0, width, height)`.
///
/// A missing, empty or disjoint crop box leaves the media box as the visible
/// region, which is what a viewer shows in that case too.
pub fn visible_box(media: [f64; 4], crop: Option<[f64; 4]>) -> (f64, f64, f64, f64) {
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

/// [`visible_box`] for one page, following the page tree for an inherited box.
pub fn page_visible_box(doc: &Document, page_id: ObjectId) -> (f64, f64, f64, f64) {
    let media =
        inherited_box(doc, page_id, |doc, id| page_mediabox(doc, id).ok()).unwrap_or(crate::docutil::A4_MEDIA_BOX);
    let crop = inherited_box(doc, page_id, |doc, id| page_cropbox(doc, id).ok().flatten());
    visible_box(media, crop)
}

/// A page box lookup that follows the page tree, so reading a document does not
/// have to materialize inherited attributes onto its pages.
pub(crate) fn inherited_box(
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

/// `/Rotate` through the page tree, where a page may inherit it (ISO 32000-1
/// 7.7.3.4). Values outside the four quarter turns are brought back into range.
pub(crate) fn page_rotation_or_inherited(doc: &Document, page_id: ObjectId) -> i32 {
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
        if let Some(rotation) = page.get(b"Rotate").ok().and_then(|value| value.as_i64().ok()) {
            return rotation.rem_euclid(360) as i32;
        }
        current = page.get(b"Parent").ok().and_then(|value| value.as_reference().ok());
    }
    0
}

/// The transform from display space (top-left origin, `/Rotate` applied) into
/// the page's own coordinates, with the size of the displayed page.
///
/// Display space is measured from the corner of the visible box (7.7.3.3), so
/// the origin translation comes first and the rotation second.
pub fn display_frame(rotation: i32, visible: (f64, f64, f64, f64)) -> (Matrix, f64, f64) {
    let (x0, y0, page_w, page_h) = visible;
    let (display_width, display_height) = Matrix::displayed_size(rotation, page_w, page_h);
    let to_page = Matrix::translate(x0, y0).mul(Matrix::display_to_page(rotation, page_w, page_h));
    (to_page, display_width, display_height)
}

fn corners(rect: [f64; 4]) -> (f64, f64, f64, f64) {
    (rect[0].min(rect[2]), rect[1].min(rect[3]), rect[0].max(rect[2]), rect[1].max(rect[3]))
}
