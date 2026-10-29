//! A JSON record carried inside a PDF, used by the redaction certificate and
//! the evidence bundle manifest.
//!
//! The record is embedded twice: as an embedded file (`/Names /EmbeddedFiles`)
//! and as an XMP packet in the catalog `/Metadata`. Stripping removes both
//! again and leaves foreign embedded files and foreign XMP alone, so "the file
//! minus the record" is well defined and can be hashed for verification.

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};

use crate::error::{PdfError, PdfResult};

/// Where a record lives: its embedded file name, the file's description and
/// the XMP namespace that marks its metadata packet as ours.
pub struct RecordKind {
    pub file_name: &'static str,
    pub description: &'static str,
    pub xmp_namespace: &'static str,
}

fn load(pdf: &[u8]) -> PdfResult<Document> {
    Document::load_mem(pdf).map_err(|error| PdfError::InvalidPdf(format!("{error}")))
}

fn save(doc: &mut Document) -> PdfResult<Vec<u8>> {
    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|error| PdfError::InvalidPdf(format!("{error}")))?;
    Ok(out)
}

fn catalog_id(doc: &Document) -> PdfResult<ObjectId> {
    doc.trailer.get(b"Root").and_then(Object::as_reference).map_err(|error| PdfError::InvalidPdf(format!("{error}")))
}

/// Finds the record's file specification id, if the PDF carries one.
fn filespec(doc: &Document, kind: &RecordKind) -> Option<ObjectId> {
    let catalog = doc.catalog().ok()?;
    let names = catalog.get(b"Names").ok()?.as_dict().ok()?;
    let embedded = names.get(b"EmbeddedFiles").ok()?.as_dict().ok()?;
    let pairs = embedded.get(b"Names").ok()?.as_array().ok()?;
    for pair in pairs.chunks(2) {
        let [name, reference] = pair else { continue };
        if name.as_str().ok() == Some(kind.file_name.as_bytes()) {
            if let Ok(id) = reference.as_reference() {
                return Some(id);
            }
        }
    }
    None
}

/// Reads the record's bytes from a loaded document.
pub fn read_record(doc: &Document, kind: &RecordKind) -> Option<Vec<u8>> {
    let filespec_id = filespec(doc, kind)?;
    let filespec = doc.get_dictionary(filespec_id).ok()?;
    let ef = filespec.get(b"EF").ok()?.as_dict().ok()?;
    let stream_id = ef.get(b"F").ok()?.as_reference().ok()?;
    let stream = doc.get_object(stream_id).ok()?.as_stream().ok()?;
    Some(stream.content.clone())
}

/// Embeds `json` as the record's embedded file and `xmp` as the catalog
/// metadata.
pub fn embed_record(pdf: &[u8], kind: &RecordKind, json: Vec<u8>, xmp: String) -> PdfResult<Vec<u8>> {
    let mut doc = load(pdf)?;
    let stream_id = doc.add_object(Stream::new(
        dictionary! {
            "Type" => "EmbeddedFile",
            "Subtype" => Object::Name(b"application/json".to_vec()),
        },
        json,
    ));
    let filespec_id = doc.add_object(dictionary! {
        "Type" => "Filespec",
        "F" => Object::string_literal(kind.file_name),
        "Desc" => Object::string_literal(kind.description),
        "EF" => dictionary! { "F" => Object::Reference(stream_id) },
    });

    let catalog_id = catalog_id(&doc)?;
    {
        let catalog = doc.get_dictionary_mut(catalog_id).map_err(|error| PdfError::InvalidPdf(format!("{error}")))?;
        // Merge into an existing name tree rather than replacing it.
        let mut embedded = match catalog.get(b"Names").ok().and_then(|value| value.as_dict().ok()) {
            Some(names) => match names.get(b"EmbeddedFiles").ok().and_then(|value| value.as_dict().ok()) {
                Some(tree) => tree.clone(),
                None => dictionary! {},
            },
            None => dictionary! {},
        };
        let mut pairs: Vec<Object> =
            embedded.get(b"Names").ok().and_then(|value| value.as_array().ok()).cloned().unwrap_or_default();
        pairs.push(Object::string_literal(kind.file_name));
        pairs.push(Object::Reference(filespec_id));
        embedded.set("Names", Object::Array(pairs));
        let mut names = match catalog.get(b"Names").ok().and_then(|value| value.as_dict().ok()) {
            Some(existing) => existing.clone(),
            None => Dictionary::new(),
        };
        names.set("EmbeddedFiles", Object::Dictionary(embedded));
        catalog.set("Names", Object::Dictionary(names));
    }

    let metadata_id = doc.add_object(Stream::new(
        dictionary! {
            "Type" => "Metadata",
            "Subtype" => "XML",
        },
        xmp.into_bytes(),
    ));
    {
        let catalog = doc.get_dictionary_mut(catalog_id).map_err(|error| PdfError::InvalidPdf(format!("{error}")))?;
        catalog.set("Metadata", Object::Reference(metadata_id));
    }
    save(&mut doc)
}

/// Removes the record (embedded file + our XMP metadata) from the PDF.
pub fn strip_record(pdf: &[u8], kind: &RecordKind) -> PdfResult<Vec<u8>> {
    let mut doc = load(pdf)?;

    // Collect and remove the filespec + its stream.
    let mut removed: Vec<ObjectId> = Vec::new();
    if let Some(filespec_id) = filespec(&doc, kind) {
        if let Ok(filespec) = doc.get_dictionary(filespec_id) {
            if let Ok(stream_id) = filespec
                .get(b"EF")
                .and_then(|ef| ef.as_dict())
                .and_then(|ef| ef.get(b"F"))
                .and_then(Object::as_reference)
            {
                removed.push(stream_id);
            }
        }
        removed.push(filespec_id);
    }

    let catalog_id = catalog_id(&doc)?;
    let metadata_id = {
        let catalog = doc.get_dictionary(catalog_id).map_err(|error| PdfError::InvalidPdf(format!("{error}")))?;
        // Only drop metadata that is ours; a foreign XMP packet stays.
        catalog.get(b"Metadata").ok().and_then(|value| value.as_reference().ok()).filter(|id| {
            doc.get_object(*id)
                .ok()
                .and_then(|object| object.as_stream().ok())
                .map(|stream| String::from_utf8_lossy(&stream.content).contains(kind.xmp_namespace))
                .unwrap_or(false)
        })
    };

    {
        let catalog = doc.get_dictionary_mut(catalog_id).map_err(|error| PdfError::InvalidPdf(format!("{error}")))?;
        if let Some(id) = metadata_id {
            removed.push(id);
            catalog.remove(b"Metadata");
        }
        // Rebuild the name tree without our entry; drop the tree when empty.
        let names = catalog.get(b"Names").ok().and_then(|value| value.as_dict().ok()).cloned();
        if let Some(mut names) = names {
            let embedded = names.get(b"EmbeddedFiles").ok().and_then(|value| value.as_dict().ok()).cloned();
            if let Some(mut embedded) = embedded {
                let mut pairs: Vec<Object> =
                    embedded.get(b"Names").ok().and_then(|value| value.as_array().ok()).cloned().unwrap_or_default();
                pairs = pairs
                    .chunks(2)
                    .filter(|pair| pair.first().and_then(|name| name.as_str().ok()) != Some(kind.file_name.as_bytes()))
                    .flat_map(|pair| pair.to_vec())
                    .collect();
                if pairs.is_empty() {
                    names.remove(b"EmbeddedFiles");
                } else {
                    embedded.set("Names", Object::Array(pairs));
                    names.set("EmbeddedFiles", Object::Dictionary(embedded));
                }
                if names.is_empty() {
                    catalog.remove(b"Names");
                } else {
                    catalog.set("Names", Object::Dictionary(names));
                }
            }
        }
    }

    for id in removed {
        doc.objects.remove(&id);
    }
    save(&mut doc)
}

/// Escapes a value for an XMP element body.
pub fn xmp_escape(value: &str) -> String {
    value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// An XMP packet with one `rdf:Description` in `namespace` (prefix `omni`)
/// holding `fields` as `<omni:Name>value</omni:Name>` elements.
pub fn xmp_packet(namespace: &str, fields: &[(&str, String)]) -> String {
    let body: String = fields
        .iter()
        .map(|(name, value)| format!("      <omni:{name}>{}</omni:{name}>\n", xmp_escape(value)))
        .collect();
    format!(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n\
<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n\
  <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n\
    <rdf:Description rdf:about=\"\" xmlns:omni=\"{namespace}\">\n\
{body}    </rdf:Description>\n\
  </rdf:RDF>\n\
</x:xmpmeta>\n\
<?xpacket end=\"w\"?>"
    )
}
