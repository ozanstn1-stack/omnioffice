//! Paints the collected draws into a sheet's content stream and resources.

use super::{Alpha, Draw, FontSpec, Look, Synth};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn install(doc: &mut Document, sheet_id: ObjectId, draws: Vec<Draw>) {
    let Some(resources_id) = resources_id(doc, sheet_id) else { return };
    let Some(mut resources) = doc.get_dictionary(resources_id).ok().cloned() else { return };
    let mut xobjects = sub(&resources, b"XObject");
    let mut graphics = sub(&resources, b"ExtGState");
    let mut properties = sub(&resources, b"Properties");
    let mut body = String::from("\n");
    // One subset font per distinct character set on this sheet: the bundled
    // face is parsed once and the font object written once.
    let mut fonts: BTreeMap<BTreeSet<char>, Option<ObjectId>> = BTreeMap::new();
    for (index, draw) in draws.iter().enumerate() {
        let name = format!("AnX{}", index);
        let xobject = match &draw.look {
            Look::Stream(id) => *id,
            Look::Synth(synth) => {
                let form = synthesized_form(doc, synth, &mut fonts);
                doc.add_object(Object::Stream(form))
            }
        };
        xobjects.set(name.as_str(), Object::Reference(xobject));
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

/// The form XObject that carries a synthesized appearance. Its content is in
/// page space, so the block's placement matrix is all that is needed; the box
/// only bounds the content.
pub(super) fn synthesized_form(
    doc: &mut Document,
    synth: &Synth,
    fonts: &mut BTreeMap<BTreeSet<char>, Option<ObjectId>>,
) -> Stream {
    let mut dict = Dictionary::new();
    dict.set("Type", Object::Name(b"XObject".to_vec()));
    dict.set("Subtype", Object::Name(b"Form".to_vec()));
    dict.set("BBox", Object::Array(synth.bbox.iter().map(|value| Object::Real(*value as f32)).collect()));
    let mut content = Vec::with_capacity(synth.ops.len() + 8);
    if synth.multiply {
        // The blend state belongs to this form: applying it in the sheet would
        // need the state in the sheet's own resources.
        dict.set("Resources", Object::Dictionary(multiply_resources()));
        content.extend_from_slice(b"/H0 gs\n");
    }
    if let Some(font) = &synth.font {
        if let Some(resources) = font_resources(doc, font, fonts) {
            dict.set("Resources", Object::Dictionary(resources));
        }
    }
    content.extend_from_slice(&synth.ops);
    Stream::new(dict, content)
}

/// The form resources for synthesized text: one standard font referenced by
/// name, with the glyphs WinAnsi lacks declared through `/Differences`.
fn font_resources(
    doc: &mut Document,
    font: &FontSpec,
    fonts: &mut BTreeMap<BTreeSet<char>, Option<ObjectId>>,
) -> Option<Dictionary> {
    let FontSpec::Standard(font) = font else {
        if let FontSpec::Subset(chars) = font {
            let id = *fonts
                .entry(chars.clone())
                .or_insert_with(|| crate::textfont::SubsetFace::bundled()?.write(doc, chars).ok());
            // A face that cannot be written leaves that caption without a
            // resource; every other annotation on the sheet still paints.
            return Some(named_font(crate::textfont::RESOURCE_NAME.as_bytes(), id?));
        }
        let FontSpec::Resource { key, id } = font else { unreachable!() };
        return Some(named_font(key, *id));
    };
    let mut encoding = Dictionary::new();
    encoding.set("Type", Object::Name(b"Encoding".to_vec()));
    encoding.set("BaseEncoding", Object::Name(b"WinAnsiEncoding".to_vec()));
    let mut differences: Vec<Object> = vec![Object::Integer(i64::from(super::appearance::extra_base()))];
    for name in super::appearance::differences() {
        differences.push(Object::Name(name.as_bytes().to_vec()));
    }
    encoding.set("Differences", Object::Array(differences));

    let mut font_dict = Dictionary::new();
    font_dict.set("Type", Object::Name(b"Font".to_vec()));
    font_dict.set("Subtype", Object::Name(b"Type1".to_vec()));
    font_dict.set("BaseFont", Object::Name(font.base_font().as_bytes().to_vec()));
    font_dict.set("Encoding", Object::Dictionary(encoding));

    let mut fonts = Dictionary::new();
    fonts.set("FOmni0", Object::Dictionary(font_dict));
    let mut resources = Dictionary::new();
    resources.set("Font", Object::Dictionary(fonts));
    Some(resources)
}

/// `/Font << key <reference> >>`: the form resources of one referenced font.
fn named_font(key: &[u8], id: ObjectId) -> Dictionary {
    let mut fonts = Dictionary::new();
    fonts.set(key, Object::Reference(id));
    let mut resources = Dictionary::new();
    resources.set("Font", Object::Dictionary(fonts));
    resources
}

/// A highlight blends with what is underneath it.
fn multiply_resources() -> Dictionary {
    let mut state = Dictionary::new();
    state.set("Type", Object::Name(b"ExtGState".to_vec()));
    state.set("BM", Object::Name(b"Multiply".to_vec()));
    let mut states = Dictionary::new();
    states.set("H0", Object::Dictionary(state));
    let mut resources = Dictionary::new();
    resources.set("ExtGState", Object::Dictionary(states));
    resources
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
