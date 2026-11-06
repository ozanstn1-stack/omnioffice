//! Paints the collected draws into a sheet's content stream and resources.

use super::{Alpha, Draw};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

pub(super) fn install(doc: &mut Document, sheet_id: ObjectId, draws: Vec<Draw>) {
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
