//! Sheet construction for N-up imposition.
//!
//! One sheet is a fresh page dictionary whose content stream paints the source
//! pages (already Form XObjects) in slot order, plus an optional light border,
//! and whose `/Resources` names those forms. The placement of every slot is
//! reported back with the page so later steps (link annotations) can map
//! source-page coordinates into the sheet's own space.

use super::{Layout, PageForm};
use crate::docutil::Matrix;
use crate::error::PdfResult;
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};
use std::collections::HashMap;

/// One source page painted on a sheet: where it came from, the matrix that
/// takes the source page's user space into the sheet space (source rotation
/// included), and the sheet-space rectangle of the cell it may draw in.
pub(super) struct Placement {
    pub source: ObjectId,
    pub matrix: Matrix,
    pub clip: [f64; 4],
}

/// Builds one output sheet page and reports how each painted slot was placed.
pub(super) fn build_sheet_page(
    doc: &mut Document,
    forms: &[PageForm],
    form_index: &HashMap<u32, usize>,
    slots: &[Option<u32>],
    layout: &Layout,
    border: bool,
) -> PdfResult<(ObjectId, Vec<Placement>)> {
    let mut xobjects = Dictionary::new();
    let mut content: Vec<u8> = Vec::new();
    let mut placements: Vec<Placement> = Vec::new();
    for (slot, page) in slots.iter().enumerate() {
        let Some(page) = page else { continue };
        let form = &forms[form_index[page]];
        let scale = (layout.cell_w / form.display_w).min(layout.cell_h / form.display_h);
        let col = slot % layout.cols;
        let row = slot / layout.cols;
        let placed_w = form.display_w * scale;
        let placed_h = form.display_h * scale;
        let x = layout.cell_x(col) + (layout.cell_w - placed_w) / 2.0;
        let y = layout.cell_y(row) + (layout.cell_h - placed_h) / 2.0;
        let name = format!("Fm{}", form_index[page]);
        xobjects.set(name.as_str(), Object::Reference(form.id));
        // The page -> display transform for the source rotation: applying its
        // inverse paints the page as it was displayed, pre-rotating the
        // placement instead of relying on a `/Rotate` on the sheet.
        let to_display =
            Matrix::display_to_page(form.rotation, form.page_w, form.page_h).inverse().unwrap_or(Matrix::IDENTITY);
        let matrix = Matrix::translate(x, y).mul(Matrix::scale(scale, scale)).mul(to_display);
        content.extend_from_slice(format!("q\n{}\n/{} Do\nQ\n", matrix.to_cm(), name).as_bytes());
        if border {
            content.extend_from_slice(
                format!("0.5 w\n0.5 G\n{x:.4} {y:.4} {placed_w:.4} {placed_h:.4} re S\n").as_bytes(),
            );
        }
        placements.push(Placement { source: form.source, matrix, clip: [x, y, x + placed_w, y + placed_h] });
    }
    let resources_id = doc.add_object(Object::Dictionary(dictionary! {
        "XObject" => Object::Dictionary(xobjects),
    }));
    let content_id = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), content)));
    let sheet_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Page",
        "MediaBox" => vec![
            Object::Real(0.0),
            Object::Real(0.0),
            Object::Real(layout.sheet_w as f32),
            Object::Real(layout.sheet_h as f32),
        ],
        "Resources" => Object::Reference(resources_id),
        "Contents" => Object::Reference(content_id),
    }));
    Ok((sheet_id, placements))
}
