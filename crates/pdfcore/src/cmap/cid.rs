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
//! * a codespace that is not exactly **two bytes** per code - [`crate::cmap`]
//!   does not report the length of the source code it inverted, and
//!   `face/slots.rs` writes four hex digits per code, so a one-byte font would
//!   silently shift every glyph after the mistake,
//! * a base CMap other than an identity one: an unknown base (`90ms-RKSJ-H` and
//!   friends) brings its own codespace with it, and splitting our code with it
//!   would draw the wrong glyphs,
//! * overlapping entries that disagree, since neither viewer is authoritative
//!   about which one wins.
//!
//! Entries outside the declared codespace are kept but never matched: a viewer
//! never decodes those codes either.

use super::lex::Token;

/// One run of consecutive codes that select consecutive CIDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Run {
    low: u16,
    high: u16,
    /// The CID `low` selects; the rest follow it.
    cid: u16,
}

/// A code-to-CID table, sorted and normalised.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct CidMap {
    /// The codespace as byte ranges, so a code is matched byte by byte.
    space: Vec<([u8; 2], [u8; 2])>,
    runs: Vec<Run>,
    /// Codes the CMap sends to `.notdef`, which must never be drawn.
    notdef: Vec<(u16, u16)>,
    /// Whether an identity base CMap fills the codes no run mentions.
    identity: bool,
}

impl CidMap {
    /// The CID `code` selects, or `None` when the code is outside the codespace,
    /// mapped to `.notdef`, or not mapped at all.
    pub(crate) fn cid(&self, code: u16) -> Option<u16> {
        let bytes = code.to_be_bytes();
        let inside = self
            .space
            .iter()
            .any(|(low, high)| (low[0]..=high[0]).contains(&bytes[0]) && (low[1]..=high[1]).contains(&bytes[1]));
        if !inside {
            return None;
        }
        if self.notdef.iter().any(|(low, high)| (*low..=*high).contains(&code)) {
            return None;
        }
        let index = self.runs.partition_point(|run| run.high < code);
        match self.runs.get(index) {
            Some(run) if run.low <= code => run.cid.checked_add(code - run.low),
            _ if self.identity => Some(code),
            _ => None,
        }
    }

    /// Visits every run boundary; used by the fuzz target to prove that a map
    /// which built successfully never panics while being read.
    #[cfg(any(test, fuzzing))]
    pub(super) fn probe(&self) -> usize {
        let mut seen = 0usize;
        for run in &self.runs {
            for code in [run.low.saturating_sub(1), run.low, run.high, run.high.saturating_add(1)] {
                if self.cid(code).is_some() {
                    seen += 1;
                }
            }
        }
        for (low, high) in &self.notdef {
            let _ = self.cid(*low);
            let _ = self.cid(*high);
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
    let text = String::from_utf8_lossy(program);
    let tokens = super::lex::tokenize(&text);
    let mut map = CidMap::default();
    let mut at = 0;
    // The dictionary's `/UseCMap` is a base like any other: only an identity one
    // is understood.
    if dict_base.is_some_and(|name| name != b"Identity-H") {
        return None;
    }
    let mut base: Option<&[u8]> = dict_base;
    let mut entries = 0usize;
    while at < tokens.len() {
        match &tokens[at] {
            Token::Keyword("begincodespacerange") => {
                at = codespace(&tokens, at + 1, &mut map)?;
            }
            Token::Keyword("begincidrange") => {
                at = cid_ranges(&tokens, at + 1, &mut map, &mut entries)?;
            }
            Token::Keyword("begincidchar") => {
                at = cid_chars(&tokens, at + 1, &mut map, &mut entries)?;
            }
            Token::Keyword("beginnotdefrange") => {
                at = notdef_ranges(&tokens, at + 1, &mut map, &mut entries)?;
            }
            Token::Keyword("beginnotdefchar") => {
                at = notdef_chars(&tokens, at + 1, &mut map, &mut entries)?;
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
    if map.space.is_empty() {
        if !map.identity {
            return None;
        }
        // An identity base declares the whole two-byte space.
        map.space.push(([0, 0], [0xFF, 0xFF]));
    }
    if map.runs.is_empty() && !map.identity {
        return None;
    }
    map.normalise()
}

/// Whether a base CMap name is the horizontal identity encoding.
fn identity_name(name: &str) -> bool {
    name == "Identity-H"
}

/// Bytes of a two-byte code, or `None` when the hex string is another width.
fn code(bytes: &[u8]) -> Option<u16> {
    (bytes.len() == 2).then(|| u16::from_be_bytes([bytes[0], bytes[1]]))
}

/// A CID operand: a decimal integer that fits the two-byte code space.
fn cid(token: &Token<'_>) -> Option<u16> {
    match token {
        Token::Number(value) => u16::try_from(*value).ok(),
        _ => None,
    }
}

/// Reads `begincodespacerange` entries; every range must be two bytes wide.
fn codespace(tokens: &[Token<'_>], mut at: usize, map: &mut CidMap) -> Option<usize> {
    while let Some(Token::Hex(low)) = tokens.get(at) {
        let Token::Hex(high) = tokens.get(at + 1)? else { return None };
        let (low, high) = (code(low)?, code(high)?);
        if high < low {
            return None;
        }
        map.space.push((low.to_be_bytes(), high.to_be_bytes()));
        at += 2;
        if map.space.len() > super::MAX_ENTRIES {
            return None;
        }
    }
    closed(tokens, at, "endcodespacerange")
}

/// Reads `begincidrange` entries: `<low> <high> cid`.
fn cid_ranges(tokens: &[Token<'_>], mut at: usize, map: &mut CidMap, entries: &mut usize) -> Option<usize> {
    while let Some(Token::Hex(low)) = tokens.get(at) {
        let (Token::Hex(high), cid_token) = (tokens.get(at + 1)?, tokens.get(at + 2)?) else { return None };
        let (low, high, cid) = (code(low)?, code(high)?, cid(cid_token)?);
        *entries += 1;
        if high < low {
            return None;
        }
        // The last code of the run selects the last CID; both must fit.
        let last = u32::from(cid).checked_add(u32::from(high) - u32::from(low))?;
        if last > u32::from(u16::MAX) {
            return None;
        }
        map.runs.push(Run { low, high, cid });
        at += 3;
        if *entries > super::MAX_ENTRIES {
            return None;
        }
    }
    closed(tokens, at, "endcidrange")
}

/// Reads `begincidchar` entries: `<code> cid`.
fn cid_chars(tokens: &[Token<'_>], mut at: usize, map: &mut CidMap, entries: &mut usize) -> Option<usize> {
    while let Some(Token::Hex(code_bytes)) = tokens.get(at) {
        let value = code(code_bytes)?;
        let cid = cid(tokens.get(at + 1)?)?;
        *entries += 1;
        map.runs.push(Run { low: value, high: value, cid });
        at += 2;
        if *entries > super::MAX_ENTRIES {
            return None;
        }
    }
    closed(tokens, at, "endcidchar")
}

/// Reads `beginnotdefrange` entries: `<low> <high> cid`, where the CID is
/// ignored because the range never draws.
fn notdef_ranges(tokens: &[Token<'_>], mut at: usize, map: &mut CidMap, entries: &mut usize) -> Option<usize> {
    while let Some(Token::Hex(low)) = tokens.get(at) {
        let (Token::Hex(high), Token::Number(_)) = (tokens.get(at + 1)?, tokens.get(at + 2)?) else { return None };
        let (low, high) = (code(low)?, code(high)?);
        *entries += 1;
        if high < low {
            return None;
        }
        map.notdef.push((low, high));
        at += 3;
        if *entries > super::MAX_ENTRIES {
            return None;
        }
    }
    closed(tokens, at, "endnotdefrange")
}

/// Reads `beginnotdefchar` entries: `<code> cid`.
fn notdef_chars(tokens: &[Token<'_>], mut at: usize, map: &mut CidMap, entries: &mut usize) -> Option<usize> {
    while let Some(Token::Hex(code_bytes)) = tokens.get(at) {
        let value = code(code_bytes)?;
        if !matches!(tokens.get(at + 1), Some(Token::Number(_))) {
            return None;
        }
        *entries += 1;
        map.notdef.push((value, value));
        at += 2;
        if *entries > super::MAX_ENTRIES {
            return None;
        }
    }
    closed(tokens, at, "endnotdefchar")
}

/// The keyword that must close a block, or the position after it.
fn closed(tokens: &[Token<'_>], at: usize, end: &str) -> Option<usize> {
    match tokens.get(at) {
        Some(Token::Keyword(keyword)) if *keyword == end => Some(at + 1),
        _ => None,
    }
}

impl CidMap {
    /// Sorts the runs and merges the ones that agree, refusing a conflict.
    fn normalise(mut self) -> Option<Self> {
        self.runs.sort_unstable_by_key(|run| (run.low, run.high));
        let mut runs: Vec<Run> = Vec::with_capacity(self.runs.len());
        for run in self.runs {
            match runs.last_mut() {
                // The same mapping written twice is fine, and so is a run that
                // continues the previous one with the same offset.
                Some(last) if last.high >= run.low => {
                    let last_offset = i64::from(last.cid) - i64::from(last.low);
                    let run_offset = i64::from(run.cid) - i64::from(run.low);
                    if last_offset != run_offset {
                        return None;
                    }
                    last.high = last.high.max(run.high);
                }
                _ => runs.push(run),
            }
        }
        self.runs = runs;
        self.notdef.sort_unstable();
        if self.notdef.windows(2).any(|pair| pair[0].1 >= pair[1].0) {
            return None;
        }
        // A notdef range that covers a mapped code would make the two
        // disagree, and viewers differ on which one wins.
        for (low, high) in &self.notdef {
            let index = self.runs.partition_point(|run| run.high < *low);
            if self.runs.get(index).is_some_and(|run| run.low <= *high) {
                return None;
            }
        }
        Some(self)
    }
}

#[cfg(test)]
mod tests;
