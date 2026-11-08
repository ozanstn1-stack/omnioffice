//! The code space of one CMap: which codes it admits, and of which width.
//!
//! A CMap declares its codes as ranges, and a code is only meaningful inside
//! one. Most fonts declare a single two-byte range (`<0000> <FFFF>`); a Japanese
//! Shift-JIS font declares a one-byte range and a two-byte one, and then the
//! width is part of the code: `<41>` and `<0041>` are two different codes.
//!
//! One rule keeps such a map readable by a viewer: the ranges must be
//! **prefix-free**. If a one-byte range covered the first byte of a two-byte
//! range, a viewer would split our string at the wrong place and draw the wrong
//! glyphs without any error, so a map that breaks it is refused as a whole.

use super::super::lex::Token;
use super::super::Code;

/// The one-byte and two-byte ranges a code must fall in.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Space {
    /// Two-byte ranges as byte pairs, so a code is matched byte by byte.
    wide: Vec<([u8; 2], [u8; 2])>,
    /// One-byte ranges.
    narrow: Vec<(u8, u8)>,
}

impl Space {
    /// Reads `begincodespacerange` entries and the position after its end.
    pub(super) fn read(tokens: &[Token<'_>], mut at: usize, limit: usize) -> Option<(Self, usize)> {
        let mut space = Self::default();
        while let Some(Token::Hex(low)) = tokens.get(at) {
            let Token::Hex(high) = tokens.get(at + 1)? else { return None };
            space.push(Code::from_bytes(low)?, Code::from_bytes(high)?)?;
            at += 2;
            if space.ranges() > limit {
                return None;
            }
        }
        match tokens.get(at) {
            Some(Token::Keyword("endcodespacerange")) => Some((space, at + 1)),
            _ => None,
        }
    }

    /// One range; both ends must have the same width, and it must not be
    /// reversed.
    fn push(&mut self, low: Code, high: Code) -> Option<()> {
        if low.width() != high.width() || high < low {
            return None;
        }
        match low.width() {
            1 => self.narrow.push((low.value() as u8, high.value() as u8)),
            _ => self.wide.push((low.value().to_be_bytes(), high.value().to_be_bytes())),
        }
        Some(())
    }

    /// Whether any one-byte range was declared.
    pub(super) fn has_narrow(&self) -> bool {
        !self.narrow.is_empty()
    }

    /// Whether a code of this width has a range to fall in at all.
    pub(super) fn has_width(&self, width: u8) -> bool {
        match width {
            1 => !self.narrow.is_empty(),
            _ => !self.wide.is_empty(),
        }
    }

    /// Whether `code`, at its own width, falls inside a declared range.
    pub(super) fn contains(&self, code: Code) -> bool {
        match code.width() {
            1 => {
                let value = code.value() as u8;
                self.narrow.iter().any(|(low, high)| (*low..=*high).contains(&value))
            }
            _ => {
                let bytes = code.value().to_be_bytes();
                self.wide
                    .iter()
                    .any(|(low, high)| (low[0]..=high[0]).contains(&bytes[0]) && (low[1]..=high[1]).contains(&bytes[1]))
            }
        }
    }

    /// Whether no one-byte range can be mistaken for the first byte of a
    /// two-byte code. `identity` stands for the whole two-byte space.
    pub(super) fn prefix_free(&self, identity: bool) -> bool {
        let overlaps_lead = |range: (u8, u8)| {
            if identity {
                return true;
            }
            self.wide.iter().any(|(low, high)| range.0 <= high[0] && low[0] <= range.1)
        };
        !self.narrow.iter().copied().any(overlaps_lead)
    }

    /// Whether nothing at all was declared.
    pub(super) fn is_empty(&self) -> bool {
        self.wide.is_empty() && self.narrow.is_empty()
    }

    /// Declares the whole two-byte space, which an identity base stands for.
    pub(super) fn identity(&mut self) {
        if self.wide.is_empty() {
            self.wide.push(([0, 0], [0xFF, 0xFF]));
        }
    }

    fn ranges(&self) -> usize {
        self.wide.len() + self.narrow.len()
    }
}
