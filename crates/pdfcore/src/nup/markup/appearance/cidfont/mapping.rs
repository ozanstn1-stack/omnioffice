//! How a code of a composite font reaches its glyph: `/Encoding` for the CID,
//! and `/CIDToGIDMap` for the glyph id.
//!
//! `/Encoding` is either a CMap name or a stream of its own. Only
//! `Identity-H`, and an embedded stream whose codes are two bytes wide
//! ([`crate::cmap::cid`]), are understood: anything else would draw a glyph
//! whose width and text were guessed.

use lopdf::{Dictionary, Document, Object};

use super::super::super::super::dests::resolve;
use crate::cmap::cid::CidMap;
use crate::cmap::Code;

/// A `/CIDToGIDMap` stream holds two bytes per CID.
const MAX_GID_MAP: usize = 128 * 1024;

/// How a code selects a CID.
pub(super) enum Encoding {
    /// `Identity-H`: the code is the CID.
    IdentityH,
    /// An embedded CMap, whose codes are two bytes wide.
    Embedded(CidMap),
}

/// The two steps from a code to the glyph it draws.
pub(super) struct Mapping {
    encoding: Encoding,
    /// `None` for the identity mapping, which is also the default.
    gids: Option<Vec<u8>>,
}

impl Mapping {
    /// The CID `code` selects, or `None` when the map does not admit the code.
    pub(super) fn cid(&self, code: Code) -> Option<u16> {
        match &self.encoding {
            // An identity encoding is two bytes wide by definition.
            Encoding::IdentityH => Some(code.value()),
            Encoding::Embedded(map) => map.cid(code),
        }
    }

    /// The code as it will be written, or `None` when the font cannot draw it:
    /// an unmapped code, CID 0 (`.notdef`), or a glyph id of 0.
    pub(super) fn admit(&self, code: Code) -> Option<Code> {
        let cid = self.cid(code)?;
        if cid == 0 || !self.has_glyph(cid) {
            return None;
        }
        Some(code)
    }

    /// An estimate of what the encoding and the glyph map hold.
    pub(super) fn heap_bytes(&self) -> usize {
        let map = match &self.encoding {
            Encoding::IdentityH => 0,
            Encoding::Embedded(map) => map.heap_bytes(),
        };
        map + self.gids.as_ref().map_or(0, Vec::len)
    }

    /// Whether `/CIDToGIDMap` gives `cid` a glyph other than `.notdef`.
    fn has_glyph(&self, cid: u16) -> bool {
        let Some(table) = &self.gids else { return true };
        let index = usize::from(cid) * 2;
        match (table.get(index), table.get(index + 1)) {
            (Some(high), Some(low)) => u16::from_be_bytes([*high, *low]) != 0,
            // A CID past the end of the map has no glyph.
            _ => false,
        }
    }
}

/// Reads the font's `/Encoding`, refusing a vertical mode and a base CMap we do
/// not ship.
pub(super) fn encoding(doc: &Document, type0: &Dictionary) -> Option<Encoding> {
    match dict_stream(doc, type0.get(b"Encoding").ok()?)? {
        Parsed::Identity => Some(Encoding::IdentityH),
        Parsed::Embedded { program, base } => crate::cmap::cid::read(&program, base.as_deref()).map(Encoding::Embedded),
    }
}

/// What an `/Encoding` entry names.
enum Parsed {
    Identity,
    Embedded { program: Vec<u8>, base: Option<Vec<u8>> },
}

/// Resolves the `/Encoding` entry: a name, or a stream that must be horizontal.
fn dict_stream(doc: &Document, value: &Object) -> Option<Parsed> {
    match resolve(doc, value) {
        Object::Name(name) if name == b"Identity-H" => Some(Parsed::Identity),
        Object::Name(_) => None,
        Object::Stream(stream) => {
            let dict = &stream.dict;
            match dict.get(b"WMode") {
                Ok(mode) if mode.as_i64().ok() != Some(0) => return None,
                _ => {}
            }
            let base = match dict.get(b"UseCMap") {
                Err(_) => None,
                Ok(Object::Name(name)) => Some(name.clone()),
                // A chained embedded CMap is out of scope.
                Ok(_) => None,
            };
            let program = stream.decompressed_content_with_limit(crate::cmap::MAX_STREAM).ok()?;
            Some(Parsed::Embedded { program, base })
        }
        _ => None,
    }
}

/// The `/CIDToGIDMap` of a `CIDFontType2`: identity when absent, a table when
/// it is a stream, and nothing when it says something else.
pub(super) fn gid_map(doc: &Document, child: &Dictionary) -> Option<Option<Vec<u8>>> {
    let Ok(value) = child.get(b"CIDToGIDMap") else { return Some(None) };
    match resolve(doc, value) {
        Object::Name(name) if name == b"Identity" => Some(None),
        Object::Stream(stream) => {
            let data = stream.decompressed_content_with_limit(MAX_GID_MAP).ok()?;
            Some(Some(data))
        }
        _ => None,
    }
}

/// Wraps the two steps for a font whose `/Encoding` and map have been read.
pub(super) fn mapping(doc: &Document, type0: &Dictionary, child: &Dictionary, subtype: &[u8]) -> Option<Mapping> {
    let encoding = encoding(doc, type0)?;
    let gids = match subtype {
        b"CIDFontType2" => gid_map(doc, child)?,
        b"CIDFontType0" => None,
        _ => return None,
    };
    Some(Mapping { encoding, gids })
}

#[cfg(test)]
mod tests;
