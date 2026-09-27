//! PDF/A validation and honest, best-effort conversion.
//!
//! The validator inspects the real document structure with lopdf (and reuses
//! `inspect` for fonts, JavaScript and attachments). It does not trust an XMP
//! claim: a file saying `pdfaid:part 2` while carrying JavaScript fails.
//!
//! The exact checks, per requested level:
//!
//! | id                    | rule                                                       |
//! |-----------------------|------------------------------------------------------------|
//! | `pdfa.encryption`     | encrypted files fail                                       |
//! | `pdfa.file-version`   | A-1 fails on PDF 2.0; A-2/A-3 warn below PDF 1.7           |
//! | `pdfa.xmp-pdfaid`     | XMP `pdfaid:part`/`pdfaid:conformance` must match the level |
//! | `pdfa.output-intent`  | needs a `GTS_PDFA1` output intent with a `/DestOutputProfile` ICC stream (fail A-1, warn A-2/A-3) |
//! | `pdfa.fonts-embedded` | every font must carry a font program                       |
//! | `pdfa.javascript`     | JavaScript fails A-1, warns A-2/A-3                        |
//! | `pdfa.embedded-files` | attachments fail A-1/A-2, are allowed in A-3b              |
//! | `pdfa.title`          | a title in Info or XMP is required (warning)               |
//! | `pdfa.need-appearances` | `/NeedAppearances true` fails                            |
//! | `pdfa.launch-actions` | any `/S /Launch` action fails                              |
//! | `pdfa.trailer-id`     | a trailer `/ID` is expected (warning)                      |
//!
//! Conversion applies only fixes that are actually achievable with lopdf:
//! JavaScript, actions and embedded files are stripped through the sanitizer,
//! an XMP packet with the correct `pdfaid:part`/`conformance` is written, an
//! sRGB `GTS_PDFA1` output intent **with a generated ICC v4 profile** as its
//! `/DestOutputProfile` is declared, `/NeedAppearances` is turned off, and
//! non-embedded simple fonts receive a bundled substitute program (see
//! [`crate::fontembed`]). The per-font outcome, including every honest skip
//! (CID/Type0, symbolic, custom encodings, non-metric substitutes), travels
//! back in [`PdfaReport::font_embedding`].
//!
//! What this still does **not** do, and the validator therefore still reports:
//! fonts are embedded whole (no subsetting), Type0/CID fonts are skipped, and
//! the PT Sans fallback used for Times/Courier/unknown families is not
//! metric-compatible. The ICC profile is generated deterministically to the
//! ICC v4 layout and checked structurally by this crate's tests; it has not
//! been run through an external validator such as veraPDF. The converter never
//! claims a success it did not achieve.

use std::path::Path;

use lopdf::{dictionary, Dictionary, Document, Object, Stream};
use serde::{Deserialize, Serialize};

use crate::docutil;
use crate::error::{PdfError, PdfResult};
use crate::fontembed::{self, EmbeddedFontReport};
use crate::inspect;
use crate::progress::{CancelToken, ProgressCallback, ProgressReporter};
use crate::sanitize::{sanitize_document, SanitizeOptions};

/// The PDF/A conformance levels this module understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PdfaLevel {
    A1b,
    A2b,
    A3b,
}

impl PdfaLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            PdfaLevel::A1b => "A-1b",
            PdfaLevel::A2b => "A-2b",
            PdfaLevel::A3b => "A-3b",
        }
    }

    /// The `pdfaid:part` value this level corresponds to.
    pub fn part(&self) -> u8 {
        match self {
            PdfaLevel::A1b => 1,
            PdfaLevel::A2b => 2,
            PdfaLevel::A3b => 3,
        }
    }

    /// Parses `1b`, `A-1b`, `PDF/A-2B`... into a level.
    pub fn parse(value: &str) -> PdfResult<Self> {
        let normalized: String = value
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_lowercase();
        match normalized.as_str() {
            "a1b" | "1b" | "pdfa1b" | "pdfa1" => Ok(PdfaLevel::A1b),
            "a2b" | "2b" | "pdfa2b" | "pdfa2" => Ok(PdfaLevel::A2b),
            "a3b" | "3b" | "pdfa3b" | "pdfa3" => Ok(PdfaLevel::A3b),
            _ => Err(PdfError::InvalidInput(format!("unknown PDF/A level '{value}'"))),
        }
    }
}

/// One validation result. `status` is `pass`, `warning` or `fail`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfaCheck {
    pub id: String,
    pub level: String,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfaReport {
    pub valid: bool,
    pub level: String,
    pub checks: Vec<PdfaCheck>,
    pub failures: usize,
    pub warnings: usize,
    /// True when the report was produced by [`convert_pdfa`].
    pub converted: bool,
    /// Per-font outcome of the conversion's embedding step. Empty for a plain
    /// [`validate_pdfa`] run; a font that could not be embedded is reported
    /// here with its skip reason instead of being silently dropped.
    #[serde(default)]
    pub font_embedding: Vec<EmbeddedFontReport>,
}

fn check(id: &str, level: PdfaLevel, status: &str, message: impl Into<String>) -> PdfaCheck {
    PdfaCheck {
        id: id.to_string(),
        level: level.as_str().to_string(),
        status: status.to_string(),
        message: message.into(),
    }
}

fn resolve_dict(doc: &Document, value: Option<&Object>) -> Option<Dictionary> {
    match value? {
        Object::Reference(id) => doc.get_dictionary(*id).ok().cloned(),
        Object::Dictionary(dict) => Some(dict.clone()),
        _ => None,
    }
}

fn resolve_array(doc: &Document, value: Option<&Object>) -> Option<Vec<Object>> {
    match value? {
        Object::Reference(id) => match doc.get_object(*id).ok()? {
            Object::Array(items) => Some(items.clone()),
            _ => None,
        },
        Object::Array(items) => Some(items.clone()),
        _ => None,
    }
}

fn resolve_stream<'a>(doc: &'a Document, value: Option<&'a Object>) -> Option<&'a Stream> {
    match value? {
        Object::Reference(id) => match doc.get_object(*id).ok()? {
            Object::Stream(stream) => Some(stream),
            _ => None,
        },
        Object::Stream(stream) => Some(stream),
        _ => None,
    }
}

/// The document's XMP packet, decompressed, when present.
fn xmp_content(doc: &Document) -> Option<String> {
    let catalog = doc.catalog().ok()?;
    let stream = resolve_stream(doc, catalog.get(b"Metadata").ok())?;
    let bytes = stream.decompressed_content().ok()?;
    Some(String::from_utf8_lossy(&bytes).to_string())
}

/// Reads `<tag>value</tag>` or `tag="value"` from an XMP packet.
fn xmp_value(xmp: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    if let Some(start) = xmp.find(&open) {
        let start = start + open.len();
        let close = format!("</{tag}>");
        if let Some(end) = xmp[start..].find(&close) {
            return Some(xmp[start..start + end].trim().to_string());
        }
    }
    let needle = format!("{tag}=\"");
    let start = xmp.find(&needle)? + needle.len();
    let end = xmp[start..].find('"')? + start;
    Some(xmp[start..end].trim().to_string())
}

fn version_parts(version: &str) -> (u32, u32) {
    let mut parts = version.split('.').filter_map(|part| part.parse::<u32>().ok());
    (parts.next().unwrap_or(1), parts.next().unwrap_or(0))
}

/// The exact output-intent requirement that fails, so the check message can
/// name it instead of a bare boolean.
enum OutputIntentState {
    /// No `/S /GTS_PDFA1` output intent dictionary was found.
    Missing,
    /// A GTS_PDFA1 intent exists but none carries `/DestOutputProfile`.
    NoProfile,
    /// A profile stream exists but is structurally unusable; the string names
    /// the failing detail.
    BadProfile(String),
    /// A GTS_PDFA1 intent with a usable ICC stream, `components` = `/N`.
    Present { components: i64 },
}

/// Resolves the document's output intents down to the strongest GTS_PDFA1
/// state found. A profile is "usable" here when it is a non-empty stream with
/// a positive `/N` component count; the ICC bytes themselves are not parsed
/// by the validator (the converter generates them, see `srgb_v4_icc_profile`).
fn gts_pdfa1_output_intent(doc: &Document) -> OutputIntentState {
    let catalog = match doc.catalog() {
        Ok(catalog) => catalog,
        Err(_) => return OutputIntentState::Missing,
    };
    let intents = match resolve_array(doc, catalog.get(b"OutputIntents").ok()) {
        Some(items) => items,
        None => return OutputIntentState::Missing,
    };
    let mut state = OutputIntentState::Missing;
    for item in &intents {
        let intent = match resolve_dict(doc, Some(item)) {
            Some(intent) => intent,
            None => continue,
        };
        if intent.get(b"S").ok().and_then(|value| value.as_name().ok()) != Some(b"GTS_PDFA1") {
            continue;
        }
        match resolve_stream(doc, intent.get(b"DestOutputProfile").ok()) {
            Some(stream) => {
                let components = stream
                    .dict
                    .get(b"N")
                    .ok()
                    .and_then(|value| value.as_i64().ok())
                    .unwrap_or(0);
                if components <= 0 {
                    if matches!(state, OutputIntentState::Missing | OutputIntentState::NoProfile) {
                        state = OutputIntentState::BadProfile("has no positive /N component count".to_string());
                    }
                } else if stream.content.is_empty() {
                    if !matches!(state, OutputIntentState::Present { .. }) {
                        state = OutputIntentState::BadProfile("is an empty stream".to_string());
                    }
                } else {
                    return OutputIntentState::Present { components };
                }
            }
            None => {
                if matches!(state, OutputIntentState::Missing) {
                    state = OutputIntentState::NoProfile;
                }
            }
        }
    }
    state
}

fn need_appearances_true(doc: &Document) -> bool {
    let catalog = match doc.catalog() {
        Ok(catalog) => catalog,
        Err(_) => return false,
    };
    let acro = match resolve_dict(doc, catalog.get(b"AcroForm").ok()) {
        Some(acro) => acro,
        None => return false,
    };
    match acro.get(b"NeedAppearances") {
        Ok(Object::Boolean(value)) => *value,
        Ok(Object::Name(value)) => value == b"true",
        _ => false,
    }
}

fn has_launch_action(doc: &Document) -> bool {
    doc.objects.values().any(|object| {
        let dict = match object {
            Object::Dictionary(dict) => dict,
            Object::Stream(stream) => &stream.dict,
            _ => return false,
        };
        dict.get(b"S").ok().and_then(|value| value.as_name().ok()) == Some(b"Launch")
    })
}

fn trailer_has_id(doc: &Document) -> bool {
    match doc.trailer.get(b"ID").ok().and_then(|value| value.as_array().ok()) {
        Some(items) => items.len() >= 2 && items.iter().all(|item| matches!(item, Object::String(_, _))),
        None => false,
    }
}

fn finish(level: PdfaLevel, checks: Vec<PdfaCheck>, converted: bool) -> PdfaReport {
    let failures = checks.iter().filter(|entry| entry.status == "fail").count();
    let warnings = checks.iter().filter(|entry| entry.status == "warning").count();
    PdfaReport {
        valid: failures == 0,
        level: level.as_str().to_string(),
        checks,
        failures,
        warnings,
        converted,
        font_embedding: Vec::new(),
    }
}

/// Validates a file against a PDF/A level. The checks are listed in the module
/// documentation; every one inspects the document, never its claims alone.
pub fn validate_pdfa(path: &Path, level: PdfaLevel) -> PdfResult<PdfaReport> {
    let doc = Document::load(path).map_err(|error| PdfError::from_lopdf(error, Some(path)))?;

    // An encrypted file cannot be read further, so report the one definitive
    // failure instead of producing noise from unreadable streams.
    if doc.is_encrypted() {
        let checks = vec![check(
            "pdfa.encryption",
            level,
            "fail",
            "The document is encrypted; PDF/A forbids encryption.",
        )];
        return Ok(finish(level, checks, false));
    }

    let inspection = inspect::inspect_document(path, None)?;
    let mut checks: Vec<PdfaCheck> = Vec::new();

    checks.push(check("pdfa.encryption", level, "pass", "The document is not encrypted."));

    let (major, minor) = version_parts(&doc.version);
    match level {
        PdfaLevel::A1b => {
            if major > 1 {
                checks.push(check(
                    "pdfa.file-version",
                    level,
                    "fail",
                    format!("PDF/A-1 is based on PDF 1.4; this file declares PDF {}.", doc.version),
                ));
            } else {
                checks.push(check(
                    "pdfa.file-version",
                    level,
                    "pass",
                    format!("PDF version {} is a 1.x version.", doc.version),
                ));
            }
        }
        PdfaLevel::A2b | PdfaLevel::A3b => {
            if major == 1 && minor < 7 {
                checks.push(check(
                    "pdfa.file-version",
                    level,
                    "warning",
                    format!("PDF version {} predates PDF 1.7, the baseline for PDF/A-2 and PDF/A-3.", doc.version),
                ));
            } else {
                checks.push(check(
                    "pdfa.file-version",
                    level,
                    "pass",
                    format!("PDF version {} is acceptable for {}.", doc.version, level.as_str()),
                ));
            }
        }
    }

    let xmp = xmp_content(&doc);
    let expected_part = level.part().to_string();
    match xmp.as_deref() {
        Some(text) => {
            let part = xmp_value(text, "pdfaid:part");
            let conformance = xmp_value(text, "pdfaid:conformance");
            if part.as_deref() == Some(expected_part.as_str())
                && conformance.as_deref() == Some("B")
            {
                checks.push(check(
                    "pdfa.xmp-pdfaid",
                    level,
                    "pass",
                    format!("XMP declares pdfaid:part {expected_part} and conformance B."),
                ));
            } else {
                checks.push(check(
                    "pdfa.xmp-pdfaid",
                    level,
                    "fail",
                    format!(
                        "XMP must declare pdfaid:part {expected_part} and conformance B; found part {} conformance {}.",
                        part.as_deref().unwrap_or("<missing>"),
                        conformance.as_deref().unwrap_or("<missing>")
                    ),
                ));
            }
        }
        None => checks.push(check(
            "pdfa.xmp-pdfaid",
            level,
            "fail",
            "The document has no XMP metadata packet, so the PDF/A identifier is missing.",
        )),
    }

    match gts_pdfa1_output_intent(&doc) {
        OutputIntentState::Present { components } => checks.push(check(
            "pdfa.output-intent",
            level,
            "pass",
            format!("A GTS_PDFA1 output intent with a {components}-component ICC profile is present."),
        )),
        OutputIntentState::NoProfile => {
            let status = if level == PdfaLevel::A1b { "fail" } else { "warning" };
            checks.push(check(
                "pdfa.output-intent",
                level,
                status,
                "A GTS_PDFA1 output intent is present but carries no /DestOutputProfile ICC stream.",
            ));
        }
        OutputIntentState::BadProfile(detail) => {
            let status = if level == PdfaLevel::A1b { "fail" } else { "warning" };
            checks.push(check(
                "pdfa.output-intent",
                level,
                status,
                format!("A GTS_PDFA1 output intent is present but its /DestOutputProfile {detail}."),
            ));
        }
        OutputIntentState::Missing => {
            if level == PdfaLevel::A1b {
                checks.push(check(
                    "pdfa.output-intent",
                    level,
                    "fail",
                    "PDF/A-1 requires a GTS_PDFA1 output intent and none was found.",
                ));
            } else {
                checks.push(check(
                    "pdfa.output-intent",
                    level,
                    "warning",
                    "No GTS_PDFA1 output intent was found; strict validators will reject the file.",
                ));
            }
        }
    }

    let unembedded: Vec<String> = inspection
        .fonts
        .iter()
        .filter(|font| !font.embedded)
        .map(|font| font.name.clone())
        .collect();
    if unembedded.is_empty() {
        checks.push(check(
            "pdfa.fonts-embedded",
            level,
            "pass",
            "Every font in the document is embedded.",
        ));
    } else {
        checks.push(check(
            "pdfa.fonts-embedded",
            level,
            "fail",
            format!(
                "{} font(s) are not embedded: {}.",
                unembedded.len(),
                unembedded.iter().take(4).cloned().collect::<Vec<_>>().join(", ")
            ),
        ));
    }

    match level {
        PdfaLevel::A1b => {
            if inspection.has_javascript {
                checks.push(check(
                    "pdfa.javascript",
                    level,
                    "fail",
                    "PDF/A-1 forbids JavaScript and the document contains some.",
                ));
            } else {
                checks.push(check("pdfa.javascript", level, "pass", "No JavaScript was found."));
            }
        }
        PdfaLevel::A2b | PdfaLevel::A3b => {
            if inspection.has_javascript {
                checks.push(check(
                    "pdfa.javascript",
                    level,
                    "warning",
                    "The document contains JavaScript, which strict readers of PDF/A-2 and PDF/A-3 ignore.",
                ));
            } else {
                checks.push(check("pdfa.javascript", level, "pass", "No JavaScript was found."));
            }
        }
    }

    if inspection.embedded_files.is_empty() {
        checks.push(check("pdfa.embedded-files", level, "pass", "No embedded files were found."));
    } else if level == PdfaLevel::A3b {
        checks.push(check(
            "pdfa.embedded-files",
            level,
            "pass",
            format!("{} embedded file(s) are allowed in PDF/A-3b.", inspection.embedded_files.len()),
        ));
    } else {
        checks.push(check(
            "pdfa.embedded-files",
            level,
            "fail",
            format!(
                "{} embedded file(s) are present; {} forbids attachments.",
                inspection.embedded_files.len(),
                level.as_str()
            ),
        ));
    }

    let xmp_title = xmp.as_deref().is_some_and(|text| xmp_value(text, "dc:title").is_some());
    if !inspection.title_override.trim().is_empty() || xmp_title {
        checks.push(check("pdfa.title", level, "pass", "The document declares a title."));
    } else {
        checks.push(check(
            "pdfa.title",
            level,
            "warning",
            "No document title was found in the Info dictionary or in XMP.",
        ));
    }

    if need_appearances_true(&doc) {
        checks.push(check(
            "pdfa.need-appearances",
            level,
            "fail",
            "The AcroForm requests /NeedAppearances, which PDF/A forbids.",
        ));
    } else {
        checks.push(check(
            "pdfa.need-appearances",
            level,
            "pass",
            "The form does not request /NeedAppearances.",
        ));
    }

    if has_launch_action(&doc) {
        checks.push(check(
            "pdfa.launch-actions",
            level,
            "fail",
            "A /Launch action is present; PDF/A forbids launching external programs.",
        ));
    } else {
        checks.push(check("pdfa.launch-actions", level, "pass", "No /Launch action was found."));
    }

    if trailer_has_id(&doc) {
        checks.push(check("pdfa.trailer-id", level, "pass", "The trailer carries a file identifier."));
    } else {
        checks.push(check(
            "pdfa.trailer-id",
            level,
            "warning",
            "The trailer has no /ID; archival tools expect a file identifier.",
        ));
    }

    Ok(finish(level, checks, false))
}

fn escape_xml(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(character),
        }
    }
    out
}

fn write_xmp_metadata(doc: &mut Document, level: PdfaLevel, title: &str) -> PdfResult<()> {
    let title = escape_xml(title.trim());
    let packet = format!(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n\
         <x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n\
         <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n\
         <rdf:Description rdf:about=\"\" xmlns:pdfaid=\"http://www.aiim.org/pdfa/ns/id/\">\n\
         <pdfaid:part>{part}</pdfaid:part>\n\
         <pdfaid:conformance>B</pdfaid:conformance>\n\
         </rdf:Description>\n\
         <rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n\
         <dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">{title}</rdf:li></rdf:Alt></dc:title>\n\
         </rdf:Description>\n\
         </rdf:RDF>\n\
         </x:xmpmeta>\n\
         <?xpacket end=\"w\"?>\n",
        part = level.part()
    );
    let stream = Stream::new(
        dictionary! {
            "Type" => "Metadata",
            "Subtype" => "XML",
        },
        packet.into_bytes(),
    );
    let id = doc.add_object(Object::Stream(stream));
    doc.catalog_mut()?.set("Metadata", Object::Reference(id));
    Ok(())
}

/// Encodes a value as an ICC `s15Fixed16Number` (16 fractional bits,
/// big-endian two's complement).
fn fix16(value: f64) -> i32 {
    let scaled = (value * 65536.0).round();
    let clamped = scaled.clamp(i32::MIN as f64, i32::MAX as f64);
    clamped as i32
}

/// `XYZType` tag body: type signature, reserved word and three s15Fixed16
/// values.
fn icc_xyz_tag(x: f64, y: f64, z: f64) -> Vec<u8> {
    let mut out = Vec::with_capacity(20);
    out.extend_from_slice(b"XYZ ");
    out.extend_from_slice(&[0u8; 4]);
    for value in [x, y, z] {
        out.extend_from_slice(&fix16(value).to_be_bytes());
    }
    out
}

/// The sRGB transfer curve as an ICC `parametricCurveType` with function type
/// 4: `Y = (a*X + b)^g` for `X >= d`, `Y = c*X + f` otherwise, using the
/// IEC 61966-2.1 constants. The same tag body is shared by rTRC/gTRC/bTRC.
fn srgb_trc_tag() -> Vec<u8> {
    const PARAMETERS: [f64; 7] = [
        2.4,             // g
        1.0 / 1.055,     // a
        0.055 / 1.055,   // b
        1.0 / 12.92,     // c
        0.04045,         // d
        0.0,             // e
        0.0,             // f
    ];
    let mut out = Vec::with_capacity(40);
    out.extend_from_slice(b"para");
    out.extend_from_slice(&[0u8; 4]);
    out.extend_from_slice(&4u16.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    for parameter in PARAMETERS {
        out.extend_from_slice(&fix16(parameter).to_be_bytes());
    }
    out
}

/// `multiLocalizedUnicodeType` tag body with one `en-US` record, the ICC v4
/// way to carry `desc` and `cprt` text.
fn icc_mluc_tag(text: &str) -> Vec<u8> {
    let mut string = Vec::with_capacity(text.len() * 2);
    for unit in text.encode_utf16() {
        string.extend_from_slice(&unit.to_be_bytes());
    }
    let mut out = Vec::with_capacity(28 + string.len() + 3);
    out.extend_from_slice(b"mluc");
    out.extend_from_slice(&[0u8; 4]);
    out.extend_from_slice(&1u32.to_be_bytes()); // record count
    out.extend_from_slice(&12u32.to_be_bytes()); // record size
    out.extend_from_slice(b"enUS"); // ISO 639-1 language + ISO 3166-1 country
    out.extend_from_slice(&(string.len() as u32).to_be_bytes());
    out.extend_from_slice(&28u32.to_be_bytes()); // string offset within the tag
    out.extend_from_slice(&string);
    while out.len() % 4 != 0 {
        out.push(0);
    }
    out
}

/// Builds a deterministic, structurally valid minimal sRGB ICC v4 profile:
/// `mntr` class, `RGB ` data space, `XYZ ` PCS, a 9-tag table (r/g/bXYZ,
/// r/g/bTRC, wtpt, desc, cprt) with 4-byte aligned tag data, the D50 PCS
/// illuminant, D50-adapted sRGB colorants and the IEC 61966-2.1 parametric
/// TRC. The profile ID is left zero, which the ICC spec defines as "not
/// calculated"; the creation date is fixed so the bytes are reproducible.
///
/// Verified: the tag table, offsets, sizes and type signatures are checked by
/// this crate's structural walker test. Not verified: an external validator
/// (e.g. veraPDF) has not been run against the emitted profile, so callers
/// must not describe it as a certified ICC profile.
pub fn srgb_v4_icc_profile() -> Vec<u8> {
    // D50-adapted sRGB colorants (Bradford adaptation), the matrix used by
    // published sRGB v4 profiles. Values are s15Fixed16-encoded below.
    const WHITE: (f64, f64, f64) = (0.9642, 1.0, 0.8249);
    const RED: (f64, f64, f64) = (0.4360747, 0.2225045, 0.0139322);
    const GREEN: (f64, f64, f64) = (0.3850649, 0.7168786, 0.0971045);
    const BLUE: (f64, f64, f64) = (0.1430804, 0.0606169, 0.7141733);

    // Tag table entries must be sorted by signature.
    let tags: [(&[u8; 4], Vec<u8>); 9] = [
        (b"bTRC", srgb_trc_tag()),
        (b"bXYZ", icc_xyz_tag(BLUE.0, BLUE.1, BLUE.2)),
        (b"cprt", icc_mluc_tag("Public domain. sRGB values per IEC 61966-2.1.")),
        (b"desc", icc_mluc_tag("sRGB IEC61966-2.1")),
        (b"gTRC", srgb_trc_tag()),
        (b"gXYZ", icc_xyz_tag(GREEN.0, GREEN.1, GREEN.2)),
        (b"rTRC", srgb_trc_tag()),
        (b"rXYZ", icc_xyz_tag(RED.0, RED.1, RED.2)),
        (b"wtpt", icc_xyz_tag(WHITE.0, WHITE.1, WHITE.2)),
    ];

    const HEADER_SIZE: usize = 128;
    let table_size = 4 + tags.len() * 12;
    let mut offsets: Vec<(usize, usize)> = Vec::with_capacity(tags.len());
    let mut offset = HEADER_SIZE + table_size;
    for (_, data) in &tags {
        offsets.push((offset, data.len()));
        offset += data.len();
        offset = (offset + 3) & !3; // tag data is 4-byte aligned
    }
    let total = offset;

    let mut profile: Vec<u8> = vec![0u8; HEADER_SIZE];
    profile.extend_from_slice(&(tags.len() as u32).to_be_bytes());
    for ((signature, _), (tag_offset, tag_size)) in tags.iter().zip(&offsets) {
        profile.extend_from_slice(*signature);
        profile.extend_from_slice(&(*tag_offset as u32).to_be_bytes());
        profile.extend_from_slice(&(*tag_size as u32).to_be_bytes());
    }
    for ((_, data), (tag_offset, _)) in tags.iter().zip(&offsets) {
        debug_assert_eq!(profile.len(), *tag_offset);
        profile.extend_from_slice(data);
        while profile.len() % 4 != 0 {
            profile.push(0);
        }
    }

    // Header, per ICC.1:2022 Table 17.
    profile[0..4].copy_from_slice(&(total as u32).to_be_bytes()); // profile size
    profile[8..12].copy_from_slice(&0x0430_0000u32.to_be_bytes()); // v4.3.0
    profile[12..16].copy_from_slice(b"mntr"); // display device profile
    profile[16..20].copy_from_slice(b"RGB "); // data colour space
    profile[20..24].copy_from_slice(b"XYZ "); // profile connection space
    // Fixed creation date keeps the output deterministic; only ranges matter.
    profile[24..26].copy_from_slice(&2026u16.to_be_bytes());
    profile[26..28].copy_from_slice(&1u16.to_be_bytes());
    profile[28..30].copy_from_slice(&1u16.to_be_bytes());
    profile[36..40].copy_from_slice(b"acsp");
    profile[68..72].copy_from_slice(&fix16(WHITE.0).to_be_bytes());
    profile[72..76].copy_from_slice(&fix16(WHITE.1).to_be_bytes());
    profile[76..80].copy_from_slice(&fix16(WHITE.2).to_be_bytes());
    profile
}

fn write_output_intent(doc: &mut Document) -> PdfResult<()> {
    // `/DestOutputProfile` is what makes the output intent real: it names the
    // ICC profile that defines the colour space the document is intended for.
    // `/N 3` is three-component RGB, `/Alternate` names the device space a
    // reader without ICC support should fall back to. The profile bytes are
    // generated deterministically in this crate.
    let profile_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {
            "N" => 3i64,
            "Alternate" => "DeviceRGB",
        },
        srgb_v4_icc_profile(),
    )));
    let intent = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "OutputIntent",
        "S" => "GTS_PDFA1",
        "OutputConditionIdentifier" => Object::String(b"sRGB IEC61966-2.1".to_vec(), lopdf::StringFormat::Literal),
        "Info" => Object::String(b"sRGB IEC61966-2.1".to_vec(), lopdf::StringFormat::Literal),
        "DestOutputProfile" => Object::Reference(profile_id),
    }));
    doc.catalog_mut()?
        .set("OutputIntents", Object::Array(vec![Object::Reference(intent)]));
    Ok(())
}

fn remove_need_appearances(doc: &mut Document) {
    let acro_id = doc
        .catalog()
        .ok()
        .and_then(|catalog| catalog.get(b"AcroForm").ok())
        .and_then(|value| value.as_reference().ok());
    if let Some(id) = acro_id {
        if let Ok(Object::Dictionary(acro)) = doc.get_object_mut(id) {
            acro.remove(b"NeedAppearances");
        }
    }
}

/// Converts a file towards `level` with the fixes lopdf can actually apply,
/// then re-validates the written output and returns that real report. The
/// font-embedding step runs before saving and its per-font outcome is carried
/// in [`PdfaReport::font_embedding`]; fonts it refuses (CID/Type0, symbolic,
/// custom encodings) still make the re-run validator report `valid: false`,
/// and the converter never claims a success it did not achieve.
pub fn convert_pdfa(
    input: &Path,
    output: &Path,
    level: PdfaLevel,
    progress: &ProgressCallback,
    cancel: &CancelToken,
) -> PdfResult<PdfaReport> {
    cancel.check()?;
    let mut doc = docutil::load_document(input, None)?;
    let reporter = ProgressReporter::new(progress);
    reporter.emit_step("pdfa.convert", 0, 5);

    let sanitize_options = SanitizeOptions {
        remove_javascript: true,
        remove_embedded_files: true,
        remove_actions: true,
        remove_metadata: false,
        remove_open_action: true,
        remove_form_actions: true,
        remove_annotations_unsafe: false,
        remove_links: false,
    };
    let _ = sanitize_document(&mut doc, &sanitize_options, cancel)?;
    reporter.emit_step("pdfa.convert", 1, 5);

    let title = crate::metadata::read_metadata(&doc).title;
    write_xmp_metadata(&mut doc, level, &title)?;
    write_output_intent(&mut doc)?;
    remove_need_appearances(&mut doc);
    reporter.emit_step("pdfa.convert", 2, 5);

    // Address the main PDF/A failure mode: fonts the file only names get a
    // bundled substitute program. The report lists what was embedded and what
    // was skipped, and why; skipped fonts keep failing validation honestly.
    let font_embedding = fontembed::embed_missing_fonts_in_document(&mut doc);
    reporter.emit_step("pdfa.convert", 3, 5);

    docutil::save_document(&mut doc, output, true)?;
    reporter.emit_step("pdfa.convert", 4, 5);

    let mut report = validate_pdfa(output, level)?;
    report.converted = true;
    report.font_embedding = font_embedding;
    reporter.emit_step("pdfa.convert", 5, 5);
    Ok(report)
}
