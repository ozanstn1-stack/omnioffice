//! Built-in PDF repair that needs no external engine (Android, and desktops
//! without qpdf).
//!
//! A damaged file's cross-reference data cannot be trusted, so the rebuild
//! ignores it and works from the bytes, the way qpdf's reconstruction does:
//!
//! 1. find every `N G obj` header and work out where each object ends,
//!    tolerating a missing `endobj`, garbage between objects, a truncated
//!    tail and a wrong stream `/Length` (the payload then runs to
//!    `endstream`);
//! 2. parse every definition with lopdf's own parser and keep the last
//!    readable definition of each object (incremental updates append);
//! 3. expand object streams and pick the catalog from the newest trailer (or
//!    a `/Type /Catalog` object), building a new catalog and page tree from
//!    the `/Type /Page` objects in file order when none survives;
//! 4. write the objects reachable from the new trailer with a fresh
//!    cross-reference table.
//!
//! The input is untrusted: object counts, decoded object streams, nesting and
//! scanning work are bounded, and file data is only read through checked
//! accessors, so no input can make the rebuild panic or run away.
//!
//! Encrypted files keep their encryption. Objects are written back under
//! their original numbers with their strings and streams untouched (still
//! encrypted), so the existing `/Encrypt` dictionary and `/ID` stay valid.
//! Only objects stored in object streams need the file key; that works when
//! the file opens without a password and is refused with `PasswordRequired`
//! otherwise.

use crate::docutil::INHERITED_ATTRS;
use crate::error::{PdfError, PdfResult};
use crate::progress::CancelToken;
use lopdf::encryption::{decrypt_object, encrypt_object};
use lopdf::xref::{Xref, XrefType};
use lopdf::{dictionary, Dictionary, Document, EncryptionState, LoadOptions, Object, ObjectId, Stream, StringFormat};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Write;
use std::ops::Range;

/// Largest file the built-in engine accepts; the whole file is held in memory.
pub const MAX_INPUT_BYTES: u64 = 512 * 1024 * 1024;
/// Object definitions considered (top-level and object-stream members).
const MAX_OBJECTS: usize = 1_000_000;
/// The largest object number PDF allows (ISO 32000-1, annex C).
const MAX_OBJECT_NUMBER: u32 = 8_388_607;
/// Array/dictionary nesting; lopdf's own parser stops at the same depth.
const MAX_NESTING: usize = 100;
/// Whitespace allowed inside an `N G obj` header.
const MAX_HEADER_SPACE: usize = 32;
/// Whitespace allowed between a stream payload and its keywords.
const MAX_STREAM_SPACE: usize = 32;
/// Decoded size of one object stream, and of all of them together.
const MAX_OBJSTM_BYTES: usize = 32 * 1024 * 1024;
const MAX_OBJSTM_TOTAL_BYTES: usize = 256 * 1024 * 1024;
/// Trailer dictionaries kept (the newest ones win).
const MAX_TRAILERS: usize = 64;
/// Depth of page-tree walks and `/Parent` chains.
const MAX_TREE_DEPTH: usize = 64;
/// Raw object bytes handed to lopdf per parse batch.
const PARSE_BATCH_BYTES: usize = 64 * 1024 * 1024;
/// How often long loops look at the cancel flag.
const CANCEL_STRIDE: usize = 4096;
/// Literal strings needing more escapes than this are written as hex, which
/// keeps lopdf's writer (quadratic in the escape count) fast on hostile input.
const MAX_LITERAL_ESCAPES: usize = 64;

/// A rebuilt document, serialized and ready to be written.
#[derive(Debug, Clone)]
pub struct RebuiltPdf {
    pub bytes: Vec<u8>,
    /// Pages in the rebuilt page tree.
    pub pages: u32,
    /// Objects written.
    pub objects: usize,
    /// The output carries the input's (unchanged) encryption.
    pub encrypted: bool,
    /// What was lost or reconstructed, for the repair report.
    pub warnings: Vec<String>,
}

/// Rebuilds a PDF from the objects found in `data`, ignoring its
/// cross-reference data.
pub fn rebuild_pdf(data: &[u8], cancel: &CancelToken) -> PdfResult<RebuiltPdf> {
    check_input_size(data.len() as u64)?;
    let mut warnings = Vec::new();
    let (headers, capped) = find_headers(data);
    if capped {
        warnings.push(format!(
            "The file has more objects than the built-in repair handles ({MAX_OBJECTS}); the rest were left out."
        ));
    }
    cancel.check()?;
    let scan = scan_file(data, &headers, cancel)?;
    drop(headers);
    if scan.objects.is_empty() {
        return Err(PdfError::CorruptPdf("no PDF objects could be found in the file".into()));
    }

    // Every definition and trailer goes through lopdf's parser in one pass.
    let values: Vec<&[u8]> = scan
        .objects
        .iter()
        .map(|object| &object.value)
        .chain(scan.trailers.iter().map(|trailer| &trailer.value))
        .map(|range| data.get(range.clone()).unwrap_or_default())
        .collect();
    let mut parsed = parse_values(&values, cancel)?.into_iter();
    drop(values);

    let mut unreadable = scan.unreadable;
    let mut first_seen: HashMap<u32, usize> = HashMap::new();
    let mut trailers: Vec<(usize, Dictionary)> = Vec::new();
    // Definitions in file order; a later definition replaces an earlier one.
    let mut latest: HashMap<ObjectId, Definition> = HashMap::new();
    for raw in &scan.objects {
        first_seen.entry(raw.id.0).or_insert(raw.position);
        let body = match (parsed.next().flatten(), &raw.stream) {
            (Some(Object::Dictionary(dict)), Some(_)) if dict.has_type(b"XRef") => {
                // A cross-reference stream is only useful as a trailer.
                trailers.push((raw.position, dict));
                continue;
            }
            (Some(Object::Dictionary(dict)), Some(stream)) => Body::Stream(dict, stream.clone()),
            (Some(Object::Stream(_)), _) | (Some(_), Some(_)) | (None, _) => {
                unreadable += 1;
                continue;
            }
            (Some(object), None) => Body::Plain(object),
        };
        latest.insert(raw.id, Definition { position: raw.position, order: 0, body });
    }
    for raw in &scan.trailers {
        if let Some(Object::Dictionary(dict)) = parsed.next().flatten() {
            trailers.push((raw.position, dict));
        }
    }
    trailers.sort_by_key(|(position, _)| *position);
    if unreadable > 0 {
        warnings.push(format!("{unreadable} damaged object(s) could not be read and were left out."));
    }
    cancel.check()?;

    let mut objects = materialize_streams(data, latest);
    let mut positions: HashMap<ObjectId, usize> =
        objects.iter().map(|(id, definition)| (*id, definition.position)).collect();

    let encrypt = newest(&trailers, |trailer| trailer.get(b"Encrypt").ok().cloned());
    let encrypted = encrypt.is_some();
    // With no trailer at all, an encryption dictionary means the strings and
    // streams are encrypted under an /ID that is gone.
    if trailers.is_empty() && objects.values().any(|definition| looks_like_encryption_dict(&definition.object)) {
        return Err(PdfError::Unsupported(
            "the file is encrypted but its trailer is lost, so the built-in repair cannot rebuild it".into(),
        ));
    }

    expand_object_streams(&mut objects, &trailers, encrypt.as_ref(), &mut first_seen, &mut warnings, cancel)?;
    for (id, definition) in &objects {
        positions.insert(*id, definition.position);
    }
    let mut objects: BTreeMap<ObjectId, Object> =
        objects.into_iter().map(|(id, definition)| (id, definition.object)).collect();

    let (root, pages) = build_catalog(&mut objects, &trailers, &positions, &first_seen, &mut warnings)?;
    let mut trailer = Dictionary::new();
    trailer.set("Root", Object::Reference(root));
    let info = newest(&trailers, |trailer| {
        trailer
            .get(b"Info")
            .and_then(Object::as_reference)
            .ok()
            .filter(|id| matches!(objects.get(id), Some(Object::Dictionary(_))))
    });
    if let Some(info) = info {
        trailer.set("Info", Object::Reference(info));
    }
    if let Some(file_id) =
        newest(&trailers, |trailer| trailer.get(b"ID").ok().filter(|id| id.as_array().is_ok()).cloned())
    {
        trailer.set("ID", file_id);
    }
    if let Some(encrypt) = encrypt {
        trailer.set("Encrypt", encrypt);
    }

    prune_unreachable(&mut objects, &trailer, &positions);
    hex_encode_escaped_strings(&mut objects);
    cancel.check()?;

    let object_count = objects.len();
    let mut document = Document::new();
    document.version = header_version(data);
    document.reference_table = Xref::new(0, XrefType::CrossReferenceTable);
    document.max_id = objects.keys().next_back().map_or(0, |id| id.0);
    document.objects = objects;
    document.trailer = trailer;
    let mut bytes = Vec::new();
    document
        .save_to(&mut bytes)
        .map_err(|error| PdfError::Internal(format!("could not write the rebuilt PDF: {error}")))?;
    Ok(RebuiltPdf {
        bytes,
        pages: u32::try_from(pages).unwrap_or(u32::MAX),
        objects: object_count,
        encrypted,
        warnings,
    })
}

/// Refuses inputs above [`MAX_INPUT_BYTES`] before they are read.
pub fn check_input_size(len: u64) -> PdfResult<()> {
    if len > MAX_INPUT_BYTES {
        return Err(PdfError::Unsupported(format!(
            "the file is larger than the built-in repair limit of {} MB",
            MAX_INPUT_BYTES / (1024 * 1024)
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Lexing helpers. Every read is bounds-checked; `limit` is an exclusive end.
// ---------------------------------------------------------------------------

fn is_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | b'\x0c' | b'\0')
}

fn is_delimiter(byte: u8) -> bool {
    matches!(byte, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%')
}

fn is_regular(byte: u8) -> bool {
    !is_space(byte) && !is_delimiter(byte)
}

fn byte_at(data: &[u8], pos: usize, limit: usize) -> Option<u8> {
    if pos < limit {
        data.get(pos).copied()
    } else {
        None
    }
}

fn starts_with_at(data: &[u8], pos: usize, needle: &[u8]) -> bool {
    data.get(pos..).is_some_and(|rest| rest.starts_with(needle))
}

fn parse_ascii<T: std::str::FromStr>(bytes: &[u8]) -> Option<T> {
    std::str::from_utf8(bytes).ok()?.parse().ok()
}

/// Skips whitespace and comments.
fn skip_space(data: &[u8], mut pos: usize, limit: usize) -> usize {
    while let Some(byte) = byte_at(data, pos, limit) {
        if byte == b'%' {
            while byte_at(data, pos, limit).is_some_and(|byte| byte != b'\r' && byte != b'\n') {
                pos += 1;
            }
        } else if is_space(byte) {
            pos += 1;
        } else {
            break;
        }
    }
    pos
}

/// Skips at most `max` whitespace bytes (no comments).
fn skip_plain_space(data: &[u8], pos: usize, max: usize) -> usize {
    let mut end = pos;
    while end - pos < max && data.get(end).is_some_and(|&byte| is_space(byte)) {
        end += 1;
    }
    end
}

fn regular_end(data: &[u8], mut pos: usize, limit: usize) -> usize {
    while byte_at(data, pos, limit).is_some_and(is_regular) {
        pos += 1;
    }
    pos
}

/// End of `1..=max` ASCII digits at `pos`, or `None`.
fn digits_end(data: &[u8], pos: usize, max: usize) -> Option<usize> {
    let mut end = pos;
    while data.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
        if end - pos > max {
            return None;
        }
    }
    (end > pos).then_some(end)
}

fn literal_string_end(data: &[u8], start: usize, limit: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut pos = start;
    while let Some(byte) = byte_at(data, pos, limit) {
        match byte {
            b'\\' => {
                pos += 2;
                continue;
            }
            b'(' => depth += 1,
            b')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(pos + 1);
                }
            }
            _ => {}
        }
        pos += 1;
    }
    None
}

fn hex_string_end(data: &[u8], start: usize, limit: usize) -> Option<usize> {
    let mut pos = start + 1;
    while let Some(byte) = byte_at(data, pos, limit) {
        if byte == b'>' {
            return Some(pos + 1);
        }
        if !byte.is_ascii_hexdigit() && !is_space(byte) {
            return None;
        }
        pos += 1;
    }
    None
}

/// The `G R` that turns the integer ending at `pos` into a reference.
fn reference_end(data: &[u8], pos: usize, limit: usize) -> Option<usize> {
    let generation = skip_space(data, pos, limit);
    if generation == pos {
        return None;
    }
    let generation_end = digits_end(data, generation, 5)?;
    let keyword = skip_space(data, generation_end, limit);
    if keyword == generation_end || byte_at(data, keyword, limit) != Some(b'R') {
        return None;
    }
    if byte_at(data, keyword + 1, limit).is_some_and(is_regular) {
        return None;
    }
    Some(keyword + 1)
}

/// Keywords that cannot appear inside a direct object; meeting one means the
/// object is unterminated and its bytes run into the file structure.
fn is_structural_keyword(token: &[u8]) -> bool {
    matches!(token, b"obj" | b"endobj" | b"stream" | b"endstream" | b"xref" | b"trailer" | b"startxref")
}

/// End of the direct object starting at `start` (after whitespace), or `None`
/// when it is malformed or does not end before `limit`.
///
/// This only finds boundaries; the bytes are parsed by lopdf afterwards. Work
/// is linear in the bytes scanned and nesting is capped like lopdf's parser.
fn skip_object(data: &[u8], start: usize, limit: usize) -> Option<usize> {
    let mut pos = start;
    let mut depth = 0usize;
    loop {
        pos = skip_space(data, pos, limit);
        let byte = byte_at(data, pos, limit)?;
        let next = byte_at(data, pos + 1, limit);
        match byte {
            b'<' if next == Some(b'<') => {
                depth += 1;
                pos += 2;
            }
            b'[' => {
                depth += 1;
                pos += 1;
            }
            b'>' if next == Some(b'>') => {
                depth = depth.checked_sub(1)?;
                pos += 2;
            }
            b']' => {
                depth = depth.checked_sub(1)?;
                pos += 1;
            }
            b'(' => pos = literal_string_end(data, pos, limit)?,
            b'<' => pos = hex_string_end(data, pos, limit)?,
            b'/' => pos = regular_end(data, pos + 1, limit),
            b')' | b'>' | b'{' | b'}' => return None,
            _ => {
                let token_start = pos;
                pos = regular_end(data, pos, limit);
                let token = data.get(token_start..pos)?;
                if is_structural_keyword(token) {
                    return None;
                }
                if depth == 0 && token.iter().all(u8::is_ascii_digit) {
                    if let Some(end) = reference_end(data, pos, limit) {
                        pos = end;
                    }
                }
            }
        }
        if depth > MAX_NESTING {
            return None;
        }
        if depth == 0 {
            return Some(pos);
        }
    }
}

/// A direct integer `/Length` in the stream dictionary spanning `dict`.
fn direct_length(data: &[u8], dict: &Range<usize>) -> Option<usize> {
    let limit = dict.end;
    let mut pos = dict.start + 2;
    loop {
        pos = skip_space(data, pos, limit);
        if byte_at(data, pos, limit)? != b'/' {
            return None;
        }
        let key_end = regular_end(data, pos + 1, limit);
        let value_start = skip_space(data, key_end, limit);
        let value_end = skip_object(data, value_start, limit)?;
        if data.get(pos + 1..key_end)? == b"Length" {
            return parse_ascii(data.get(value_start..value_end)?);
        }
        pos = value_end;
    }
}

/// Whether `length` bytes from `start` end right before `endstream`.
fn length_fits(data: &[u8], start: usize, length: usize) -> bool {
    let Some(end) = start.checked_add(length) else {
        return false;
    };
    end <= data.len() && starts_with_at(data, skip_plain_space(data, end, MAX_STREAM_SPACE), b"endstream")
}

/// Drops the end-of-line marker that precedes `endstream` from a payload.
fn trim_eol(data: &[u8], start: usize, mut end: usize) -> usize {
    if end > start && data.get(end - 1) == Some(&b'\n') {
        end -= 1;
    }
    if end > start && data.get(end - 1) == Some(&b'\r') {
        end -= 1;
    }
    end
}

/// First payload byte after the `stream` keyword ending at `keyword_end`.
fn stream_data_start(data: &[u8], keyword_end: usize) -> usize {
    let mut pos = keyword_end;
    while matches!(data.get(pos), Some(b' ' | b'\t')) && pos - keyword_end < MAX_STREAM_SPACE {
        pos += 1;
    }
    match (data.get(pos), data.get(pos + 1)) {
        (Some(b'\r'), Some(b'\n')) => pos + 2,
        (Some(b'\n' | b'\r'), _) => pos + 1,
        // No end-of-line at all: the payload starts right after the keyword.
        _ => keyword_end,
    }
}

/// Finds a keyword at increasing offsets in linear total time: a search
/// result is reused until the caller moves past it, so no byte is scanned
/// twice by monotonic queries.
struct Finder {
    needle: &'static [u8],
    searched_from: usize,
    found: Option<usize>,
    valid: bool,
}

impl Finder {
    fn new(needle: &'static [u8]) -> Self {
        Self { needle, searched_from: 0, found: None, valid: false }
    }

    fn next_from(&mut self, data: &[u8], from: usize) -> Option<usize> {
        if self.valid && from >= self.searched_from {
            match self.found {
                None => return None,
                Some(found) if found >= from => return Some(found),
                Some(_) => {}
            }
        }
        let found = data
            .get(from..)
            .and_then(|rest| rest.windows(self.needle.len()).position(|window| window == self.needle))
            .map(|offset| from + offset);
        self.searched_from = from;
        self.found = found;
        self.valid = true;
        found
    }
}

// ---------------------------------------------------------------------------
// Scanning
// ---------------------------------------------------------------------------

struct Header {
    pos: usize,
    body: usize,
    id: ObjectId,
}

/// `N G obj` at `pos`: the id and the offset right after `obj`.
fn parse_header(data: &[u8], pos: usize) -> Option<(ObjectId, usize)> {
    let number_end = digits_end(data, pos, 10)?;
    let generation = skip_plain_space(data, number_end, MAX_HEADER_SPACE);
    if generation == number_end {
        return None;
    }
    let generation_end = digits_end(data, generation, 5)?;
    let keyword = skip_plain_space(data, generation_end, MAX_HEADER_SPACE);
    if keyword == generation_end || !starts_with_at(data, keyword, b"obj") {
        return None;
    }
    let body = keyword + 3;
    if data.get(body).is_some_and(|&byte| is_regular(byte)) {
        return None;
    }
    let number: u32 = parse_ascii(data.get(pos..number_end)?)?;
    let generation: u16 = parse_ascii(data.get(generation..generation_end)?)?;
    if number == 0 || number > MAX_OBJECT_NUMBER {
        return None;
    }
    Some(((number, generation), body))
}

/// Every `N G obj` header that starts a token, in file order. Headers inside
/// stream payloads are found too; the object scan skips those it has already
/// consumed.
fn find_headers(data: &[u8]) -> (Vec<Header>, bool) {
    let mut headers = Vec::new();
    let mut pos = 0;
    while let Some(&byte) = data.get(pos) {
        let boundary = pos.checked_sub(1).and_then(|prev| data.get(prev)).is_none_or(|&prev| is_space(prev));
        if byte.is_ascii_digit() && boundary {
            if let Some((id, body)) = parse_header(data, pos) {
                if headers.len() >= MAX_OBJECTS {
                    return (headers, true);
                }
                headers.push(Header { pos, body, id });
                pos = body;
                continue;
            }
        }
        pos += 1;
    }
    (headers, false)
}

#[derive(Debug, Clone)]
struct RawStream {
    /// Payload start and the end found by the scan.
    start: usize,
    end: usize,
    /// The payload end came from a direct `/Length` that matched `endstream`.
    length_trusted: bool,
}

struct RawObject {
    id: ObjectId,
    position: usize,
    /// The object's value (for a stream, its dictionary).
    value: Range<usize>,
    stream: Option<RawStream>,
}

struct RawTrailer {
    position: usize,
    value: Range<usize>,
}

struct Scan {
    objects: Vec<RawObject>,
    trailers: Vec<RawTrailer>,
    unreadable: usize,
}

struct Finders {
    endstream: Finder,
    endobj: Finder,
    trailer: Finder,
}

/// Works out where the object at `header` ends. The value must end before
/// `limit` (the next header), which bounds the work for unterminated values;
/// a stream payload may run past it.
fn scan_object(data: &[u8], header: &Header, limit: usize, finders: &mut Finders) -> Option<(RawObject, usize)> {
    let len = data.len();
    let value_start = skip_space(data, header.body, limit);
    let value_end = skip_object(data, value_start, limit)?;
    let after_value = skip_space(data, value_end, len);
    let value = value_start..value_end;
    if starts_with_at(data, value_start, b"<<") && starts_with_at(data, after_value, b"stream") {
        let start = stream_data_start(data, after_value + b"stream".len());
        let declared = direct_length(data, &value).filter(|&length| length_fits(data, start, length));
        let (end, keyword_end, length_trusted) = match declared {
            Some(length) => {
                let end = start + length;
                (end, skip_plain_space(data, end, MAX_STREAM_SPACE) + b"endstream".len(), true)
            }
            // A wrong or indirect /Length: the payload runs to `endstream`,
            // or for a damaged stream without one to the next `endobj` (or
            // the end of a truncated file).
            None => match finders.endstream.next_from(data, start) {
                Some(found) => (trim_eol(data, start, found), found + b"endstream".len(), false),
                None => match finders.endobj.next_from(data, start) {
                    Some(found) => (trim_eol(data, start, found), found, false),
                    None => (len, len, false),
                },
            },
        };
        let close = skip_space(data, keyword_end, len);
        let object_end = if starts_with_at(data, close, b"endobj") { close + b"endobj".len() } else { keyword_end };
        let stream = RawStream { start, end, length_trusted };
        return Some((RawObject { id: header.id, position: header.pos, value, stream: Some(stream) }, object_end));
    }
    let object_end =
        if starts_with_at(data, after_value, b"endobj") { after_value + b"endobj".len() } else { value_end };
    Some((RawObject { id: header.id, position: header.pos, value, stream: None }, object_end))
}

/// Records the `trailer` dictionaries between `from` and `to` (outside every
/// object).
fn collect_trailers(data: &[u8], from: usize, to: usize, finder: &mut Finder, out: &mut Vec<RawTrailer>) {
    let mut cursor = from;
    while let Some(found) = finder.next_from(data, cursor) {
        if found >= to {
            break;
        }
        cursor = found + b"trailer".len();
        let start = skip_space(data, cursor, to);
        if !starts_with_at(data, start, b"<<") {
            continue;
        }
        if let Some(end) = skip_object(data, start, to) {
            out.push(RawTrailer { position: found, value: start..end });
            if out.len() > MAX_TRAILERS {
                out.remove(0);
            }
            cursor = end;
        }
    }
}

fn scan_file(data: &[u8], headers: &[Header], cancel: &CancelToken) -> PdfResult<Scan> {
    let mut finders = Finders {
        endstream: Finder::new(b"endstream"),
        endobj: Finder::new(b"endobj"),
        trailer: Finder::new(b"trailer"),
    };
    let mut scan = Scan { objects: Vec::new(), trailers: Vec::new(), unreadable: 0 };
    let mut resume = 0;
    for (index, header) in headers.iter().enumerate() {
        if index % CANCEL_STRIDE == 0 {
            cancel.check()?;
        }
        if header.pos < resume {
            // Inside an object already consumed (usually a stream payload).
            continue;
        }
        collect_trailers(data, resume, header.pos, &mut finders.trailer, &mut scan.trailers);
        let limit = headers.get(index + 1).map_or(data.len(), |next| next.pos);
        match scan_object(data, header, limit, &mut finders) {
            Some((object, end)) => {
                resume = end;
                scan.objects.push(object);
            }
            None => {
                scan.unreadable += 1;
                resume = header.body;
            }
        }
    }
    collect_trailers(data, resume, data.len(), &mut finders.trailer, &mut scan.trailers);
    Ok(scan)
}

// ---------------------------------------------------------------------------
// Parsing through lopdf
// ---------------------------------------------------------------------------

/// Keeps streams out of the parse documents: a value is never a stream, and
/// lopdf would otherwise expand an `/ObjStm` hidden in a damaged value.
fn reject_streams(id: ObjectId, object: &mut Object) -> Option<(ObjectId, Object)> {
    if matches!(object, Object::Stream(_)) {
        None
    } else {
        Some((id, Object::Null))
    }
}

/// Parses raw values with lopdf's own parser.
///
/// lopdf keeps its object parser private, so the values are framed as the
/// objects of a small, well-formed PDF (numbered from 1, with an exact
/// cross-reference table) and loaded with `Document::load_mem`. A value that
/// does not parse comes back as `None`.
fn parse_values(values: &[&[u8]], cancel: &CancelToken) -> PdfResult<Vec<Option<Object>>> {
    let mut parsed = Vec::with_capacity(values.len());
    let mut start = 0;
    while start < values.len() {
        cancel.check()?;
        let mut end = start;
        let mut bytes = 0usize;
        while let Some(value) = values.get(end) {
            if end > start && bytes.saturating_add(value.len()) > PARSE_BATCH_BYTES {
                break;
            }
            bytes = bytes.saturating_add(value.len());
            end += 1;
        }
        parsed.extend(parse_batch(values.get(start..end).unwrap_or_default()));
        start = end;
    }
    Ok(parsed)
}

fn parse_batch(values: &[&[u8]]) -> Vec<Option<Object>> {
    let size = values.iter().map(|value| value.len() + 48).sum::<usize>() + 128;
    let mut buffer = Vec::with_capacity(size);
    buffer.extend_from_slice(b"%PDF-1.7\n");
    let mut offsets = Vec::with_capacity(values.len());
    for (index, value) in values.iter().enumerate() {
        offsets.push(buffer.len());
        let _ = writeln!(buffer, "{} 0 obj", index + 1);
        buffer.extend_from_slice(value);
        buffer.extend_from_slice(b"\nendobj\n");
    }
    let xref_at = buffer.len();
    if u32::try_from(xref_at).is_err() {
        return vec![None; values.len()];
    }
    let _ = write!(buffer, "xref\n0 {}\n0000000000 65535 f\r\n", values.len() + 1);
    for offset in offsets {
        let _ = write!(buffer, "{offset:010} 00000 n\r\n");
    }
    let _ = write!(buffer, "trailer\n<< /Size {} >>\nstartxref\n{xref_at}\n%%EOF\n", values.len() + 1);
    let options = LoadOptions {
        filter: Some(reject_streams),
        max_decompressed_size: Some(MAX_OBJSTM_BYTES),
        ..Default::default()
    };
    let Ok(mut document) = Document::load_mem_with_options(&buffer, options) else {
        return vec![None; values.len()];
    };
    (1..=values.len())
        .map(|number| u32::try_from(number).ok().and_then(|number| document.objects.remove(&(number, 0))))
        .map(|object| object.filter(|object| !matches!(object, Object::Stream(_))))
        .collect()
}

// ---------------------------------------------------------------------------
// Object selection
// ---------------------------------------------------------------------------

enum Body {
    Plain(Object),
    Stream(Dictionary, RawStream),
}

struct Definition {
    /// Header offset (for an object-stream member, its container's).
    position: usize,
    /// Order among definitions at one position (object-stream index + 1).
    order: usize,
    body: Body,
}

struct Chosen {
    position: usize,
    order: usize,
    object: Object,
}

/// Turns the chosen definitions into objects, copying each stream payload.
/// An indirect `/Length` is honoured when it matches `endstream`.
fn materialize_streams(data: &[u8], latest: HashMap<ObjectId, Definition>) -> HashMap<ObjectId, Chosen> {
    let lengths: HashMap<ObjectId, i64> = latest
        .iter()
        .filter_map(|(id, definition)| match &definition.body {
            Body::Plain(Object::Integer(value)) => Some((*id, *value)),
            _ => None,
        })
        .collect();
    latest
        .into_iter()
        .map(|(id, definition)| {
            let object = match definition.body {
                Body::Plain(object) => object,
                Body::Stream(dict, raw) => {
                    let indirect = dict
                        .get(b"Length")
                        .and_then(Object::as_reference)
                        .ok()
                        .and_then(|length_id| lengths.get(&length_id))
                        .and_then(|&length| usize::try_from(length).ok())
                        .filter(|&length| !raw.length_trusted && length_fits(data, raw.start, length));
                    let range = match indirect {
                        Some(length) => raw.start..raw.start + length,
                        None => raw.start..raw.end,
                    };
                    let content = data.get(range).unwrap_or_default().to_vec();
                    Object::Stream(Stream::new(dict, content))
                }
            };
            (id, Chosen { position: definition.position, order: definition.order, object })
        })
        .collect()
}

/// The newest trailer value `pick` accepts.
fn newest<T>(trailers: &[(usize, Dictionary)], pick: impl Fn(&Dictionary) -> Option<T>) -> Option<T> {
    trailers.iter().rev().find_map(|(_, trailer)| pick(trailer))
}

fn looks_like_encryption_dict(object: &Object) -> bool {
    object.as_dict().is_ok_and(|dict| {
        dict.get(b"Filter").and_then(Object::as_name).is_ok_and(|name| name == b"Standard")
            && dict.has(b"O")
            && dict.has(b"U")
            && dict.has(b"P")
    })
}

/// The key of an encrypted file, available only when it opens without a
/// password.
fn file_key(
    objects: &HashMap<ObjectId, Chosen>,
    trailers: &[(usize, Dictionary)],
    encrypt: &Object,
) -> PdfResult<EncryptionState> {
    let encrypt_id = encrypt
        .as_reference()
        .map_err(|_| PdfError::Unsupported("the file's encryption dictionary cannot be read".into()))?;
    let dict = objects
        .get(&encrypt_id)
        .and_then(|chosen| chosen.object.as_dict().ok())
        .ok_or_else(|| PdfError::Unsupported("the file's encryption dictionary is damaged".into()))?;
    let mut probe = Document::new();
    probe.objects.insert(encrypt_id, Object::Dictionary(dict.clone()));
    probe.trailer.set("Encrypt", Object::Reference(encrypt_id));
    if let Some(file_id) = newest(trailers, |trailer| trailer.get(b"ID").ok().cloned()) {
        probe.trailer.set("ID", file_id);
    }
    if probe.authenticate_password("").is_err() {
        return Err(PdfError::PasswordRequired);
    }
    EncryptionState::decode(&probe, "")
        .map_err(|error| PdfError::Unsupported(format!("the file's encryption cannot be handled: {error}")))
}

/// Splits a decoded object stream into `(index, number, value range)`.
/// Members sharing an offset keep only the first, so one slice is never
/// parsed more than once.
fn object_stream_members(dict: &Dictionary, decoded: &[u8], budget: usize) -> Vec<(usize, u32, Range<usize>)> {
    let Some(first) = dict.get(b"First").and_then(Object::as_i64).ok().and_then(|first| usize::try_from(first).ok())
    else {
        return Vec::new();
    };
    let Some(index) = decoded.get(..first) else {
        return Vec::new();
    };
    let numbers: Vec<usize> = index
        .split(|&byte| is_space(byte))
        .filter(|token| !token.is_empty())
        .map_while(parse_ascii::<usize>)
        .take(budget.saturating_mul(2))
        .collect();
    let mut entries: Vec<(usize, u32, usize)> = numbers
        .chunks_exact(2)
        .enumerate()
        .filter_map(|(slot, pair)| {
            let number = u32::try_from(*pair.first()?).ok().filter(|&n| n > 0 && n <= MAX_OBJECT_NUMBER)?;
            let offset = first.checked_add(*pair.get(1)?).filter(|&offset| offset < decoded.len())?;
            Some((slot, number, offset))
        })
        .collect();
    entries.sort_by_key(|&(slot, _, offset)| (offset, slot));
    entries.dedup_by_key(|entry| entry.2);
    let ends: Vec<usize> = entries.iter().skip(1).map(|entry| entry.2).chain(std::iter::once(decoded.len())).collect();
    let mut members: Vec<(usize, u32, Range<usize>)> =
        entries.iter().zip(ends).map(|(&(slot, number, offset), end)| (slot, number, offset..end)).collect();
    members.sort_by_key(|member| member.0);
    members
}

/// Replaces object-stream containers with their members. A member wins over
/// a top-level definition only when its container comes later in the file.
fn expand_object_streams(
    objects: &mut HashMap<ObjectId, Chosen>,
    trailers: &[(usize, Dictionary)],
    encrypt: Option<&Object>,
    first_seen: &mut HashMap<u32, usize>,
    warnings: &mut Vec<String>,
    cancel: &CancelToken,
) -> PdfResult<()> {
    let mut containers: Vec<(ObjectId, usize)> = objects
        .iter()
        .filter(|(_, chosen)| chosen.object.as_stream().is_ok_and(|stream| stream.dict.has_type(b"ObjStm")))
        .map(|(id, chosen)| (*id, chosen.position))
        .collect();
    if containers.is_empty() {
        return Ok(());
    }
    containers.sort_by_key(|&(_, position)| position);
    let key = match encrypt {
        Some(encrypt) => Some(file_key(objects, trailers, encrypt)?),
        None => None,
    };

    let mut budget_bytes = MAX_OBJSTM_TOTAL_BYTES;
    let mut budget_objects = MAX_OBJECTS.saturating_sub(objects.len());
    let mut failed = 0usize;
    let mut decoded_streams: Vec<(ObjectId, usize, Dictionary, Vec<u8>)> = Vec::new();
    for (id, position) in containers {
        cancel.check()?;
        let Some(chosen) = objects.remove(&id) else {
            continue;
        };
        let mut container = chosen.object;
        if let Some(key) = &key {
            if decrypt_object(key, id, &mut container).is_err() {
                failed += 1;
                continue;
            }
        }
        let Ok(stream) = container.as_stream() else {
            continue;
        };
        match stream.decompressed_content_with_limit(budget_bytes.min(MAX_OBJSTM_BYTES)) {
            Ok(decoded) => {
                budget_bytes = budget_bytes.saturating_sub(decoded.len());
                decoded_streams.push((id, position, stream.dict.clone(), decoded));
            }
            Err(_) => failed += 1,
        }
    }

    // (member id, container position, order, decoded stream, range)
    let mut members: Vec<(ObjectId, usize, usize, usize, Range<usize>)> = Vec::new();
    for (stream_index, (_, position, dict, decoded)) in decoded_streams.iter().enumerate() {
        for (slot, number, range) in object_stream_members(dict, decoded, budget_objects) {
            if budget_objects == 0 {
                break;
            }
            budget_objects -= 1;
            members.push(((number, 0), *position, slot + 1, stream_index, range));
        }
    }
    let values: Vec<&[u8]> = members
        .iter()
        .map(|(_, _, _, stream_index, range)| {
            decoded_streams
                .get(*stream_index)
                .and_then(|(_, _, _, decoded)| decoded.get(range.clone()))
                .unwrap_or_default()
        })
        .collect();
    let parsed = parse_values(&values, cancel)?;
    drop(values);

    let mut unreadable = 0usize;
    for ((id, position, order, _, _), object) in members.into_iter().zip(parsed) {
        let Some(mut object) = object else {
            unreadable += 1;
            continue;
        };
        if let Some(key) = &key {
            // Strings inside an object stream are covered by the stream's
            // encryption; standing alone they need their own.
            if encrypt_object(key, id, &mut object).is_err() {
                unreadable += 1;
                continue;
            }
        }
        first_seen.entry(id.0).and_modify(|seen| *seen = (*seen).min(position)).or_insert(position);
        let newer = objects.get(&id).is_none_or(|existing| (existing.position, existing.order) < (position, order));
        if newer {
            objects.insert(id, Chosen { position, order, object });
        }
    }
    if failed > 0 {
        warnings.push(format!(
            "{failed} compressed object stream(s) could not be decoded; the objects stored in them are missing."
        ));
    }
    if unreadable > 0 {
        warnings.push(format!("{unreadable} compressed object(s) could not be read and were left out."));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Catalog and page tree
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum NodeKind {
    Page,
    Pages,
    Other,
}

fn node_kind(dict: &Dictionary) -> NodeKind {
    match dict.get(b"Type").and_then(Object::as_name) {
        Ok(b"Page") => NodeKind::Page,
        Ok(b"Pages") => NodeKind::Pages,
        Ok(_) => NodeKind::Other,
        // Untyped nodes are common in damaged files; judge by their keys.
        Err(_) if dict.has(b"Kids") => NodeKind::Pages,
        Err(_) if dict.has(b"Contents") || dict.has(b"MediaBox") => NodeKind::Page,
        Err(_) => NodeKind::Other,
    }
}

fn is_catalog(object: &Object) -> bool {
    object.as_dict().is_ok_and(|dict| dict.has_type(b"Catalog") || dict.has(b"Pages"))
}

/// Walks a page tree, noting anything a consistent tree would not have.
struct TreeWalk<'a> {
    objects: &'a BTreeMap<ObjectId, Object>,
    /// Page-tree nodes by the `/Parent` they name, in file order; a missing
    /// node is replaced by the nodes that name it.
    adopted: &'a HashMap<ObjectId, Vec<ObjectId>>,
    seen: HashSet<ObjectId>,
    pages: Vec<ObjectId>,
    damaged: bool,
}

impl TreeWalk<'_> {
    /// Visits `id` and returns the pages found under it.
    fn visit(&mut self, id: ObjectId, parent: Option<ObjectId>, depth: usize) -> usize {
        if depth > MAX_TREE_DEPTH || !self.seen.insert(id) {
            self.damaged = true;
            return 0;
        }
        let objects = self.objects;
        let Some(dict) = objects.get(&id).and_then(|object| object.as_dict().ok()) else {
            self.damaged = true;
            let adopted = self.adopted.get(&id).cloned().unwrap_or_default();
            return adopted.into_iter().map(|child| self.visit(child, parent, depth + 1)).sum();
        };
        if dict.get(b"Parent").and_then(Object::as_reference).ok() != parent {
            self.damaged = true;
        }
        // Readers locate pages by `/Type`, so an untyped node needs repair.
        if dict.get(b"Type").is_err() {
            self.damaged = true;
        }
        match node_kind(dict) {
            NodeKind::Page => {
                self.pages.push(id);
                1
            }
            NodeKind::Pages => {
                let Ok(kids) = dict.get(b"Kids").and_then(Object::as_array) else {
                    self.damaged = true;
                    return 0;
                };
                let mut count = 0;
                for kid in kids {
                    match kid.as_reference() {
                        Ok(kid) => count += self.visit(kid, Some(id), depth + 1),
                        Err(_) => self.damaged = true,
                    }
                }
                if dict.get(b"Count").and_then(Object::as_i64).ok() != i64::try_from(count).ok() {
                    self.damaged = true;
                }
                count
            }
            NodeKind::Other => {
                self.damaged = true;
                0
            }
        }
    }
}

/// Arrays longer than this are shared as an indirect object instead of being
/// copied onto every page that inherits them.
const MAX_INLINE_ARRAY: usize = 8;

/// Replaces the page tree with one node holding `pages`, after giving each
/// page the attributes it inherited from its old ancestors.
///
/// A large inherited value (a `/Resources` dictionary above all) is written
/// once as an indirect object and referenced from every page that inherits
/// it; copying it onto each page would multiply the file by the page count.
fn flatten_page_tree(objects: &mut BTreeMap<ObjectId, Object>, pages: &[ObjectId], root: ObjectId) -> usize {
    // Per page, the attributes it inherits and the ancestor each comes from.
    let mut inherited: Vec<(ObjectId, Vec<(Vec<u8>, ObjectId)>)> = Vec::with_capacity(pages.len());
    for &page in pages {
        let Some(dict) = objects.get(&page).and_then(|object| object.as_dict().ok()) else {
            continue;
        };
        let mut missing: Vec<&[u8]> = INHERITED_ATTRS.iter().copied().filter(|key| !dict.has(key)).collect();
        let mut found = Vec::new();
        let mut seen = HashSet::new();
        let mut parent = dict.get(b"Parent").and_then(Object::as_reference).ok();
        while let Some(parent_id) = parent {
            if missing.is_empty() || seen.len() >= MAX_TREE_DEPTH || !seen.insert(parent_id) {
                break;
            }
            let Some(node) = objects.get(&parent_id).and_then(|object| object.as_dict().ok()) else {
                break;
            };
            missing.retain(|key| {
                let present = node.has(key);
                if present {
                    found.push((key.to_vec(), parent_id));
                }
                !present
            });
            parent = node.get(b"Parent").and_then(Object::as_reference).ok();
        }
        inherited.push((page, found));
    }

    // Each distinct (ancestor, attribute) value is stored once.
    let mut shared: HashMap<(ObjectId, Vec<u8>), Object> = HashMap::new();
    let mut added: Vec<(ObjectId, Object)> = Vec::new();
    let mut next = root.0;
    for (_, found) in &inherited {
        for (key, node) in found {
            if shared.contains_key(&(*node, key.clone())) {
                continue;
            }
            let Some(value) =
                objects.get(node).and_then(|object| object.as_dict().ok()).and_then(|dict| dict.get(key).ok())
            else {
                continue;
            };
            let large = match value {
                Object::Dictionary(_) => true,
                Object::Array(items) => items.len() > MAX_INLINE_ARRAY,
                _ => false,
            };
            let id = next.checked_add(1).filter(|number| *number <= MAX_OBJECT_NUMBER);
            let replacement = match (large, id) {
                (true, Some(number)) => {
                    next = number;
                    added.push(((number, 0), value.clone()));
                    Object::Reference((number, 0))
                }
                _ => value.clone(),
            };
            shared.insert((*node, key.clone()), replacement);
        }
    }
    objects.extend(added);

    let mut sizeless = 0;
    for (page, found) in inherited {
        if let Some(Object::Dictionary(dict)) = objects.get_mut(&page) {
            for (key, node) in found {
                if let Some(value) = shared.get(&(node, key.clone())) {
                    dict.set(key, value.clone());
                }
            }
            if !dict.has(b"MediaBox") {
                // US Letter, the default most readers assume as well.
                dict.set(
                    "MediaBox",
                    vec![Object::Integer(0), Object::Integer(0), Object::Integer(612), Object::Integer(792)],
                );
                sizeless += 1;
            }
            dict.set("Type", "Page");
            dict.set("Parent", Object::Reference(root));
        }
    }
    let kids: Vec<Object> = pages.iter().map(|&page| Object::Reference(page)).collect();
    objects.insert(
        root,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => i64::try_from(pages.len()).unwrap_or(i64::MAX),
        }),
    );
    sizeless
}

fn next_object_id(objects: &BTreeMap<ObjectId, Object>) -> PdfResult<ObjectId> {
    let last = objects.keys().next_back().map_or(0, |id| id.0);
    last.checked_add(1)
        .map(|number| (number, 0))
        .ok_or_else(|| PdfError::CorruptPdf("the file uses every object number".into()))
}

/// Finds or builds the catalog and makes sure its page tree is usable.
/// Returns the catalog id and the page count.
fn build_catalog(
    objects: &mut BTreeMap<ObjectId, Object>,
    trailers: &[(usize, Dictionary)],
    positions: &HashMap<ObjectId, usize>,
    first_seen: &HashMap<u32, usize>,
    warnings: &mut Vec<String>,
) -> PdfResult<(ObjectId, usize)> {
    let from_trailer = newest(trailers, |trailer| {
        trailer.get(b"Root").and_then(Object::as_reference).ok().filter(|id| objects.get(id).is_some_and(is_catalog))
    });
    let catalog = from_trailer.or_else(|| {
        objects
            .iter()
            .filter(|(_, object)| object.as_dict().is_ok_and(|dict| dict.has_type(b"Catalog")))
            .max_by_key(|(id, _)| positions.get(id).copied().unwrap_or(0))
            .map(|(id, _)| *id)
    });

    // Page-tree nodes by the parent they name, in file order.
    let order_of = |id: &ObjectId| first_seen.get(&id.0).copied().unwrap_or(usize::MAX);
    let mut adopted: HashMap<ObjectId, Vec<ObjectId>> = HashMap::new();
    let mut all_pages: Vec<ObjectId> = Vec::new();
    for (id, object) in objects.iter() {
        let Ok(dict) = object.as_dict() else {
            continue;
        };
        if node_kind(dict) == NodeKind::Other {
            continue;
        }
        if dict.has_type(b"Page") {
            all_pages.push(*id);
        }
        if let Ok(parent) = dict.get(b"Parent").and_then(Object::as_reference) {
            adopted.entry(parent).or_default().push(*id);
        }
    }
    all_pages.sort_by_key(|id| (order_of(id), *id));
    for children in adopted.values_mut() {
        children.sort_by_key(|id| (order_of(id), *id));
    }

    let (pages, note) = match catalog {
        Some(catalog) => {
            let view: &BTreeMap<ObjectId, Object> = objects;
            let mut walk =
                TreeWalk { objects: view, adopted: &adopted, seen: HashSet::new(), pages: Vec::new(), damaged: false };
            match view.get(&catalog).and_then(|object| object.as_dict().ok()).map(|dict| dict.get(b"Pages")) {
                Some(Ok(Object::Reference(pages_root))) => {
                    walk.visit(*pages_root, None, 0);
                }
                _ => walk.damaged = true,
            }
            if walk.pages.is_empty() {
                let note =
                    "The page tree had no readable pages; it was rebuilt from the page objects found, in file order.";
                (all_pages, note.to_string())
            } else if walk.damaged {
                let note = format!("The page tree was damaged and has been rebuilt with {} page(s).", walk.pages.len());
                (walk.pages, note)
            } else {
                return Ok((catalog, walk.pages.len()));
            }
        }
        None => {
            let note = "No document catalog was found; a new one was built from the page objects found, in file order.";
            (all_pages, note.to_string())
        }
    };
    if pages.is_empty() {
        return Err(PdfError::CorruptPdf("no pages could be recovered from the file".into()));
    }
    let pages_root = next_object_id(objects)?;
    let sizeless = flatten_page_tree(objects, &pages, pages_root);
    let catalog = match catalog {
        Some(catalog) => {
            if let Some(Object::Dictionary(dict)) = objects.get_mut(&catalog) {
                dict.set("Pages", Object::Reference(pages_root));
            }
            catalog
        }
        None => {
            let catalog = next_object_id(objects)?;
            objects.insert(catalog, Object::Dictionary(dictionary! { "Type" => "Catalog", "Pages" => pages_root }));
            catalog
        }
    };
    warnings.push(note);
    if sizeless > 0 {
        warnings.push(format!("{sizeless} page(s) had no page size; US Letter was assumed."));
    }
    Ok((catalog, pages.len()))
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

/// Drops every object the trailer cannot reach (stale revisions, xref and
/// object streams, scan noise), then keeps one generation per object number.
fn prune_unreachable(
    objects: &mut BTreeMap<ObjectId, Object>,
    trailer: &Dictionary,
    positions: &HashMap<ObjectId, usize>,
) {
    let mut reached: HashSet<ObjectId> = HashSet::new();
    let mut queue: Vec<ObjectId> = Vec::new();
    for (_, value) in trailer.iter() {
        collect_references(value, &mut reached, &mut queue);
    }
    while let Some(id) = queue.pop() {
        if let Some(object) = objects.get(&id) {
            collect_references(object, &mut reached, &mut queue);
        }
    }
    objects.retain(|id, _| reached.contains(id));

    // A cross-reference table has one entry per number; keep the newest.
    let mut by_number: HashMap<u32, ObjectId> = HashMap::new();
    let mut stale = Vec::new();
    for &id in objects.keys() {
        let position = |id: &ObjectId| positions.get(id).copied().unwrap_or(usize::MAX);
        match by_number.get(&id.0) {
            Some(kept) if position(kept) >= position(&id) => stale.push(id),
            Some(kept) => {
                stale.push(*kept);
                by_number.insert(id.0, id);
            }
            None => {
                by_number.insert(id.0, id);
            }
        }
    }
    for id in stale {
        objects.remove(&id);
    }
}

/// Queues the references in `root` (iteratively: nesting is attacker-controlled).
fn collect_references(root: &Object, reached: &mut HashSet<ObjectId>, queue: &mut Vec<ObjectId>) {
    let mut stack = vec![root];
    while let Some(object) = stack.pop() {
        match object {
            Object::Reference(id) => {
                if reached.insert(*id) {
                    queue.push(*id);
                }
            }
            Object::Array(items) => stack.extend(items.iter()),
            Object::Dictionary(dict) => stack.extend(dict.iter().map(|(_, value)| value)),
            Object::Stream(stream) => stack.extend(stream.dict.iter().map(|(_, value)| value)),
            _ => {}
        }
    }
}

fn hex_encode_escaped_strings(objects: &mut BTreeMap<ObjectId, Object>) {
    let mut stack: Vec<&mut Object> = objects.values_mut().collect();
    while let Some(object) = stack.pop() {
        match object {
            Object::String(bytes, format) => {
                let escapes = bytes.iter().filter(|&&byte| matches!(byte, b'(' | b')' | b'\\' | b'\r')).count();
                if escapes > MAX_LITERAL_ESCAPES {
                    *format = StringFormat::Hexadecimal;
                }
            }
            Object::Array(items) => stack.extend(items.iter_mut()),
            Object::Dictionary(dict) => stack.extend(dict.iter_mut().map(|(_, value)| value)),
            Object::Stream(stream) => stack.extend(stream.dict.iter_mut().map(|(_, value)| value)),
            _ => {}
        }
    }
}

/// The `%PDF-x.y` version of the input, or 1.7.
fn header_version(data: &[u8]) -> String {
    let window = data.get(..data.len().min(1024)).unwrap_or_default();
    window
        .windows(5)
        .position(|marker| marker == b"%PDF-")
        .and_then(|at| window.get(at + 5..))
        .map(|rest| rest.iter().take(4).take_while(|&&byte| byte.is_ascii_digit() || byte == b'.').copied().collect())
        .and_then(|version: Vec<u8>| String::from_utf8(version).ok())
        .filter(|version| {
            let mut parts = version.split('.');
            matches!((parts.next(), parts.next(), parts.next()), (Some(major), Some(minor), None)
                if major.len() == 1 && !minor.is_empty())
        })
        .unwrap_or_else(|| "1.7".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skip_object_finds_value_ends() {
        let data = b"<< /A [1 2 (a(b)\\)c)] /B <414243> /C 5 0 R >> rest";
        assert_eq!(skip_object(data, 0, data.len()), Some(data.len() - 5));
        let data = b"7 0 R endobj";
        assert_eq!(skip_object(data, 0, data.len()), Some(5));
        let data = b"<< /A (unterminated";
        assert_eq!(skip_object(data, 0, data.len()), None);
        let data = b"<< /A 1 endobj";
        assert_eq!(skip_object(data, 0, data.len()), None);
        let nested = [b"[".repeat(MAX_NESTING + 1), b"]".repeat(MAX_NESTING + 1)].concat();
        assert_eq!(skip_object(&nested, 0, nested.len()), None);
    }

    #[test]
    fn headers_need_a_token_boundary() {
        let (headers, _) = find_headers(b"1 0 obj\nx2 0 obj\n 3 0 obj\n12 0 objx\n0 0 obj\n");
        let ids: Vec<ObjectId> = headers.iter().map(|header| header.id).collect();
        assert_eq!(ids, vec![(1, 0), (3, 0)]);
    }

    #[test]
    fn direct_length_reads_only_a_direct_integer() {
        let data = b"<< /Filter /X /Length 12 >>";
        assert_eq!(direct_length(data, &(0..data.len())), Some(12));
        let data = b"<< /Length 12 0 R >>";
        assert_eq!(direct_length(data, &(0..data.len())), None);
    }

    #[test]
    fn finder_reuses_its_last_result() {
        let data = b"aa endstream bb endstream";
        let mut finder = Finder::new(b"endstream");
        assert_eq!(finder.next_from(data, 0), Some(3));
        assert_eq!(finder.next_from(data, 2), Some(3));
        assert_eq!(finder.next_from(data, 4), Some(16));
        assert_eq!(finder.next_from(data, 17), None);
    }

    #[test]
    fn header_version_falls_back_to_1_7() {
        assert_eq!(header_version(b"%PDF-1.4\n"), "1.4");
        assert_eq!(header_version(b"junk%PDF-2.0\r"), "2.0");
        assert_eq!(header_version(b"%PDF-x"), "1.7");
        assert_eq!(header_version(b""), "1.7");
    }
}
