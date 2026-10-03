//! Page selection parsing: `1,3,5-8,12` style input, page range lists for
//! splitting, and "every N pages" planning.

use crate::error::{PdfError, PdfResult};
use serde::{Deserialize, Serialize};

/// Parses a page selection expression into a sorted, de-duplicated list of
/// 1-based page numbers. Accepts `*` for "all pages", comma separated numbers
/// and `a-b` ranges. Whitespace is ignored.
pub fn parse_page_selection(input: &str, total_pages: u32) -> PdfResult<Vec<u32>> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(PdfError::InvalidInput("empty page selection".into()));
    }
    if trimmed == "*" || trimmed.eq_ignore_ascii_case("all") {
        return Ok((1..=total_pages).collect());
    }
    let mut pages: Vec<u32> = Vec::new();
    for part in trimmed.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((a, b)) = part.split_once('-') {
            let start: u32 =
                a.trim().parse().map_err(|_| PdfError::InvalidInput(format!("invalid page range '{part}'")))?;
            let end: u32 =
                b.trim().parse().map_err(|_| PdfError::InvalidInput(format!("invalid page range '{part}'")))?;
            if start == 0 || end == 0 || start > end {
                return Err(PdfError::InvalidInput(format!("invalid page range '{part}'")));
            }
            if end > total_pages {
                return Err(PdfError::RangeOutOfBounds);
            }
            pages.extend(start..=end);
        } else if part.eq_ignore_ascii_case("end") {
            pages.push(total_pages);
        } else {
            let n: u32 = part.parse().map_err(|_| PdfError::InvalidInput(format!("invalid page number '{part}'")))?;
            if n == 0 || n > total_pages {
                return Err(PdfError::RangeOutOfBounds);
            }
            pages.push(n);
        }
    }
    if pages.is_empty() {
        return Err(PdfError::InvalidInput("empty page selection".into()));
    }
    pages.sort_unstable();
    pages.dedup();
    Ok(pages)
}

/// A split strategy chosen by the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "mode")]
pub enum SplitMode {
    /// One range per group, e.g. "1-5", "6-10", "11-20".
    Ranges { ranges: Vec<String> },
    /// New document every N pages.
    EveryN { n: u32 },
    /// One document per page.
    Individual,
    /// Cut before each of the given pages (page 1 is always a start).
    AtPages { pages: Vec<u32> },
}

/// Returns the page groups (1-based inclusive ranges) for a split request.
pub fn plan_split(mode: &SplitMode, total_pages: u32) -> PdfResult<Vec<(u32, u32)>> {
    if total_pages == 0 {
        return Err(PdfError::InvalidPdf("document has no pages".into()));
    }
    match mode {
        SplitMode::Ranges { ranges } => {
            let mut groups = Vec::new();
            for r in ranges {
                let parts: Vec<&str> = r.split('-').collect();
                let (a, b) = match parts.as_slice() {
                    [only] => {
                        let n: u32 =
                            only.trim().parse().map_err(|_| PdfError::InvalidInput(format!("invalid range '{r}'")))?;
                        (n, n)
                    }
                    [a, b] => {
                        let a: u32 =
                            a.trim().parse().map_err(|_| PdfError::InvalidInput(format!("invalid range '{r}'")))?;
                        let b: u32 =
                            b.trim().parse().map_err(|_| PdfError::InvalidInput(format!("invalid range '{r}'")))?;
                        (a, b)
                    }
                    _ => return Err(PdfError::InvalidInput(format!("invalid range '{r}'"))),
                };
                if a == 0 || b == 0 || a > b || b > total_pages {
                    return Err(PdfError::RangeOutOfBounds);
                }
                groups.push((a, b));
            }
            if groups.is_empty() {
                return Err(PdfError::InvalidInput("no ranges given".into()));
            }
            Ok(groups)
        }
        SplitMode::EveryN { n } => {
            if *n == 0 {
                return Err(PdfError::InvalidInput("chunk size must be >= 1".into()));
            }
            let mut groups = Vec::new();
            let mut start = 1;
            while start <= total_pages {
                let end = (start + n - 1).min(total_pages);
                groups.push((start, end));
                start = end + 1;
            }
            Ok(groups)
        }
        SplitMode::Individual => Ok((1..=total_pages).map(|p| (p, p)).collect()),
        SplitMode::AtPages { pages } => {
            let mut cuts: Vec<u32> = pages.iter().copied().filter(|p| *p > 1 && *p <= total_pages).collect();
            cuts.sort_unstable();
            cuts.dedup();
            let mut groups = Vec::new();
            let mut start = 1;
            for cut in cuts {
                if cut > start {
                    groups.push((start, cut - 1));
                }
                start = cut.max(start);
            }
            if start <= total_pages {
                groups.push((start, total_pages));
            }
            Ok(groups)
        }
    }
}

/// Expands (start,end) groups to a flat, ordered, 1-based page list.
pub fn groups_to_pages(groups: &[(u32, u32)]) -> Vec<u32> {
    let mut pages = Vec::new();
    for (a, b) in groups {
        pages.extend(*a..=*b);
    }
    pages
}

/// Formats a group list like "1-5, 6-10" for output file naming.
pub fn format_groups(groups: &[(u32, u32)]) -> String {
    groups.iter().map(|(a, b)| if a == b { format!("{a}") } else { format!("{a}-{b}") }).collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_selection_lists() {
        assert_eq!(parse_page_selection("1,3,5-8,12", 20).unwrap(), vec![1, 3, 5, 6, 7, 8, 12]);
        assert_eq!(parse_page_selection(" 2 - 4 ", 10).unwrap(), vec![2, 3, 4]);
        assert_eq!(parse_page_selection("*", 3).unwrap(), vec![1, 2, 3]);
        assert_eq!(parse_page_selection("end", 7).unwrap(), vec![7]);
    }

    #[test]
    fn rejects_out_of_bounds_and_garbage() {
        assert!(matches!(parse_page_selection("9-12", 10), Err(PdfError::RangeOutOfBounds)));
        assert!(matches!(parse_page_selection("abc", 10), Err(PdfError::InvalidInput(_))));
        assert!(matches!(parse_page_selection("", 10), Err(PdfError::InvalidInput(_))));
        assert!(matches!(parse_page_selection("5-2", 10), Err(PdfError::InvalidInput(_))));
    }

    #[test]
    fn plans_splits() {
        let every = SplitMode::EveryN { n: 4 };
        assert_eq!(plan_split(&every, 10).unwrap(), vec![(1, 4), (5, 8), (9, 10)]);
        let individual = SplitMode::Individual;
        assert_eq!(plan_split(&individual, 3).unwrap().len(), 3);
        let at = SplitMode::AtPages { pages: vec![4, 8] };
        assert_eq!(plan_split(&at, 10).unwrap(), vec![(1, 3), (4, 7), (8, 10)]);
        let ranges = SplitMode::Ranges { ranges: vec!["1-5".into(), "6-10".into()] };
        assert_eq!(plan_split(&ranges, 10).unwrap(), vec![(1, 5), (6, 10)]);
    }
}
