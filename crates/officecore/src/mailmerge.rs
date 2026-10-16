//! Address mail merge: fill a Writer template's `{{alan}}` placeholders from a
//! data table (CSV) and produce one document per row.
//!
//! A template is any Writer document whose text (paragraph runs and table
//! cells, in the body, headers and footers) contains `{{name}}` placeholders.
//! [`placeholders`] lists them, [`missing_fields`] reports the ones a row does
//! not fill, and [`merge_document`] substitutes them. Turkish characters are
//! plain UTF-8 text, so `{{ad}}` -> `Ayşe Yılmaz` needs no special handling.

use crate::model::{Block, Run, TextDocument};
use std::collections::{BTreeMap, BTreeSet};

/// Field names are `[A-Za-z0-9_]` between double braces.
fn is_field_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

/// Every `{{name}}` placeholder in the document, deduplicated and sorted.
pub fn placeholders(document: &TextDocument) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    collect_in_blocks(&document.blocks, &mut found);
    collect_in_blocks(&document.header, &mut found);
    collect_in_blocks(&document.footer, &mut found);
    found
}

fn collect_in_blocks(blocks: &[Block], found: &mut BTreeSet<String>) {
    for block in blocks {
        match block {
            Block::Paragraph { runs, .. } => {
                // Scan the joined paragraph text so a placeholder split across
                // runs is still found (and reported as missing when unfilled).
                let joined: String = runs.iter().map(|run| run.text.as_str()).collect();
                scan_text(&joined, found);
            }
            Block::Table { table } => {
                for row in &table.rows {
                    for cell in &row.cells {
                        collect_in_blocks(&cell.blocks, found);
                    }
                }
            }
            _ => {}
        }
    }
}

fn scan_text(text: &str, found: &mut BTreeSet<String>) {
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0;
    while index + 1 < chars.len() {
        if chars[index] == '{' && chars[index + 1] == '{' {
            let mut end = index + 2;
            let mut name = String::new();
            while end < chars.len() && is_field_char(chars[end]) {
                name.push(chars[end]);
                end += 1;
            }
            if end + 1 < chars.len() && chars[end] == '}' && chars[end + 1] == '}' && !name.is_empty() {
                found.insert(name);
                index = end + 2;
                continue;
            }
        }
        index += 1;
    }
}

/// Placeholders the template uses but `values` does not provide.
pub fn missing_fields(document: &TextDocument, values: &BTreeMap<String, String>) -> Vec<String> {
    placeholders(document).into_iter().filter(|name| !values.contains_key(name)).collect()
}

/// Replaces every `{{name}}` in `text` with `values[name]`. An unknown
/// placeholder is left untouched (the caller reports it with [`missing_fields`]).
fn substitute(text: &str, values: &BTreeMap<String, String>) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '{' && index + 1 < chars.len() && chars[index + 1] == '{' {
            let mut end = index + 2;
            let mut name = String::new();
            while end < chars.len() && is_field_char(chars[end]) {
                name.push(chars[end]);
                end += 1;
            }
            if end + 1 < chars.len() && chars[end] == '}' && chars[end + 1] == '}' && !name.is_empty() {
                match values.get(&name) {
                    Some(value) => out.push_str(value),
                    None => {
                        out.push_str("{{");
                        out.push_str(&name);
                        out.push_str("}}");
                    }
                }
                index = end + 2;
                continue;
            }
        }
        out.push(chars[index]);
        index += 1;
    }
    out
}

/// True when a known placeholder in the paragraph is split across runs, so a
/// per-run substitution would miss it.
fn runs_span_placeholder(runs: &[Run], values: &BTreeMap<String, String>) -> bool {
    let joined: String = runs.iter().map(|run| run.text.as_str()).collect();
    let mut in_joined = BTreeSet::new();
    scan_text(&joined, &mut in_joined);
    if in_joined.is_empty() {
        return false;
    }
    let mut in_runs = BTreeSet::new();
    for run in runs {
        scan_text(&run.text, &mut in_runs);
    }
    in_joined.iter().any(|name| values.contains_key(name) && !in_runs.contains(name))
}

fn merge_blocks(blocks: &mut [Block], values: &BTreeMap<String, String>) {
    for block in blocks {
        match block {
            Block::Paragraph { runs, .. } => {
                if runs_span_placeholder(runs, values) {
                    // The placeholder is split across runs: merge the paragraph
                    // so it can be filled (formatting follows the first run,
                    // like any generated mail-merge output).
                    let joined: String = runs.iter().map(|run| run.text.as_str()).collect();
                    let filled = substitute(&joined, values);
                    let template = runs.first().cloned().unwrap_or_default();
                    *runs = vec![Run { text: filled, ..template }];
                } else {
                    for run in runs {
                        run.text = substitute(&run.text, values);
                    }
                }
            }
            Block::Table { table } => {
                for row in &mut table.rows {
                    for cell in &mut row.cells {
                        merge_blocks(&mut cell.blocks, values);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Returns a copy of `template` with every placeholder filled from `values`.
pub fn merge_document(template: &TextDocument, values: &BTreeMap<String, String>) -> TextDocument {
    let mut document = template.clone();
    merge_blocks(&mut document.blocks, values);
    merge_blocks(&mut document.header, values);
    merge_blocks(&mut document.footer, values);
    document
}

/// Decodes a CSV/TSV byte buffer: a UTF-8 BOM is stripped, valid UTF-8 is kept
/// as-is, and anything else is read as Windows-1254 (the Turkish Excel default)
/// so names like "Şişli" survive instead of turning into replacement marks.
fn decode_csv(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => bytes.iter().map(|byte| cp1254_char(*byte)).collect(),
    }
}

/// One Windows-1254 byte to a character. The Turkish letters are the only ones
/// that differ from Latin-1; 0x80-0x9F carries the usual cp1252 punctuation.
fn cp1254_char(byte: u8) -> char {
    match byte {
        0xD0 => 'Ğ',
        0xDD => 'İ',
        0xDE => 'Ş',
        0xF0 => 'ğ',
        0xFD => 'ı',
        0xFE => 'ş',
        0x80 => '€',
        0x85 => '…',
        0x91 | 0x92 => '\'',
        0x93 | 0x94 => '"',
        0x96 => '–',
        0x97 => '—',
        other => other as char,
    }
}

/// Parses CSV (RFC-4180 style: quoted fields, `""` escapes, CRLF or LF) into
/// one map per data row. The first row is the header. A semicolon delimiter is
/// detected when the header line has more semicolons than commas.
pub fn parse_rows(bytes: &[u8]) -> Vec<BTreeMap<String, String>> {
    let text = decode_csv(bytes);
    let delimiter = detect_delimiter(&text);
    let records = parse_records(&text, delimiter);
    let Some(header) = records.first() else { return Vec::new() };
    let mut rows = Vec::new();
    for record in records.iter().skip(1) {
        // A blank line is skipped rather than producing an empty row.
        if record.iter().all(|field| field.trim().is_empty()) {
            continue;
        }
        let mut row = BTreeMap::new();
        for (index, name) in header.iter().enumerate() {
            let key = name.trim().to_string();
            if key.is_empty() {
                continue;
            }
            row.insert(key, record.get(index).cloned().unwrap_or_default().trim().to_string());
        }
        rows.push(row);
    }
    rows
}

fn detect_delimiter(text: &str) -> char {
    let header = text.lines().next().unwrap_or("");
    let commas = header.matches(',').count();
    let semicolons = header.matches(';').count();
    if semicolons > commas {
        ';'
    } else {
        ','
    }
}

fn parse_records(text: &str, delimiter: char) -> Vec<Vec<String>> {
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if in_quotes {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(ch);
            }
        } else if ch == '"' {
            in_quotes = true;
        } else if ch == delimiter {
            record.push(std::mem::take(&mut field));
        } else if ch == '\n' {
            record.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut record));
        } else if ch == '\r' {
            // Consumed as part of CRLF; a lone CR is ignored.
        } else {
            field.push(ch);
        }
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    records
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ParaProps, Run, TableCell, TableData, TableRow};

    fn template() -> TextDocument {
        let mut document = TextDocument::new_blank("Mektup");
        document.blocks = vec![
            Block::Paragraph {
                props: ParaProps::default(),
                runs: vec![Run { text: "Sayın {{ad}} {{soyad}},".into(), ..Default::default() }],
            },
            Block::Table {
                table: TableData {
                    rows: vec![TableRow {
                        cells: vec![TableCell {
                            blocks: vec![Block::Paragraph {
                                props: ParaProps::default(),
                                runs: vec![Run { text: "{{sehir}}".into(), ..Default::default() }],
                            }],
                            ..Default::default()
                        }],
                        ..Default::default()
                    }],
                    ..Default::default()
                },
            },
        ];
        document
    }

    fn row(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(key, value)| (key.to_string(), value.to_string())).collect()
    }

    #[test]
    fn lists_placeholders_across_runs_and_tables() {
        let found = placeholders(&template());
        assert_eq!(found.into_iter().collect::<Vec<_>>(), vec!["ad", "sehir", "soyad"]);
    }

    #[test]
    fn merges_turkish_values() {
        let values = row(&[("ad", "Ayşe"), ("soyad", "Yılmaz"), ("sehir", "İstanbul")]);
        let merged = merge_document(&template(), &values);
        let text = merged.plain_text();
        assert!(text.contains("Sayın Ayşe Yılmaz,"), "{text}");
        assert!(text.contains("İstanbul"), "{text}");
        // The original template is untouched.
        assert!(template().plain_text().contains("{{ad}}"));
    }

    #[test]
    fn reports_missing_fields() {
        let values = row(&[("ad", "Ayşe")]);
        assert_eq!(missing_fields(&template(), &values), vec!["sehir", "soyad"]);
        // An unknown placeholder stays in the output rather than vanishing.
        let merged = merge_document(&template(), &values);
        assert!(merged.plain_text().contains("{{soyad}}"));
    }

    #[test]
    fn parses_comma_and_semicolon_csv_with_quotes() {
        let comma = parse_rows(b"ad,soyad\n\"Ayse, Jr.\",Yilmaz\n");
        assert_eq!(comma.len(), 1);
        assert_eq!(comma[0]["ad"], "Ayse, Jr.");
        assert_eq!(comma[0]["soyad"], "Yilmaz");
        // A quoted field with an embedded quote.
        let quoted = parse_rows(b"ad,not\n\"A\"\"B\",x\n");
        assert_eq!(quoted[0]["ad"], "A\"B");
        // A semicolon header wins when it has more semicolons.
        let semi = parse_rows(b"ad;soyad\nAyse;Yilmaz\n");
        assert_eq!(semi[0]["soyad"], "Yilmaz");
    }

    #[test]
    fn blank_lines_are_skipped() {
        let rows = parse_rows(b"ad,soyad\nAyse,Yilmaz\n\n,\n");
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn strips_a_utf8_bom_from_the_header() {
        // Excel writes a BOM; it used to become part of the first header name,
        // so `{{ad}}` never matched the column.
        let rows = parse_rows(b"\xEF\xBB\xBFad,soyad\nAyse,Yilmaz\n");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get("ad").map(String::as_str), Some("Ayse"), "row: {:?}", rows[0]);
    }

    #[test]
    fn decodes_windows_1254_bytes() {
        // Turkish Excel saves CSV as Windows-1254: Ş=0xDE, ş=0xFE.
        let rows = parse_rows(b"ad\n\xDEi\xFEli\n");
        assert_eq!(rows[0].get("ad").map(String::as_str), Some("Şişli"), "row: {:?}", rows[0]);
    }

    #[test]
    fn fills_a_placeholder_split_across_runs() {
        let mut document = TextDocument::new_blank("Bolunmus");
        document.blocks = vec![Block::Paragraph {
            props: ParaProps::default(),
            runs: vec![
                Run { text: "Sayın {{ad".into(), ..Default::default() },
                Run { text: "}},".into(), ..Default::default() },
            ],
        }];
        // Detection sees it even though no single run contains it.
        assert!(placeholders(&document).contains("ad"), "placeholders: {:?}", placeholders(&document));
        let mut values = BTreeMap::new();
        values.insert("ad".to_string(), "Ayşe".to_string());
        let merged = merge_document(&document, &values);
        assert!(merged.plain_text().contains("Sayın Ayşe,"), "merged: {}", merged.plain_text());
    }
}
