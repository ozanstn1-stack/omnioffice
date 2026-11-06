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
//! Widgets, popups, links, redactions and the printing-production kinds are
//! left alone on purpose: flattening a form field without its `/AcroForm`
//! bookkeeping can print the wrong value, and a flattened redaction would look
//! like redacted content while the text is still there.

use super::dests::resolve;
use super::links::{clip_rect, map_rect, number, read_rect, source_annotations};
use super::sheet::Placement;
use crate::docutil::{page_rotation, Matrix};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

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

/// One annotation ready to be painted on a sheet.
struct Draw {
    appearance: ObjectId,
    /// Appearance space -> sheet space (the `cm` written before the `Do`).
    matrix: Matrix,
    /// Sheet-space rectangle the block is clipped to.
    clip: [f64; 4],
    alpha: Option<Alpha>,
    /// Optional-content group the block is nested in.
    oc: Option<ObjectId>,
}

/// `/CA`, `/ca` and `/BM`, when they differ from the defaults.
struct Alpha {
    stroke: f64,
    fill: f64,
    blend: Option<Vec<u8>>,
}

/// Paints every printable annotation of the source pages onto its sheet.
///
/// Never fails: an annotation with a broken appearance is skipped on its own,
/// and a sheet with nothing to paint is left byte-identical.
pub(super) fn carry_markup(doc: &mut Document, sheets: &[(ObjectId, Vec<Placement>)]) {
    // Read everything against an immutable document first, then write.
    let mut pending: Vec<(ObjectId, Vec<Draw>)> = Vec::with_capacity(sheets.len());
    for (sheet_id, placements) in sheets {
        let mut draws: Vec<Draw> = Vec::new();
        for placement in placements {
            let rotation = page_rotation(doc, placement.source).unwrap_or(0);
            for annot in source_annotations(doc, placement.source) {
                if draws.len() >= MAX_MARKUP_PER_SHEET {
                    break;
                }
                if let Some(draw) = markup_draw(doc, &annot, placement, rotation) {
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
fn markup_draw(doc: &Document, annot: &Dictionary, placement: &Placement, rotation: i32) -> Option<Draw> {
    let subtype = annot.get(b"Subtype").ok()?.as_name().ok()?;
    if !PRINTABLE.contains(&subtype) {
        return None;
    }
    let flags = annot.get(b"F").ok().and_then(|value| value.as_i64().ok()).unwrap_or(0);
    if flags & FLAG_PRINT == 0 || flags & FLAG_HIDDEN != 0 {
        return None;
    }
    let rect = read_rect(doc, annot.get(b"Rect").ok()?)?;
    let (appearance, bbox, appearance_matrix) = appearance_of(doc, annot)?;
    let fit = fit_to_rect(bbox, appearance_matrix, rect)?;
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
    let clip = clip_rect(map_rect(&placement.matrix, rect), placement.clip)?;
    Some(Draw { appearance, matrix, clip, alpha: alpha_of(annot), oc: oc_of(annot) })
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
fn install(doc: &mut Document, sheet_id: ObjectId, draws: Vec<Draw>) {
    let Some(resources_id) = resources_id(doc, sheet_id) else { return };
    let Some(mut resources) = doc.get_dictionary(resources_id).ok().cloned() else { return };
    let mut xobjects = sub(&resources, b"XObject");
    let mut graphics = sub(&resources, b"ExtGState");
    let mut properties = sub(&resources, b"Properties");
    let mut body = String::from("\n");
    for (index, draw) in draws.iter().enumerate() {
        let name = format!("AnX{}", index);
        xobjects.set(name.as_str(), Object::Reference(draw.appearance));
        let [x0, y0, x1, y1] = draw.clip;
        body.push_str("q\n");
        body.push_str(&format!("{x0:.4} {y0:.4} {:.4} {:.4} re W n\n", x1 - x0, y1 - y0));
        if let Some(alpha) = &draw.alpha {
            let state = format!("AnGs{}", index);
            graphics.set(state.as_str(), Object::Dictionary(alpha_dict(alpha)));
            body.push_str(&format!("/{} gs\n", state));
        }
        if let Some(oc) = draw.oc {
            let state = format!("AnOc{}", index);
            properties.set(state.as_str(), Object::Reference(oc));
            body.push_str(&format!("/OC /{} BDC\n", state));
        }
        body.push_str(&format!("{}\n/{name} Do\n", draw.matrix.to_cm()));
        if draw.oc.is_some() {
            body.push_str("EMC\n");
        }
        body.push_str("Q\n");
    }
    if !xobjects.is_empty() {
        resources.set("XObject", Object::Dictionary(xobjects));
    }
    if !graphics.is_empty() {
        resources.set("ExtGState", Object::Dictionary(graphics));
    }
    if !properties.is_empty() {
        resources.set("Properties", Object::Dictionary(properties));
    }
    if let Ok(target) = doc.get_dictionary_mut(resources_id) {
        *target = resources;
    }
    append_content(doc, sheet_id, body.as_bytes());
}

fn alpha_dict(alpha: &Alpha) -> Dictionary {
    let mut dict = Dictionary::new();
    dict.set("Type", Object::Name(b"ExtGState".to_vec()));
    dict.set("CA", Object::Real(alpha.stroke as f32));
    dict.set("ca", Object::Real(alpha.fill as f32));
    if let Some(blend) = &alpha.blend {
        dict.set("BM", Object::Name(blend.clone()));
    }
    dict
}

/// The sheet's `/Resources` dictionary object, created if it is inline.
fn resources_id(doc: &mut Document, sheet_id: ObjectId) -> Option<ObjectId> {
    let value = doc.get_dictionary(sheet_id).ok()?.get(b"Resources").ok()?.clone();
    match value {
        Object::Reference(id) => Some(id),
        Object::Dictionary(dict) => {
            let id = doc.add_object(Object::Dictionary(dict));
            if let Ok(page) = doc.get_dictionary_mut(sheet_id) {
                page.set("Resources", Object::Reference(id));
            }
            Some(id)
        }
        _ => None,
    }
}

fn sub(resources: &Dictionary, key: &[u8]) -> Dictionary {
    resources.get(key).ok().and_then(|value| value.as_dict().ok()).cloned().unwrap_or_default()
}

/// Appends a content stream to the sheet, keeping the existing one.
fn append_content(doc: &mut Document, sheet_id: ObjectId, body: &[u8]) {
    let Ok(contents) = doc.get_dictionary(sheet_id).and_then(|page| page.get(b"Contents").cloned()) else {
        return;
    };
    let stream_id = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), body.to_vec())));
    let items = match contents {
        Object::Reference(id) => vec![Object::Reference(id), Object::Reference(stream_id)],
        Object::Array(mut items) => {
            items.push(Object::Reference(stream_id));
            items
        }
        _ => return,
    };
    if let Ok(page) = doc.get_dictionary_mut(sheet_id) {
        page.set("Contents", Object::Array(items));
    }
}

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
