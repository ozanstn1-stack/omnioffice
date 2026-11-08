//! The token stream a CMap program is read from.
//!
//! A CMap is producer-written PostScript, so it arrives as untrusted bytes. Two
//! readers share this lexer: the `/ToUnicode` reader in the parent module, which
//! only needs hex strings and the `bf*` keywords, and the `/Encoding` reader in
//! [`super::cid`], which also needs names (for `usecmap`) and integers (for the
//! CIDs). Comments and dictionaries are skipped; a value the reader does not
//! expect is left for it to refuse, never guessed.

/// One token of a CMap stream.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Token<'a> {
    /// A bare word: an operator such as `endbfchar`.
    Keyword(&'a str),
    /// A `<…>` hex string, already decoded.
    Hex(Vec<u8>),
    /// A `/Name`.
    Name(&'a str),
    /// A decimal integer, which may be negative.
    Number(i64),
    /// The `[` of an array destination.
    Open,
    /// The `]` that closes it.
    Close,
}

/// Splits a CMap into tokens, skipping comments, dictionaries and anything else
/// the readers do not consume.
pub(super) fn tokenize(text: &str) -> Vec<Token<'_>> {
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
                tokens.push(Token::Open);
                at += 1;
            }
            b']' => {
                tokens.push(Token::Close);
                at += 1;
            }
            b'<' if bytes.get(at + 1) == Some(&b'<') => at += 2,
            b'<' => {
                let start = at + 1;
                let Some(offset) = text[start..].find('>') else { break };
                let end = start + offset;
                let digits: Vec<u8> =
                    bytes[start..end].iter().copied().filter(|byte| !byte.is_ascii_whitespace()).collect();
                tokens.push(Token::Hex(hex_to_bytes(&digits)));
                at = end + 1;
            }
            b'/' => {
                let start = at + 1;
                at = start;
                while at < bytes.len() && !bytes[at].is_ascii_whitespace() {
                    at += 1;
                }
                tokens.push(Token::Name(&text[start..at]));
            }
            b'-' if bytes.get(at + 1).is_some_and(u8::is_ascii_digit) => {
                let start = at;
                at += 1;
                while at < bytes.len() && bytes[at].is_ascii_digit() {
                    at += 1;
                }
                tokens.push(Token::Number(text[start..at].parse().unwrap_or(i64::MAX)));
            }
            byte if byte.is_ascii_digit() => {
                let start = at;
                while at < bytes.len() && bytes[at].is_ascii_digit() {
                    at += 1;
                }
                tokens.push(Token::Number(text[start..at].parse().unwrap_or(i64::MAX)));
            }
            byte if byte.is_ascii_alphabetic() => {
                let start = at;
                while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_') {
                    at += 1;
                }
                tokens.push(Token::Keyword(&text[start..at]));
            }
            _ => at += 1,
        }
        // A bounded number of tokens: a stream of one-digit tokens cannot grow
        // this list past the stream's own length, and that is already capped.
        if tokens.len() > super::MAX_STREAM {
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
