//! Function names in XLSX and ODS files.
//!
//! Excel 2010 and later store the functions added after Excel 2007 with a
//! `_xlfn.` prefix (`_xlfn._xlws.` for SORT and FILTER) and refuse to evaluate
//! the plain name; ODF writes the same functions as `COM.MICROSOFT.*` or under
//! its own name. The editor keeps plain names, so the prefix is added on export
//! and stripped on import. The lists below are the Calc function catalogue
//! (`src/office/calc/functions/*.ts`, 274 functions) split by what the file
//! must say; `every_registered_function_is_classified_once` fails when the
//! catalogue grows, so a new function cannot ship without being classified.
//!
//! To refresh the lists after adding a function, print `functionNames()` from
//! `src/office/calc/registry.ts` once `registerBuiltinFunctions()` has run and
//! sort the new names into the three groups.

use officecore::formula::{from_odf, from_xlsx, to_odf, to_xlsx};
use officecore::model::*;
use officecore::zip::{ZipLimits, ZipReader, ZipWriter};
use officecore::{odf, xlsx};
use std::collections::BTreeSet;
use std::path::PathBuf;

/// Stored as `_xlfn.NAME` (77 functions, the Excel 2010 to 365 additions).
#[rustfmt::skip]
const PREFIXED: &[&str] = &[
    "AGGREGATE", "ARRAYTOTEXT", "BINOM.DIST", "BINOM.DIST.RANGE", "BINOM.INV", "CEILING.MATH", "CEILING.PRECISE",
    "CHOOSECOLS", "CHOOSEROWS", "COMBINA", "CONCAT", "CONFIDENCE.NORM", "CONFIDENCE.T", "COVARIANCE.P",
    "COVARIANCE.S", "DAYS", "DROP", "EXPAND", "EXPON.DIST", "FLOOR.MATH", "FLOOR.PRECISE", "FORECAST.LINEAR",
    "FORMULATEXT", "GAUSS", "HSTACK", "IFNA", "IFS", "ISFORMULA", "ISOWEEKNUM", "LET", "MAXIFS", "MINIFS",
    "MODE.MULT", "MODE.SNGL", "NORM.DIST", "NORM.INV", "NORM.S.DIST", "NORM.S.INV", "NUMBERVALUE", "PERCENTILE.EXC",
    "PERCENTILE.INC", "PERCENTRANK.EXC", "PERCENTRANK.INC", "PERMUTATIONA", "PHI", "POISSON.DIST", "QUARTILE.EXC",
    "QUARTILE.INC", "RANK.EQ", "SEQUENCE", "SORTBY", "STDEV.P", "STDEV.S", "SWITCH", "T.DIST", "T.DIST.2T",
    "T.DIST.RT", "T.INV", "T.INV.2T", "TAKE", "TEXTAFTER", "TEXTBEFORE", "TEXTJOIN", "TEXTSPLIT", "TOCOL", "TOROW",
    "UNICHAR", "UNICODE", "UNIQUE", "VAR.P", "VAR.S", "VSTACK", "WRAPCOLS", "WRAPROWS", "XLOOKUP", "XMATCH", "XOR",
];

/// Stored as `_xlfn._xlws.NAME`.
const WORKSHEET_NAMESPACE: &[&str] = &["FILTER", "SORT"];

/// Stored under their plain name: Excel 2007 and earlier, and the ones Excel
/// keeps plain on purpose (`CEILING`, `ISO.CEILING`, `NORMDIST`, ...).
#[rustfmt::skip]
const PLAIN: &[&str] = &[
    "ABS", "ACOS", "ADDRESS", "AND", "ASIN", "ATAN", "ATAN2", "AVEDEV", "AVERAGE", "AVERAGEIF", "AVERAGEIFS",
    "BINOMDIST", "CEILING", "CELL", "CHAR", "CHOOSE", "CLEAN", "CODE", "COLUMN", "COLUMNS", "COMBIN", "CONCATENATE",
    "CONFIDENCE", "CORREL", "COS", "COUNT", "COUNTA", "COUNTBLANK", "COUNTIF", "COUNTIFS", "COUNTUNIQUE", "COVAR",
    "CRITBINOM", "CUMIPMT", "CUMPRINC", "DATE", "DATEDIF", "DATEVALUE", "DAY", "DB", "DDB", "DEGREES", "DEVSQ",
    "DOLLAR", "EDATE", "EFFECT", "EOMONTH", "EVEN", "EXACT", "EXP", "EXPONDIST", "FACT", "FACTDOUBLE", "FALSE",
    "FIND", "FIXED", "FLOOR", "FORECAST", "FV", "GCD", "GEOMEAN", "GROWTH", "HARMEAN", "HLOOKUP", "HOUR",
    "HYPERLINK", "IF", "IFERROR", "INDEX", "INDIRECT", "INFO", "INT", "INTERCEPT", "IRR", "ISBLANK", "ISERR",
    "ISERROR", "ISEVEN", "ISLOGICAL", "ISNA", "ISNONTEXT", "ISNUMBER", "ISO.CEILING", "ISODD", "ISTEXT", "LARGE",
    "LCM", "LEFT", "LEN", "LN", "LOG", "LOG10", "LOOKUP", "LOWER", "MATCH", "MAX", "MEDIAN", "MID", "MIN", "MINUTE",
    "MIRR", "MOD", "MODE", "MONTH", "MROUND", "MULTINOMIAL", "N", "NA", "NETWORKDAYS", "NOMINAL", "NORMDIST",
    "NORMINV", "NORMSDIST", "NORMSINV", "NOT", "NOW", "NPER", "NPV", "ODD", "OFFSET", "OR", "PEARSON", "PERCENTILE",
    "PERCENTRANK", "PERMUT", "PI", "PMT", "POISSON", "POWER", "PRODUCT", "PROPER", "PV", "QUARTILE", "QUOTIENT",
    "RADIANS", "RAND", "RANDBETWEEN", "RANK", "RATE", "REPLACE", "REPT", "RIGHT", "ROUND", "ROUNDDOWN", "ROUNDUP",
    "ROW", "ROWS", "RSQ", "SEARCH", "SECOND", "SIGN", "SIN", "SLN", "SLOPE", "SMALL", "SQRT", "STANDARDIZE",
    "STDEV", "STDEVP", "STEYX", "SUBSTITUTE", "SUBTOTAL", "SUM", "SUMIF", "SUMIFS", "SUMPRODUCT", "SUMSQ",
    "SUMX2MY2", "SUMX2PY2", "SUMXMY2", "SYD", "T", "TAN", "TDIST", "TEXT", "TIME", "TIMEVALUE", "TINV", "TODAY",
    "TRANSPOSE", "TREND", "TRIM", "TRUE", "TRUNC", "UPPER", "VALUE", "VAR", "VARP", "VLOOKUP", "WEEKDAY", "WEEKNUM",
    "WORKDAY", "XIRR", "XNPV", "YEAR",
];

fn registered() -> BTreeSet<&'static str> {
    PREFIXED.iter().chain(WORKSHEET_NAMESPACE).chain(PLAIN).copied().collect()
}

#[test]
fn every_registered_function_is_classified_once() {
    let total = PREFIXED.len() + WORKSHEET_NAMESPACE.len() + PLAIN.len();
    assert_eq!(total, 274, "the catalogue has 274 functions; update the lists and the README count together");
    assert_eq!(registered().len(), total, "a function is listed in two groups");
}

/// Reads the registrations out of the TypeScript sources when the repository
/// layout is there (it is not in a packaged crate): every
/// `registerFunction("NAME"` must be in the lists above.
#[test]
fn catalogue_sources_agree_with_the_lists() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../src/office/calc/functions");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        eprintln!("skipped: {} is not present", dir.display());
        return;
    };
    let known = registered();
    let mut found = BTreeSet::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("ts") {
            continue;
        }
        let source = std::fs::read_to_string(&path).unwrap();
        for opener in ["registerFunction(", "registerContextFunction("] {
            for (at, _) in source.match_indices(opener) {
                let rest = source[at + opener.len()..].trim_start();
                if let Some(rest) = rest.strip_prefix('"') {
                    if let Some(end) = rest.find('"') {
                        found.insert(rest[..end].to_string());
                    }
                }
            }
        }
    }
    assert!(
        found.len() > 150,
        "the scan found only {} registrations; has the registration style changed?",
        found.len()
    );
    for name in &found {
        assert!(known.contains(name.as_str()), "{name} is registered in the catalogue but not classified here");
    }
}

#[test]
fn prefixed_functions_get_their_prefix_in_xlsx() {
    for name in PREFIXED {
        assert_eq!(to_xlsx(&format!("={name}(B2:C3)+1")), format!("_xlfn.{name}(B2:C3)+1"), "{name}");
    }
    for name in WORKSHEET_NAMESPACE {
        assert_eq!(to_xlsx(&format!("={name}(B2:C3)+1")), format!("_xlfn._xlws.{name}(B2:C3)+1"), "{name}");
    }
    for name in PLAIN {
        assert_eq!(to_xlsx(&format!("={name}(B2:C3)+1")), format!("{name}(B2:C3)+1"), "{name}");
    }
}

#[test]
fn every_catalogue_function_round_trips_through_xlsx_text() {
    for name in registered() {
        let formula = format!("={name}(A1,B2:C3)+1");
        assert_eq!(from_xlsx(&to_xlsx(&formula)), formula, "{name}");
    }
}

fn formula_workbook(formulas: &[&str]) -> Workbook {
    let mut workbook = Workbook::new_blank("Functions");
    let sheet = &mut workbook.sheets[0];
    sheet.name = "Data".into();
    for (row, value) in [4.0, 8.0, 15.0, 16.0, 23.0, 42.0].iter().enumerate() {
        sheet.set(&format!("A{}", row + 1), Cell { value: CellValue::Number(*value), ..Default::default() });
    }
    for (index, formula) in formulas.iter().enumerate() {
        sheet.set(
            &format!("C{}", index + 1),
            Cell { value: CellValue::Number(1.0), formula: Some((*formula).into()), ..Default::default() },
        );
    }
    workbook
}

/// The new functions the Calc engine gained, one formula each.
const NEW_FUNCTION_FORMULAS: &[&str] = &[
    "=SUBTOTAL(9,A1:A6)",
    "=AGGREGATE(9,6,A1:A6)",
    "=NORM.DIST(A1,10,5,TRUE)",
    "=T.DIST(1.5,10,TRUE)",
    "=HSTACK(A1:A2,A3:A4)",
    "=VSTACK(A1:A2,A3:A4)",
    "=TAKE(A1:A6,3)",
    "=DROP(A1:A6,2)",
    "=CHOOSECOLS(A1:C2,1,3)",
    "=TEXTBEFORE(\"a-b-c\",\"-\")",
    "=ARRAYTOTEXT(A1:A3)",
    "=FORECAST.LINEAR(7,A1:A6,A1:A6)",
    "=CEILING.MATH(4.2)",
    "=MODE.MULT(A1:A6)",
    "=PERCENTRANK.INC(A1:A6,15)",
    "=BINOM.DIST.RANGE(10,0.5,3,5)",
    "=CONFIDENCE.T(0.05,2,10)",
    "=PHI(0.5)+GAUSS(1)",
    "=DAYS(A5,A1)",
    "=SORT(A1:A6,1,-1)",
    "=FILTER(A1:A6,A1:A6>10)",
    "=XLOOKUP(15,A1:A6,A1:A6)",
    "=LET(x,A1+1,y,x*2,x+y)",
    "=SUM(SEQUENCE(3))",
    "=IFERROR(UNIQUE(A1:A6),0)",
];

fn sheet_part(bytes: &[u8]) -> String {
    ZipReader::open(bytes.to_vec()).unwrap().read_text("xl/worksheets/sheet1.xml").unwrap()
}

#[test]
fn xlsx_export_writes_excel_names_and_import_reads_them_back() {
    let workbook = formula_workbook(NEW_FUNCTION_FORMULAS);
    let bytes = xlsx::write_xlsx(&workbook).unwrap();
    let sheet = sheet_part(&bytes);
    for expected in [
        "<f>SUBTOTAL(9,A1:A6)</f>",
        "<f>_xlfn.AGGREGATE(9,6,A1:A6)</f>",
        "<f>_xlfn.NORM.DIST(A1,10,5,TRUE)</f>",
        "<f>_xlfn.T.DIST(1.5,10,TRUE)</f>",
        "<f>_xlfn.HSTACK(A1:A2,A3:A4)</f>",
        "<f>_xlfn.TAKE(A1:A6,3)</f>",
        "<f>_xlfn.CHOOSECOLS(A1:C2,1,3)</f>",
        "<f>_xlfn.TEXTBEFORE(\"a-b-c\",\"-\")</f>",
        "<f>_xlfn.ARRAYTOTEXT(A1:A3)</f>",
        "<f>_xlfn.FORECAST.LINEAR(7,A1:A6,A1:A6)</f>",
        "<f>_xlfn.CEILING.MATH(4.2)</f>",
        "<f>_xlfn.MODE.MULT(A1:A6)</f>",
        "<f>_xlfn.PERCENTRANK.INC(A1:A6,15)</f>",
        "<f>_xlfn.BINOM.DIST.RANGE(10,0.5,3,5)</f>",
        "<f>_xlfn.CONFIDENCE.T(0.05,2,10)</f>",
        "<f>_xlfn.PHI(0.5)+_xlfn.GAUSS(1)</f>",
        "<f>_xlfn.DAYS(A5,A1)</f>",
        "<f>_xlfn._xlws.SORT(A1:A6,1,-1)</f>",
        "<f>_xlfn._xlws.FILTER(A1:A6,A1:A6&gt;10)</f>",
        "<f>_xlfn.XLOOKUP(15,A1:A6,A1:A6)</f>",
        "<f>_xlfn.LET(_xlpm.x,A1+1,_xlpm.y,_xlpm.x*2,_xlpm.x+_xlpm.y)</f>",
        "<f>SUM(_xlfn.SEQUENCE(3))</f>",
        "<f>IFERROR(_xlfn.UNIQUE(A1:A6),0)</f>",
    ] {
        assert!(sheet.contains(expected), "missing {expected} in {sheet}");
    }
    // The file must not carry a bare future function Excel would not evaluate.
    assert!(!sheet.contains("<f>NORM.DIST"));

    let read = xlsx::read_workbook_bytes(&bytes).unwrap();
    let data = &read.workbook.sheets[0];
    for (index, formula) in NEW_FUNCTION_FORMULAS.iter().enumerate() {
        let cell = data.get(&format!("C{}", index + 1)).unwrap_or_else(|| panic!("C{} lost", index + 1));
        assert_eq!(cell.formula.as_deref(), Some(*formula), "C{}", index + 1);
    }
}

/// The worksheet part of a file Excel wrote: prefixed formulas with cached
/// values, a dynamic-array spill anchor and a LET.
const EXCEL_SHEET: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n",
    "<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">",
    "<dimension ref=\"A1:C4\"/><sheetData>",
    "<row r=\"1\"><c r=\"A1\"><v>4</v></c><c r=\"B1\"><f>_xlfn.NORM.DIST(A1,0,1,TRUE)</f><v>0.99996832875816688</v></c></row>",
    "<row r=\"2\"><c r=\"A2\"><v>9</v></c><c r=\"B2\"><f>_xlfn.CONCAT(A1,A2)</f><v>49</v></c></row>",
    "<row r=\"3\"><c r=\"A3\"><v>16</v></c><c r=\"B3\"><f>_xlfn.LET(_xlpm.n,A3,_xlpm.n*2)</f><v>32</v></c></row>",
    "<row r=\"4\"><c r=\"A4\"><v>25</v></c><c r=\"B4\"><f>SUM(_xlfn._xlws.SORT(A1:A4,1,-1))</f><v>54</v></c>",
    "<c r=\"C4\"><f>_xlfn.IFNA(_xlfn.XLOOKUP(9,A1:A4,A1:A4),0)</f><v>9</v></c></row>",
    "</sheetData></worksheet>",
);

fn with_sheet(bytes: &[u8], sheet: &str) -> Vec<u8> {
    let reader = ZipReader::open(bytes.to_vec()).unwrap();
    let mut writer = ZipWriter::new();
    for (name, data) in reader.read_all(ZipLimits::default()).unwrap() {
        if name == "xl/worksheets/sheet1.xml" {
            writer.add_text(&name, sheet);
        } else {
            writer.add(&name, &data);
        }
    }
    writer.finish()
}

#[test]
fn xlsx_import_strips_the_prefixes_excel_writes() {
    let package = with_sheet(&xlsx::write_xlsx(&formula_workbook(&[])).unwrap(), EXCEL_SHEET);
    let read = xlsx::read_workbook_bytes(&package).unwrap();
    let data = &read.workbook.sheets[0];
    let formula = |address: &str| data.get(address).and_then(|cell| cell.formula.clone());
    assert_eq!(formula("B1").as_deref(), Some("=NORM.DIST(A1,0,1,TRUE)"));
    assert_eq!(formula("B2").as_deref(), Some("=CONCAT(A1,A2)"));
    assert_eq!(formula("B3").as_deref(), Some("=LET(n,A3,n*2)"));
    assert_eq!(formula("B4").as_deref(), Some("=SUM(SORT(A1:A4,1,-1))"));
    assert_eq!(formula("C4").as_deref(), Some("=IFNA(XLOOKUP(9,A1:A4,A1:A4),0)"));
    // The cached value Excel stored comes along with the formula.
    assert_eq!(data.get("B1").map(|cell| cell.value.clone()), Some(CellValue::Number(0.999_968_328_758_166_9)));
    assert_eq!(data.get("B3").map(|cell| cell.value.clone()), Some(CellValue::Number(32.0)));
}

#[test]
fn imported_excel_formulas_export_with_their_prefixes_again() {
    let package = with_sheet(&xlsx::write_xlsx(&formula_workbook(&[])).unwrap(), EXCEL_SHEET);
    let read = xlsx::read_workbook_bytes(&package).unwrap();
    let sheet = sheet_part(&xlsx::write_xlsx(&read.workbook).unwrap());
    assert!(sheet.contains("<f>_xlfn.NORM.DIST(A1,0,1,TRUE)</f>"), "{sheet}");
    assert!(sheet.contains("<f>_xlfn.LET(_xlpm.n,A3,_xlpm.n*2)</f>"), "{sheet}");
    assert!(sheet.contains("<f>SUM(_xlfn._xlws.SORT(A1:A4,1,-1))</f>"), "{sheet}");
    assert!(sheet.contains("<f>_xlfn.IFNA(_xlfn.XLOOKUP(9,A1:A4,A1:A4),0)</f>"), "{sheet}");
}

fn content_xml(bytes: &[u8]) -> String {
    ZipReader::open(bytes.to_vec()).unwrap().read_text("content.xml").unwrap()
}

#[test]
fn ods_export_uses_openformula_syntax_and_libreoffice_function_names() {
    let workbook = formula_workbook(&[
        "=NORM.DIST(A1,10,5,TRUE)",
        "=SUM(A1,A2)+ATAN2(1,1)",
        "=IF(A1>1,\"a;b,c\",\"A1\")",
        "=CEILING(A1,2)+DAYS(A5,A1)",
        "=SUMX2MY2(A1:A3,A4:A6)",
        "=Second!B2+SUM('My Sheet'!A1:B3)",
        "=FORMULATEXT(A1)",
    ]);
    let bytes = odf::write_ods(&workbook).unwrap();
    let content = content_xml(&bytes);
    for expected in [
        "of:=COM.MICROSOFT.NORM.DIST([.A1];10;5;TRUE)",
        "of:=SUM([.A1];[.A2])+ATAN2(1;1)",
        "of:=IF([.A1]&gt;1;&quot;a;b,c&quot;;&quot;A1&quot;)",
        "of:=COM.MICROSOFT.CEILING([.A1];2)+DAYS([.A5];[.A1])",
        "of:=SUMX2MY2([.A1:.A3];[.A4:.A6])",
        "of:=[$Second.B2]+SUM([$&apos;My Sheet&apos;.A1:.B3])",
        "of:=FORMULA([.A1])",
    ] {
        assert!(content.contains(expected), "missing {expected} in {content}");
    }
    let read = odf::read_ods(&bytes).unwrap();
    let data = &read.workbook.sheets[0];
    for (index, expected) in [
        "=NORM.DIST(A1,10,5,TRUE)",
        "=SUM(A1,A2)+ATAN2(1,1)",
        "=IF(A1>1,\"a;b,c\",\"A1\")",
        "=CEILING(A1,2)+DAYS(A5,A1)",
        "=SUMX2MY2(A1:A3,A4:A6)",
        "=Second!B2+SUM('My Sheet'!A1:B3)",
        "=FORMULATEXT(A1)",
    ]
    .iter()
    .enumerate()
    {
        assert_eq!(data.get(&format!("C{}", index + 1)).and_then(|cell| cell.formula.as_deref()), Some(*expected));
    }
}

/// LibreOffice resolves the `of:` in `of:=SUM(...)` through a declared
/// namespace; without it every formula in the file reads `Err:510`.
#[test]
fn ods_declares_the_formula_namespace() {
    let content = content_xml(&odf::write_ods(&formula_workbook(&["=SUM(A1:A3)"])).unwrap());
    assert!(content.contains("xmlns:of=\"urn:oasis:names:tc:opendocument:xmlns:of:1.2\""), "{content}");
}

#[test]
fn every_catalogue_function_round_trips_through_ods_text() {
    for name in registered() {
        let formula = format!("={name}(A1,B2:C3)+1");
        let stored = to_odf(&formula);
        assert!(!stored.contains(&format!("[.{name}]")), "{name} was taken for a cell: {stored}");
        assert_eq!(from_odf(&stored), formula, "{name}");
    }
}

#[test]
fn excel_only_functions_are_microsoft_prefixed_in_ods() {
    for name in PREFIXED.iter().chain(WORKSHEET_NAMESPACE) {
        let stored = to_odf(&format!("={name}(A1)"));
        assert!(
            stored.starts_with("of:=COM.MICROSOFT.")
                || stored.starts_with(&format!("of:={name}("))
                || *name == "FORMULATEXT",
            "{name}: {stored}"
        );
    }
}

/// What LibreOffice 24.2 writes for formulas with Excel-only functions.
#[test]
fn ods_import_reads_libreoffice_formulas() {
    let content = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
        "<office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
        "xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" ",
        "xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" office:version=\"1.3\">",
        "<office:body><office:spreadsheet><table:table table:name=\"Sheet1\">",
        "<table:table-row>",
        "<table:table-cell office:value-type=\"float\" office:value=\"4\"><text:p>4</text:p></table:table-cell>",
        "<table:table-cell table:formula=\"of:=COM.MICROSOFT.NORM.DIST([.A1];0;1;1)\" office:value-type=\"float\" office:value=\"1\"><text:p>1</text:p></table:table-cell>",
        "<table:table-cell table:formula=\"of:=COM.MICROSOFT.T.DIST.2T([.A1];10)+ISOWEEKNUM([.A1])\" office:value-type=\"float\" office:value=\"2\"><text:p>2</text:p></table:table-cell>",
        "<table:table-cell table:formula=\"of:=FORMULA([.A1])\" office:value-type=\"string\" office:string-value=\"\"><text:p></text:p></table:table-cell>",
        "<table:table-cell table:formula=\"of:=SUM([$Sheet1.A1:.A1];[Sheet1.A1:Sheet1.A1])\" office:value-type=\"float\" office:value=\"8\"><text:p>8</text:p></table:table-cell>",
        "</table:table-row></table:table></office:spreadsheet></office:body></office:document-content>",
    );
    let mut writer = ZipWriter::new();
    writer.add_text("mimetype", "application/vnd.oasis.opendocument.spreadsheet");
    writer.add_text("content.xml", content);
    let read = odf::read_ods(&writer.finish()).unwrap();
    let data = &read.workbook.sheets[0];
    let formula = |address: &str| data.get(address).and_then(|cell| cell.formula.clone());
    assert_eq!(formula("B1").as_deref(), Some("=NORM.DIST(A1,0,1,1)"));
    assert_eq!(formula("C1").as_deref(), Some("=T.DIST.2T(A1,10)+ISOWEEKNUM(A1)"));
    assert_eq!(formula("D1").as_deref(), Some("=FORMULATEXT(A1)"));
    assert_eq!(formula("E1").as_deref(), Some("=SUM(Sheet1!A1:A1,Sheet1!A1:A1)"));
}
