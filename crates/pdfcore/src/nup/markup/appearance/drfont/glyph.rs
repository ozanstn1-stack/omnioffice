//! A glyph name to the character it stands for.
//!
//! `/Differences` lists glyph names and the appearance needs characters: the
//! extra-glyph table covers the Latin extras a Turkish document uses, and the
//! `uniXXXX`/`uXXXX` forms cover what else a producer may write.

/// A glyph name to a character: the extra-glyph table, `uniXXXX` and `uXXXX`.
pub(super) fn glyph_symbol(name: &[u8]) -> Option<char> {
    let text = std::str::from_utf8(name).ok()?;
    if let Some(symbol) = super::super::textfit::glyph_symbol(text) {
        return Some(symbol);
    }
    if let Some(hex) = text.strip_prefix("uni") {
        return u32::from_str_radix(hex, 16).ok().and_then(char::from_u32);
    }
    let hex = text.strip_prefix('u')?;
    u32::from_str_radix(hex, 16).ok().and_then(char::from_u32).filter(|_| (4..=6).contains(&hex.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_names_resolve() {
        assert_eq!(glyph_symbol(b"gbreve"), Some('ğ'));
        assert_eq!(glyph_symbol(b"uni011F"), Some('ğ'));
        assert_eq!(glyph_symbol(b"u011F"), Some('ğ'));
        assert_eq!(glyph_symbol(b"notaglyph"), None);
    }
}
