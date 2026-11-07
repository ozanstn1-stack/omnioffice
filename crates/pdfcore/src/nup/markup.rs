//! Flattens printable annotations onto the imposed sheets.
//!
//! A sheet paints its source pages as Form XObjects, and an annotation cannot
//! live inside a form: it belongs to a page. Links and the outline are carried
//! separately (`links`, `outline`); the markup kinds that are meant to reach
//! paper - notes, highlights, ink, stamps, shapes - are painted here.
//!
//! The appearance stream is **referenced, never copied**: it is added to the
//! sheet's `/XObject` resources under a fresh name and drawn with a `Do`, with
//! the transformation that takes the stream's own box to the annotation
//! rectangle and then into the sheet. An annotation whose appearance is missing
//! or unreadable is skipped; it never fails the imposition.
//!
//! Widgets are painted when `widget::classify` trusts their appearance; popups,
//! links, redactions and the printing-production kinds are left alone: a
//! flattened redaction would look redacted while the text is still there.

use super::dests::resolve;
use super::links::{clip_rect, map_rect, number, read_rect, source_annotations};
use super::sheet::Placement;
use crate::docutil::{page_rotation, Matrix};
use lopdf::{Dictionary, Document, Object, ObjectId};

mod appearance;
mod install;
mod widget;

use install::install;

/// `/Hidden`: a hidden annotation is not printed even when Print is set.
const FLAG_HIDDEN: i64 = 2;
/// `/Print`.
const FLAG_PRINT: i64 = 4;
/// `/NoRotate`: the appearance keeps its orientation when the page is rotated.
const FLAG_NOROTATE: i64 = 16;
/// A sheet with more than this many painted annotations is not worth the risk.
const MAX_MARKUP_PER_SHEET: usize = 4096;

/// Annotation kinds that are meant to be printed. `Widget`, `Popup`, `Link`,
/// `Redact`, `Watermark`, `PrinterMark`, `TrapNet` and the multimedia kinds are
/// deliberately absent.
const PRINTABLE: &[&[u8]] = &[
    b"Text",
    b"FreeText",
    b"Line",
    b"Square",
    b"Circle",
    b"Polygon",
    b"PolyLine",
    b"Highlight",
    b"Underline",
    b"Squiggly",
    b"StrikeOut",
    b"Caret",
    b"Ink",
    b"Stamp",
    b"FileAttachment",
];

/// Where a draw's content comes from.
pub(super) enum Look {
    /// An appearance stream that was already in the document.
    Stream(ObjectId),
    /// Operators synthesized from the annotation's own geometry.
    Synth(Synth),
}

/// A synthesized appearance: operators in page space plus the box they cover.
pub(super) struct Synth {
    pub(super) ops: Vec<u8>,
    pub(super) bbox: [f64; 4],
    /// `/Highlight` blends, so its form needs a Multiply graphics state.
    pub(super) multiply: bool,
    /// The standard font the content references by name, if it draws text.
    pub(super) font: Option<appearance::Std14>,
}

/// One annotation ready to be painted on a sheet.
pub(super) struct Draw {
    pub(super) look: Look,
    /// Appearance space -> sheet space (the `cm` written before the `Do`).
    pub(super) matrix: Matrix,
    /// Sheet-space rectangle the block is clipped to.
    pub(super) clip: [f64; 4],
    pub(super) alpha: Option<Alpha>,
    /// Optional-content group the block is nested in.
    pub(super) oc: Option<ObjectId>,
}

/// `/CA`, `/ca` and `/BM`, when they differ from the defaults.
pub(super) struct Alpha {
    pub(super) stroke: f64,
    pub(super) fill: f64,
    pub(super) blend: Option<Vec<u8>>,
}

/// Paints every printable annotation of the source pages onto its sheet.
///
/// Never fails: an annotation with a broken appearance is skipped on its own,
/// and a sheet with nothing to paint is left byte-identical.
pub(super) fn carry_markup(doc: &mut Document, sheets: &[(ObjectId, Vec<Placement>)]) {
    // Read everything against an immutable document first, then write.
    let field_context = widget::AcroCtx::from_doc(doc);
    let mut pending: Vec<(ObjectId, Vec<Draw>)> = Vec::with_capacity(sheets.len());
    for (sheet_id, placements) in sheets {
        let mut draws: Vec<Draw> = Vec::new();
        for placement in placements {
            let rotation = page_rotation(doc, placement.source).unwrap_or(0);
            for annot in source_annotations(doc, placement.source) {
                if draws.len() >= MAX_MARKUP_PER_SHEET {
                    break;
                }
                if let Some(draw) = markup_draw(doc, &annot, placement, rotation, &field_context) {
                    draws.push(draw);
                }
            }
        }
        pending.push((*sheet_id, draws));
    }
    for (sheet_id, draws) in pending {
        if !draws.is_empty() {
            install(doc, sheet_id, draws);
        }
    }
}

/// Turns one annotation into a draw, or `None` when it must stay behind.
fn markup_draw(
    doc: &Document,
    annot: &Dictionary,
    placement: &Placement,
    rotation: i32,
    context: &widget::AcroCtx,
) -> Option<Draw> {
    let subtype = annot.get(b"Subtype").ok()?.as_name().ok()?;
    let is_widget = subtype == b"Widget";
    if !is_widget && !PRINTABLE.contains(&subtype) {
        return None;
    }
    if is_widget && widget::classify(doc, annot, context) != widget::Decision::Draw {
        return None;
    }
    let flags = annot.get(b"F").ok().and_then(|value| value.as_i64().ok()).unwrap_or(0);
    if flags & FLAG_PRINT == 0 || flags & FLAG_HIDDEN != 0 {
        return None;
    }
    let rect = read_rect(doc, annot.get(b"Rect").ok()?)?;
    // An annotation that ships an appearance is never re-synthesized: a broken
    // /AP means "nothing to draw", not "guess from the geometry".
    let (look, fit, boxed) = if annot.has(b"AP") {
        let (stream, bbox, appearance_matrix) = appearance_of(doc, annot)?;
        (Look::Stream(stream), fit_to_rect(bbox, appearance_matrix, rect)?, rect)
    } else {
        let synth = appearance::synthesize(doc, annot, subtype, rect)?;
        let boxed = synth.bbox;
        (Look::Synth(synth), Matrix::IDENTITY, boxed)
    };
    let matrix = if flags & FLAG_NOROTATE != 0 && rotation.rem_euclid(360) != 0 {
        placement.matrix.mul(no_rotate(rect, rotation)).mul(fit)
    } else {
        placement.matrix.mul(fit)
    };
    if !finite(&matrix) {
        return None;
    }
    // The page's own box is `placement.clip`, so a link and its markup resolve
    // to the same area on the sheet.
    let clip = clip_rect(map_rect(&placement.matrix, boxed), placement.clip)?;
    Some(Draw { look, matrix, clip, alpha: alpha_of(annot), oc: oc_of(annot) })
}

/// The `/AP /N` stream, its `/BBox` and its `/Matrix`.
fn appearance_of(doc: &Document, annot: &Dictionary) -> Option<(ObjectId, [f64; 4], Matrix)> {
    let appearance = resolve(doc, annot.get(b"AP").ok()?).as_dict().ok()?;
    let normal = appearance.get(b"N").ok()?;
    let stream_id = appearance_stream(doc, annot, normal)?;
    let stream = doc.get_object(stream_id).ok()?.as_stream().ok()?;
    if let Ok(subtype) = stream.dict.get(b"Subtype") {
        if subtype.as_name().ok()? != b"Form" {
            return None;
        }
    }
    let bbox = read_rect(doc, stream.dict.get(b"BBox").ok()?)?;
    let matrix = match stream.dict.get(b"Matrix").ok() {
        Some(value) => Matrix(read_matrix(value)?),
        None => Matrix::IDENTITY,
    };
    Some((stream_id, bbox, matrix))
}

/// A stream behind `/N`, or the `/AS` state of a state dictionary.
fn appearance_stream(doc: &Document, annot: &Dictionary, normal: &Object) -> Option<ObjectId> {
    let Object::Reference(id) = normal else { return None };
    let object = doc.get_object(*id).ok()?;
    if object.as_stream().is_ok() {
        return Some(*id);
    }
    let states = object.as_dict().ok()?;
    let state = annot.get(b"AS").ok()?.as_name().ok()?;
    states.get(state).ok()?.as_reference().ok()
}

/// Maps the appearance's box onto the annotation rectangle (spec 12.5.5).
fn fit_to_rect(bbox: [f64; 4], matrix: Matrix, rect: [f64; 4]) -> Option<Matrix> {
    let boxed = map_rect(&matrix, bbox);
    let width = boxed[2] - boxed[0];
    let height = boxed[3] - boxed[1];
    let target_w = rect[2] - rect[0];
    let target_h = rect[3] - rect[1];
    if !(width > 1e-6 && height > 1e-6 && target_w.abs() > 1e-6 && target_h.abs() > 1e-6) {
        return None;
    }
    Some(
        Matrix::translate(rect[0], rect[1])
            .mul(Matrix::scale(target_w / width, target_h / height))
            .mul(Matrix::translate(-boxed[0], -boxed[1])),
    )
}

/// `/NoRotate` keeps the appearance upright around the rectangle's top-left
/// corner; the placement matrix already carries the page rotation, so undoing
/// it here leaves the two rotations cancelling out.
fn no_rotate(rect: [f64; 4], rotation: i32) -> Matrix {
    let anchor = (rect[0], rect[3]);
    Matrix::translate(anchor.0, anchor.1)
        .mul(Matrix::rotate_deg(rotation as f64))
        .mul(Matrix::translate(-anchor.0, -anchor.1))
}

fn alpha_of(annot: &Dictionary) -> Option<Alpha> {
    let stroke = annot.get(b"CA").ok().and_then(number).map(|value| value.clamp(0.0, 1.0)).unwrap_or(1.0);
    let fill = annot.get(b"ca").ok().and_then(number).map(|value| value.clamp(0.0, 1.0)).unwrap_or(stroke);
    let blend = annot.get(b"BM").ok().and_then(|value| value.as_name().ok()).map(<[u8]>::to_vec);
    let plain = (stroke - 1.0).abs() < 1e-9 && (fill - 1.0).abs() < 1e-9;
    let normal_blend = blend.as_deref().is_none_or(|name| name == b"Normal" || name == b"Compatible");
    if plain && normal_blend {
        return None;
    }
    Some(Alpha { stroke, fill, blend })
}

fn oc_of(annot: &Dictionary) -> Option<ObjectId> {
    annot.get(b"OC").ok()?.as_reference().ok()
}

/// Adds the draws to the sheet's content stream and resources.
fn read_matrix(value: &Object) -> Option<[f64; 6]> {
    let items = value.as_array().ok()?;
    if items.len() != 6 {
        return None;
    }
    let mut matrix = [0.0; 6];
    for (slot, item) in items.iter().enumerate() {
        matrix[slot] = number(item)?;
    }
    Some(matrix)
}

fn finite(matrix: &Matrix) -> bool {
    matrix.0.iter().all(|value| value.is_finite())
}
