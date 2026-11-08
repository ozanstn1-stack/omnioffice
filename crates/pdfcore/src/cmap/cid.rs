//! The code-to-CID table of an embedded `/Encoding` CMap.
//!
//! A Type0 font names its encoding either with a CMap name we would have to ship
//! (`Identity-H`) or with a stream of its own: a small PostScript program whose
//! `begincidrange`/`begincidchar` entries say which CID each code selects. This
//! module reads that program, so a note can be drawn with a font whose codes are
//! not their own CIDs.
//!
//! Strips of it are refused the same way the rest of the drawing path refuses
//! them, because a wrong guess here draws the wrong glyph without any error:
//!
//! * a base CMap other than an identity one: an unknown base (`90ms-RKSJ-H` and
//!   friends) brings its own codespace with it, and splitting our code with it
//!   would draw the wrong glyphs,
//! * one- and two-byte ranges that are not prefix-free ([`space`]),
//! * overlapping entries that disagree, since neither viewer is authoritative
//!   about which one wins.
//!
//! Entries outside the declared codespace are kept but never matched: a viewer
//! never decodes those codes either. A map that declares one-byte ranges is
//! stricter, because the width is part of the code there.

mod space;
mod table;

use super::lex::Token;
use super::Code;
use space::Space;
use table::{code_of, Run, Table};

/// A code-to-CID table, sorted and normalised.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct CidMap {
    space: Space,
    /// Two-byte codes, keyed by value.
    wide: Table,
    /// One-byte codes, keyed by value.
    narrow: Table,
    /// Whether an identity base CMap fills the two-byte codes no run mentions.
    identity: bool,
}

impl CidMap {
    /// The code as it will be written, or `None` when the map cannot draw it.
    ///
    /// A map with only two-byte ranges promotes a one-byte `/ToUnicode` code to
    /// two bytes, which is what a sloppy producer meant. A map with one-byte
    /// ranges is strict: the width is part of the code, so the code has to be
    /// declared at that width.
    fn admit(&self, code: Code) -> Option<Code> {
        let code = if self.space.has_narrow() { code } else { Code::two(code.value()) };
        // A code outside the declared space is never decoded, by us or by a
        // viewer: the entry may be written, but it can never be reached.
        self.space.contains(code).then_some(code)
    }

    /// The CID `code` selects, or `None` when the code is outside the codespace,
    /// mapped to `.notdef`, or not mapped at all.
    pub(crate) fn cid(&self, code: Code) -> Option<u16> {
        let code = self.admit(code)?;
        let table = if code.width() == 1 { &self.narrow } else { &self.wide };
        let value = code.value();
        if table.notdef.iter().any(|(low, high)| (*low..=*high).contains(&value)) {
            return None;
        }
        let index = table.runs.partition_point(|run| run.high < value);
        match table.runs.get(index) {
            Some(run) if run.low <= value => run.cid.checked_add(value - run.low),
            _ if self.identity && code.width() == 2 => Some(value),
            _ => None,
        }
    }

    /// Visits every run boundary; used by the fuzz target to prove that a map
    /// which built successfully never panics while being read.
    #[cfg(any(test, fuzzing))]
    pub(crate) fn probe(&self) -> usize {
        let mut seen = 0usize;
        for (table, width) in [(&self.wide, 2u8), (&self.narrow, 1)] {
            for run in &table.runs {
                for value in [run.low.saturating_sub(1), run.low, run.high, run.high.saturating_add(1)] {
                    if self.cid(code_of(width, value)).is_some() {
                        seen += 1;
                    }
                }
            }
            for (low, high) in &table.notdef {
                let _ = self.cid(code_of(width, *low));
                let _ = self.cid(code_of(width, *high));
            }
        }
        seen
    }
}

/// Reads a CMap program into its code-to-CID table. `dict_base` is the stream
/// dictionary's `/UseCMap` name, when it has one.
pub(crate) fn read(program: &[u8], dict_base: Option<&[u8]>) -> Option<CidMap> {
    if program.len() > super::MAX_STREAM {
        return None;
    }
    // The dictionary's `/UseCMap` is a base like any other: only an identity one
    // is understood.
    if dict_base.is_some_and(|name| name != b"Identity-H") {
        return None;
    }
    let text = String::from_utf8_lossy(program);
    let tokens = super::lex::tokenize(&text);
    let mut map = CidMap::default();
    let mut at = 0;
    let mut base: Option<&[u8]> = dict_base;
    let mut entries = 0usize;
    while at < tokens.len() {
        match &tokens[at] {
            Token::Keyword("begincodespacerange") => {
                let (space, next) = Space::read(&tokens, at + 1, super::MAX_ENTRIES)?;
                map.space.absorb(space, super::MAX_ENTRIES)?;
                at = next;
            }
            Token::Keyword("begincidrange") => {
                at = runs(&tokens, at + 1, &mut map, &mut entries)?;
            }
            Token::Keyword("begincidchar") => {
                at = singles(&tokens, at + 1, &mut map, &mut entries, false)?;
            }
            Token::Keyword("beginnotdefrange") => {
                at = notdef_ranges(&tokens, at + 1, &mut map, &mut entries)?;
            }
            Token::Keyword("beginnotdefchar") => {
                at = singles(&tokens, at + 1, &mut map, &mut entries, true)?;
            }
            Token::Keyword("beginbfchar" | "beginbfrange") => return None,
            Token::Name(name) => {
                if tokens.get(at + 1) == Some(&Token::Keyword("usecmap")) {
                    // Two bases, or a program that disagrees with its
                    // dictionary, are ambiguous rather than wrong: refuse.
                    if base.is_some() || !identity_name(name) {
                        return None;
                    }
                    base = Some(name.as_bytes());
                    at += 2;
                } else {
                    at += 1;
                }
            }
            Token::Keyword("def") => {
                // `/WMode 1 def` asks for vertical writing.
                if let (Some(Token::Name(name)), Some(Token::Number(mode))) =
                    (tokens.get(at.wrapping_sub(2)), tokens.get(at.wrapping_sub(1)))
                {
                    if *name == "WMode" && *mode != 0 {
                        return None;
                    }
                }
                at += 1;
            }
            _ => at += 1,
        }
        if entries > super::MAX_ENTRIES {
            return None;
        }
    }
    map.identity = base.is_some();
    if map.identity {
        map.space.identity();
    }
    if map.space.is_empty() || (map.wide.runs.is_empty() && map.narrow.runs.is_empty() && !map.identity) {
        return None;
    }
    // A one-byte range inside a two-byte lead byte would make a viewer split the
    // string at the wrong place.
    if !map.space.prefix_free(map.identity) {
        return None;
    }
    // A declared width is a promise: every entry must fall in a range of its own
    // width. A two-byte map without one-byte ranges keeps the older, lenient
    // reading, so the output of documents that work today does not move.
    let strict = map.space.has_narrow();
    for (table, width) in [(&map.wide, 2u8), (&map.narrow, 1)] {
        if !table.runs.is_empty() && !map.space.has_width(width) {
            return None;
        }
        if strict {
            let inside = |value: u16| map.space.contains(code_of(width, value));
            if !table.runs.iter().all(|run| inside(run.low) && inside(run.high)) {
                return None;
            }
            if !table.notdef.iter().all(|(low, high)| inside(*low) && inside(*high)) {
                return None;
            }
        }
    }
    map.wide.normalise()?;
    map.narrow.normalise()?;
    Some(map)
}

/// Whether a base CMap name is the horizontal identity encoding.
fn identity_name(name: &str) -> bool {
    name == "Identity-H"
}

/// The CID operand: a decimal integer that fits the two-byte code space.
fn cid_value(token: &Token<'_>) -> Option<u16> {
    match token {
        Token::Number(value) => u16::try_from(*value).ok(),
        _ => None,
    }
}

/// The table a code of this width belongs to.
fn table_for(map: &mut CidMap, width: u8) -> &mut Table {
    if width == 1 {
        &mut map.narrow
    } else {
        &mut map.wide
    }
}

/// Reads `begincidrange` entries: `<low> <high> cid`.
fn runs(tokens: &[Token<'_>], mut at: usize, map: &mut CidMap, entries: &mut usize) -> Option<usize> {
    while let Some(Token::Hex(low)) = tokens.get(at) {
        let (Token::Hex(high), cid_token) = (tokens.get(at + 1)?, tokens.get(at + 2)?) else { return None };
        let (low, high, cid) = (Code::from_bytes(low)?, Code::from_bytes(high)?, cid_value(cid_token)?);
        *entries += 1;
        if low.width() != high.width() || high < low {
            return None;
        }
        // The last code of the run selects the last CID; both must fit.
        let span = high.value() - low.value();
        let last = u32::from(cid).checked_add(u32::from(span))?;
        if last > u32::from(u16::MAX) {
            return None;
        }
        table_for(map, low.width()).runs.push(Run { low: low.value(), high: high.value(), cid });
        at += 3;
        if *entries > super::MAX_ENTRIES {
            return None;
        }
    }
    closed(tokens, at, "endcidrange")
}

/// Reads `begincidchar` and `beginnotdefchar` entries: `<code> cid`.
fn singles(tokens: &[Token<'_>], mut at: usize, map: &mut CidMap, entries: &mut usize, notdef: bool) -> Option<usize> {
    while let Some(Token::Hex(code_bytes)) = tokens.get(at) {
        let code = Code::from_bytes(code_bytes)?;
        let cid = cid_value(tokens.get(at + 1)?)?;
        *entries += 1;
        let table = table_for(map, code.width());
        if notdef {
            table.notdef.push((code.value(), code.value()));
        } else {
            table.runs.push(Run { low: code.value(), high: code.value(), cid });
        }
        at += 2;
        if *entries > super::MAX_ENTRIES {
            return None;
        }
    }
    let end = if notdef { "endnotdefchar" } else { "endcidchar" };
    closed(tokens, at, end)
}

/// Reads `beginnotdefrange` entries: `<low> <high> cid`, where the CID is
/// ignored because the range never draws.
fn notdef_ranges(tokens: &[Token<'_>], mut at: usize, map: &mut CidMap, entries: &mut usize) -> Option<usize> {
    while let Some(Token::Hex(low)) = tokens.get(at) {
        let (Token::Hex(high), Token::Number(_)) = (tokens.get(at + 1)?, tokens.get(at + 2)?) else { return None };
        let (low, high) = (Code::from_bytes(low)?, Code::from_bytes(high)?);
        *entries += 1;
        if low.width() != high.width() || high < low {
            return None;
        }
        table_for(map, low.width()).notdef.push((low.value(), high.value()));
        at += 3;
        if *entries > super::MAX_ENTRIES {
            return None;
        }
    }
    closed(tokens, at, "endnotdefrange")
}

/// The keyword that must close a block, or the position after it.
fn closed(tokens: &[Token<'_>], at: usize, end: &str) -> Option<usize> {
    match tokens.get(at) {
        Some(Token::Keyword(keyword)) if *keyword == end => Some(at + 1),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
