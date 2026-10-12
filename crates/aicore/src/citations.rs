//! Verified citations for document chat.
//!
//! The chat model is asked to back every claim with a page marker **and a short
//! quote** from the source, e.g. `[page 3: "toplam 1.200,00 TL"]`. This module
//! parses the markers the model may use — `[page 3]`, `[sayfa 3]`,
//! `[pages 3-4]`, `[sayfalar 3-4]`, `(p. 4)`, `(s. 4)` and the quoted forms —
//! and checks each quote against the retrieved page text with a fuzzy match.
//! A citation to a page that was not retrieved, or whose quote does not match,
//! is left unverified so the UI can say so instead of the user trusting it.

/// One citation the model wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCitation {
    pub page: u32,
    /// The short quote the model attached, when it gave one.
    pub quote: String,
}

/// A minimum token overlap for a multi-word quote to count as verified.
const MATCH_THRESHOLD: f64 = 0.7;

/// Lowercases and folds Turkish letters, keeps only alphanumeric words.
pub fn normalize_for_match(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        let folded = match ch.to_lowercase().next().unwrap_or(ch) {
            'ı' | 'i' => 'i',
            'ğ' => 'g',
            'ş' => 's',
            'ç' => 'c',
            'ö' => 'o',
            'ü' => 'u',
            'â' | 'a' => 'a',
            'î' => 'i',
            'û' => 'u',
            other => other,
        };
        if folded.is_alphanumeric() {
            out.push(folded);
        } else {
            out.push(' ');
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn tokens(normalized: &str) -> Vec<&str> {
    normalized.split_whitespace().filter(|token| token.chars().count() >= 2).collect()
}

/// True when `quote` plausibly comes from `source`: a short quote must appear
/// verbatim, a longer one must share most of its words with the source.
pub fn quote_matches(quote: &str, source: &str) -> bool {
    let quote_norm = normalize_for_match(quote);
    let source_norm = normalize_for_match(source);
    if quote_norm.is_empty() || source_norm.is_empty() {
        return false;
    }
    let quote_tokens = tokens(&quote_norm);
    if quote_tokens.len() < 3 {
        // Too short for an overlap test; require a verbatim substring.
        return source_norm.contains(&quote_norm);
    }
    let source_set: std::collections::HashSet<&str> = source_norm.split_whitespace().collect();
    let present = quote_tokens.iter().filter(|token| source_set.contains(**token)).count();
    (present as f64) / (quote_tokens.len() as f64) >= MATCH_THRESHOLD
}

/// Parses every citation marker in `text`, expanding a page range into one
/// citation per page.
pub fn parse_citations(text: &str) -> Vec<ParsedCitation> {
    parse_citations_expanded(text)
}

/// `page 3`, `sayfa 3`, `page 3: "quote"`.
fn parse_bracket(inner: &str) -> Option<ParsedCitation> {
    let trimmed = inner.trim();
    let (keyword, rest) = trimmed.split_once(char::is_whitespace)?;
    if !matches!(keyword.trim().to_lowercase().as_str(), "page" | "sayfa" | "s." | "sf") {
        return None;
    }
    let (pages_part, quote) = split_quote(rest.trim());
    let page: u32 = pages_part.trim().parse().ok()?;
    Some(ParsedCitation { page, quote })
}

/// `(p. 4)`, `(s. 4)`, `(p.4)`.
fn parse_parenthesis(inner: &str) -> Option<ParsedCitation> {
    let trimmed = inner.trim();
    let lower = trimmed.to_lowercase();
    let rest = lower.strip_prefix("p.").or_else(|| lower.strip_prefix("s."))?;
    let (pages_part, quote) = split_quote(rest.trim());
    let page: u32 = pages_part.trim().parse().ok()?;
    Some(ParsedCitation { page, quote })
}

/// Splits `3: "quote"` / `3` / `3-4` into the page part and the quote.
fn split_quote(rest: &str) -> (String, String) {
    if let Some((pages, tail)) = rest.split_once(':') {
        let quote = tail.trim().trim_matches(['"', '\'', '“', '”', '‘', '’']).trim().to_string();
        return (pages.trim().to_string(), quote);
    }
    (rest.trim().to_string(), String::new())
}

/// Expands `[pages 3-4]` into one citation per page (the parser returns the
/// range's first page; this helper gives every page in the marker).
pub fn parse_citations_expanded(text: &str) -> Vec<ParsedCitation> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '[' {
            if let Some(end) = chars[index + 1..].iter().position(|ch| *ch == ']') {
                let inner: String = chars[index + 1..index + 1 + end].iter().collect();
                out.extend(expand_bracket(&inner));
                index += end + 2;
                continue;
            }
        }
        if chars[index] == '(' {
            if let Some(end) = chars[index + 1..].iter().position(|ch| *ch == ')') {
                let inner: String = chars[index + 1..index + 1 + end].iter().collect();
                if let Some(citation) = parse_parenthesis(&inner) {
                    out.push(citation);
                }
                index += end + 2;
                continue;
            }
        }
        index += 1;
    }
    out
}

fn expand_bracket(inner: &str) -> Vec<ParsedCitation> {
    let trimmed = inner.trim();
    let Some((keyword, rest)) = trimmed.split_once(char::is_whitespace) else {
        return Vec::new();
    };
    let rest = rest.trim();
    if !matches!(keyword.trim().to_lowercase().as_str(), "pages" | "sayfalar") {
        return parse_bracket(inner).into_iter().collect();
    }
    let (pages_part, quote) = split_quote(rest);
    let Some((start, end)) = pages_part.split_once(['-', '–']) else {
        return Vec::new();
    };
    let (Ok(start), Ok(end)) = (start.trim().parse::<u32>(), end.trim().parse::<u32>()) else {
        return Vec::new();
    };
    if end < start || end - start > 20 {
        return Vec::new();
    }
    (start..=end).map(|page| ParsedCitation { page, quote: quote.clone() }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_supported_formats() {
        assert_eq!(parse_citations("Bkz. [page 3]"), vec![ParsedCitation { page: 3, quote: String::new() }]);
        assert_eq!(parse_citations("Bkz. [sayfa 3]"), vec![ParsedCitation { page: 3, quote: String::new() }]);
        assert_eq!(parse_citations("Bkz. (p. 4)"), vec![ParsedCitation { page: 4, quote: String::new() }]);
        assert_eq!(parse_citations("Bkz. (s. 4)"), vec![ParsedCitation { page: 4, quote: String::new() }]);
        let quoted = parse_citations("[page 3: \"toplam 1.200,00 TL\"]");
        assert_eq!(quoted.len(), 1);
        assert_eq!(quoted[0].page, 3);
        assert_eq!(quoted[0].quote, "toplam 1.200,00 TL");
    }

    #[test]
    fn expands_a_page_range() {
        let expanded = parse_citations_expanded("Bkz. [pages 3-4]");
        assert_eq!(expanded.iter().map(|c| c.page).collect::<Vec<_>>(), vec![3, 4]);
        let turkish = parse_citations_expanded("Bkz. [sayfalar 2-3]");
        assert_eq!(turkish.iter().map(|c| c.page).collect::<Vec<_>>(), vec![2, 3]);
    }

    #[test]
    fn ignores_ordinary_text() {
        assert!(parse_citations("No citation here.").is_empty());
        assert!(parse_citations("[page x]").is_empty());
        assert!(parse_citations("[pages 5-3]").is_empty());
    }

    #[test]
    fn verifies_a_quote_against_the_source() {
        let source = "Mal hizmet toplam tutarı 1.200,00 TL olarak hesaplanmıştır ve KDV dahildir.";
        assert!(quote_matches("toplam tutarı 1.200,00 TL", source));
        // A short quote must appear verbatim.
        assert!(quote_matches("KDV dahildir", source));
        // An unrelated quote is rejected.
        assert!(!quote_matches("kedi köpek balık", source));
        assert!(!quote_matches("", source));
    }

    #[test]
    fn normalizes_turkish_letters() {
        assert_eq!(normalize_for_match("İSTANBUL ığşçöü"), "istanbul igscou");
        // The same quote with different case/letters still matches.
        let source = "Ödenecek tutar 2.205,00 TL'dir.";
        assert!(quote_matches("ödenecek tutar", source));
    }
}
