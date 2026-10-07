//! Glyph-usage scan for font subsetting, plus the ToUnicode CMap reader the
//! Type0 path needs to turn CIDs back into characters.
//!
//! [`crate::fontembed`] may subset a substitute program only when it knows
//! every character code the document shows with that font. This module
//! interprets the text-showing operators (`Tj`, `TJ`, `'`, `"`) of every page
//! content stream, every form XObject reached through `Do` (recursively, with
//! the caller's current font inherited), every tiling pattern reached through
//! `scn`/`SCN`, and every annotation appearance stream (`/AP` `/N`, `/R`,
//! `/D`, including appearance-state sub-dictionaries). The current font
//! follows `Tf`, an ExtGState `/Font` applied with `gs`, and `q`/`Q`.
//!
//! Codes are recorded raw: one byte per code for simple fonts, two bytes for
//! Type0 fonts with the Identity-H/V CMap. Turning them into glyph ids is the
//! caller's job because it depends on the substitute program.
//!
//! The answer is deliberately conservative. A font's usage is reported as
//! unknown, and the caller then embeds the full program, when:
//! * a content stream that can select the font fails strict parsing or cannot
//!   be decompressed, or the scan's work budget runs out;
//! * the font is listed in the AcroForm default resources (`/DR`): a viewer
//!   regenerating a field appearance may show any character with it;
//! * any object the scan did not interpret refers to the font, for example the
//!   resources of a Type 3 glyph procedure or a form XObject no page draws.
//!   Every indirect reference to the font anywhere in the file is matched
//!   against the resource dictionaries the scan actually walked.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use lopdf::content::Content;
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

/// Upper bound for all content the scan decompresses (bomb protection).
const MAX_DECOMPRESSED: usize = 256 * 1024 * 1024;
/// Form XObject nesting the scan follows; deeper nesting is "unknown".
const MAX_FORM_DEPTH: usize = 32;
/// Distinct form/pattern/appearance runs before the scan gives up.
const MAX_FORM_RUNS: usize = 50_000;
const MAX_PAGES: usize = 100_000;
/// Page-tree depth followed when collecting inherited resources.
const MAX_TREE_DEPTH: usize = 64;
/// Object nesting followed by the reference walk; lopdf's parser already
/// bounds nesting well below this.
const MAX_REFERENCE_DEPTH: usize = 256;
/// Mapping steps a ToUnicode CMap may cost before it is treated as hostile.
const MAX_CMAP_WORK: usize = 1 << 20;
/// Longest ToUnicode target kept (UTF-16 units); ligatures need 3.
const MAX_CMAP_TARGET: usize = 32;

const DEFAULT_RESOURCES_REASON: &str =
    "the font is listed in the AcroForm default resources, so a form field may show any character with it";
const UNREAD_REFERENCE_REASON: &str =
    "the font is referenced from an object the usage scan does not interpret (for example a Type 3 glyph procedure or a form XObject no page draws)";

/// What the scan learned about one font.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FontUsage {
    /// Every code the document shows with the font: single bytes for simple
    /// fonts, two-byte codes (CIDs) for Identity-H/V Type0 fonts. Empty when
    /// the font is listed but never used.
    Codes(BTreeSet<u16>),
    /// Usage could not be determined reliably; the reason is reportable.
    Unknown(String),
}

/// The owned result of [`scan_document`].
pub(crate) struct UsageScan {
    codes: HashMap<ObjectId, BTreeSet<u16>>,
    uncertain: HashMap<ObjectId, String>,
    /// Font -> the indirect objects whose references to it the scan walked.
    covered: HashMap<ObjectId, HashSet<ObjectId>>,
    default_resource_fonts: HashSet<ObjectId>,
    aborted: Option<String>,
    /// Resource dictionaries of the form XObjects, tiling patterns and
    /// appearance streams that were interpreted. Their fonts need a program
    /// just like page fonts do.
    pub(crate) extra_resources: Vec<Dictionary>,
}

impl UsageScan {
    /// Resolves the usage of each font in `fonts`, applying the conservative
    /// rules from the module documentation.
    pub(crate) fn usage(&self, doc: &Document, fonts: &[ObjectId]) -> HashMap<ObjectId, FontUsage> {
        let targets: HashSet<ObjectId> = fonts.iter().copied().collect();
        let holders = reference_holders(doc, &targets);
        let mut out: HashMap<ObjectId, FontUsage> = HashMap::with_capacity(fonts.len());
        for font in fonts {
            let unread_holder = holders.get(font).is_some_and(|holders| {
                holders.iter().any(|holder| !self.covered.get(font).is_some_and(|covered| covered.contains(holder)))
            });
            let usage = if let Some(reason) = &self.aborted {
                FontUsage::Unknown(reason.clone())
            } else if self.default_resource_fonts.contains(font) {
                FontUsage::Unknown(DEFAULT_RESOURCES_REASON.to_string())
            } else if let Some(reason) = self.uncertain.get(font) {
                FontUsage::Unknown(reason.clone())
            } else if unread_holder {
                FontUsage::Unknown(UNREAD_REFERENCE_REASON.to_string())
            } else {
                FontUsage::Codes(self.codes.get(font).cloned().unwrap_or_default())
            };
            out.insert(*font, usage);
        }
        out
    }
}

/// Walks every page, its annotation appearances and everything they draw.
/// Never fails: anything it cannot read makes the affected fonts "unknown".
pub(crate) fn scan_document(doc: &Document) -> UsageScan {
    let mut scanner = Scanner::new(doc);
    let default_resources = scanner.default_resources();
    if let Some(level) = default_resources {
        scanner.default_resource_fonts = scanner.chain_fonts(&[level]).into_iter().map(|(font, _)| font).collect();
    }
    let fallback: Vec<ResourceLevel<'_>> = default_resources.into_iter().collect();
    let pages = doc.get_pages();
    if pages.len() > MAX_PAGES {
        scanner.aborted = Some("the document has more pages than the usage scan covers".to_string());
    }
    for page_id in pages.values() {
        if scanner.aborted.is_some() {
            break;
        }
        scanner.scan_page(*page_id, &fallback);
    }
    UsageScan {
        codes: scanner.codes,
        uncertain: scanner.uncertain,
        covered: scanner.covered,
        default_resource_fonts: scanner.default_resource_fonts,
        aborted: scanner.aborted,
        extra_resources: scanner.extra_resources,
    }
}

/// One resource dictionary in a lookup chain, with the indirect object that
/// holds it (the dictionary's own id, or the page/stream it is inlined in).
#[derive(Clone, Copy)]
struct ResourceLevel<'a> {
    dict: &'a Dictionary,
    container: ObjectId,
}

#[derive(Clone, Copy)]
enum CodeWidth {
    One,
    Two,
    /// A Type0 font with a CMap other than Identity-H/V.
    Unknown,
}

struct Scanner<'a> {
    doc: &'a Document,
    codes: HashMap<ObjectId, BTreeSet<u16>>,
    uncertain: HashMap<ObjectId, String>,
    covered: HashMap<ObjectId, HashSet<ObjectId>>,
    default_resource_fonts: HashSet<ObjectId>,
    widths: HashMap<ObjectId, CodeWidth>,
    visited: HashSet<(ObjectId, Option<ObjectId>, Option<ObjectId>)>,
    extra_resources: Vec<Dictionary>,
    extra_containers: HashSet<ObjectId>,
    form_runs: usize,
    decompressed: usize,
    aborted: Option<String>,
}

impl<'a> Scanner<'a> {
    fn new(doc: &'a Document) -> Self {
        Self {
            doc,
            codes: HashMap::new(),
            uncertain: HashMap::new(),
            covered: HashMap::new(),
            default_resource_fonts: HashSet::new(),
            widths: HashMap::new(),
            visited: HashSet::new(),
            extra_resources: Vec::new(),
            extra_containers: HashSet::new(),
            form_runs: 0,
            decompressed: 0,
            aborted: None,
        }
    }

    fn scan_page(&mut self, page_id: ObjectId, fallback: &[ResourceLevel<'a>]) {
        let chain = self.page_chain(page_id);
        let mut content: Vec<u8> = Vec::new();
        let mut readable = true;
        for stream_id in self.doc.get_page_contents(page_id) {
            if let Ok(Object::Stream(stream)) = self.doc.get_object(stream_id) {
                match self.decompress(stream) {
                    Some(bytes) => {
                        content.extend_from_slice(&bytes);
                        content.push(b'\n');
                    }
                    None => readable = false,
                }
            }
        }
        if readable {
            self.run(&content, &chain, None, 0);
        } else {
            self.chain_uncertain(&chain, None, "a page content stream could not be decompressed");
        }

        let annotations = self.doc.get_page_annotations(page_id).unwrap_or_default();
        for annotation in annotations {
            self.scan_appearances(annotation, fallback);
        }
    }

    /// Runs every appearance stream of an annotation. A stream without its own
    /// `/Resources` is resolved against the AcroForm `/DR`, the way viewers
    /// treat widget appearances.
    fn scan_appearances(&mut self, annotation: &'a Dictionary, fallback: &[ResourceLevel<'a>]) {
        let Some(appearance) = self.deref(annotation.get(b"AP").ok()).and_then(|value| value.as_dict().ok()) else {
            return;
        };
        for key in [b"N".as_slice(), b"R".as_slice(), b"D".as_slice()] {
            let Ok(value) = appearance.get(key) else {
                continue;
            };
            let states = match value {
                Object::Reference(id) => match self.doc.get_object(*id) {
                    Ok(Object::Stream(stream)) => {
                        self.run_form(*id, stream, fallback, None, 1);
                        continue;
                    }
                    Ok(Object::Dictionary(states)) => states,
                    _ => continue,
                },
                Object::Dictionary(states) => states,
                _ => continue,
            };
            for (_, state) in states.iter() {
                if let Object::Reference(id) = state {
                    if let Ok(Object::Stream(stream)) = self.doc.get_object(*id) {
                        self.run_form(*id, stream, fallback, None, 1);
                    }
                }
            }
        }
    }

    /// Interprets one content stream with the given resources.
    fn run(&mut self, content: &[u8], chain: &[ResourceLevel<'a>], inherited: Option<ObjectId>, depth: usize) {
        let operations = match Content::decode_strict(content) {
            Ok(content) => content.operations,
            Err(_) => {
                self.chain_uncertain(chain, inherited, "a content stream that can select the font could not be parsed");
                return;
            }
        };
        self.chain_covered(chain);
        let mut font = inherited;
        let mut saved: Vec<Option<ObjectId>> = Vec::new();
        for operation in &operations {
            if self.aborted.is_some() {
                return;
            }
            let operands = &operation.operands;
            match operation.operator.as_str() {
                "q" => saved.push(font),
                "Q" => {
                    if let Some(previous) = saved.pop() {
                        font = previous;
                    }
                }
                "Tf" => {
                    font = operands
                        .first()
                        .and_then(|value| value.as_name().ok())
                        .and_then(|name| self.lookup(chain, b"Font", name))
                        .and_then(|(value, _)| value.as_reference().ok());
                }
                "gs" => {
                    if let Some(selected) = operands
                        .first()
                        .and_then(|value| value.as_name().ok())
                        .and_then(|name| self.state_font(chain, name))
                    {
                        font = Some(selected);
                    }
                }
                "Tj" | "'" => {
                    if let Some(Object::String(bytes, _)) = operands.last() {
                        self.record(font, bytes);
                    }
                }
                "\"" => {
                    if let Some(Object::String(bytes, _)) = operands.get(2) {
                        self.record(font, bytes);
                    }
                }
                "TJ" => {
                    if let Some(Object::Array(items)) = operands.first() {
                        for item in items {
                            if let Object::String(bytes, _) = item {
                                self.record(font, bytes);
                            }
                        }
                    }
                }
                "Do" => {
                    if let Some(name) = operands.first().and_then(|value| value.as_name().ok()) {
                        self.run_xobject(chain, name, font, depth);
                    }
                }
                "scn" | "SCN" => {
                    if let Some(name) = operands.last().and_then(|value| value.as_name().ok()) {
                        self.run_pattern(chain, name, depth);
                    }
                }
                _ => {}
            }
        }
    }

    fn run_xobject(&mut self, chain: &[ResourceLevel<'a>], name: &[u8], font: Option<ObjectId>, depth: usize) {
        let Some((Object::Reference(id), _)) = self.lookup(chain, b"XObject", name) else {
            return;
        };
        let Ok(Object::Stream(stream)) = self.doc.get_object(*id) else {
            return;
        };
        if stream.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Form".as_slice()) {
            self.run_form(*id, stream, chain, font, depth + 1);
        }
    }

    /// Tiling patterns (PatternType 1) carry a content stream; shading
    /// patterns are plain dictionaries and draw no text.
    fn run_pattern(&mut self, chain: &[ResourceLevel<'a>], name: &[u8], depth: usize) {
        let Some((Object::Reference(id), _)) = self.lookup(chain, b"Pattern", name) else {
            return;
        };
        let Ok(Object::Stream(stream)) = self.doc.get_object(*id) else {
            return;
        };
        if stream.dict.get(b"PatternType").and_then(Object::as_i64).ok() == Some(1) {
            self.run_form(*id, stream, chain, None, depth + 1);
        }
    }

    /// Runs a form-like stream (form XObject, tiling pattern, appearance). A
    /// stream without its own `/Resources` uses the caller's chain, as PDF 1.1
    /// era files rely on.
    fn run_form(
        &mut self,
        id: ObjectId,
        stream: &'a Stream,
        parent: &[ResourceLevel<'a>],
        font: Option<ObjectId>,
        depth: usize,
    ) {
        let own = stream.dict.get(b"Resources").ok().and_then(|value| self.level(value, id));
        let chain: Vec<ResourceLevel<'a>> = match own {
            Some(level) => vec![level],
            None => parent.to_vec(),
        };
        if depth > MAX_FORM_DEPTH || self.form_runs >= MAX_FORM_RUNS {
            self.chain_uncertain(&chain, font, "form XObjects nest deeper than the usage scan follows");
            return;
        }
        let inherited_key = if own.is_some() { None } else { parent.first().map(|level| level.container) };
        if !self.visited.insert((id, font, inherited_key)) {
            return;
        }
        self.form_runs += 1;
        if let Some(level) = own {
            if self.extra_containers.insert(level.container) {
                self.extra_resources.push(level.dict.clone());
            }
        }
        match self.decompress(stream) {
            Some(content) => self.run(&content, &chain, font, depth),
            None => self.chain_uncertain(&chain, font, "a form XObject could not be decompressed"),
        }
    }

    fn record(&mut self, font: Option<ObjectId>, bytes: &[u8]) {
        let Some(font) = font else {
            return;
        };
        let doc = self.doc;
        let width = *self.widths.entry(font).or_insert_with(|| code_width(doc, font));
        match width {
            CodeWidth::One => self.codes.entry(font).or_default().extend(bytes.iter().map(|byte| *byte as u16)),
            CodeWidth::Two => self
                .codes
                .entry(font)
                .or_default()
                .extend(bytes.as_chunks::<2>().0.iter().map(|pair| u16::from_be_bytes(*pair))),
            CodeWidth::Unknown => {
                self.uncertain.entry(font).or_insert_with(|| {
                    "the font's CMap is not Identity-H/V, so its codes cannot be decoded".to_string()
                });
            }
        }
    }

    fn decompress(&mut self, stream: &Stream) -> Option<Vec<u8>> {
        let remaining = MAX_DECOMPRESSED.saturating_sub(self.decompressed);
        match stream.decompressed_content_with_limit(remaining) {
            Ok(bytes) => {
                self.decompressed += bytes.len();
                Some(bytes)
            }
            Err(lopdf::Error::Decompress(lopdf::DecompressError::MemoryLimitExceeded { .. })) => {
                self.aborted = Some("the document's content exceeds the usage scan's decompression budget".to_string());
                None
            }
            Err(_) => None,
        }
    }

    /// The page's own resources first, then each inherited level up the tree.
    fn page_chain(&self, page_id: ObjectId) -> Vec<ResourceLevel<'a>> {
        let mut chain: Vec<ResourceLevel<'a>> = Vec::new();
        let mut seen: HashSet<ObjectId> = HashSet::new();
        let mut node_id = page_id;
        while seen.insert(node_id) && seen.len() <= MAX_TREE_DEPTH {
            let Ok(node) = self.doc.get_dictionary(node_id) else {
                break;
            };
            if let Some(level) = node.get(b"Resources").ok().and_then(|value| self.level(value, node_id)) {
                chain.push(level);
            }
            match node.get(b"Parent") {
                Ok(Object::Reference(parent)) => node_id = *parent,
                _ => break,
            }
        }
        chain
    }

    fn default_resources(&self) -> Option<ResourceLevel<'a>> {
        let catalog_id = self.doc.trailer.get(b"Root").ok()?.as_reference().ok()?;
        let catalog = self.doc.get_dictionary(catalog_id).ok()?;
        let (acro_form, acro_container) = self.dict_at(catalog.get(b"AcroForm").ok(), catalog_id)?;
        let (dict, container) = self.dict_at(acro_form.get(b"DR").ok(), acro_container)?;
        Some(ResourceLevel { dict, container })
    }

    fn level(&self, value: &'a Object, container: ObjectId) -> Option<ResourceLevel<'a>> {
        self.dict_at(Some(value), container).map(|(dict, container)| ResourceLevel { dict, container })
    }

    /// A dictionary value plus the indirect object that holds it.
    fn dict_at(&self, value: Option<&'a Object>, container: ObjectId) -> Option<(&'a Dictionary, ObjectId)> {
        match value? {
            Object::Reference(id) => self.doc.get_dictionary(*id).ok().map(|dict| (dict, *id)),
            Object::Dictionary(dict) => Some((dict, container)),
            _ => None,
        }
    }

    fn array_at(&self, value: Option<&'a Object>, container: ObjectId) -> Option<(&'a [Object], ObjectId)> {
        match value? {
            Object::Reference(id) => match self.doc.get_object(*id).ok()? {
                Object::Array(items) => Some((items.as_slice(), *id)),
                _ => None,
            },
            Object::Array(items) => Some((items.as_slice(), container)),
            _ => None,
        }
    }

    fn deref(&self, value: Option<&'a Object>) -> Option<&'a Object> {
        match value? {
            Object::Reference(id) => self.doc.get_object(*id).ok(),
            other => Some(other),
        }
    }

    /// Looks `name` up in the `category` sub-dictionary of each level.
    fn lookup(&self, chain: &[ResourceLevel<'a>], category: &[u8], name: &[u8]) -> Option<(&'a Object, ObjectId)> {
        for level in chain {
            let Some((map, container)) = self.dict_at(level.dict.get(category).ok(), level.container) else {
                continue;
            };
            if let Ok(value) = map.get(name) {
                return Some((value, container));
            }
        }
        None
    }

    /// The font an ExtGState sets through its `/Font [font size]` entry.
    fn state_font(&self, chain: &[ResourceLevel<'a>], name: &[u8]) -> Option<ObjectId> {
        let (value, container) = self.lookup(chain, b"ExtGState", name)?;
        let (state, state_container) = self.dict_at(Some(value), container)?;
        let (array, _) = self.array_at(state.get(b"Font").ok(), state_container)?;
        array.first()?.as_reference().ok()
    }

    /// Every indirect font a chain can select, with the object holding each
    /// reference: `/Font` entries and ExtGState `/Font` arrays.
    fn chain_fonts(&self, chain: &[ResourceLevel<'a>]) -> Vec<(ObjectId, ObjectId)> {
        let mut fonts: Vec<(ObjectId, ObjectId)> = Vec::new();
        for level in chain {
            if let Some((map, container)) = self.dict_at(level.dict.get(b"Font").ok(), level.container) {
                for (_, value) in map.iter() {
                    if let Object::Reference(id) = value {
                        fonts.push((*id, container));
                    }
                }
            }
            if let Some((states, container)) = self.dict_at(level.dict.get(b"ExtGState").ok(), level.container) {
                for (_, value) in states.iter() {
                    let Some((state, state_container)) = self.dict_at(Some(value), container) else {
                        continue;
                    };
                    let Some((array, array_container)) = self.array_at(state.get(b"Font").ok(), state_container) else {
                        continue;
                    };
                    if let Some(Object::Reference(id)) = array.first() {
                        fonts.push((*id, array_container));
                    }
                }
            }
        }
        fonts
    }

    /// The chain's font references have been seen by an interpreted stream.
    fn chain_covered(&mut self, chain: &[ResourceLevel<'a>]) {
        for (font, container) in self.chain_fonts(chain) {
            self.covered.entry(font).or_default().insert(container);
        }
    }

    fn chain_uncertain(&mut self, chain: &[ResourceLevel<'a>], inherited: Option<ObjectId>, reason: &str) {
        let fonts = self.chain_fonts(chain).into_iter().map(|(font, _)| font).chain(inherited);
        for font in fonts.collect::<Vec<_>>() {
            self.uncertain.entry(font).or_insert_with(|| reason.to_string());
        }
    }
}

/// Bytes per code for the text shown with `font`.
fn code_width(doc: &Document, font: ObjectId) -> CodeWidth {
    let Ok(dict) = doc.get_dictionary(font) else {
        return CodeWidth::Unknown;
    };
    if dict.get(b"Subtype").and_then(Object::as_name).ok() != Some(b"Type0".as_slice()) {
        return CodeWidth::One;
    }
    match dict.get(b"Encoding") {
        Ok(Object::Name(name)) if name == b"Identity-H" || name == b"Identity-V" => CodeWidth::Two,
        _ => CodeWidth::Unknown,
    }
}

/// For each target, the indirect objects anywhere in the file that hold a
/// reference to it (inline values included, stream data excluded).
fn reference_holders(doc: &Document, targets: &HashSet<ObjectId>) -> HashMap<ObjectId, HashSet<ObjectId>> {
    let mut holders: HashMap<ObjectId, HashSet<ObjectId>> = HashMap::new();
    if targets.is_empty() {
        return holders;
    }
    let mut found: Vec<ObjectId> = Vec::new();
    for (id, object) in &doc.objects {
        found.clear();
        if !collect_references(object, targets, &mut found, 0) {
            // Too deep to walk: assume it refers to every target.
            found.extend(targets.iter().copied());
        }
        for target in &found {
            holders.entry(*target).or_default().insert(*id);
        }
    }
    holders
}

/// Returns false when the walk stopped at the depth bound.
fn collect_references(object: &Object, targets: &HashSet<ObjectId>, found: &mut Vec<ObjectId>, depth: usize) -> bool {
    if depth > MAX_REFERENCE_DEPTH {
        return false;
    }
    match object {
        Object::Reference(id) => {
            if targets.contains(id) {
                found.push(*id);
            }
            true
        }
        Object::Array(items) => items.iter().all(|item| collect_references(item, targets, found, depth + 1)),
        Object::Dictionary(dict) => dict.iter().all(|(_, value)| collect_references(value, targets, found, depth + 1)),
        Object::Stream(stream) => {
            stream.dict.iter().all(|(_, value)| collect_references(value, targets, found, depth + 1))
        }
        _ => true,
    }
}

// ---------------------------------------------------------------------------
// ToUnicode CMaps
// ---------------------------------------------------------------------------

/// Reads the `bfchar`/`bfrange` mappings of a ToUnicode CMap for one- and
/// two-byte source codes (the code space of Identity-H/V text) into
/// code -> UTF-16 units. Longer source codes are ignored, glyph-name targets
/// are ignored, and a range target's last unit is incremented across the
/// range as PDF 32000-1 9.10.3 describes. Returns `None` for a structurally
/// broken CMap or one whose ranges would cost more than a fixed work budget.
pub(crate) fn parse_to_unicode(data: &[u8]) -> Option<BTreeMap<u16, Vec<u16>>> {
    let mut lexer = Lexer { data, position: 0 };
    let mut map: BTreeMap<u16, Vec<u16>> = BTreeMap::new();
    let mut work = 0usize;
    while let Some(token) = lexer.next_token() {
        match token {
            Token::Word(b"beginbfchar") => loop {
                work += 1;
                if work > MAX_CMAP_WORK {
                    return None;
                }
                match lexer.next_token()? {
                    Token::Word(b"endbfchar") => break,
                    Token::Hex(source) => {
                        let target = lexer.next_token()?;
                        if let (Some(code), Token::Hex(target)) = (source_code(&source), target) {
                            insert_target(&mut map, code, utf16_units(&target));
                        }
                    }
                    _ => return None,
                }
            },
            Token::Word(b"beginbfrange") => loop {
                work += 1;
                if work > MAX_CMAP_WORK {
                    return None;
                }
                let low = match lexer.next_token()? {
                    Token::Word(b"endbfrange") => break,
                    Token::Hex(low) => low,
                    _ => return None,
                };
                let Token::Hex(high) = lexer.next_token()? else {
                    return None;
                };
                let range = match (source_code(&low), source_code(&high)) {
                    (Some(low), Some(high)) if low <= high => Some((low, high)),
                    _ => None,
                };
                match lexer.next_token()? {
                    Token::Hex(target) => {
                        let Some((low, high)) = range else {
                            continue;
                        };
                        let base = utf16_units(&target);
                        let Some(last) = base.last().copied() else {
                            continue;
                        };
                        for code in low..=high {
                            work += 1;
                            if work > MAX_CMAP_WORK {
                                return None;
                            }
                            let Some(unit) = last.checked_add(code - low) else {
                                break;
                            };
                            let mut units = base.clone();
                            if let Some(slot) = units.last_mut() {
                                *slot = unit;
                            }
                            insert_target(&mut map, code, units);
                        }
                    }
                    Token::Open => {
                        let mut code = range.map(|(low, _)| low);
                        loop {
                            work += 1;
                            if work > MAX_CMAP_WORK {
                                return None;
                            }
                            match lexer.next_token()? {
                                Token::Close => break,
                                Token::Hex(target) => {
                                    if let (Some(current), Some((_, high))) = (code, range) {
                                        if current <= high {
                                            insert_target(&mut map, current, utf16_units(&target));
                                        }
                                        code = current.checked_add(1);
                                    }
                                }
                                _ => return None,
                            }
                        }
                    }
                    _ => return None,
                }
            },
            _ => {}
        }
    }
    Some(map)
}

fn insert_target(map: &mut BTreeMap<u16, Vec<u16>>, code: u16, units: Vec<u16>) {
    if !units.is_empty() && units.len() <= MAX_CMAP_TARGET {
        map.insert(code, units);
    }
}

fn source_code(bytes: &[u8]) -> Option<u16> {
    match bytes {
        [single] => Some(*single as u16),
        [high, low] => Some(u16::from_be_bytes([*high, *low])),
        _ => None,
    }
}

/// UTF-16BE bytes to code units; a stray odd byte becomes its own unit.
fn utf16_units(bytes: &[u8]) -> Vec<u16> {
    let (pairs, rest) = bytes.as_chunks::<2>();
    let mut units: Vec<u16> = pairs.iter().map(|pair| u16::from_be_bytes(*pair)).collect();
    if let [odd] = rest {
        units.push(*odd as u16);
    }
    units
}

enum Token<'a> {
    Hex(Vec<u8>),
    Open,
    Close,
    Word(&'a [u8]),
}

/// A minimal PostScript tokenizer: enough of the CMap syntax to find the
/// mapping sections. Strings, dictionaries and procedures are skipped.
struct Lexer<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> Lexer<'a> {
    fn next_token(&mut self) -> Option<Token<'a>> {
        loop {
            let byte = *self.data.get(self.position)?;
            match byte {
                _ if is_whitespace(byte) => self.position += 1,
                b'%' => {
                    while self.data.get(self.position).is_some_and(|byte| *byte != b'\n' && *byte != b'\r') {
                        self.position += 1;
                    }
                }
                b'[' => {
                    self.position += 1;
                    return Some(Token::Open);
                }
                b']' => {
                    self.position += 1;
                    return Some(Token::Close);
                }
                b'<' if self.data.get(self.position + 1) == Some(&b'<') => {
                    self.position += 2;
                    return Some(Token::Word(b"<<"));
                }
                b'<' => {
                    self.position += 1;
                    let mut nibbles: Vec<u8> = Vec::new();
                    loop {
                        let byte = *self.data.get(self.position)?;
                        self.position += 1;
                        match byte {
                            b'>' => break,
                            b'0'..=b'9' => nibbles.push(byte - b'0'),
                            b'a'..=b'f' => nibbles.push(byte - b'a' + 10),
                            b'A'..=b'F' => nibbles.push(byte - b'A' + 10),
                            _ if is_whitespace(byte) => {}
                            _ => return Some(Token::Word(b"")),
                        }
                    }
                    if nibbles.len() % 2 == 1 {
                        nibbles.push(0);
                    }
                    return Some(Token::Hex(
                        nibbles.as_chunks::<2>().0.iter().map(|pair| pair[0] << 4 | pair[1]).collect(),
                    ));
                }
                b'(' => {
                    self.skip_literal_string();
                    return Some(Token::Word(b"()"));
                }
                _ => {
                    let start = self.position;
                    self.position += 1;
                    while self.data.get(self.position).is_some_and(|byte| !is_whitespace(*byte) && !is_delimiter(*byte))
                    {
                        self.position += 1;
                    }
                    return Some(Token::Word(&self.data[start..self.position]));
                }
            }
        }
    }

    fn skip_literal_string(&mut self) {
        let mut depth = 0usize;
        while let Some(byte) = self.data.get(self.position).copied() {
            self.position += 1;
            match byte {
                b'\\' => self.position += 1,
                b'(' => depth += 1,
                b')' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return;
                    }
                }
                _ => {}
            }
        }
    }
}

fn is_whitespace(byte: u8) -> bool {
    matches!(byte, b'\0' | b'\t' | b'\n' | b'\x0C' | b'\r' | b' ')
}

fn is_delimiter(byte: u8) -> bool {
    matches!(byte, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_unicode_reads_chars_ranges_and_arrays() {
        let cmap = b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n\
            /CMapName /Adobe-Identity-UCS def\n\
            1 begincodespacerange <0000> <FFFF> endcodespacerange\n\
            2 beginbfchar <0003> <0020> <0024> <0041> endbfchar\n\
            2 beginbfrange <0044> <0046> <0061> <0050> <0051> [<0066006C> <D835DC00>] endbfrange\n\
            endcmap CMapName currentdict /CMap defineresource pop end end";
        let map = parse_to_unicode(cmap).expect("parse");
        assert_eq!(map.get(&0x0003), Some(&vec![0x20]));
        assert_eq!(map.get(&0x0024), Some(&vec![0x41]));
        assert_eq!(map.get(&0x0046), Some(&vec![0x63]), "a range increments its target");
        assert_eq!(map.get(&0x0050), Some(&vec![0x66, 0x6C]), "array targets map one by one");
        assert_eq!(map.get(&0x0051), Some(&vec![0xD835, 0xDC00]));
        assert_eq!(map.len(), 7);
    }

    #[test]
    fn to_unicode_rejects_garbage_without_panicking() {
        assert!(parse_to_unicode(b"beginbfrange <00> endbfrange").is_none());
        assert!(parse_to_unicode(b"beginbfchar <0001> <").is_none());
        assert_eq!(parse_to_unicode(b"").map(|map| map.len()), Some(0));
        let mut hostile = b"beginbfrange ".to_vec();
        for _ in 0..40 {
            hostile.extend_from_slice(b"<0000> <FFFF> <0041> ");
        }
        hostile.extend_from_slice(b"endbfrange");
        assert!(parse_to_unicode(&hostile).is_none(), "the work budget must stop overlapping full ranges");
    }
}
