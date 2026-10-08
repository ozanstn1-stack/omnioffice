//! Formula text across file formats.
//!
//! The editor keeps a formula the way the formula bar shows it: `=SUM(A1,B2)`
//! with plain function names. The files do not:
//!
//! * XLSX stores every function Excel added after 2007 with a `_xlfn.` prefix
//!   (`_xlfn.NORM.DIST`), `SORT` and `FILTER` with `_xlfn._xlws.`, and the
//!   names a `LET` defines with `_xlpm.`. Without the prefix Excel opens the
//!   file with `#NAME?` in every such cell.
//! * ODF writes `;` between arguments, `[.A1:.B2]` or `[$Sheet2.A1]` for
//!   references, `{1;2|3;4}` for inline arrays and `COM.MICROSOFT.` in front of
//!   the Excel functions it has no name for.
//!
//! The function tables are Microsoft's list of "future functions" as
//! LibreOffice imports it (checked against LibreOffice 24.2 for every function
//! it implements) plus the Excel 365 dynamic-array family. A name outside the
//! tables is left alone, so a formula that only Excel understands survives a
//! round trip through the editor as it came.

/// Editor-side name -> stored prefix, for functions Excel stores as `_xlfn.NAME`.
/// Sorted for binary search; `tables_are_sorted_and_unique` keeps it so.
#[rustfmt::skip]
const XLFN: &[&str] = &[
    "ACOT", "ACOTH", "AGGREGATE", "ANCHORARRAY", "ARABIC", "ARRAYTOTEXT", "BAHTTEXT", "BASE", "BETA.DIST", "BETA.INV",
    "BINOM.DIST", "BINOM.DIST.RANGE", "BINOM.INV", "BITAND", "BITLSHIFT", "BITOR", "BITRSHIFT", "BITXOR",
    "CEILING.MATH", "CEILING.PRECISE", "CHISQ.DIST", "CHISQ.DIST.RT", "CHISQ.INV", "CHISQ.INV.RT", "CHISQ.TEST",
    "CHOOSECOLS", "CHOOSEROWS", "COMBINA", "CONCAT", "CONFIDENCE.NORM", "CONFIDENCE.T", "COT", "COTH", "COVARIANCE.P",
    "COVARIANCE.S", "CSC", "CSCH", "DAYS", "DECIMAL", "DROP", "ERF.PRECISE", "ERFC.PRECISE", "EXPAND", "EXPON.DIST",
    "F.DIST", "F.DIST.RT", "F.INV", "F.INV.RT", "F.TEST", "FILTERXML", "FLOOR.MATH", "FLOOR.PRECISE", "FORECAST.ETS",
    "FORECAST.ETS.CONFINT", "FORECAST.ETS.SEASONALITY", "FORECAST.ETS.STAT", "FORECAST.LINEAR", "FORMULATEXT",
    "GAMMA", "GAMMA.DIST", "GAMMA.INV", "GAMMALN.PRECISE", "GAUSS", "HSTACK", "HYPGEOM.DIST", "IFNA", "IFS",
    "ISFORMULA", "ISOWEEKNUM", "LET", "LOGNORM.DIST", "LOGNORM.INV", "MAXIFS", "MINIFS", "MODE.MULT", "MODE.SNGL",
    "MUNIT", "NEGBINOM.DIST", "NORM.DIST", "NORM.INV", "NORM.S.DIST", "NORM.S.INV", "NUMBERVALUE", "PDURATION",
    "PERCENTILE.EXC", "PERCENTILE.INC", "PERCENTRANK.EXC", "PERCENTRANK.INC", "PERMUTATIONA", "PHI", "POISSON.DIST",
    "QUARTILE.EXC", "QUARTILE.INC", "RANDARRAY", "RANK.AVG", "RANK.EQ", "RRI", "SEC", "SECH", "SEQUENCE", "SHEET",
    "SHEETS", "SINGLE", "SKEW.P", "SORTBY", "STDEV.P", "STDEV.S", "SWITCH", "T.DIST", "T.DIST.2T", "T.DIST.RT",
    "T.INV", "T.INV.2T", "T.TEST", "TAKE", "TEXTAFTER", "TEXTBEFORE", "TEXTJOIN", "TEXTSPLIT", "TOCOL", "TOROW",
    "UNICHAR", "UNICODE", "UNIQUE", "VALUETOTEXT", "VAR.P", "VAR.S", "VSTACK", "WEBSERVICE", "WEIBULL.DIST",
    "WRAPCOLS", "WRAPROWS", "XLOOKUP", "XMATCH", "XOR", "Z.TEST",
];

/// The two functions Excel stores in the worksheet namespace (`_xlfn._xlws.SORT`).
const XLFN_WS: &[&str] = &["FILTER", "SORT"];

/// Members of [`XLFN`] that ODF already has a name for, so the file carries them
/// without `COM.MICROSOFT.`.
#[rustfmt::skip]
const ODF_OWN: &[&str] = &[
    "ACOT", "ACOTH", "ARABIC", "BASE", "BITAND", "BITLSHIFT", "BITOR", "BITRSHIFT", "BITXOR", "COMBINA", "COT", "COTH",
    "CSC", "CSCH", "DAYS", "DECIMAL", "GAMMA", "GAUSS", "IFNA", "ISFORMULA", "ISOWEEKNUM", "MUNIT", "NUMBERVALUE",
    "PDURATION", "PERMUTATIONA", "PHI", "RRI", "SEC", "SECH", "SHEET", "SHEETS", "UNICHAR", "UNICODE", "XOR",
];

/// Plain in XLSX, but ODF's own function of the same name behaves differently
/// (`CEILING` rounds negative numbers another way), so LibreOffice keeps the
/// Excel one under `COM.MICROSOFT.`.
const ODF_EXCEL_ONLY: &[&str] = &["CEILING", "FLOOR", "ISO.CEILING", "NETWORKDAYS.INTL", "WORKDAY.INTL"];

/// Editor name <-> ODF name where ODF spells the function differently.
const ODF_RENAMES: &[(&str, &str)] = &[("FORMULATEXT", "FORMULA"), ("SKEW.P", "SKEWP")];

/// Prefixes ODF producers put in front of a function name.
const ODF_PREFIXES: &[&str] = &["COM.MICROSOFT.", "ORG.OPENOFFICE.", "ORG.LIBREOFFICE.", "LEGACY."];

/// Prefixes OOXML puts in front of a name; all of them are dropped on import.
const XLSX_PREFIXES: &[&str] = &["_xlfn.", "_xlws.", "_xlpm."];

fn listed(table: &[&str], name: &str) -> bool {
    table.binary_search(&name).is_ok()
}

/// The prefix Excel stores `name` with, or `None` for a plain function.
pub fn xlsx_prefix(name: &str) -> Option<&'static str> {
    let upper = name.to_ascii_uppercase();
    if listed(XLFN, &upper) {
        Some("_xlfn.")
    } else if listed(XLFN_WS, &upper) {
        Some("_xlfn._xlws.")
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Tokens
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum Token<'a> {
    /// A `"..."` string literal, quotes included.
    Text(&'a str),
    /// A `'...'` quoted sheet name, quotes included.
    Quoted(&'a str),
    /// A `[...]` group: a structured reference in editor text, a reference in ODF.
    Group(&'a str),
    /// A name, cell reference or function name.
    Word(&'a str),
    /// A number literal.
    Number(&'a str),
    /// Any other single character (operators, separators, spaces).
    Other(char),
}

fn word_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '.' | '$' | '\\')
}

/// Splits formula text into tokens. Every byte of the input lands in exactly one
/// token, so concatenating the tokens gives the input back.
fn tokenize(formula: &str) -> Vec<Token<'_>> {
    let bytes = formula.as_bytes();
    let mut tokens = Vec::new();
    let mut at = 0usize;
    while let Some(c) = formula[at..].chars().next() {
        let start = at;
        at += c.len_utf8();
        match c {
            '"' | '\'' => {
                // A doubled quote is an escaped quote and keeps the literal open.
                // Stepping bytes is safe here: a continuation byte is never a quote.
                while at < bytes.len() {
                    let byte = bytes[at];
                    at += 1;
                    if byte == c as u8 {
                        if bytes.get(at) != Some(&byte) {
                            break;
                        }
                        at += 1;
                    }
                }
                let text = &formula[start..at];
                tokens.push(if c == '"' { Token::Text(text) } else { Token::Quoted(text) });
            }
            '[' => {
                let mut depth = 1usize;
                let mut quoted = false;
                while at < bytes.len() && depth > 0 {
                    match bytes[at] {
                        b'\'' => quoted = !quoted,
                        b'[' if !quoted => depth += 1,
                        b']' if !quoted => depth -= 1,
                        _ => {}
                    }
                    at += 1;
                }
                tokens.push(Token::Group(&formula[start..at]));
            }
            _ if c.is_ascii_digit() || (c == '.' && bytes.get(at).is_some_and(u8::is_ascii_digit)) => {
                let digits = |from: usize| bytes[from..].iter().take_while(|byte| byte.is_ascii_digit()).count();
                at += digits(at);
                if c != '.' && bytes.get(at) == Some(&b'.') {
                    at += 1 + digits(at + 1);
                }
                // An exponent needs digits after it; `1E` is a number and a word.
                if bytes.get(at).is_some_and(|byte| byte.eq_ignore_ascii_case(&b'e')) {
                    let sign = usize::from(matches!(bytes.get(at + 1), Some(b'+' | b'-')));
                    let exponent = digits(at + 1 + sign);
                    if exponent > 0 {
                        at += 1 + sign + exponent;
                    }
                }
                tokens.push(Token::Number(&formula[start..at]));
            }
            _ if word_char(c) && c != '.' => {
                while let Some(next) = formula[at..].chars().next().filter(|next| word_char(*next)) {
                    at += next.len_utf8();
                }
                tokens.push(Token::Word(&formula[start..at]));
            }
            _ => tokens.push(Token::Other(c)),
        }
    }
    tokens
}

/// True when the token at `index` is a function name: a word straight in front
/// of `(`.
fn is_call(tokens: &[Token<'_>], index: usize) -> bool {
    matches!(tokens.get(index), Some(Token::Word(_))) && tokens.get(index + 1) == Some(&Token::Other('('))
}

fn push_token(out: &mut String, token: &Token<'_>) {
    match token {
        Token::Text(text) | Token::Quoted(text) | Token::Group(text) | Token::Word(text) | Token::Number(text) => {
            out.push_str(text)
        }
        Token::Other(c) => out.push(*c),
    }
}

/// Removes every OOXML prefix a name can carry (`_xlfn._xlws.SORT` -> `SORT`).
fn strip_prefixes<'a>(word: &'a str, prefixes: &[&str]) -> &'a str {
    let mut rest = word;
    'again: loop {
        for prefix in prefixes {
            if rest.len() > prefix.len()
                && rest.is_char_boundary(prefix.len())
                && rest[..prefix.len()].eq_ignore_ascii_case(prefix)
            {
                rest = &rest[prefix.len()..];
                continue 'again;
            }
        }
        return rest;
    }
}

// ---------------------------------------------------------------------------
// Error values
// ---------------------------------------------------------------------------

/// The codes Excel accepts in a `t="e"` cell.
const EXCEL_ERRORS: &[&str] = &[
    "#BLOCKED!",
    "#CALC!",
    "#CONNECT!",
    "#DIV/0!",
    "#EXTERNAL!",
    "#FIELD!",
    "#GETTING_DATA",
    "#N/A",
    "#NAME?",
    "#NULL!",
    "#NUM!",
    "#PYTHON!",
    "#REF!",
    "#SPILL!",
    "#UNKNOWN!",
    "#VALUE!",
];

/// An error value as an XLSX error cell must hold it. A code Excel knows is kept
/// (`#CALC!`, `#SPILL!`); LibreOffice's `Err:5xx` become their Excel equivalent;
/// anything else is `#VALUE!`, because an unknown literal makes Excel report the
/// file as damaged.
pub fn excel_error(text: &str) -> &'static str {
    let text = text.trim();
    if let Some(code) = EXCEL_ERRORS.iter().find(|code| code.eq_ignore_ascii_case(text)) {
        return code;
    }
    match text.strip_prefix("Err:").and_then(|number| number.trim().parse::<u32>().ok()) {
        Some(503) => "#NUM!",
        Some(524) => "#REF!",
        Some(525) => "#NAME?",
        Some(532) => "#DIV/0!",
        _ => "#VALUE!",
    }
}

// ---------------------------------------------------------------------------
// XLSX
// ---------------------------------------------------------------------------

/// One open parenthesis while a formula is rewritten for XLSX.
#[derive(Default)]
struct Frame {
    /// The call is `LET(`: its odd-numbered arguments are names.
    is_let: bool,
    /// Index of the argument being read.
    argument: usize,
    /// No token of the current argument has been seen yet.
    at_start: bool,
    /// Names whose value has been read, so later arguments can use them (upper case).
    names: Vec<String>,
    /// The name being defined; it is in scope once its value has ended.
    pending: Option<String>,
}

/// Formula text as an XLSX `<f>` element holds it: no leading `=`, future
/// functions prefixed with `_xlfn.` and `LET` names with `_xlpm.`.
pub fn to_xlsx(formula: &str) -> String {
    let body = formula.trim().trim_start_matches('=');
    let tokens = tokenize(body);
    let mut out = String::with_capacity(body.len() + 16);
    let mut frames: Vec<Frame> = Vec::new();
    let mut opens_let = false;
    let mut index = 0usize;
    while index < tokens.len() {
        let token = tokens[index];
        match token {
            Token::Word(word) if is_call(&tokens, index) => {
                // A name that carries a prefix already is not in the tables.
                match xlsx_prefix(word) {
                    Some(prefix) => {
                        out.push_str(prefix);
                        out.push_str(&word.to_ascii_uppercase());
                    }
                    None => out.push_str(word),
                }
                opens_let = word.eq_ignore_ascii_case("LET");
                if let Some(frame) = frames.last_mut() {
                    frame.at_start = false;
                }
                index += 1;
                continue;
            }
            Token::Word(word) => {
                let name = word.to_ascii_uppercase();
                let next = tokens[index + 1..].iter().find(|token| **token != Token::Other(' '));
                let defining = frames
                    .last()
                    .is_some_and(|frame| frame.is_let && frame.at_start && frame.argument.is_multiple_of(2))
                    && next == Some(&Token::Other(','));
                if defining {
                    if let Some(frame) = frames.last_mut() {
                        frame.pending = Some(name);
                    }
                    out.push_str("_xlpm.");
                } else if tokens.get(index + 1) != Some(&Token::Other('!'))
                    && index.checked_sub(1).and_then(|before| tokens.get(before)) != Some(&Token::Other('!'))
                    && frames.iter().any(|frame| frame.names.contains(&name))
                {
                    out.push_str("_xlpm.");
                }
                out.push_str(word);
            }
            Token::Other('(') => {
                frames.push(Frame { is_let: opens_let, at_start: true, ..Default::default() });
                out.push('(');
            }
            Token::Other(')') => {
                frames.pop();
                out.push(')');
            }
            Token::Other(',') => {
                if let Some(frame) = frames.last_mut() {
                    // The comma after a name's value brings the name into scope.
                    if frame.argument % 2 == 1 {
                        frame.names.extend(frame.pending.take());
                    }
                    frame.argument += 1;
                    frame.at_start = true;
                }
                out.push(',');
            }
            other => push_token(&mut out, &other),
        }
        opens_let = false;
        if !matches!(token, Token::Other(' ') | Token::Other('(') | Token::Other(',')) {
            if let Some(frame) = frames.last_mut() {
                frame.at_start = false;
            }
        }
        index += 1;
    }
    out
}

/// An XLSX formula as the editor holds it: leading `=`, every OOXML prefix gone.
pub fn from_xlsx(formula: &str) -> String {
    format!("={}", unprefix_xlsx(formula))
}

/// The same without the `=`, for the formulas inside rules and table columns.
pub fn unprefix_xlsx(formula: &str) -> String {
    let body = formula.trim().trim_start_matches('=');
    let mut out = String::with_capacity(body.len());
    for token in tokenize(body) {
        match token {
            Token::Word(word) => out.push_str(strip_prefixes(word, XLSX_PREFIXES)),
            other => push_token(&mut out, &other),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// ODF
// ---------------------------------------------------------------------------

/// The name ODF files use for an editor function.
fn odf_function(name: &str) -> String {
    let upper = name.to_ascii_uppercase();
    if let Some((_, odf)) = ODF_RENAMES.iter().find(|(ours, _)| *ours == upper) {
        return (*odf).to_string();
    }
    let excel_only = (listed(XLFN, &upper) && !listed(ODF_OWN, &upper))
        || listed(XLFN_WS, &upper)
        || ODF_EXCEL_ONLY.contains(&upper.as_str());
    if excel_only {
        format!("COM.MICROSOFT.{upper}")
    } else {
        name.to_string()
    }
}

/// The editor name for a function an ODF file spells its own way.
fn editor_function(name: &str) -> String {
    let bare = strip_prefixes(name, ODF_PREFIXES);
    let upper = bare.to_ascii_uppercase();
    match ODF_RENAMES.iter().find(|(_, odf)| *odf == upper) {
        Some((ours, _)) => (*ours).to_string(),
        None => bare.to_string(),
    }
}

fn is_column(word: &str) -> bool {
    let letters = word.strip_prefix('$').unwrap_or(word);
    (1..=3).contains(&letters.len()) && letters.chars().all(|c| c.is_ascii_alphabetic())
}

fn is_row(word: &str) -> bool {
    let digits = word.strip_prefix('$').unwrap_or(word);
    !digits.is_empty() && digits.len() <= 7 && digits.chars().all(|c| c.is_ascii_digit())
}

fn is_cell(word: &str) -> bool {
    let rest = word.strip_prefix('$').unwrap_or(word);
    let letters = rest.chars().take_while(char::is_ascii_alphabetic).count();
    let after = &rest[letters..];
    (1..=3).contains(&letters) && is_row(after)
}

/// The sheet-less part of a reference at `index` (`A1`, `A1:B2`, `A:C`, `2:4`)
/// and how many tokens it spans, or `None` when it is not one.
fn reference_at<'a>(tokens: &[Token<'a>], index: usize) -> Option<(String, usize)> {
    let first = match tokens.get(index)? {
        Token::Word(word) | Token::Number(word) => *word,
        _ => return None,
    };
    let kind = |word: &str| (is_cell(word), is_column(word), is_row(word));
    let (cell, column, row) = kind(first);
    if !(cell || column || row) {
        return None;
    }
    if tokens.get(index + 1) == Some(&Token::Other(':')) {
        if let Some(Token::Word(second) | Token::Number(second)) = tokens.get(index + 2) {
            let (cell2, column2, row2) = kind(second);
            if (cell && cell2) || (column && column2) || (row && row2) {
                return Some((format!(".{first}:.{second}"), 3));
            }
        }
    }
    // A lone column or row is a name or a number, not a reference.
    cell.then(|| (format!(".{first}"), 1))
}

/// An editor formula as an ODF `table:formula` value (`of:=...`).
pub fn to_odf(formula: &str) -> String {
    let body = formula.trim().trim_start_matches('=');
    let tokens = tokenize(body);
    let mut out = String::with_capacity(body.len() + 8);
    out.push_str("of:=");
    let mut braces = 0usize;
    let mut index = 0usize;
    while index < tokens.len() {
        // `Sheet2!A1` and `'My Sheet'!A1:B2`
        let sheet = match tokens[index] {
            Token::Word(name) if tokens.get(index + 1) == Some(&Token::Other('!')) && !is_call(&tokens, index) => {
                Some(name.to_string())
            }
            Token::Quoted(name) if tokens.get(index + 1) == Some(&Token::Other('!')) => Some(name.to_string()),
            _ => None,
        };
        if let Some(sheet) = &sheet {
            if let Some((reference, used)) = reference_at(&tokens, index + 2) {
                out.push_str(&format!("[${sheet}{reference}]"));
                index += 2 + used;
                continue;
            }
        }
        if let Some((reference, used)) = reference_at(&tokens, index).filter(|_| !is_call(&tokens, index)) {
            out.push_str(&format!("[{reference}]"));
            index += used;
            continue;
        }
        match tokens[index] {
            Token::Word(word) if is_call(&tokens, index) => out.push_str(&odf_function(word)),
            Token::Other('{') => {
                braces += 1;
                out.push('{');
            }
            Token::Other('}') => {
                braces = braces.saturating_sub(1);
                out.push('}');
            }
            Token::Other(',') => out.push(';'),
            Token::Other(';') if braces > 0 => out.push('|'),
            other => push_token(&mut out, &other),
        }
        index += 1;
    }
    out
}

/// One end of an ODF reference: `$Sheet2.A1`, `$'My Sheet'.A1`, `.A1`.
fn odf_endpoint(text: &str) -> Option<(Option<String>, String)> {
    let mut quoted = false;
    let mut split = None;
    for (index, c) in text.char_indices() {
        match c {
            '\'' => quoted = !quoted,
            '.' if !quoted => {
                split = Some(index);
                break;
            }
            _ => {}
        }
    }
    let split = split?;
    let sheet = text[..split].trim_start_matches('$');
    let cell = &text[split + 1..];
    if cell.is_empty() {
        return None;
    }
    Some(((!sheet.is_empty()).then(|| sheet.to_string()), cell.to_string()))
}

/// `[$Sheet2.A1:.B2]` as editor text, or `None` for something that is not a
/// plain reference (`#REF!`).
fn odf_reference(group: &str) -> Option<String> {
    let inner = group.strip_prefix('[')?.strip_suffix(']')?;
    let mut quoted = false;
    let mut colon = None;
    for (index, c) in inner.char_indices() {
        match c {
            '\'' => quoted = !quoted,
            ':' if !quoted => {
                colon = Some(index);
                break;
            }
            _ => {}
        }
    }
    let (first, second) = match colon {
        Some(at) => (&inner[..at], Some(&inner[at + 1..])),
        None => (inner, None),
    };
    let (sheet, start) = odf_endpoint(first)?;
    let mut out = String::new();
    if let Some(sheet) = sheet {
        out.push_str(&sheet);
        out.push('!');
    }
    out.push_str(&start);
    if let Some(second) = second {
        let (_, end) = odf_endpoint(second)?;
        out.push(':');
        out.push_str(&end);
    }
    Some(out)
}

/// An ODF `table:formula` value as editor text (`=...`).
pub fn from_odf(formula: &str) -> String {
    let body = formula.trim();
    let body = ["of:=", "oooc:=", "msoxl:=", "="].iter().find_map(|prefix| body.strip_prefix(prefix)).unwrap_or(body);
    let tokens = tokenize(body);
    let mut out = String::with_capacity(body.len() + 1);
    out.push('=');
    let mut braces = 0usize;
    for (index, token) in tokens.iter().enumerate() {
        match token {
            Token::Group(group) => match odf_reference(group) {
                Some(reference) => out.push_str(&reference),
                // Not a plain reference (`[.#REF!]`): keep what is inside.
                None => out.push_str(group.trim_matches(['[', ']']).trim_start_matches('.')),
            },
            Token::Word(word) if is_call(&tokens, index) => out.push_str(&editor_function(word)),
            Token::Other('{') => {
                braces += 1;
                out.push('{');
            }
            Token::Other('}') => {
                braces = braces.saturating_sub(1);
                out.push('}');
            }
            // Arguments and the columns of an inline array both use `;`; `|` ends an array row.
            Token::Other(';') => out.push(','),
            Token::Other('|') if braces > 0 => out.push(';'),
            other => push_token(&mut out, other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_literals_are_kept_or_replaced_by_one_excel_accepts() {
        for code in EXCEL_ERRORS {
            assert_eq!(excel_error(code), *code);
        }
        assert_eq!(excel_error("#calc!"), "#CALC!");
        assert_eq!(excel_error(" #N/A "), "#N/A");
        assert_eq!(excel_error("Err:503"), "#NUM!");
        assert_eq!(excel_error("Err:532"), "#DIV/0!");
        assert_eq!(excel_error("Err:502"), "#VALUE!");
        assert_eq!(excel_error(""), "#VALUE!");
        assert_eq!(excel_error("divide by zero"), "#VALUE!");
    }

    #[test]
    fn tables_are_sorted_and_unique() {
        assert!(EXCEL_ERRORS.windows(2).all(|pair| pair[0] < pair[1]));
        for table in [XLFN, XLFN_WS, ODF_OWN] {
            assert!(table.windows(2).all(|pair| pair[0] < pair[1]), "table out of order near {:?}", table);
        }
        for name in ODF_OWN {
            assert!(listed(XLFN, name), "{name} is ODF-native but not a future function");
        }
        assert!(XLFN.iter().all(|name| !listed(XLFN_WS, name)));
    }

    #[test]
    fn tokens_cover_the_input() {
        for formula in [
            "SUM(A1:B2)*2%",
            "IF(A1=\"a \"\"quoted\"\" b\",'My Sheet'!A1,1.5E+3)",
            "Sales[[#This Row],[Amount]]+1",
            "{1,2;3,4}",
            "T.DIST.2T(-1.2,10)",
            "naïve + 日本",
        ] {
            let mut joined = String::new();
            for token in tokenize(formula) {
                push_token(&mut joined, &token);
            }
            assert_eq!(joined, formula);
        }
    }

    #[test]
    fn xlsx_prefixes_future_functions() {
        assert_eq!(to_xlsx("=NORM.DIST(1,0,1,TRUE)"), "_xlfn.NORM.DIST(1,0,1,TRUE)");
        assert_eq!(to_xlsx("=SUM(A1:A3)+T.DIST.2T(2,5)"), "SUM(A1:A3)+_xlfn.T.DIST.2T(2,5)");
        assert_eq!(to_xlsx("=hstack(A1:A2,B1:B2)"), "_xlfn.HSTACK(A1:A2,B1:B2)");
        assert_eq!(to_xlsx("=SORT(A1:A9,1,-1)"), "_xlfn._xlws.SORT(A1:A9,1,-1)");
        assert_eq!(to_xlsx("=FILTER(A1:B9,B1:B9>3)"), "_xlfn._xlws.FILTER(A1:B9,B1:B9>3)");
        assert_eq!(to_xlsx("=IFERROR(SUBTOTAL(9,A1:A4),0)"), "IFERROR(SUBTOTAL(9,A1:A4),0)");
        assert_eq!(to_xlsx("=CEILING(2.5,1)+ISO.CEILING(2.5)"), "CEILING(2.5,1)+ISO.CEILING(2.5)");
    }

    #[test]
    fn xlsx_prefix_leaves_text_and_references_alone() {
        assert_eq!(to_xlsx("=\"NORM.DIST(\"&A1"), "\"NORM.DIST(\"&A1");
        // A name that merely looks like a future function is not a call.
        assert_eq!(to_xlsx("=DAYS+1"), "DAYS+1");
        assert_eq!(to_xlsx("='SORT'!A1+Phi!B2"), "'SORT'!A1+Phi!B2");
        assert_eq!(to_xlsx("=Sales[Phi]"), "Sales[Phi]");
        // Already prefixed text is not prefixed twice.
        assert_eq!(to_xlsx("=_xlfn.XLOOKUP(1,A:A,B:B)"), "_xlfn.XLOOKUP(1,A:A,B:B)");
    }

    #[test]
    fn xlsx_let_names_get_the_parameter_prefix() {
        assert_eq!(to_xlsx("=LET(x,5,x*2)"), "_xlfn.LET(_xlpm.x,5,_xlpm.x*2)");
        assert_eq!(to_xlsx("=LET(a, A1, b, a+1, a*b)"), "_xlfn.LET(_xlpm.a, A1, _xlpm.b, _xlpm.a+1, _xlpm.a*_xlpm.b)");
        assert_eq!(to_xlsx("=LET(n,3,SUM(SEQUENCE(n)))"), "_xlfn.LET(_xlpm.n,3,SUM(_xlfn.SEQUENCE(_xlpm.n)))");
        assert_eq!(to_xlsx("=LET(x,1,x)"), "_xlfn.LET(_xlpm.x,1,_xlpm.x)");
        // The value of a name is read before the name exists.
        assert_eq!(to_xlsx("=LET(x,x+1,x)"), "_xlfn.LET(_xlpm.x,x+1,_xlpm.x)");
        // Outside the LET the same word is an ordinary name.
        assert_eq!(to_xlsx("=LET(x,1,x)+x"), "_xlfn.LET(_xlpm.x,1,_xlpm.x)+x");
        assert_eq!(to_xlsx("=LET(x,1,Sheet2!x)"), "_xlfn.LET(_xlpm.x,1,Sheet2!x)");
    }

    #[test]
    fn xlsx_reading_drops_every_prefix() {
        assert_eq!(from_xlsx("_xlfn.NORM.DIST(1,0,1,TRUE)"), "=NORM.DIST(1,0,1,TRUE)");
        assert_eq!(from_xlsx("_xlfn._xlws.SORT(A1:A9)"), "=SORT(A1:A9)");
        assert_eq!(from_xlsx("_xlfn.LET(_xlpm.x,5,_xlpm.x*2)"), "=LET(x,5,x*2)");
        assert_eq!(from_xlsx("=SUM(A1)"), "=SUM(A1)");
        assert_eq!(from_xlsx("_xlfn.WEBSERVICE(\"_xlfn.x\")"), "=WEBSERVICE(\"_xlfn.x\")");
        assert_eq!(from_xlsx("_XLFN.TAKE(A1:C3,2)"), "=TAKE(A1:C3,2)");
    }

    #[test]
    fn xlsx_formulas_round_trip() {
        for formula in [
            "=NORM.DIST(A1,0,1,TRUE)+T.DIST(1,2,FALSE)",
            "=LET(x,5,y,x*2,x+y)",
            "=TEXTJOIN(\", \",TRUE,A1:A3)",
            "=VSTACK(A1:B2,SORT(D1:E2,1,-1))",
            "=SUM(Sales[Amount])",
        ] {
            assert_eq!(from_xlsx(&to_xlsx(formula)), formula);
        }
    }

    #[test]
    fn odf_formula_uses_semicolons_and_bracketed_references() {
        assert_eq!(to_odf("=SUM(A1:B2)+1"), "of:=SUM([.A1:.B2])+1");
        assert_eq!(to_odf("=IF(A1>1,B1,$C$2)"), "of:=IF([.A1]>1;[.B1];[.$C$2])");
        assert_eq!(to_odf("=Sheet2!A1+'My Sheet'!B2:C3"), "of:=[$Sheet2.A1]+[$'My Sheet'.B2:.C3]");
        assert_eq!(to_odf("=SUM(A:A)+SUM(2:4)"), "of:=SUM([.A:.A])+SUM([.2:.4])");
        assert_eq!(to_odf("={1,2;3,4}"), "of:={1;2|3;4}");
    }

    #[test]
    fn odf_formula_keeps_function_names_and_text() {
        assert_eq!(to_odf("=ATAN2(1,1)+LOG10(100)"), "of:=ATAN2(1;1)+LOG10(100)");
        assert_eq!(to_odf("=SUMX2MY2(A1:A2,B1:B2)"), "of:=SUMX2MY2([.A1:.A2];[.B1:.B2])");
        assert_eq!(to_odf("=IF(A1,\"a,b;A1\",\"\"\"B2\"\"\")"), "of:=IF([.A1];\"a,b;A1\";\"\"\"B2\"\"\")");
        assert_eq!(to_odf("=A1&\"x\""), "of:=[.A1]&\"x\"");
        assert_eq!(to_odf("=VAT_RATE*A1"), "of:=VAT_RATE*[.A1]");
    }

    #[test]
    fn odf_formula_prefixes_excel_functions() {
        assert_eq!(to_odf("=NORM.DIST(A1,0,1,TRUE)"), "of:=COM.MICROSOFT.NORM.DIST([.A1];0;1;TRUE)");
        assert_eq!(
            to_odf("=CEILING(A1,1)+FLOOR(A2,1)"),
            "of:=COM.MICROSOFT.CEILING([.A1];1)+COM.MICROSOFT.FLOOR([.A2];1)"
        );
        assert_eq!(to_odf("=SORT(A1:A3)"), "of:=COM.MICROSOFT.SORT([.A1:.A3])");
        // ODF has its own names for these.
        assert_eq!(to_odf("=DAYS(A1,A2)+XOR(TRUE,FALSE)+PHI(1)"), "of:=DAYS([.A1];[.A2])+XOR(TRUE;FALSE)+PHI(1)");
        assert_eq!(to_odf("=FORMULATEXT(A1)"), "of:=FORMULA([.A1])");
        assert_eq!(to_odf("=SUM(A1)"), "of:=SUM([.A1])");
    }

    #[test]
    fn odf_formula_reads_what_libreoffice_writes() {
        assert_eq!(from_odf("of:=SUM([.A1:.A2])"), "=SUM(A1:A2)");
        assert_eq!(from_odf("of:=[.A1]*2"), "=A1*2");
        assert_eq!(from_odf("of:=COM.MICROSOFT.NORM.DIST([.A1];0;1;1)"), "=NORM.DIST(A1,0,1,1)");
        assert_eq!(from_odf("of:=IF([.A1]>1;\"a;b\";[$Sheet2.B2])"), "=IF(A1>1,\"a;b\",Sheet2!B2)");
        assert_eq!(from_odf("of:=SUM([$'My Sheet'.A1:.B2])"), "=SUM('My Sheet'!A1:B2)");
        assert_eq!(from_odf("of:=SUM([Sheet2.A1:Sheet2.B2])"), "=SUM(Sheet2!A1:B2)");
        assert_eq!(from_odf("of:=FORMULA([.A1])"), "=FORMULATEXT(A1)");
        assert_eq!(from_odf("of:={1;2|3;4}"), "={1,2;3,4}");
        assert_eq!(from_odf("of:=[.A:.A]"), "=A:A");
        assert_eq!(from_odf("of:=ORG.OPENOFFICE.EASTERSUNDAY(2024)"), "=EASTERSUNDAY(2024)");
        assert_eq!(from_odf("of:=[.#REF!]+1"), "=#REF!+1");
    }

    #[test]
    fn odf_formulas_round_trip() {
        for formula in [
            "=SUM(A1:B2)+1",
            "=IF(A1>1,\"x;y\",Sheet2!$B$2)",
            "=T.INV.2T(0.05,A1)+ATAN2(1,2)",
            "=SUM('My Sheet'!A1:B2)",
            "=XOR(A1,B1)+SKEW.P(A1:A5)",
        ] {
            assert_eq!(from_odf(&to_odf(formula)), formula);
        }
    }
}
