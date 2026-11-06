//! Printable annotation fixtures and readers for the N-up flattening tests.
//!
//! The page builder lives in `link_fixtures.rs`; this module only adds
//! appearance streams, annotation dictionaries and the reader that parses the
//! sheet content stream back into draw blocks.

#![allow(dead_code)]

use crate::fixtures::{rect_array, Fixture};
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};

/// A tiny appearance stream: a stroked square, so it is visible and cheap.
pub fn appearance(bbox: [f64; 4], matrix: Option<[f64; 6]>) -> Object {
    let [x0, y0, x1, y1] = bbox;
    let body = format!(
        "q\n1 0 0 RG\n2 w\n{:.4} {:.4} {:.4} {:.4} re S\nQ\n",
        x0 + 5.0,
        y0 + 5.0,
        x1 - x0 - 10.0,
        y1 - y0 - 10.0
    );
    form(body.as_bytes(), bbox, matrix)
}

/// A form XObject with the given content, box and optional matrix.
pub fn form(body: &[u8], bbox: [f64; 4], matrix: Option<[f64; 6]>) -> Object {
    let mut dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Form",
        "BBox" => rect_array(bbox),
    };
    if let Some(matrix) = matrix {
        dict.set("Matrix", Object::Array(matrix.iter().map(|value| Object::Real(*value as f32)).collect()));
    }
    Object::Stream(Stream::new(dict, body.to_vec()))
}

/// An annotation of `subtype` covering `rect`, with the given flags and `/AP /N`
/// stream.
pub fn markup(subtype: &str, rect: [f64; 4], flags: Option<i64>, appearance: ObjectId) -> Dictionary {
    let mut annot = dictionary! {
        "Type" => "Annot",
        "Subtype" => Object::Name(subtype.as_bytes().to_vec()),
        "Rect" => rect_array(rect),
        "AP" => dictionary! { "N" => Object::Reference(appearance) },
    };
    if let Some(flags) = flags {
        annot.set("F", Object::Integer(flags));
    }
    annot
}

/// `/AP /N` as a state dictionary: the caller names the states and picks one
/// with `/AS`.
/// Stores a `/N` state dictionary and returns the reference an `/AP` can use.
pub fn state_dictionary(fixture: &mut Fixture, states: &[(&str, ObjectId)]) -> ObjectId {
    let mut dict = Dictionary::new();
    for (name, id) in states {
        dict.set(*name, Object::Reference(*id));
    }
    fixture.add_object(Object::Dictionary(dict))
}

/// One `q … Q` block the flattening pass wrote onto a sheet.
pub struct Block {
    pub clip: [f64; 4],
    pub cm: [f64; 6],
    pub name: String,
    pub graphics_state: Option<String>,
    pub marked_content: Option<String>,
}

/// Parses the sheet content stream into the blocks the pass appends.
pub fn blocks(content: &str) -> Vec<Block> {
    let mut out: Vec<Block> = Vec::new();
    let mut current: Option<Block> = None;
    for line in content.lines() {
        let line = line.trim();
        if line == "q" {
            current = Some(Block {
                clip: [0.0; 4],
                cm: [0.0; 6],
                name: String::new(),
                graphics_state: None,
                marked_content: None,
            });
            continue;
        }
        let Some(block) = current.as_mut() else { continue };
        if line == "Q" {
            if let Some(done) = current.take() {
                out.push(done);
            }
            continue;
        }
        if let Some(operands) = line.strip_suffix(" re W n") {
            if let Some(values) = numbers(operands, 4) {
                block.clip = [values[0], values[1], values[0] + values[2], values[1] + values[3]];
            }
        } else if let Some(operands) = line.strip_suffix(" cm") {
            if let Some(values) = numbers(operands, 6) {
                block.cm = [values[0], values[1], values[2], values[3], values[4], values[5]];
            }
        } else if let Some(name) = line.strip_suffix(" Do") {
            block.name = name.trim_start_matches('/').to_string();
        } else if let Some(state) = line.strip_suffix(" gs") {
            block.graphics_state = Some(state.trim_start_matches('/').to_string());
        } else if let Some(state) = line.strip_suffix(" BDC") {
            block.marked_content = Some(state.rsplit_once('/').map(|(_, name)| name).unwrap_or(state).to_string());
        }
    }
    out
}

fn numbers(operands: &str, count: usize) -> Option<Vec<f64>> {
    let values: Vec<f64> = operands.split_whitespace().map(|value| value.parse().ok()).collect::<Option<_>>()?;
    (values.len() == count).then_some(values)
}

/// The `/Rect` of the first `/Link` annotation of a sheet, when it carries one.
pub fn link_rect(doc: &Document, sheet_id: ObjectId) -> Option<[f64; 4]> {
    for annot in annots(doc, sheet_id) {
        if annot.get(b"Subtype").ok()?.as_name().ok()? == b"Link" {
            return read_rect(annot.get(b"Rect").ok()?);
        }
    }
    None
}

/// The annotations of a page, resolving one level of indirection.
pub fn annots(doc: &Document, page_id: ObjectId) -> Vec<Dictionary> {
    let page = doc.get_dictionary(page_id).ok();
    let Some(page) = page else { return Vec::new() };
    let items = match page.get(b"Annots") {
        Ok(Object::Array(items)) => items.clone(),
        Ok(Object::Reference(id)) => {
            doc.get_object(*id).ok().and_then(|object| object.as_array().ok().cloned()).unwrap_or_default()
        }
        _ => return Vec::new(),
    };
    items
        .iter()
        .filter_map(|item| match item {
            Object::Reference(id) => doc.get_dictionary(*id).ok().cloned(),
            Object::Dictionary(dict) => Some(dict.clone()),
            _ => None,
        })
        .collect()
}

pub fn read_rect(value: &Object) -> Option<[f64; 4]> {
    let items = value.as_array().ok()?;
    if items.len() != 4 {
        return None;
    }
    let mut rect = [0.0; 4];
    for (slot, item) in items.iter().enumerate() {
        rect[slot] = match item {
            Object::Integer(value) => *value as f64,
            Object::Real(value) => *value as f64,
            _ => return None,
        };
    }
    Some(rect)
}

/// The XObject reference behind a sheet resource name.
pub fn xobject(doc: &Document, sheet_id: ObjectId, name: &str) -> Option<ObjectId> {
    let page = doc.get_dictionary(sheet_id).ok()?;
    let resources = match page.get(b"Resources").ok()? {
        Object::Reference(id) => doc.get_dictionary(*id).ok()?,
        Object::Dictionary(dict) => dict,
        _ => return None,
    };
    let xobjects = resources.get(b"XObject").ok()?.as_dict().ok()?;
    xobjects.get(name.as_bytes()).ok()?.as_reference().ok()
}

/// The `/ExtGState` dictionary behind a sheet resource name.
pub fn graphics_state(doc: &Document, sheet_id: ObjectId, name: &str) -> Option<Dictionary> {
    let page = doc.get_dictionary(sheet_id).ok()?;
    let resources = match page.get(b"Resources").ok()? {
        Object::Reference(id) => doc.get_dictionary(*id).ok()?,
        Object::Dictionary(dict) => dict,
        _ => return None,
    };
    let states = resources.get(b"ExtGState").ok()?.as_dict().ok()?;
    states.get(name.as_bytes()).ok()?.as_dict().ok().cloned()
}

pub fn number(value: &Object) -> Option<f64> {
    match value {
        Object::Integer(value) => Some(*value as f64),
        Object::Real(value) => Some(*value as f64),
        _ => None,
    }
}

/// Corners of a box mapped through a matrix, as an axis-aligned rectangle.
pub fn map_rect(matrix: [f64; 6], rect: [f64; 4]) -> [f64; 4] {
    let corners = [(rect[0], rect[1]), (rect[2], rect[1]), (rect[0], rect[3]), (rect[2], rect[3])];
    let mapped =
        corners.map(|(x, y)| (matrix[0] * x + matrix[2] * y + matrix[4], matrix[1] * x + matrix[3] * y + matrix[5]));
    let min_x = mapped.iter().map(|point| point.0).fold(f64::MAX, f64::min);
    let max_x = mapped.iter().map(|point| point.0).fold(f64::MIN, f64::max);
    let min_y = mapped.iter().map(|point| point.1).fold(f64::MAX, f64::min);
    let max_y = mapped.iter().map(|point| point.1).fold(f64::MIN, f64::max);
    [min_x, min_y, max_x, max_y]
}

pub fn close(a: [f64; 4], b: [f64; 4], tolerance: f64) -> bool {
    (0..4).all(|index| (a[index] - b[index]).abs() <= tolerance)
}
