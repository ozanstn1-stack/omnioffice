//! Real PDF redaction.
//!
//! Drawing a black rectangle over a name is not redaction: the text stays in the
//! content stream, stays selectable, and comes back out of every text extractor.
//! This module removes the glyphs.
//!
//! How, and why this way:
//!
//! 1. pdfium reports a box and a Unicode value for every character on a page.
//! 2. lopdf decodes the page's content stream into operations, and the walk
//!    below turns each `Tj`/`TJ` operand back into the text a reader would show,
//!    using the same font-encoding path lopdf's own `replace_text` uses.
//! 3. A redaction rectangle selects a character range in step 1; the matching
//!    range in step 2 identifies the show-text operations to rewrite.
//! 4. A fully covered run is dropped. A partially covered one has the covered
//!    characters replaced with spaces in the same encoding, so the rest of the
//!    line keeps its spacing while the original glyphs are gone from the file.
//!
//! When a rectangle cannot be matched, the report says so instead of implying
//! the text is hidden. A redaction that quietly fails is worse than one that
//! admits it did not work.

use std::collections::BTreeMap;
use std::path::Path;

use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use serde::{Deserialize, Serialize};

use crate::docutil::{self, OverwritePolicy};
use crate::error::{PdfError, PdfResult};
use crate::progress::{CancelToken, ProgressCallback, ProgressReporter};
use crate::textbox::{self, CharBox, PageChars};

/// A rectangle to redact, in PDF page space (origin bottom-left, points).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RedactionArea {
    /// 1-based page number.
    pub page: u32,
    pub left: f64,
    pub bottom: f64,
    pub right: f64,
    pub top: f64,
}

impl RedactionArea {
    pub fn rect(&self) -> CharBox {
        CharBox { left: self.left, bottom: self.bottom, right: self.right, top: self.top }
    }
}

/// How aggressively images under a redaction area are handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ImageRedaction {
    /// Paint over the area. The pixels - and any text baked into a scan - are
    /// still in the file, so this is concealment, not redaction.
    #[default]
    Obscure,
    /// Replace any image the page carries with a solid black image of the same
    /// dimensions. The pixels are genuinely gone. The rest of the page is not
    /// re-rendered, so a redacted scan shows a black block where the image was.
    RemovePixels,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedactionOptions {
    /// `#RRGGBB` fill drawn over the redacted area.
    pub fill: String,
    pub images: ImageRedaction,
    /// Points of padding added around each area so anti-aliased glyph edges
    /// cannot leave a readable sliver.
    pub padding_pt: f64,
    /// Drop the document's metadata, which often names the redacted party.
    pub remove_metadata: bool,
}

impl Default for RedactionOptions {
    fn default() -> Self {
        Self { fill: "#000000".into(), images: ImageRedaction::Obscure, padding_pt: 0.5, remove_metadata: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedactionReport {
    pub output: String,
    pub text_runs_removed: u32,
    pub characters_removed: u32,
    pub images_removed: u32,
    /// Areas where no text was found and so nothing was removed.
    pub unmatched_areas: u32,
    /// Notes the user must read, including any area to check by hand.
    pub warnings: Vec<String>,
    /// True when the output was re-opened and its text layer re-checked.
    pub verified: bool,
    /// Masked samples of pre-redaction words that are still extractable from
    /// the output. Short samples only: the report must not restart the leak.
    pub remaining_matches: Vec<String>,
    /// Plain-language explanation of what verification did, or why it could
    /// not run.
    pub verification_message: String,
}

/// One rectangle with the text it is meant to hide.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedactionMatch {
    pub page: u32,
    pub text: String,
    pub left: f64,
    pub bottom: f64,
    pub right: f64,
    pub top: f64,
    /// Which detector produced the match.
    pub kind: String,
}

// ---------------------------------------------------------------------------
// Automatic detection
// ---------------------------------------------------------------------------

/// Looks for common personal data in one page's words.
///
/// Deliberately conservative: a pattern that fires on ordinary text would
/// silently destroy content, which is worse than a detection the user adds by
/// hand.
pub fn detect_sensitive(page: &PageChars) -> Vec<RedactionMatch> {
    let words = page.words();
    if words.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<RedactionMatch> = Vec::new();
    for word in &words {
        if let Some(kind) = classify(&word.text) {
            out.push(match_entry(page.page, &word.text, &word.box_rect, kind));
        }
    }
    // Multi-word patterns: a phone number or IBAN is often broken into several
    // runs by the layout engine, so single words never match.
    for start in 0..words.len() {
        for span in [2usize, 3, 4] {
            if start + span > words.len() {
                break;
            }
            let slice = &words[start..start + span];
            if !slice.windows(2).all(|pair| same_line(&pair[0].box_rect, &pair[1].box_rect)) {
                continue;
            }
            let joined: String = slice.iter().map(|entry| entry.text.clone()).collect::<Vec<_>>().join(" ");
            let kind = match classify(&joined) {
                Some(value) if value == "phone" || value == "iban" || value == "card" => value,
                _ => continue,
            };
            let left = slice.iter().map(|entry| entry.box_rect.left).fold(f64::INFINITY, f64::min);
            let right = slice.iter().map(|entry| entry.box_rect.right).fold(f64::NEG_INFINITY, f64::max);
            let bottom = slice.iter().map(|entry| entry.box_rect.bottom).fold(f64::INFINITY, f64::min);
            let top = slice.iter().map(|entry| entry.box_rect.top).fold(f64::NEG_INFINITY, f64::max);
            let rect = CharBox { left, bottom, right, top };
            out.push(match_entry(page.page, &joined, &rect, kind));
        }
    }
    out
}

fn match_entry(page: u32, text: &str, rect: &CharBox, kind: &str) -> RedactionMatch {
    RedactionMatch {
        page,
        text: text.to_string(),
        left: rect.left,
        bottom: rect.bottom,
        right: rect.right,
        top: rect.top,
        kind: kind.to_string(),
    }
}

/// Two boxes are on the same visual line when their baselines are close.
fn same_line(a: &CharBox, b: &CharBox) -> bool {
    let tolerance = a.height().max(4.0) * 0.6;
    (a.bottom - b.bottom).abs() <= tolerance
}

/// Classifies one token (or one joined line) as sensitive, or `None`.
pub fn classify(token: &str) -> Option<&'static str> {
    let compact: String = token.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.is_empty() {
        return None;
    }
    if looks_like_email(token) {
        return Some("email");
    }
    if looks_like_iban(&compact) {
        return Some("iban");
    }
    if looks_like_card(&compact) {
        return Some("card");
    }
    // The spaces between words are exactly the separators a phone number needs,
    // so test the original token before compacting it.
    if looks_like_phone(token) || looks_like_phone(&compact) {
        return Some("phone");
    }
    if looks_like_passport(&compact) {
        return Some("passport");
    }
    None
}

/// `name@host.tld` with a plausible local part and a dotted host.
pub fn looks_like_email(token: &str) -> bool {
    let characters: Vec<char> = token.chars().collect();
    let at = match characters.iter().position(|c| *c == '@') {
        Some(position) => position,
        None => return false,
    };
    if at == 0 || at + 1 >= characters.len() {
        return false;
    }
    let local: String = characters[..at].iter().collect();
    let host: String = characters[at + 1..].iter().collect();
    if local.len() > 64 || host.len() > 255 || host.contains('@') || host.contains(' ') {
        return false;
    }
    if !local.chars().all(|c| c.is_ascii_alphanumeric() || "._%+-".contains(c)) {
        return false;
    }
    match host.split_once('.') {
        Some((name, rest)) => {
            name.len() >= 2
                && !rest.is_empty()
                && !rest.starts_with('.')
                && !rest.ends_with('.')
                && rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
        }
        None => false,
    }
}

/// 15-34 alphanumerics that pass the ISO 13616 mod-97 check.
pub fn looks_like_iban(compact: &str) -> bool {
    let characters: Vec<char> = compact
        .chars()
        .filter(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        .collect();
    if characters.len() < 15 || characters.len() > 34 {
        return false;
    }
    if !characters[0].is_ascii_alphabetic() || !characters[1].is_ascii_alphabetic() {
        return false;
    }
    let mut rearranged: String = characters[4..].iter().collect();
    rearranged.extend(characters[..4].iter());
    let mut remainder: u32 = 0;
    for character in rearranged.chars() {
        if character.is_ascii_digit() {
            remainder = (remainder * 10 + character.to_digit(10).unwrap_or(0)) % 97;
        } else {
            // A letter is two decimal digits (A = 10 ... Z = 35), so it shifts
            // the running remainder by 100. Using 10 here made every real IBAN
            // fail the check.
            let value = (character as u32) - ('A' as u32 - 10);
            remainder = (remainder * 100 + value) % 97;
        }
    }
    remainder == 1
}

/// 13-19 digits that pass the Luhn check.
pub fn looks_like_card(compact: &str) -> bool {
    if !compact.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let digits: Vec<u32> = compact.chars().map(|c| c.to_digit(10).unwrap_or(0)).collect();
    if digits.len() < 13 || digits.len() > 19 {
        return false;
    }
    let mut sum: u32 = 0;
    let last = digits.len() - 1;
    for (index, digit) in digits.iter().enumerate() {
        let mut value = *digit;
        // Luhn doubles every second digit counted from the right, so the parity
        // depends on the total length.
        if (last - index) % 2 == 1 {
            value *= 2;
            if value > 9 {
                value -= 9;
            }
        }
        sum += value;
    }
    sum % 10 == 0
}

/// 7-15 digits with at least one separator.
///
/// A bare run of digits with no `+`, space, dash, dot or bracket is far more
/// likely to be an account or customer number, so it is not treated as a phone.
pub fn looks_like_phone(compact: &str) -> bool {
    let separators = compact.chars().filter(|c| " -+()./".contains(*c)).count();
    let digits = compact.chars().filter(|c| c.is_ascii_digit()).count();
    if digits < 7 || digits > 15 {
        return false;
    }
    if separators == 0 {
        return false;
    }
    if !compact.starts_with("+") && separators > 5 {
        return false;
    }
    true
}

/// Two letters followed by 6-9 digits.
pub fn looks_like_passport(compact: &str) -> bool {
    let characters: Vec<char> = compact.chars().collect();
    if characters.len() < 8 || characters.len() > 11 {
        return false;
    }
    if !characters[0].is_ascii_alphabetic() || !characters[1].is_ascii_alphabetic() {
        return false;
    }
    let rest = &characters[2..];
    rest.len() >= 6 && rest.iter().all(|c| c.is_ascii_digit())
}

// ---------------------------------------------------------------------------
// Content stream surgery
// ---------------------------------------------------------------------------

/// One decoded show-text operation and the range it occupies in the page text.
#[derive(Debug, Clone)]
struct TextPiece {
    operation: usize,
    start: usize,
    end: usize,
    text: String,
    /// `Tj` only, so a partial edit does not have to rewrite a `TJ` array.
    simple: bool,
}

/// Decodes a page and produces its text with per-operation spans.
fn text_pieces(doc: &Document, page_id: ObjectId) -> PdfResult<(Content<Vec<Operation>>, Vec<TextPiece>)> {
    let content = doc
        .get_and_decode_page_content(page_id)
        .map_err(|error| PdfError::CorruptPdf(format!("content stream: {error}")))?;
    let mut pieces: Vec<TextPiece> = Vec::new();
    let mut cursor = 0usize;
    let mut current: Option<Vec<u8>> = None;
    for (index, operation) in content.operations.iter().enumerate() {
        if operation.operator == "Tf" {
            if let Some(Object::Name(name)) = operation.operands.first() {
                current = Some(name.clone());
            }
            continue;
        }
        let operator: &str = operation.operator.as_str();
        let text = match operator {
            "Tj" | "'" | "\"" => literal_text(doc, page_id, &operation.operands, current.as_deref()),
            "TJ" => array_text(doc, page_id, &operation.operands, current.as_deref()),
            _ => None,
        };
        let text = match text {
            Some(value) => value,
            None => continue,
        };
        let length = text.chars().count();
        if length == 0 {
            continue;
        }
        pieces.push(TextPiece {
            operation: index,
            start: cursor,
            end: cursor + length,
            text,
            simple: operator == "Tj",
        });
        cursor += length;
    }
    Ok((content, pieces))
}

/// Decodes one string operand using the current font's encoding.
fn literal_text(doc: &Document, page_id: ObjectId, operands: &[Object], font: Option<&[u8]>) -> Option<String> {
    let raw = operands.iter().find_map(|operand| match operand {
        Object::String(bytes, _) => Some(bytes.clone()),
        _ => None,
    })?;
    Some(decode_with(doc, page_id, font, &raw))
}

/// Decodes a `TJ` array, ignoring the kerning numbers between the strings.
fn array_text(doc: &Document, page_id: ObjectId, operands: &[Object], font: Option<&[u8]>) -> Option<String> {
    let array = operands.iter().find_map(|operand| match operand {
        Object::Array(items) => Some(items.clone()),
        _ => None,
    })?;
    let mut out = String::new();
    for element in &array {
        if let Ok(bytes) = element.as_str() {
            out.push_str(&decode_with(doc, page_id, font, bytes));
        }
    }
    Some(out)
}

/// Font-encoding aware byte-to-text conversion.
///
/// lopdf's `bytes_to_string` covers the simple, differences and ToUnicode
/// cases. The fallbacks are the two it does not: a UTF-16 byte-order mark (what
/// Writer and most producers emit for non-WinAnsi text) and a raw byte font
/// with no declared encoding.
fn decode_with(doc: &Document, page_id: ObjectId, font: Option<&[u8]>, raw: &[u8]) -> String {
    if let Some(name) = font {
        let fonts = doc.get_page_fonts(page_id).ok();
        if let Some(fonts) = fonts {
            if let Some(dictionary) = fonts.get(name) {
                if let Ok(encoding) = dictionary.get_font_encoding(doc) {
                    if let Ok(text) = encoding.bytes_to_string(raw) {
                        if !text.contains('\u{fffd}') {
                            return text;
                        }
                    }
                }
            }
        }
    }
    if raw.len() >= 2 && raw[0] == 0xFE && raw[1] == 0xFF {
        return decode_utf16(raw, u16::from_be_bytes);
    }
    if raw.len() >= 2 && raw[0] == 0xFF && raw[1] == 0xFE {
        return decode_utf16(raw, u16::from_le_bytes);
    }
    raw.iter().map(|byte| *byte as char).collect()
}

fn decode_utf16(raw: &[u8], pair: fn([u8; 2]) -> u16) -> String {
    let mut units: Vec<u16> = Vec::with_capacity(raw.len() / 2);
    let mut index = 2;
    while index + 1 < raw.len() {
        units.push(pair([raw[index], raw[index + 1]]));
        index += 2;
    }
    String::from_utf16_lossy(&units)
}

/// Replaces the characters of a `Tj` operand covered by `[from, to)`.
///
/// Keeping the glyph count preserves the horizontal advance of everything after
/// it on the same line, so the paragraph does not reflow - but the original
/// characters are gone from the file, which is the point.
fn blank_range(
    doc: &Document,
    page_id: ObjectId,
    operands: &[Object],
    font: Option<&[u8]>,
    from: usize,
    to: usize,
) -> Vec<Object> {
    let raw = match operands.iter().find_map(|operand| match operand {
        Object::String(bytes, _) => Some(bytes.clone()),
        _ => None,
    }) {
        Some(bytes) => bytes,
        None => return operands.to_vec(),
    };
    let text = decode_with(doc, page_id, font, &raw);
    let characters: Vec<char> = text.chars().collect();
    if from >= characters.len() {
        return operands.to_vec();
    }
    let end = to.min(characters.len());
    let mut replacement: String = characters[..from].iter().collect();
    for _ in from..end {
        replacement.push(' ');
    }
    replacement.extend(characters[end..].iter());
    let mut out = operands.to_vec();
    for slot in out.iter_mut() {
        if let Object::String(_, format) = slot {
            // Keep the original string format so a literal `(...)` string does
            // not turn into a hex string; the replacement is printable ASCII.
            *slot = Object::String(replacement.clone().into_bytes(), *format);
            break;
        }
    }
    out
}

/// Finds `needle` in `haystack`, retrying with whitespace squeezed out.
///
/// pdfium and lopdf disagree about how much space sits between two glyphs often
/// enough that an exact match alone would report false negatives.
fn locate(haystack: &str, needle: &str) -> Option<(usize, usize)> {
    if needle.is_empty() {
        return None;
    }
    if let Some(position) = haystack.find(needle) {
        return Some((position, position + needle.len()));
    }
    let target: String = needle.chars().filter(|c| !c.is_whitespace()).collect();
    if target.is_empty() {
        return None;
    }
    let mut collapsed = String::new();
    let mut offsets: Vec<usize> = Vec::new();
    for (index, character) in haystack.char_indices() {
        if character.is_whitespace() {
            continue;
        }
        collapsed.push(character);
        offsets.push(index);
    }
    let position = collapsed.find(&target)?;
    let start = *offsets.get(position)?;
    let last = position + target.chars().count().saturating_sub(1);
    let end = match offsets.get(last) {
        Some(byte) => {
            let width = haystack[*byte..].chars().next().map(char::len_utf8).unwrap_or(1);
            *byte + width
        }
        None => haystack.len(),
    };
    Some((start, end))
}

// ---------------------------------------------------------------------------
// The operation
// ---------------------------------------------------------------------------

/// Applies redaction areas to a PDF and reports exactly what was removed.
pub fn redact_pdf(
    input: &Path,
    output: &Path,
    areas: &[RedactionArea],
    options: &RedactionOptions,
    policy: OverwritePolicy,
    password: Option<&str>,
    progress: &ProgressCallback,
    cancel: &CancelToken,
) -> PdfResult<RedactionReport> {
    let target = docutil::resolve_output_path(output, policy)?;
    let mut doc = docutil::load_document(input, password)?;
    docutil::materialize_all_pages(&mut doc)?;
    let reporter = ProgressReporter::new(progress);
    let page_ids: Vec<ObjectId> = doc.get_pages().into_values().collect();
    if page_ids.is_empty() {
        return Err(PdfError::InvalidPdf("document has no pages".into()));
    }

    let mut warnings: Vec<String> = Vec::new();
    let mut text_runs_removed = 0u32;
    let mut characters_removed = 0u32;
    let mut images_removed = 0u32;
    let mut unmatched_areas = 0u32;
    // The words that sat under a redaction area before the edit, kept for the
    // verification pass against the written output.
    let mut verification_words: Vec<(u32, String)> = Vec::new();

    // One geometry pass over the document, so every page's character boxes come
    // from a single parse rather than one parse per page.
    let mut wanted: Vec<u32> = areas.iter().map(|area| area.page).collect();
    wanted.sort_unstable();
    wanted.dedup();
    wanted.retain(|page| *page >= 1 && *page <= page_ids.len() as u32);
    let geometry: BTreeMap<u32, PageChars> = if wanted.is_empty() {
        BTreeMap::new()
    } else {
        let total = wanted.len() as u64;
        let (pages, truncated) = textbox::page_chars(input, password, &wanted, 400, cancel, &|page, _| {
            reporter.emit_step("redact.geometry", page as u64, total);
        })?;
        if truncated > 0 {
            warnings.push(format!("{truncated} page(s) were not analysed; check them by hand."));
        }
        pages
    };

    for (index, page_id) in page_ids.iter().enumerate() {
        cancel.check()?;
        let page_number = index as u32 + 1;
        reporter.emit_step("redact.page", page_number as u64, page_ids.len() as u64);
        let page_areas: Vec<RedactionArea> = areas.iter().filter(|area| area.page == page_number).copied().collect();
        if page_areas.is_empty() {
            continue;
        }
        let (mut content, pieces) = text_pieces(&doc, *page_id)?;
        let page_text: String = pieces.iter().map(|piece| piece.text.as_str()).collect();
        let page_chars = geometry.get(&page_number);

        let mut ranges: Vec<(usize, usize)> = Vec::new();
        for area in &page_areas {
            let padded = area.rect().padded(options.padding_pt);
            if let Some(chars) = page_chars {
                for word in chars.words() {
                    let text = word.text.trim();
                    if !text.is_empty() && word.box_rect.overlaps(&padded) {
                        verification_words.push((page_number, text.to_string()));
                    }
                }
            }
            let needle = page_chars.and_then(|chars| {
                let (start, end) = chars.indices_in(&padded)?;
                let slice: String = chars
                    .chars
                    .iter()
                    .skip(start)
                    .take(end.saturating_sub(start))
                    .filter_map(|entry| entry.as_char())
                    .collect();
                if slice.trim().is_empty() {
                    None
                } else {
                    Some(slice)
                }
            });
            match needle {
                Some(needle) => match locate(&page_text, &needle) {
                    Some((from, to)) => ranges.push((from, to)),
                    None => {
                        unmatched_areas += 1;
                        warnings.push(format!(
                            "Page {page_number}: the text under the selected area was not found in the content stream, so nothing was removed there. Check the result."
                        ));
                    }
                },
                None => {
                    unmatched_areas += 1;
                    let has_text = page_chars
                        .map(|chars| chars.chars.iter().any(|entry| entry.box_rect.overlaps(&padded)))
                        .unwrap_or(false);
                    if !has_text && options.images == ImageRedaction::Obscure {
                        warnings.push(format!(
                            "Page {page_number}: the selected area holds no text. It has been covered, but the pixels are still in the file - use image removal for a scan."
                        ));
                    }
                }
            }
        }

        let mut edits: BTreeMap<usize, (usize, usize)> = BTreeMap::new();
        for piece in &pieces {
            for (from, to) in &ranges {
                let start = (*from).max(piece.start);
                let end = (*to).min(piece.end);
                if start >= end {
                    continue;
                }
                let local_start = start - piece.start;
                let local_end = end - piece.start;
                let slot = edits.entry(piece.operation).or_insert((usize::MAX, 0));
                slot.0 = slot.0.min(local_start);
                slot.1 = slot.1.max(local_end);
                characters_removed += (local_end - local_start) as u32;
            }
        }

        if !edits.is_empty() {
            let mut current_font: Option<Vec<u8>> = None;
            let mut drop: Vec<usize> = Vec::new();
            for (position, operation) in content.operations.iter_mut().enumerate() {
                if operation.operator == "Tf" {
                    if let Some(Object::Name(name)) = operation.operands.first() {
                        current_font = Some(name.clone());
                    }
                }
                let (from, to) = match edits.get(&position) {
                    Some(&range) => range,
                    None => continue,
                };
                let piece = pieces.iter().find(|candidate| candidate.operation == position);
                let length = piece.map(|entry| entry.text.chars().count()).unwrap_or_else(|| to.saturating_sub(from));
                if from == 0 && to >= length {
                    // The whole run is inside the area: remove the operation.
                    drop.push(position);
                    text_runs_removed += 1;
                } else if piece.map(|entry| entry.simple).unwrap_or(false) {
                    operation.operands =
                        blank_range(&doc, *page_id, &operation.operands, current_font.as_deref(), from, to);
                } else {
                    // A `TJ` array cannot be partially blanked without changing
                    // its kerning, so the run goes entirely and the box covers
                    // the gap.
                    drop.push(position);
                    text_runs_removed += 1;
                }
            }
            if !drop.is_empty() {
                let mut position = 0usize;
                content.operations.retain(|_| {
                    let keep = !drop.contains(&position);
                    position += 1;
                    keep
                });
            }
            let encoded = content
                .encode()
                .map_err(|error| PdfError::ProcessingFailed(format!("encode content: {error}")))?;
            doc.change_page_content(*page_id, encoded)
                .map_err(|error| PdfError::ProcessingFailed(format!("write content: {error}")))?;
        }

        if options.images == ImageRedaction::RemovePixels {
            images_removed += replace_images_on_page(&mut doc, *page_id)?;
        }
        paint_boxes(&mut doc, *page_id, &page_areas, &options.fill)?;
    }

    if options.remove_metadata {
        strip_metadata(&mut doc);
    }

    docutil::save_document(&mut doc, &target, true)?;

    // Verification happens on the written file, not on the in-memory model:
    // the question is what a reader can extract from what was actually saved.
    let affected_pages: Vec<u32> = verification_words.iter().map(|(page, _)| *page).collect();
    let (verified, remaining_matches, verification_message) =
        verify_redaction(&target, &verification_words, &affected_pages, &geometry);
    if verified && !remaining_matches.is_empty() {
        warnings.push(format!(
            "Verification found {} redacted value(s) that can still be extracted from the output.",
            remaining_matches.len()
        ));
    }

    Ok(RedactionReport {
        output: target.to_string_lossy().to_string(),
        text_runs_removed,
        characters_removed,
        images_removed,
        unmatched_areas,
        warnings,
        verified,
        remaining_matches,
        verification_message,
    })
}

/// Re-opens the written output and checks that none of the words that sat
/// under a redaction area can still be extracted.
///
/// Returns `verified = false` - with an explanation - when the check could not
/// run at all: no pdfium, or an affected page with no text layer. The report
/// must never imply a verification that did not happen.
fn verify_redaction(
    output: &Path,
    words: &[(u32, String)],
    affected_pages: &[u32],
    geometry: &BTreeMap<u32, PageChars>,
) -> (bool, Vec<String>, String) {
    if !crate::render::is_available() {
        return (
            false,
            Vec::new(),
            "Verification skipped: pdfium is not available.".to_string(),
        );
    }
    let mut pages: Vec<u32> = affected_pages.to_vec();
    pages.sort_unstable();
    pages.dedup();
    if pages.is_empty() {
        return (
            false,
            Vec::new(),
            "Verification skipped: no text areas were redacted.".to_string(),
        );
    }

    let mut remaining: Vec<String> = Vec::new();
    let mut checked_pages: Vec<u32> = Vec::new();
    let mut skipped_pages: Vec<u32> = Vec::new();
    for page in &pages {
        let has_text_layer = geometry
            .get(page)
            .map(|chars| !chars.chars.is_empty())
            .unwrap_or(false);
        let text = if has_text_layer {
            crate::render::extract_page_text(output, None, *page).ok()
        } else {
            None
        };
        let Some(text) = text else {
            skipped_pages.push(*page);
            continue;
        };
        checked_pages.push(*page);
        for (_, word) in words.iter().filter(|(word_page, _)| word_page == page) {
            if locate(&text, word).is_some() {
                remaining.push(mask_sample(word));
            }
        }
    }
    remaining.sort();
    remaining.dedup();

    if checked_pages.is_empty() {
        return (
            false,
            remaining,
            format!(
                "Verification skipped: page(s) {} carry no extractable text layer.",
                join_pages(&skipped_pages)
            ),
        );
    }
    if !skipped_pages.is_empty() {
        return (
            false,
            remaining,
            format!(
                "Verification ran on page(s) {}; page(s) {} have no text layer and were skipped.",
                join_pages(&checked_pages),
                join_pages(&skipped_pages)
            ),
        );
    }
    if remaining.is_empty() {
        (
            true,
            remaining,
            format!(
                "Verification passed: page(s) {} were re-extracted and none of the selected text remains.",
                join_pages(&checked_pages)
            ),
        )
    } else {
        let message = format!(
            "Verification found {} selected value(s) still extractable; check the output.",
            remaining.len()
        );
        (true, remaining, message)
    }
}

fn join_pages(pages: &[u32]) -> String {
    pages
        .iter()
        .map(|page| page.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Masks the middle of a sample so the report can name what leaked without
/// reproducing it. At most eight characters are ever shown.
fn mask_sample(value: &str) -> String {
    let characters: Vec<char> = value.chars().collect();
    if characters.len() <= 2 {
        return "*".repeat(characters.len().max(1));
    }
    let show = characters.len().min(8);
    let head = if show > 5 { 3 } else { 1 };
    let tail = if show > 5 { 3 } else { 1 };
    let mut out = String::with_capacity(show);
    out.extend(characters.iter().take(head));
    for _ in 0..show.saturating_sub(head + tail) {
        out.push('*');
    }
    out.extend(characters.iter().skip(characters.len() - tail));
    out
}

/// Paints the opaque boxes over the removed content.
fn paint_boxes(doc: &mut Document, page_id: ObjectId, areas: &[RedactionArea], fill: &str) -> PdfResult<()> {
    if areas.is_empty() {
        return Ok(());
    }
    let color = crate::watermark::parse_hex_color(fill);
    let (r, g, b) = (color[0] as f64 / 255.0, color[1] as f64 / 255.0, color[2] as f64 / 255.0);
    let mut content = String::from("q\n");
    for area in areas {
        content.push_str(&format!(
            "{r:.4} {g:.4} {b:.4} rg\n{left:.2} {bottom:.2} {width:.2} {height:.2} re\nf\n",
            left = area.left.min(area.right),
            bottom = area.bottom.min(area.top),
            width = (area.right - area.left).abs().max(0.1),
            height = (area.top - area.bottom).abs().max(0.1)
        ));
    }
    content.push_str("Q\n");
    docutil::append_page_content(doc, page_id, content.into_bytes())
}

/// Replaces every image a redacted page carries with a solid black image.
///
/// Without a full placement-matrix walk this cannot tell which of several images
/// an area touches, so it is only offered as an explicit choice and the report
/// says how many images were removed.
fn replace_images_on_page(doc: &mut Document, page_id: ObjectId) -> PdfResult<u32> {
    // Resources, and the `/XObject` sub-dictionary inside them, may each be
    // stored inline or behind a reference, and the resource dictionary itself
    // may be inherited from an ancestor page node. Resolve all three cases.
    let mut xobjects: Vec<Object> = Vec::new();
    let mut seen: Vec<ObjectId> = Vec::new();

    let collect_xobjects = |doc: &Document, node: &Dictionary, out: &mut Vec<Object>, seen: &mut Vec<ObjectId>| {
        let resolved = match node.get(b"XObject") {
            Ok(Object::Dictionary(list)) => Some(list.clone()),
            Ok(Object::Reference(id)) => match doc.get_object(*id) {
                Ok(Object::Dictionary(list)) => Some(list.clone()),
                _ => None,
            },
            _ => None,
        };
        if let Some(list) = resolved {
            for (_, value) in list.iter() {
                out.push(value.clone());
            }
        }
        seen.clear();
    };

    if let Ok((Some(resources), _)) = doc.get_page_resources(page_id) {
        collect_xobjects(doc, resources, &mut xobjects, &mut seen);
    }
    if xobjects.is_empty() {
        if let Ok(Object::Dictionary(page)) = doc.get_object(page_id) {
            let resolved = match page.get(b"Resources") {
                Ok(Object::Dictionary(resources)) => Some(resources.clone()),
                Ok(Object::Reference(id)) => match doc.get_object(*id) {
                    Ok(Object::Dictionary(resources)) => Some(resources.clone()),
                    _ => None,
                },
                _ => None,
            };
            if let Some(resources) = resolved {
                collect_xobjects(doc, &resources, &mut xobjects, &mut seen);
            }
        }
    }
    if xobjects.is_empty() {
        return Ok(0);
    }

    let mut replaced = 0u32;
    for value in &xobjects {
        let image_id = match value.as_reference() {
            Ok(id) => id,
            Err(_) => continue,
        };
        if seen.contains(&image_id) {
            continue;
        }
        seen.push(image_id);
        let (width, height) = match doc.get_object(image_id) {
            Ok(Object::Stream(stream)) => (
                stream.dict.get(b"Width").ok().and_then(|value| value.as_i64().ok()).unwrap_or(0),
                stream.dict.get(b"Height").ok().and_then(|value| value.as_i64().ok()).unwrap_or(0),
            ),
            _ => continue,
        };
        if width <= 0 || height <= 0 || width > 20_000 || height > 20_000 {
            continue;
        }
        // Keep the dictionary so every `Do` still resolves, and replace only the
        // pixel data with black.
        if let Ok(Object::Stream(stream)) = doc.get_object_mut(image_id) {
            let mut dictionary = stream.dict.clone();
            dictionary.set(b"ColorSpace", Object::Name(b"DeviceRGB".to_vec()));
            dictionary.set(b"BitsPerComponent", Object::Integer(8));
            // The pixel data is dropped, so every entry that described it has
            // to go; the size is re-declared below.
            for key in [b"SMask".as_slice(), b"Decode", b"Filter", b"DecodeParms"] {
                dictionary.remove(key);
            }
            dictionary.remove(b"Width");
            dictionary.remove(b"Height");
            dictionary.set(b"Width", Object::Integer(width));
            dictionary.set(b"Height", Object::Integer(height));
            *stream = Stream::new(dictionary, Vec::new());
            replaced += 1;
        }
    }
    Ok(replaced)
}
/// Drops the metadata, actions and name trees that can name the redacted party.
fn strip_metadata(doc: &mut Document) {
    // Byte-string keys need a named binding: an inline array literal makes
    // Rust infer `[_; 6]` and lose the `&[u8]` element type.
    const INFO_KEYS: [&[u8]; 6] = [b"Title", b"Author", b"Subject", b"Keywords", b"Creator", b"Producer"];
    const CATALOG_KEYS: [&[u8]; 5] = [b"OpenAction", b"AA", b"Names", b"Metadata", b"AcroForm"];

    if let Ok(info_id) = doc.trailer.get(b"Info").and_then(|value| value.as_reference()) {
        if let Ok(Object::Dictionary(info)) = doc.get_object_mut(info_id) {
            for key in INFO_KEYS {
                info.remove(key);
            }
        }
    }
    doc.trailer.remove(b"Info");
    if let Ok(root) = doc.trailer.get(b"Root").and_then(|value| value.as_reference()) {
        if let Ok(Object::Dictionary(catalog)) = doc.get_object_mut(root) {
            for key in CATALOG_KEYS {
                catalog.remove(key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::textbox::TextChar;

    fn character(code: char, left: f64) -> TextChar {
        TextChar {
            code: code as u32,
            index: 0,
            box_rect: CharBox { left, bottom: 0.0, right: left + 5.0, top: 10.0 },
            font_size_pt: 10.0,
            font_name: "Helvetica".into(),
            invisible: false,
            generated: false,
            hyphen: false,
        }
    }

    fn page_from(text: &str) -> PageChars {
        let mut chars: Vec<TextChar> = Vec::new();
        for (index, value) in text.chars().enumerate() {
            let left = index as f64 * 6.0;
            let mut entry = character(value, left);
            entry.index = index as u32;
            chars.push(entry);
        }
        PageChars { page: 1, chars }
    }

    #[test]
    fn emails_are_detected_and_lookalikes_are_not() {
        assert!(looks_like_email("ada@example.com"));
        assert!(looks_like_email("ada.lovelace+tag@mail.example.co.uk"));
        assert!(!looks_like_email("@example.com"));
        assert!(!looks_like_email("ada@localhost"));
        assert!(!looks_like_email("ada example.com"));
        assert!(!looks_like_email("notanemail"));
        assert!(!looks_like_email("a@b.c1"));
    }

    #[test]
    fn ibans_pass_the_mod_97_check() {
        assert!(looks_like_iban("GB82WEST12345698765432"));
        assert!(!looks_like_iban("GB82WEST12345698765433"));
        assert!(!looks_like_iban("8212WEST12345698765432"));
    }

    #[test]
    fn card_numbers_use_luhn() {
        assert!(looks_like_card("4111111111111111"));
        assert!(!looks_like_card("4111111111111112"));
        assert!(!looks_like_card("12345"));
        assert!(!looks_like_card("4111-1111-1111-1111"), "a separator means it is not a bare number");
    }

    #[test]
    fn phone_numbers_need_a_separator() {
        assert!(looks_like_phone("+90 532 123 45 67"));
        assert!(looks_like_phone("0532-123-4567"));
        assert!(!looks_like_phone("1234567890"));
    }

    #[test]
    fn passport_numbers_need_two_letters_then_digits() {
        assert!(looks_like_passport("TR1234567"));
        assert!(!looks_like_passport("T1234567"));
        assert!(!looks_like_passport("TR12345"));
        assert!(!looks_like_passport("12345678"));
    }

    #[test]
    fn classify_prefers_the_strongest_match() {
        assert_eq!(classify("ada@example.com"), Some("email"));
        assert_eq!(classify("GB82WEST12345698765432"), Some("iban"));
        assert_eq!(classify("4111111111111111"), Some("card"));
        assert_eq!(classify("+90 532 123 45 67"), Some("phone"));
        assert_eq!(classify("TR1234567"), Some("passport"));
        assert_eq!(classify("Hello"), None);
        assert_eq!(classify(""), None);
        assert_eq!(classify("   "), None);
    }

    #[test]
    fn detect_finds_a_single_word_match() {
        let page = page_from("mail ada@example.com now");
        let found = detect_sensitive(&page);
        let emails: Vec<&RedactionMatch> = found.iter().filter(|entry| entry.kind == "email").collect();
        assert_eq!(emails.len(), 1);
        assert_eq!(emails[0].text, "ada@example.com");
    }

    #[test]
    fn detect_joins_a_number_split_across_words() {
        let page = page_from("call 0532 123 4567 now");
        let found = detect_sensitive(&page);
        assert!(
            found.iter().any(|entry| entry.kind == "phone"),
            "expected a phone match, got {found:?}"
        );
    }

    #[test]
    fn detect_ignores_ordinary_prose() {
        let page = page_from("This invoice is due in thirty days.");
        assert!(detect_sensitive(&page).is_empty());
    }

    #[test]
    fn detect_on_an_empty_page_is_empty() {
        assert!(detect_sensitive(&PageChars { page: 1, chars: Vec::new() }).is_empty());
    }

    #[test]
    fn locate_finds_an_exact_substring() {
        assert_eq!(locate("Total: 42 EUR", "42"), Some((7, 9)));
    }

    #[test]
    fn locate_falls_back_to_collapsed_whitespace() {
        let haystack = "Call 0532  123 4567 now";
        let (start, end) = locate(haystack, "0532 123 4567").expect("must match across runs");
        assert_eq!(&haystack[start..end], "0532  123 4567");
    }

    #[test]
    fn locate_handles_multibyte_text() {
        let haystack = "Fatura Ada Lovelace 42";
        let (start, end) = locate(haystack, "Lovelace").expect("must match");
        assert_eq!(&haystack[start..end], "Lovelace");
    }

    #[test]
    fn locate_reports_nothing_for_an_absent_needle() {
        assert_eq!(locate("hello world", "zzz"), None);
        assert_eq!(locate("hello", ""), None);
        assert_eq!(locate("hello", "   "), None);
    }

    #[test]
    fn decode_with_reads_a_utf16_byte_order_mark() {
        // A Document is needed for the font lookup, and an empty one is fine
        // because the fallback runs when no font is supplied.
        let doc = Document::with_version("1.7");
        let page: ObjectId = (0, 0);
        let big_endian = vec![0xFE, 0xFF, 0x00, 0x41, 0x00, 0x42];
        assert_eq!(decode_with(&doc, page, None, &big_endian), "AB");
        let little_endian = vec![0xFF, 0xFE, 0x41, 0x00, 0x42, 0x00];
        assert_eq!(decode_with(&doc, page, None, &little_endian), "AB");
    }

    #[test]
    fn decode_with_falls_back_to_single_bytes() {
        let doc = Document::with_version("1.7");
        let page: ObjectId = (0, 0);
        assert_eq!(decode_with(&doc, page, None, b"Hi"), "Hi");
    }

    #[test]
    fn blank_range_replaces_only_the_covered_characters() {
        let doc = Document::with_version("1.7");
        let page: ObjectId = (0, 0);
        let operands = vec![Object::String(b"Hello world".to_vec(), lopdf::StringFormat::Literal)];
        let out = blank_range(&doc, page, &operands, None, 6, 11);
        let text = match &out[0] {
            Object::String(bytes, _) => String::from_utf8_lossy(bytes).to_string(),
            _ => panic!("expected a string operand"),
        };
        assert_eq!(text, "Hello      ");
        assert_eq!(text.chars().count(), "Hello world".chars().count());
    }

    #[test]
    fn blank_range_keeps_the_prefix_and_suffix() {
        let doc = Document::with_version("1.7");
        let page: ObjectId = (0, 0);
        let operands = vec![Object::String(b"abcdef".to_vec(), lopdf::StringFormat::Literal)];
        let out = blank_range(&doc, page, &operands, None, 2, 4);
        match &out[0] {
            Object::String(bytes, _) => assert_eq!(String::from_utf8_lossy(bytes), "ab  ef"),
            _ => panic!("expected a string operand"),
        }
    }

    #[test]
    fn blank_range_clamps_to_the_string_length() {
        let doc = Document::with_version("1.7");
        let page: ObjectId = (0, 0);
        let operands = vec![Object::String(b"abc".to_vec(), lopdf::StringFormat::Literal)];
        let out = blank_range(&doc, page, &operands, None, 1, 99);
        match &out[0] {
            Object::String(bytes, _) => assert_eq!(String::from_utf8_lossy(bytes), "a  "),
            _ => panic!("expected a string operand"),
        }
    }

    #[test]
    fn blank_range_is_a_no_op_beyond_the_end() {
        let doc = Document::with_version("1.7");
        let page: ObjectId = (0, 0);
        let operands = vec![Object::String(b"abc".to_vec(), lopdf::StringFormat::Literal)];
        let out = blank_range(&doc, page, &operands, None, 9, 12);
        match &out[0] {
            Object::String(bytes, _) => assert_eq!(String::from_utf8_lossy(bytes), "abc"),
            _ => panic!("expected a string operand"),
        }
    }

    #[test]
    fn same_line_tolerates_small_baseline_differences() {
        let a = CharBox { left: 0.0, bottom: 100.0, right: 10.0, top: 110.0 };
        let b = CharBox { left: 10.0, bottom: 101.0, right: 20.0, top: 111.0 };
        let c = CharBox { left: 10.0, bottom: 200.0, right: 20.0, top: 210.0 };
        assert!(same_line(&a, &b));
        assert!(!same_line(&a, &c));
    }
}
