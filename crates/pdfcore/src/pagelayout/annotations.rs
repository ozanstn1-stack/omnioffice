//! Keeping a page's annotations on the content they mark.
//!
//! Resize moves the page content through a matrix. Everything that points at that
//! content - a highlight's `/Rect`, an ink stroke's points, a line's ends - has to
//! move the same way, or the mark drifts off the words it was drawn on.

use crate::docutil::Matrix;
use crate::error::PdfResult;
use lopdf::{Document, Object, ObjectId};

/// Moves every annotation of the page through the same matrix its content went
/// through, so a highlight stays on the text it marks.
pub(super) fn transform_page_annotations(doc: &mut Document, page_id: ObjectId, matrix: Matrix) -> PdfResult<()> {
    let annots: Vec<ObjectId> = {
        let page = doc.get_dictionary(page_id)?;
        match page.get(b"Annots") {
            Ok(Object::Array(items)) => items.iter().filter_map(|item| item.as_reference().ok()).collect(),
            _ => return Ok(()),
        }
    };
    for annot_id in annots {
        let Ok(annot) = doc.get_dictionary_mut(annot_id) else { continue };
        for key in [b"Rect".as_slice(), b"QuadPoints", b"InkList", b"Vertices", b"L"] {
            let Ok(value) = annot.get_mut(key) else { continue };
            map_numbers(value, matrix);
        }
    }
    Ok(())
}

/// Maps a flat list of coordinates, or a list of such lists, through a matrix.
fn map_numbers(value: &mut Object, matrix: Matrix) {
    let Object::Array(items) = value else { return };
    let numbers: Option<Vec<f64>> = items.iter().map(|item| item.as_float().ok().map(f64::from)).collect();
    let Some(points) = numbers.filter(|points| points.len() >= 2) else {
        // A list of lists (an ink stroke's points, a polygon's vertices).
        for item in items.iter_mut() {
            map_numbers(item, matrix);
        }
        return;
    };
    let mut mapped = Vec::with_capacity(points.len());
    for pair in points.chunks(2) {
        let (x, y) = matrix.apply(pair[0], pair.get(1).copied().unwrap_or(0.0));
        mapped.push(Object::Real(x as f32));
        if pair.len() == 2 {
            mapped.push(Object::Real(y as f32));
        }
    }
    *items = mapped;
}
