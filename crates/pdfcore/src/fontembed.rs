//! Font-program embedding for honest PDF/A conversion.
//!
//! PDF/A requires every font a document uses to carry its font program. Most
//! real-world conversion failures come from the base-14 families (Helvetica,
//! Arial, Times, Courier): the file only names them and relies on the reader's
//! substitutes. This module finds those fonts in the page resources (and the
//! AcroForm default resources) and embeds a bundled substitute program.
//!
//! Substitution table. "Metric compatible" is verified from the Liberation
//! project's design goal: Liberation Sans was built as a metric-compatible
//! replacement for Arial/Helvetica, so glyph advance widths match the base-14
//! AFM and an existing `/Widths` array stays correct:
//!
//! | base font                              | substitute                | metric compatible |
//! |----------------------------------------|---------------------------|-------------------|
//! | Helvetica, Helv, Arial, ArialMT        | LiberationSans-Regular    | yes               |
//! | Helvetica-Bold, Arial-Bold, Arial-BoldMT | LiberationSans-Bold     | yes               |
//! | Helvetica-Oblique/Italic, Arial-Italic | LiberationSans-Italic     | yes               |
//! | Helvetica-BoldOblique/BoldItalic, Arial-BoldItalic | LiberationSans-BoldItalic | yes    |
//! | Times*, Courier*, anything else        | PT Sans regular or bold   | no (warning)      |
//!
//! PT Sans is a different design (different units per em and advance widths).
//! The original `/Widths` are kept because PDF/A conversion must not touch the
//! content streams; the per-font report carries a warning that spacing may
//! shift and that an italic source loses its slant.
//!
//! Deliberately not implemented, reported instead of faked:
//! * Type0/CID fonts: the text is addressed by CIDs, so a substitute program
//!   would need a CID-to-GID map; re-mapping CID text is out of scope.
//! * Symbol/ZapfDingbats and fonts whose descriptor is flagged symbolic: the
//!   bundled Latin programs have no matching glyph mapping.
//! * Custom `/Differences` encodings and encodings other than WinAnsi/MacRoman.
//! * Fonts written as inline dictionaries (they cannot be replaced in place).
//! * Fonts referenced from form XObjects or annotation appearance streams:
//!   only page resources and the AcroForm `/DR` resources are walked.
//!
//! The full font program is embedded, no subsetting: roughly 140 KB per
//! Liberation face and 450 KB per PT Sans face. Font metrics (`FontBBox`,
//! `Ascent`, `Descent`, `CapHeight`, `ItalicAngle`, PostScript name) are read
//! from the bundled TTF's own `head`/`hhea`/`OS/2`/`post`/`name` tables with
//! ab_glyph plus a small bounds-checked sfnt reader. `StemV` has no TrueType
//! equivalent (it is Type 1 AFM data): the conventional 80/120 values are used
//! and marked as an estimate in the code.

use std::collections::{BTreeMap, HashSet};

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Bundled substitute programs (SIL OFL 1.1, see assets/fonts/LICENSES.txt)
// ---------------------------------------------------------------------------

const LIBERATION_SANS_REGULAR: &[u8] = include_bytes!("../assets/fonts/LiberationSans-Regular.ttf");
const LIBERATION_SANS_BOLD: &[u8] = include_bytes!("../assets/fonts/LiberationSans-Bold.ttf");
const LIBERATION_SANS_ITALIC: &[u8] = include_bytes!("../assets/fonts/LiberationSans-Italic.ttf");
const LIBERATION_SANS_BOLD_ITALIC: &[u8] = include_bytes!("../assets/fonts/LiberationSans-BoldItalic.ttf");
const PT_SANS_REGULAR: &[u8] = include_bytes!("../assets/fonts/PT_Sans-Web-Regular.ttf");
const PT_SANS_BOLD: &[u8] = include_bytes!("../assets/fonts/PT_Sans-Web-Bold.ttf");

/// Bounds that keep a hostile document from turning the walk into unbounded
/// work. 1024 distinct fonts and 2048 report entries are far past any real
/// document; beyond that the conversion continues with what it has.
const MAX_FONTS: usize = 1024;
const MAX_REPORTS: usize = 2048;
const MAX_PAGES: usize = 100_000;

/// One font's outcome. `embedded` means "this conversion embedded a program
/// now"; a font that already carried one is reported as skipped, not embedded.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedFontReport {
    /// The resource name, e.g. `F1`.
    pub resource_name: String,
    /// The original `/BaseFont`, e.g. `Helvetica`.
    pub base_font: String,
    /// True when a substitute program was written into this document.
    pub embedded: bool,
    /// The bundled PostScript name that was embedded, when one was.
    pub substitute: Option<String>,
    /// True only for the Liberation Sans family, which is metric-compatible
    /// with the Helvetica/Arial base fonts. None when nothing was embedded.
    pub metric_compatible: Option<bool>,
    /// Why the font was left alone, when it was.
    pub skipped_reason: Option<String>,
    /// Non-fatal notes, e.g. "the substitute is not metric-compatible".
    pub warning: Option<String>,
}

impl EmbeddedFontReport {
    fn skipped(resource_name: &str, base_font: &str, reason: impl Into<String>) -> Self {
        Self {
            resource_name: resource_name.to_string(),
            base_font: base_font.to_string(),
            embedded: false,
            substitute: None,
            metric_compatible: None,
            skipped_reason: Some(reason.into()),
            warning: None,
        }
    }

    fn embedded(
        resource_name: &str,
        base_font: &str,
        substitute: &str,
        metric_compatible: bool,
        warning: Option<String>,
    ) -> Self {
        Self {
            resource_name: resource_name.to_string(),
            base_font: base_font.to_string(),
            embedded: true,
            substitute: Some(substitute.to_string()),
            metric_compatible: Some(metric_compatible),
            skipped_reason: None,
            warning,
        }
    }
}

/// The bundled programs this module can embed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Substitute {
    LiberationSansRegular,
    LiberationSansBold,
    LiberationSansItalic,
    LiberationSansBoldItalic,
    PtSansRegular,
    PtSansBold,
}

impl Substitute {
    fn program(self) -> &'static [u8] {
        match self {
            Substitute::LiberationSansRegular => LIBERATION_SANS_REGULAR,
            Substitute::LiberationSansBold => LIBERATION_SANS_BOLD,
            Substitute::LiberationSansItalic => LIBERATION_SANS_ITALIC,
            Substitute::LiberationSansBoldItalic => LIBERATION_SANS_BOLD_ITALIC,
            Substitute::PtSansRegular => PT_SANS_REGULAR,
            Substitute::PtSansBold => PT_SANS_BOLD,
        }
    }

    /// Fallback PostScript name, used only when the bundled program's name
    /// table cannot be read (which would mean a corrupt asset).
    fn postscript_name(self) -> &'static str {
        match self {
            Substitute::LiberationSansRegular => "LiberationSans",
            Substitute::LiberationSansBold => "LiberationSans-Bold",
            Substitute::LiberationSansItalic => "LiberationSans-Italic",
            Substitute::LiberationSansBoldItalic => "LiberationSans-BoldItalic",
            Substitute::PtSansRegular => "PTSans-Regular",
            Substitute::PtSansBold => "PTSans-Bold",
        }
    }

    fn bold(self) -> bool {
        matches!(self, Substitute::LiberationSansBold | Substitute::LiberationSansBoldItalic | Substitute::PtSansBold)
    }

    fn italic(self) -> bool {
        matches!(self, Substitute::LiberationSansItalic | Substitute::LiberationSansBoldItalic)
    }

    /// True only for the Liberation Sans family; verified design fact for
    /// Arial/Helvetica, no such claim for PT Sans.
    fn metric_compatible(self) -> bool {
        matches!(
            self,
            Substitute::LiberationSansRegular
                | Substitute::LiberationSansBold
                | Substitute::LiberationSansItalic
                | Substitute::LiberationSansBoldItalic
        )
    }
}

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Embeds a substitute program for every non-embedded simple font it can
/// honestly re-encode, and returns the rewritten PDF plus a per-font report.
///
/// This function never panics and never fails: malformed bytes come back
/// unchanged with a report entry saying the document could not be parsed. When
/// nothing was embedded the original bytes are returned byte for byte, so a
/// validation-only caller pays no reserialization cost.
pub fn embed_missing_fonts(pdf_bytes: &[u8]) -> (Vec<u8>, Vec<EmbeddedFontReport>) {
    let mut doc = match Document::load_mem(pdf_bytes) {
        Ok(doc) => doc,
        Err(error) => {
            return (
                pdf_bytes.to_vec(),
                vec![EmbeddedFontReport::skipped(
                    "<document>",
                    "",
                    format!("the PDF could not be parsed, so no font was embedded: {error}"),
                )],
            );
        }
    };
    let reports = embed_missing_fonts_in_document(&mut doc);
    if !reports.iter().any(|entry| entry.embedded) {
        return (pdf_bytes.to_vec(), reports);
    }
    let mut output = Vec::new();
    match doc.save_to(&mut output) {
        Ok(()) => (output, reports),
        Err(error) => {
            // Serialization failed; never hand back a half-written document.
            let mut reports = reports;
            reports.push(EmbeddedFontReport::skipped(
                "<document>",
                "",
                format!("the rewritten PDF could not be serialized: {error}"),
            ));
            (pdf_bytes.to_vec(), reports)
        }
    }
}

/// The in-place variant used by the converter, which already holds a parsed
/// [`Document`]. Walks page resources and the AcroForm `/DR` resources.
pub fn embed_missing_fonts_in_document(doc: &mut Document) -> Vec<EmbeddedFontReport> {
    let mut reports: Vec<EmbeddedFontReport> = Vec::new();
    let mut sites: BTreeMap<ObjectId, String> = BTreeMap::new();
    let mut visited_resources: HashSet<ObjectId> = HashSet::new();
    let mut visited_pages = 0usize;

    for (_, page_id) in doc.get_pages() {
        visited_pages += 1;
        if visited_pages > MAX_PAGES || sites.len() >= MAX_FONTS || reports.len() >= MAX_REPORTS {
            break;
        }
        // A broken page costs its own fonts, not the whole conversion.
        let (inline_resources, resource_ids) = match doc.get_page_resources(page_id) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if let Some(resources) = inline_resources {
            collect_font_sites(doc, resources, &mut sites, &mut reports);
        }
        for resource_id in resource_ids {
            if !visited_resources.insert(resource_id) {
                continue;
            }
            if let Ok(resources) = doc.get_dictionary(resource_id) {
                collect_font_sites(doc, resources, &mut sites, &mut reports);
            }
        }
    }

    // Form field appearance streams are generated from the AcroForm's /DR
    // resources, so fonts that only appear there still need a program.
    if let Ok(catalog) = doc.catalog() {
        if let Ok(acro_form) = catalog.get(b"AcroForm") {
            let acro = match acro_form {
                Object::Reference(id) => doc.get_dictionary(*id).ok(),
                Object::Dictionary(dict) => Some(dict),
                _ => None,
            };
            if let Some(acro) = acro {
                if let Ok(Object::Dictionary(default_resources)) = acro.get(b"DR") {
                    collect_font_sites(doc, default_resources, &mut sites, &mut reports);
                }
            }
        }
    }

    for (font_id, resource_name) in sites {
        if reports.len() >= MAX_REPORTS {
            break;
        }
        reports.push(process_font(doc, font_id, &resource_name));
    }
    reports
}

// ---------------------------------------------------------------------------
// Resource walk
// ---------------------------------------------------------------------------

/// Finds the indirect font dictionaries behind a `/Font` sub-dictionary. Only
/// indirect objects can be rewritten in place; an inline font dictionary is
/// reported as skipped rather than silently ignored.
fn collect_font_sites(
    doc: &Document,
    resources: &Dictionary,
    sites: &mut BTreeMap<ObjectId, String>,
    reports: &mut Vec<EmbeddedFontReport>,
) {
    let font_resources = match resources.get(b"Font") {
        Ok(Object::Reference(id)) => doc.get_object(*id).and_then(Object::as_dict).ok(),
        Ok(Object::Dictionary(dict)) => Some(dict),
        _ => None,
    };
    let Some(font_resources) = font_resources else {
        return;
    };
    for (name, value) in font_resources.iter() {
        let resource_name = String::from_utf8_lossy(name).to_string();
        match value {
            Object::Reference(id) => {
                if doc.get_dictionary(*id).is_ok() {
                    sites.entry(*id).or_insert(resource_name);
                } else if reports.len() < MAX_REPORTS {
                    reports.push(EmbeddedFontReport::skipped(
                        &resource_name,
                        "",
                        "the font reference does not resolve to a dictionary",
                    ));
                }
            }
            Object::Dictionary(_)
                if reports.len() < MAX_REPORTS => {
                    reports.push(EmbeddedFontReport::skipped(
                        &resource_name,
                        "",
                        "the font dictionary is written inline; only indirect font objects can be rewritten in place",
                    ));
                }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Per-font processing
// ---------------------------------------------------------------------------

fn process_font(doc: &mut Document, font_id: ObjectId, resource_name: &str) -> EmbeddedFontReport {
    let font = match doc.get_dictionary(font_id) {
        Ok(dict) => dict.clone(),
        Err(error) => {
            return EmbeddedFontReport::skipped(
                resource_name,
                "",
                format!("the font dictionary could not be read: {error}"),
            );
        }
    };
    let base_font = font
        .get(b"BaseFont")
        .ok()
        .and_then(|value| value.as_name().ok())
        .map(|name| String::from_utf8_lossy(name).to_string())
        .unwrap_or_default();
    let subtype = font
        .get(b"Subtype")
        .ok()
        .and_then(|value| value.as_name().ok())
        .map(|name| String::from_utf8_lossy(name).to_string())
        .unwrap_or_default();
    let descriptor = resolve_descriptor(doc, &font);

    // 1. A font that already carries a program passes the validator; leave it.
    if let Some(existing) = &descriptor {
        if has_font_program(existing) {
            return EmbeddedFontReport::skipped(
                resource_name,
                &base_font,
                "the font program is already embedded",
            );
        }
    }

    // 2. Only simple fonts are re-encodable here. Type0/CID text is addressed
    // by CIDs; embedding a Latin program without a CID-to-GID map would render
    // the wrong glyphs, so it is honestly refused.
    if subtype == "Type0" {
        return EmbeddedFontReport::skipped(
            resource_name,
            &base_font,
            "CID/Type0 font: glyph addressing uses CIDs and a substitute needs a CID-to-GID map; re-mapping CID text is out of scope",
        );
    }
    if subtype != "Type1" && subtype != "TrueType" {
        return EmbeddedFontReport::skipped(
            resource_name,
            &base_font,
            format!("font subtype /{subtype} is not a simple Type1/TrueType font"),
        );
    }

    // 3. Encoding. WinAnsi and MacRoman map byte codes to Unicode in a way the
    // substitute program can follow; anything else would change glyphs.
    let encoding = encoding_kind(doc, &font);
    match &encoding {
        EncodingKind::None | EncodingKind::WinAnsi | EncodingKind::MacRoman => {}
        EncodingKind::Custom => {
            return EmbeddedFontReport::skipped(
                resource_name,
                &base_font,
                "the font uses a custom /Differences encoding; glyph re-mapping is out of scope",
            );
        }
        EncodingKind::Other(name) => {
            return EmbeddedFontReport::skipped(
                resource_name,
                &base_font,
                format!("encoding /{name} is neither WinAnsi nor MacRoman"),
            );
        }
    }

    // 4. Symbolic fonts keep their own glyph mapping in the encoding; a Latin
    // substitute would render different characters.
    if let Some(existing) = &descriptor {
        let flags = existing.get(b"Flags").ok().and_then(|value| value.as_i64().ok()).unwrap_or(0);
        if flags & 4 != 0 && flags & 32 == 0 {
            return EmbeddedFontReport::skipped(
                resource_name,
                &base_font,
                "the font descriptor is flagged symbolic; the bundled Latin substitutes have no matching glyph mapping",
            );
        }
    }

    // 5. Pick the substitute by the (subset-prefix stripped) base font name.
    let normalized = normalize_base_font(&base_font);
    let substitute = match substitute_for(&normalized) {
        Ok(substitute) => substitute,
        Err(reason) => return EmbeddedFontReport::skipped(resource_name, &base_font, reason),
    };
    let program = substitute.program();
    let metrics = parse_metrics(program).unwrap_or_else(|| fallback_metrics(substitute));
    let postscript_name = if metrics.postscript_name.is_empty() {
        substitute.postscript_name().to_string()
    } else {
        metrics.postscript_name.clone()
    };

    // 6. Widths. Existing ones are kept untouched (required: content not
    // rescanned). For a font with none (the base-14 often omit /Widths) they
    // are synthesized from the substitute's hmtx, because a TrueType font
    // dictionary with no /Widths is structurally incomplete. MacRoman width
    // synthesis is not implemented, so that case is refused up front.
    let existing_widths = font.get(b"Widths").is_ok();
    if !existing_widths && encoding == EncodingKind::MacRoman {
        return EmbeddedFontReport::skipped(
            resource_name,
            &base_font,
            "the font has no /Widths and uses MacRomanEncoding; MacRoman width synthesis is not implemented",
        );
    }
    let generated_widths = if existing_widths {
        None
    } else {
        synthetic_widths(program, metrics.units_per_em)
    };
    if !existing_widths && generated_widths.is_none() {
        return EmbeddedFontReport::skipped(
            resource_name,
            &base_font,
            "the substitute program could not be parsed for width synthesis",
        );
    }

    // 7. Flags: nonsymbolic, italic and forceBold follow the embedded program;
    // the serif bit follows the replaced family policy (Times). Note that the
    // PT Sans substitute used for Times is in fact a sans-serif design, so the
    // bit records the source style, not the program's own design.
    let mut flags = 32i64;
    if metrics.is_italic || substitute.italic() {
        flags |= 64;
    }
    if metrics.is_bold || substitute.bold() {
        flags |= 262_144;
    }
    if normalized.starts_with("times") {
        flags |= 2;
    }

    // 8. Build the descriptor: keep an existing one, but always point it at
    // the program and give it the substitute's real name, flags and bounding
    // box (those describe the embedded program). Numeric metrics that describe
    // the original face are filled in only when absent.
    let mut descriptor_dict = descriptor.clone().unwrap_or_default();
    descriptor_dict.set("Type", "FontDescriptor");
    descriptor_dict.set("FontName", Object::Name(postscript_name.as_bytes().to_vec()));
    descriptor_dict.set("Flags", flags);
    descriptor_dict.set(
        "FontBBox",
        Object::Array(vec![
            Object::Integer(metrics.x_min as i64),
            Object::Integer(metrics.y_min as i64),
            Object::Integer(metrics.x_max as i64),
            Object::Integer(metrics.y_max as i64),
        ]),
    );
    set_if_missing(&mut descriptor_dict, "ItalicAngle", Object::Real(metrics.italic_angle as f32));
    set_if_missing(&mut descriptor_dict, "Ascent", Object::Integer(metrics.ascender as i64));
    set_if_missing(&mut descriptor_dict, "Descent", Object::Integer(metrics.descender as i64));
    set_if_missing(&mut descriptor_dict, "CapHeight", Object::Integer(metrics.cap_height as i64));
    // StemV is not stored in TrueType fonts (it comes from Type 1 AFM metrics);
    // 80/120 are the conventional estimate, not a value read from the file.
    set_if_missing(
        &mut descriptor_dict,
        "StemV",
        Object::Integer(if metrics.is_bold || substitute.bold() { 120 } else { 80 }),
    );

    // Replace a referenced descriptor in place, or create one when the font
    // dict had none (or pointed at something that is not a dictionary).
    let descriptor_id = match font.get(b"FontDescriptor").ok() {
        Some(Object::Reference(id)) if doc.get_dictionary(*id).is_ok() => {
            if let Ok(existing) = doc.get_dictionary_mut(*id) {
                *existing = descriptor_dict;
            }
            *id
        }
        _ => doc.add_object(Object::Dictionary(descriptor_dict)),
    };

    // 9. The program stream. /Length1 is the length of the uncompressed
    // TrueType program, as the spec requires for FontFile2.
    let program_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! { "Length1" => program.len() as i64 },
        program.to_vec(),
    )));
    if let Ok(descriptor_mut) = doc.get_dictionary_mut(descriptor_id) {
        descriptor_mut.set("FontFile2", Object::Reference(program_id));
    }

    // 10. The font dictionary. A TrueType program belongs to a /TrueType font,
    // so a /Type1 font is relabelled: PDF 32000-1 Table 111 pairs /Type1 with
    // a Type 1 program (FontFile) and /TrueType with FontFile2. BaseFont and
    // the descriptor name become the substitute's real PostScript name so the
    // embedded program is authoritative.
    if let Ok(font_mut) = doc.get_dictionary_mut(font_id) {
        font_mut.set("Subtype", Object::Name(b"TrueType".to_vec()));
        font_mut.set("BaseFont", Object::Name(postscript_name.as_bytes().to_vec()));
        font_mut.set("FontDescriptor", Object::Reference(descriptor_id));
        if encoding == EncodingKind::None {
            font_mut.set("Encoding", Object::Name(b"WinAnsiEncoding".to_vec()));
        }
        if let Some(widths) = generated_widths {
            font_mut.set("FirstChar", 32i64);
            font_mut.set("LastChar", 255i64);
            font_mut.set(
                "Widths",
                Object::Array(widths.into_iter().map(Object::Integer).collect::<Vec<_>>()),
            );
        }
    }

    let style_italic = normalized.contains("italic") || normalized.contains("oblique");
    let warning = if substitute.metric_compatible() {
        None
    } else {
        let mut note = format!(
            "the substitute ({postscript_name}) is not metric-compatible with \"{base_font}\"; the original /Widths were kept, so spacing and line breaks may shift"
        );
        if style_italic && !substitute.italic() {
            note.push_str(", and the italic style is lost");
        }
        Some(note)
    };
    EmbeddedFontReport::embedded(
        resource_name,
        &base_font,
        &postscript_name,
        substitute.metric_compatible(),
        warning,
    )
}

/// Maps a normalized (lowercase, no spaces, `,` -> `-`) base font name to a
/// bundled substitute. `Err` carries an honest reason to report.
fn substitute_for(normalized: &str) -> Result<Substitute, String> {
    if normalized.is_empty() {
        return Err("the font has no usable /BaseFont name".to_string());
    }
    if normalized.starts_with("symbol")
        || normalized.starts_with("zapfdingbats")
        || normalized.starts_with("dingbats")
    {
        return Err(format!(
            "/{normalized} is a symbolic font; the bundled Latin substitutes have no matching glyph mapping"
        ));
    }
    let bold = normalized.contains("bold");
    match normalized {
        "helvetica" | "helv" | "arial" | "arialmt" => Ok(Substitute::LiberationSansRegular),
        "helvetica-bold" | "helv-bold" | "arial-bold" | "arial-boldmt" | "arialbold" => {
            Ok(Substitute::LiberationSansBold)
        }
        "helvetica-oblique" | "helvetica-italic" | "helv-oblique" | "arial-italic" | "arial-italicmt"
        | "arial-oblique" => Ok(Substitute::LiberationSansItalic),
        "helvetica-boldoblique" | "helvetica-bolditalic" | "arial-bolditalic" | "arial-bolditalicmt"
        | "arial-boldoblique" => Ok(Substitute::LiberationSansBoldItalic),
        // Times/Courier and unknown names fall back to PT Sans. This is not a
        // metric match; the caller reports that warning.
        _ => Ok(if bold { Substitute::PtSansBold } else { Substitute::PtSansRegular }),
    }
}

/// Strips a subset prefix (`ABCDEF+`) and normalizes separators.
fn normalize_base_font(name: &str) -> String {
    let trimmed = name.trim();
    let bytes = trimmed.as_bytes();
    let stripped = if bytes.len() > 7 && bytes[6] == b'+' && bytes[..6].iter().all(u8::is_ascii_uppercase)
    {
        &trimmed[7..]
    } else {
        trimmed
    };
    stripped.replace(',', "-").replace(' ', "").to_ascii_lowercase()
}

fn resolve_descriptor(doc: &Document, font: &Dictionary) -> Option<Dictionary> {
    match font.get(b"FontDescriptor").ok()? {
        Object::Reference(id) => doc.get_dictionary(*id).ok().cloned(),
        Object::Dictionary(dict) => Some(dict.clone()),
        _ => None,
    }
}

fn has_font_program(descriptor: &Dictionary) -> bool {
    [b"FontFile".as_slice(), b"FontFile2".as_slice(), b"FontFile3".as_slice()]
        .iter()
        .any(|key| descriptor.get(key).is_ok())
}

fn set_if_missing(dict: &mut Dictionary, key: &str, value: Object) {
    if dict.get(key.as_bytes()).is_err() {
        dict.set(key, value);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum EncodingKind {
    None,
    WinAnsi,
    MacRoman,
    Custom,
    Other(String),
}

fn encoding_kind(doc: &Document, font: &Dictionary) -> EncodingKind {
    let value = match font.get(b"Encoding") {
        Ok(value) => value,
        Err(_) => return EncodingKind::None,
    };
    let name = match value {
        Object::Name(name) => name.clone(),
        Object::Reference(id) => match doc.get_object(*id) {
            Ok(Object::Name(name)) => name.clone(),
            Ok(Object::Dictionary(_)) => return EncodingKind::Custom,
            _ => return EncodingKind::Other("<malformed>".to_string()),
        },
        Object::Dictionary(_) => return EncodingKind::Custom,
        _ => return EncodingKind::Other("<malformed>".to_string()),
    };
    match name.as_slice() {
        b"WinAnsiEncoding" => EncodingKind::WinAnsi,
        b"MacRomanEncoding" => EncodingKind::MacRoman,
        other => EncodingKind::Other(String::from_utf8_lossy(other).to_string()),
    }
}

// ---------------------------------------------------------------------------
// TrueType metrics (bounds-checked; never panics on malformed input)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct FontMetrics {
    units_per_em: u16,
    x_min: i16,
    y_min: i16,
    x_max: i16,
    y_max: i16,
    ascender: i16,
    descender: i16,
    cap_height: i16,
    italic_angle: f64,
    is_bold: bool,
    is_italic: bool,
    postscript_name: String,
}

fn be_u16(data: &[u8], offset: usize) -> Option<u16> {
    let bytes = data.get(offset..offset + 2)?;
    Some(u16::from_be_bytes([bytes[0], bytes[1]]))
}

fn be_i16(data: &[u8], offset: usize) -> Option<i16> {
    be_u16(data, offset).map(|value| value as i16)
}

fn be_u32(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset + 4)?;
    Some(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

/// Reads an sfnt 16.16 fixed point value as f64.
fn be_fixed(data: &[u8], offset: usize) -> Option<f64> {
    be_u32(data, offset).map(|value| (value as i32) as f64 / 65536.0)
}

/// Locates one table of a single-font sfnt (`ttcf` collections are not used by
/// the bundled files and are rejected).
fn find_sfnt_table<'a>(program: &'a [u8], tag: &[u8; 4]) -> Option<&'a [u8]> {
    let version = be_u32(program, 0)?;
    if !matches!(version, 0x0001_0000 | 0x4F54_544F | 0x7472_7565) {
        return None;
    }
    let count = be_u16(program, 4)? as usize;
    if count == 0 || count > 512 {
        return None;
    }
    for index in 0..count {
        let start = 12 + index * 16;
        let record = program.get(start..start + 16)?;
        if &record[0..4] == tag {
            let offset = be_u32(record, 8)? as usize;
            let length = be_u32(record, 12)? as usize;
            return program.get(offset..offset.checked_add(length)?);
        }
    }
    None
}

/// Reads the descriptor-relevant metrics from a bundled TrueType program.
/// ab_glyph validates the program and supplies the hhea ascent/descent and
/// units per em; the rest comes from the sfnt tables directly because
/// ab_glyph exposes no accessor for `FontBBox`, `ItalicAngle` or `CapHeight`.
fn parse_metrics(program: &[u8]) -> Option<FontMetrics> {
    let font = FontRef::try_from_slice(program).ok()?;
    let ab_units = font.units_per_em().unwrap_or(0.0);
    let head = find_sfnt_table(program, b"head")?;
    let table_units = be_u16(head, 18).unwrap_or(0);
    let units_per_em = if (16..=16384).contains(&(table_units as i64)) {
        table_units
    } else if (16.0..=16384.0).contains(&ab_units) {
        ab_units.round() as u16
    } else {
        1000
    };
    let ascender = clamp_i16(font.ascent_unscaled());
    let descender = clamp_i16(font.descent_unscaled());
    let x_min = be_i16(head, 36)?;
    let y_min = be_i16(head, 38)?;
    let x_max = be_i16(head, 40)?;
    let y_max = be_i16(head, 42)?;
    let mac_style = be_u16(head, 44).unwrap_or(0);
    let italic_angle = find_sfnt_table(program, b"post")
        .and_then(|post| be_fixed(post, 4))
        .filter(|value| value.is_finite())
        .unwrap_or(0.0);
    // sCapHeight exists from OS/2 version 2 on. For older tables the 0.7 em
    // heuristic is a documented estimate, not a value read from the file.
    let cap_height = find_sfnt_table(program, b"OS/2")
        .and_then(|os2| {
            let version = be_u16(os2, 0)?;
            if version >= 2 {
                be_i16(os2, 88)
            } else {
                None
            }
        })
        .unwrap_or((ascender as i32 * 7 / 10) as i16);
    Some(FontMetrics {
        units_per_em,
        x_min,
        y_min,
        x_max,
        y_max,
        ascender,
        descender,
        cap_height,
        italic_angle,
        is_bold: mac_style & 1 != 0,
        is_italic: mac_style & 2 != 0,
        postscript_name: read_postscript_name(program).unwrap_or_default(),
    })
}

fn clamp_i16(value: f32) -> i16 {
    if !value.is_finite() {
        return 0;
    }
    value.round().clamp(-32768.0, 32767.0) as i16
}

/// Reads nameID 6 (PostScript name) from the name table, preferring the
/// Windows UTF-16BE record and falling back to the Mac Roman-ASCII one.
fn read_postscript_name(program: &[u8]) -> Option<String> {
    let name = find_sfnt_table(program, b"name")?;
    let count = be_u16(name, 2)? as usize;
    let string_offset = be_u16(name, 4)? as usize;
    if count == 0 || count > 4096 {
        return None;
    }
    let mut mac_fallback: Option<String> = None;
    for index in 0..count {
        let start = 6 + index * 12;
        let record = match name.get(start..start + 12) {
            Some(record) => record,
            None => break,
        };
        let (Some(platform), Some(name_id), Some(length), Some(offset)) = (
            be_u16(record, 0),
            be_u16(record, 6),
            be_u16(record, 8),
            be_u16(record, 10),
        ) else {
            continue;
        };
        let offset = offset as usize;
        if name_id != 6 {
            continue;
        }
        let Some(bytes) = name.get(string_offset + offset..string_offset + offset + length as usize)
        else {
            continue;
        };
        if platform == 3 {
            let units: Vec<u16> = bytes
                .as_chunks::<2>().0.iter()
                .take(256)
                .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
                .collect();
            return Some(String::from_utf16_lossy(&units).trim().to_string());
        }
        if platform == 1 || platform == 0 {
            mac_fallback = Some(bytes.iter().take(256).map(|&byte| byte as char).collect());
        }
    }
    mac_fallback
}

/// Values used only when a bundled program cannot be parsed (a corrupt asset,
/// not attacker input). They are conventional Helvetica-like metrics and are
/// not read from the file; the report keeps naming the substitute honestly.
fn fallback_metrics(substitute: Substitute) -> FontMetrics {
    FontMetrics {
        units_per_em: 1000,
        x_min: -170,
        y_min: -300,
        x_max: 1000,
        y_max: 900,
        ascender: 750,
        descender: -250,
        cap_height: 700,
        italic_angle: if substitute.italic() { -12.0 } else { 0.0 },
        is_bold: substitute.bold(),
        is_italic: substitute.italic(),
        postscript_name: substitute.postscript_name().to_string(),
    }
}

// ---------------------------------------------------------------------------
// Width synthesis for fonts without /Widths
// ---------------------------------------------------------------------------

/// Maps a WinAnsiEncoding byte to its Unicode character. 32..=126 and
/// 160..=255 are identity; 128..=159 are the Windows extensions. Codes 129,
/// 141, 143, 144 and 157 are undefined in WinAnsiEncoding.
fn winansi_char(code: u8) -> Option<char> {
    match code {
        32..=126 | 160..=255 => Some(code as char),
        128 => Some('\u{20AC}'),
        130 => Some('\u{201A}'),
        131 => Some('\u{0192}'),
        132 => Some('\u{201E}'),
        133 => Some('\u{2026}'),
        134 => Some('\u{2020}'),
        135 => Some('\u{2021}'),
        136 => Some('\u{02C6}'),
        137 => Some('\u{2030}'),
        138 => Some('\u{0160}'),
        139 => Some('\u{2039}'),
        140 => Some('\u{0152}'),
        142 => Some('\u{017D}'),
        145 => Some('\u{2018}'),
        146 => Some('\u{2019}'),
        147 => Some('\u{201C}'),
        148 => Some('\u{201D}'),
        149 => Some('\u{2022}'),
        150 => Some('\u{2013}'),
        151 => Some('\u{2014}'),
        152 => Some('\u{02DC}'),
        153 => Some('\u{2122}'),
        154 => Some('\u{0161}'),
        155 => Some('\u{203A}'),
        156 => Some('\u{0153}'),
        158 => Some('\u{017E}'),
        159 => Some('\u{0178}'),
        _ => None,
    }
}

/// Builds `/Widths` for codes 32..=255 (224 entries, 1000-unit glyph space)
/// from the substitute's own horizontal advances.
fn synthetic_widths(program: &[u8], units_per_em: u16) -> Option<Vec<i64>> {
    let font = FontRef::try_from_slice(program).ok()?;
    let upem = (units_per_em.max(1)) as f32;
    let scaled = font.as_scaled(PxScale::from(upem));
    let mut widths = Vec::with_capacity(224);
    for code in 32u16..=255 {
        let width = match winansi_char(code as u8) {
            Some(character) => {
                let glyph = font.glyph_id(character);
                if glyph.0 == 0 {
                    0
                } else {
                    let advance = scaled.h_advance(glyph);
                    if advance.is_finite() && advance > 0.0 {
                        (advance as f64 * 1000.0 / upem as f64).round() as i64
                    } else {
                        0
                    }
                }
            }
            None => 0,
        };
        widths.push(width);
    }
    Some(widths)
}
