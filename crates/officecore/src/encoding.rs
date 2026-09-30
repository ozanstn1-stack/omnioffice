//! Text decoding for the 8-bit fallback path.
//!
//! Document formats are UTF-8 in practice, but TXT/CSV imports and older files
//! are not. The previous fallback cast every byte to a `char` (ISO-8859-1),
//! which turns the Windows punctuation in the C1 range into invisible control
//! characters and - for exactly the Turkish documents this suite is built for -
//! turns İ, ş, ğ and ı into Ý, þ, ð and ý.
//!
//! The decoder below is Windows-1254 (Turkish): Windows-1252 plus the six
//! Turkish positions. Those six are the only difference from the Western code
//! page, so an Icelandic ð/þ/ý would decode as a Turkish letter; every other
//! Western language is unaffected, and 0xA0..=0xFF stays byte-identical to
//! Latin-1 apart from them.

/// Printable Windows-1252 replacements for 0x80..=0x9F. The five positions
/// the code page leaves undefined map to themselves, like other decoders do.
const WINDOWS_1252_C1: [char; 32] = [
    '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}',
    '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
];

/// Decodes legacy 8-bit text (Windows-1254 / Windows-1252 / ISO-8859-1).
///
/// Callers that have a real charset (the CSV import option, for example) should
/// keep honouring it; this is the fallback for bytes that are not valid UTF-8.
pub fn decode_legacy(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&byte| match byte {
            0x80..=0x9F => WINDOWS_1252_C1[(byte - 0x80) as usize],
            // The six positions where Windows-1254 differs from Windows-1252.
            0xD0 => 'Ğ',
            0xDD => 'İ',
            0xDE => 'Ş',
            0xF0 => 'ğ',
            0xFD => 'ı',
            0xFE => 'ş',
            other => other as char,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turkish_letters_survive_the_legacy_path() {
        // Windows-1254 bytes for Ğ İ Ş ğ ı ş.
        let bytes = [0xD0u8, 0xDD, 0xDE, 0xF0, 0xFD, 0xFE];
        assert_eq!(decode_legacy(&bytes), "ĞİŞğış");
        // The Latin-1 cast this replaced produced Ý, þ, ð, ý instead.
        assert!(!decode_legacy(&bytes).contains('Ý'));
    }

    #[test]
    fn c1_bytes_become_windows_punctuation() {
        // 0x93/0x94 curly quotes, 0x96 en dash, 0x80 euro, 0x85 ellipsis.
        assert_eq!(decode_legacy(&[0x93, 0x94, 0x96, 0x80, 0x85]), "“”–€…");
        assert!(!decode_legacy(&[0x96]).chars().next().unwrap().is_control());
    }

    #[test]
    fn ascii_and_latin1_are_unchanged() {
        assert_eq!(decode_legacy(b"Rapor 2026 - plain"), "Rapor 2026 - plain");
        // 0xE7 is ç in both Latin-1 and Windows-1254.
        assert_eq!(decode_legacy(&[0xE7, 0x61]), "ça");
    }
}
