//! A `/ToUnicode` CMap, inverted into the character-to-code map a renderer
//! needs to *draw* text in a composite font.
//!
//! A Type0 font's codes are glyph identifiers, and `/ToUnicode` maps those codes
//! back to the characters the document meant. Drawing text with that font means
//! walking the map the other way: character to code. The stream is
//! producer-written PostScript, so it arrives as untrusted bytes: this module
//! reads it leniently (a broken entry is skipped, the rest is kept), bounds what
//! it allocates, and never fails - a caller that gets an empty map leaves the
//! text out, which is what happens today.
//!
//! Only the horizontal `Identity-H` codespace is assumed: codes are two bytes,
//! so a source of any other length is skipped. Destinations are UTF-16BE, and
//! only a destination that decodes to exactly one Unicode scalar is kept - a
//! ligature (`fi`) has no single character to map back to, and a lone surrogate
//! is not text.

use std::collections::BTreeMap;

/// A stream larger than this is not a text map and is refused.
const MAX_STREAM: usize = 1 << 20;

/// The most mappings a map may hold: two bytes of code space.
const MAX_ENTRIES: usize = 1 << 16;

/// The inverted map: the character a document drew, and the lowest code it used
/// for it, so the result does not depend on the order of the entries.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ToUnicode {
    to_code: BTreeMap<char, u16>,
}

impl ToUnicode {
    /// The code `symbol` is drawn with, when the map has one.
    pub fn code(&self, symbol: char) -> Option<u16> {
        self.to_code.get(&symbol).copied()
    }

    pub fn len(&self) -> usize {
        self.to_code.len()
    }

    pub fn is_empty(&self) -> bool {
        self.to_code.is_empty()
    }

    /// True when the map covers every character of `text`, ignoring the line
    /// breaks the caller splits on.
    pub fn covers(&self, text: &str) -> bool {
        text.chars().filter(|symbol| !matches!(symbol, '\n' | '\r')).all(|symbol| self.to_code.contains_key(&symbol))
    }

    /// The two-byte big-endian codes for `text`, or `None` when a character is
    /// not mapped.
    pub fn encode(&self, text: &str) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(text.len() * 2);
        for symbol in text.chars() {
            let code = self.code(symbol)?;
            out.extend_from_slice(&code.to_be_bytes());
        }
        Some(out)
    }

    fn insert(&mut self, symbol: char, code: u16) {
        if self.to_code.len() >= MAX_ENTRIES {
            return;
        }
        // The lowest code wins, whatever order the entries arrive in.
        match self.to_code.get(&symbol) {
            Some(known) if *known <= code => {}
            _ => {
                self.to_code.insert(symbol, code);
            }
        }
    }
}

/// One token of a CMap stream: a keyword, a hex string, or the brackets of an
/// array destination.
#[derive(Debug, PartialEq, Eq)]
enum Tok<'a> {
    Keyword(&'a str),
    Hex(Vec<u8>),
    Open,
    Close,
}

/// Reads a `/ToUnicode` stream into the inverted map. An oversized or empty
/// stream gives an empty map.
pub fn read(bytes: &[u8]) -> ToUnicode {
    if bytes.len() > MAX_STREAM {
        return ToUnicode::default();
    }
    let text = String::from_utf8_lossy(bytes);
    let tokens = tokenize(&text);
    let mut map = ToUnicode::default();
    let mut at = 0;
    while at < tokens.len() {
        match &tokens[at] {
            Tok::Keyword("beginbfchar") => {
                at += 1;
                while let (Some(Tok::Hex(source)), Some(Tok::Hex(destination))) = (tokens.get(at), tokens.get(at + 1)) {
                    at += 2;
                    if let (Some(code), Some(symbol)) = (two_byte_code(source), single_symbol(destination)) {
                        map.insert(symbol, code);
                    }
                }
            }
            Tok::Keyword("beginbfrange") => {
                at += 1;
                while let (Some(Tok::Hex(low)), Some(Tok::Hex(high)), Some(destinations)) =
                    (tokens.get(at), tokens.get(at + 1), destinations(&tokens, at + 2))
                {
                    at = destinations.next;
                    map_range(&mut map, low, high, &destinations.list);
                }
            }
            _ => at += 1,
        }
    }
    map
}

/// A destination list and where the token stream resumes after it.
struct Destinations {
    list: Vec<Vec<u8>>,
    next: usize,
}

/// Reads the destination of a `bfrange` entry: one hex string, or an array.
fn destinations(tokens: &[Tok<'_>], at: usize) -> Option<Destinations> {
    match tokens.get(at)? {
        Tok::Hex(bytes) => Some(Destinations { list: vec![bytes.clone()], next: at + 1 }),
        Tok::Open => {
            let mut list = Vec::new();
            let mut index = at + 1;
            loop {
                match tokens.get(index)? {
                    Tok::Hex(bytes) => list.push(bytes.clone()),
                    Tok::Close => return Some(Destinations { list, next: index + 1 }),
                    Tok::Open | Tok::Keyword(_) => return None,
                }
                index += 1;
            }
        }
        Tok::Keyword(_) | Tok::Close => None,
    }
}

/// Expands one `bfrange`: the destination is incremented for every code after
/// the first, and the range stops where it would leave the code space or run
/// into a destination that is not text.
fn map_range(map: &mut ToUnicode, low: &[u8], high: &[u8], list: &[Vec<u8>]) {
    let (Some(low), Some(high)) = (two_byte_code(low), two_byte_code(high)) else { return };
    if high < low {
        return;
    }
    if list.len() == 1 {
        let base = &list[0];
        for offset in 0..=(high - low) {
            let Some(code) = low.checked_add(offset) else { break };
            let Some(destination) = increment(base, offset) else { break };
            if let Some(symbol) = single_symbol(&destination) {
                map.insert(symbol, code);
            }
        }
        return;
    }
    for (offset, destination) in list.iter().enumerate() {
        let Some(code) = low.checked_add(offset as u16) else { break };
        if code > high {
            break;
        }
        if let Some(symbol) = single_symbol(destination) {
            map.insert(symbol, code);
        }
    }
}

/// A destination incremented as one big-endian number, keeping its byte length.
fn increment(base: &[u8], offset: u16) -> Option<Vec<u8>> {
    let width = base.len();
    if width > 8 {
        return None;
    }
    let value = base.iter().fold(0u64, |value, byte| value << 8 | u64::from(*byte));
    let grown = value.checked_add(u64::from(offset))?;
    // Only a full eight-byte destination fills the whole word, so the shift
    // below is skipped there: shifting a `u64` by 64 would overflow.
    if width < 8 && grown >> (width * 8) != 0 {
        return None;
    }
    Some((0..width).map(|index| ((grown >> ((width - 1 - index) * 8)) & 0xFF) as u8).collect())
}

/// The one character a destination stands for, or `None` when it does not
/// decode to exactly one usable scalar.
fn single_symbol(bytes: &[u8]) -> Option<char> {
    if !bytes.len().is_multiple_of(2) {
        return None;
    }
    let units: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|pair| u16::from_be_bytes(*pair)).collect();
    let mut chars = char::decode_utf16(units).filter_map(Result::ok);
    let symbol = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    keep(symbol).then_some(symbol)
}

/// Characters a drawn code may not stand for: `NUL`, the replacement character,
/// private use (no agreed meaning) and the noncharacters.
fn keep(symbol: char) -> bool {
    let code = symbol as u32;
    !matches!(code, 0 | 0xFFFD)
        && !(0xE000..=0xF8FF).contains(&code)
        && !(0xF0000..=0x10FFFD).contains(&code)
        && !(0xFDD0..=0xFDEF).contains(&code)
        && code & 0xFFFF != 0xFFFE
        && code & 0xFFFF != 0xFFFF
}

fn two_byte_code(bytes: &[u8]) -> Option<u16> {
    (bytes.len() == 2).then(|| u16::from_be_bytes([bytes[0], bytes[1]]))
}

/// Splits a CMap into the tokens the two `bf*` sections use, skipping comments,
/// dictionaries and anything else the sections do not read.
fn tokenize(text: &str) -> Vec<Tok<'_>> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            byte if byte.is_ascii_whitespace() => at += 1,
            b'%' => {
                while at < bytes.len() && bytes[at] != b'\n' {
                    at += 1;
                }
            }
            b'[' => {
                tokens.push(Tok::Open);
                at += 1;
            }
            b']' => {
                tokens.push(Tok::Close);
                at += 1;
            }
            b'<' if bytes.get(at + 1) == Some(&b'<') => at += 2,
            b'<' => {
                let start = at + 1;
                let Some(offset) = text[start..].find('>') else { break };
                let end = start + offset;
                let digits: Vec<u8> =
                    bytes[start..end].iter().copied().filter(|byte| !byte.is_ascii_whitespace()).collect();
                tokens.push(Tok::Hex(hex_to_bytes(&digits)));
                at = end + 1;
            }
            b'/' => {
                while at < bytes.len() && !bytes[at].is_ascii_whitespace() {
                    at += 1;
                }
            }
            byte if byte.is_ascii_alphabetic() => {
                let start = at;
                while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_') {
                    at += 1;
                }
                tokens.push(Tok::Keyword(&text[start..at]));
            }
            _ => at += 1,
        }
        // A bounded number of tokens: a stream of one-digit tokens cannot grow
        // this list past the stream's own length, and that is already capped.
        if tokens.len() > MAX_STREAM {
            break;
        }
    }
    tokens
}

/// Hex digits to bytes; an odd last digit is padded with a zero.
fn hex_to_bytes(digits: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(digits.len() / 2 + 1);
    let mut index = 0;
    while index < digits.len() {
        let Some(high) = hex_value(digits[index]) else { break };
        match digits.get(index + 1).and_then(|byte| hex_value(*byte)) {
            Some(low) => out.push(high << 4 | low),
            None if index + 1 == digits.len() => {
                out.push(high << 4);
                break;
            }
            None => break,
        }
        index += 2;
    }
    out
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
