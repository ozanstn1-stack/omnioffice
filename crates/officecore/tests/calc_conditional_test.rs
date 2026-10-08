//! Calc conditional formatting beyond cell-value highlights: color scales,
//! data bars, icon sets and formula rules, in XLSX (`cfRule`) and ODS
//! (`calcext:conditional-format`).
//!
//! Round trips compare the whole rule; the hand-written parts are the shapes
//! Excel 365 and LibreOffice 7 write.

use officecore::compat;
use officecore::model::*;
use officecore::odf;
use officecore::xlsx;
use officecore::zip::{ZipLimits, ZipReader, ZipWriter};

fn number(value: f64) -> Cell {
    Cell { value: CellValue::Number(value), ..Default::default() }
}

fn workbook_with(rules: Vec<CondRule>) -> Workbook {
    let mut workbook = Workbook::new_blank("Rules");
    let sheet = &mut workbook.sheets[0];
    sheet.name = "Data".into();
    for row in 1..=6u32 {
        sheet.set(&format!("A{row}"), number(f64::from(row) * 10.0));
        sheet.set(&format!("B{row}"), number(f64::from(7 - row)));
    }
    for (index, mut rule) in rules.into_iter().enumerate() {
        rule.id = format!("cf{}", index + 1);
        sheet.conditional.push(rule);
    }
    workbook
}

fn stop(kind: &str, value: &str, color: &str) -> CondThreshold {
    CondThreshold { kind: kind.into(), value: value.into(), color: Some(color.into()) }
}

fn bound(kind: &str, value: &str) -> CondThreshold {
    CondThreshold { kind: kind.into(), value: value.into(), color: None }
}

fn three_color_scale() -> CondRule {
    CondRule {
        range: "A1:A6".into(),
        kind: "colorScale".into(),
        thresholds: vec![stop("min", "", "#F8696B"), stop("percentile", "50", "#FFEB84"), stop("max", "", "#63BE7B")],
        ..Default::default()
    }
}

fn two_color_scale() -> CondRule {
    CondRule {
        range: "B1:B6".into(),
        kind: "colorScale".into(),
        thresholds: vec![stop("num", "2", "#FFFFFF"), stop("percent", "90", "#5A8AC6")],
        ..Default::default()
    }
}

fn data_bar() -> CondRule {
    CondRule { range: "A1:A6".into(), kind: "dataBar".into(), fill: Some("#638EC6".into()), ..Default::default() }
}

fn custom_data_bar() -> CondRule {
    CondRule {
        range: "B1:B6".into(),
        kind: "dataBar".into(),
        fill: Some("#FF555A".into()),
        hide_value: true,
        thresholds: vec![bound("num", "0"), bound("percentile", "95")],
        ..Default::default()
    }
}

fn icon_set(name: &str, count: usize) -> CondRule {
    let thresholds = (0..count).map(|step| bound("percent", &(step * 100 / count).to_string())).collect();
    CondRule {
        range: "A1:A6".into(),
        kind: "iconSet".into(),
        icon_set: Some(name.into()),
        thresholds,
        ..Default::default()
    }
}

fn expression() -> CondRule {
    CondRule {
        range: "A2:B6".into(),
        kind: "expression".into(),
        formula: Some("AND($A2>20,$B2<4)".into()),
        fill: Some("#FFC7CE".into()),
        color: Some("#9C0006".into()),
        bold: true,
        italic: true,
        stop_if_true: true,
        ..Default::default()
    }
}

fn highlight(kind: &str, values: &[&str]) -> CondRule {
    CondRule {
        range: "A1:A6".into(),
        kind: kind.into(),
        values: values.iter().map(|value| value.to_string()).collect(),
        fill: Some("#C6EFCE".into()),
        color: Some("#006100".into()),
        top_n: matches!(kind, "top" | "bottom").then_some(3),
        ..Default::default()
    }
}

fn rules_of(workbook: &Workbook) -> &[CondRule] {
    &workbook.sheets[0].conditional
}

/// Rebuilds a package with some parts replaced.
fn rebuild(bytes: &[u8], replacements: &[(&str, &str)]) -> Vec<u8> {
    let reader = ZipReader::open(bytes.to_vec()).unwrap();
    let mut writer = ZipWriter::new();
    for (name, data) in reader.read_all(ZipLimits::default()).unwrap() {
        match replacements.iter().find(|(part, _)| *part == name) {
            Some((_, replacement)) => writer.add_text(&name, replacement),
            None => writer.add(&name, &data),
        }
    }
    writer.finish()
}

/// Writes `workbook`, then injects `cf` (conditionalFormatting blocks) into the
/// first sheet and `dxfs` into the styles, the way a foreign writer lays them out.
fn with_foreign_rules(cf: &str, dxfs: &str) -> xlsx::SheetRead {
    let bytes = xlsx::write_xlsx(&workbook_with(vec![])).unwrap();
    let reader = ZipReader::open(bytes.clone()).unwrap();
    let sheet = reader.read_text("xl/worksheets/sheet1.xml").unwrap();
    let styles = reader.read_text("xl/styles.xml").unwrap();
    assert!(styles.contains("<dxfs count=\"0\"></dxfs>"), "{styles}");
    let patched = rebuild(
        &bytes,
        &[
            ("xl/worksheets/sheet1.xml", &sheet.replace("</worksheet>", &format!("{cf}</worksheet>"))),
            ("xl/styles.xml", &styles.replace("<dxfs count=\"0\"></dxfs>", dxfs)),
        ],
    );
    xlsx::read_workbook_bytes(&patched).unwrap()
}

// ---------------------------------------------------------------------------
// XLSX round trips
// ---------------------------------------------------------------------------

#[test]
fn xlsx_color_scales_round_trip_with_two_and_three_stops() {
    let workbook = workbook_with(vec![three_color_scale(), two_color_scale()]);
    let bytes = xlsx::write_xlsx(&workbook).unwrap();
    let xml = ZipReader::open(bytes.clone()).unwrap().read_text("xl/worksheets/sheet1.xml").unwrap();
    assert!(xml.contains(
        "<cfRule type=\"colorScale\" priority=\"1\"><colorScale><cfvo type=\"min\"/><cfvo type=\"percentile\" val=\"50\"/><cfvo type=\"max\"/><color rgb=\"FFF8696B\"/><color rgb=\"FFFFEB84\"/><color rgb=\"FF63BE7B\"/></colorScale></cfRule>"
    ), "{xml}");
    assert!(xml.contains("<cfvo type=\"num\" val=\"2\"/><cfvo type=\"percent\" val=\"90\"/>"), "{xml}");
    let read = xlsx::read_workbook_bytes(&bytes).unwrap();
    assert!(read.warnings.iter().all(|warning| !warning.contains("conditional")), "{:?}", read.warnings);
    assert_eq!(rules_of(&read.workbook), rules_of(&workbook));
}

#[test]
fn xlsx_data_bars_round_trip_with_default_and_explicit_bounds() {
    let workbook = workbook_with(vec![data_bar(), custom_data_bar()]);
    let bytes = xlsx::write_xlsx(&workbook).unwrap();
    let xml = ZipReader::open(bytes.clone()).unwrap().read_text("xl/worksheets/sheet1.xml").unwrap();
    assert!(xml.contains("<dataBar><cfvo type=\"min\"/><cfvo type=\"max\"/><color rgb=\"FF638EC6\"/></dataBar>"));
    assert!(xml.contains(
        "<dataBar showValue=\"0\"><cfvo type=\"num\" val=\"0\"/><cfvo type=\"percentile\" val=\"95\"/><color rgb=\"FFFF555A\"/></dataBar>"
    ), "{xml}");
    let read = xlsx::read_workbook_bytes(&bytes).unwrap();
    assert_eq!(rules_of(&read.workbook), rules_of(&workbook));
}

#[test]
fn xlsx_icon_sets_round_trip_including_reverse_and_hidden_values() {
    let mut reversed = icon_set("3Flags", 3);
    reversed.reverse_icons = true;
    reversed.hide_value = true;
    reversed.range = "B1:B6".into();
    let workbook =
        workbook_with(vec![icon_set("3Arrows", 3), icon_set("3TrafficLights1", 3), reversed, icon_set("5Rating", 5)]);
    let bytes = xlsx::write_xlsx(&workbook).unwrap();
    let xml = ZipReader::open(bytes.clone()).unwrap().read_text("xl/worksheets/sheet1.xml").unwrap();
    assert!(xml.contains(
        "<iconSet iconSet=\"3Arrows\"><cfvo type=\"percent\" val=\"0\"/><cfvo type=\"percent\" val=\"33\"/><cfvo type=\"percent\" val=\"66\"/></iconSet>"
    ), "{xml}");
    assert!(xml.contains("<iconSet iconSet=\"3Flags\" showValue=\"0\" reverse=\"1\">"), "{xml}");
    let read = xlsx::read_workbook_bytes(&bytes).unwrap();
    assert_eq!(rules_of(&read.workbook), rules_of(&workbook));
}

#[test]
fn xlsx_formula_rules_round_trip_with_their_own_style() {
    let mut second = expression();
    second.formula = Some("MOD(ROW(),2)=0".into());
    second.fill = Some("#DDEBF7".into());
    second.color = None;
    second.bold = false;
    second.italic = false;
    second.stop_if_true = false;
    let workbook = workbook_with(vec![expression(), second]);
    let bytes = xlsx::write_xlsx(&workbook).unwrap();
    let reader = ZipReader::open(bytes.clone()).unwrap();
    let xml = reader.read_text("xl/worksheets/sheet1.xml").unwrap();
    assert!(xml.contains(
        "<cfRule type=\"expression\" dxfId=\"0\" priority=\"1\" stopIfTrue=\"1\"><formula>AND($A2&gt;20,$B2&lt;4)</formula></cfRule>"
    ), "{xml}");
    assert!(xml
        .contains("<cfRule type=\"expression\" dxfId=\"1\" priority=\"2\"><formula>MOD(ROW(),2)=0</formula></cfRule>"));
    let styles = reader.read_text("xl/styles.xml").unwrap();
    assert!(styles.contains("<dxfs count=\"2\">"), "{styles}");
    assert!(styles.contains(
        "<dxf><font><b/><i/><color rgb=\"FF9C0006\"/></font><fill><patternFill><bgColor rgb=\"FFFFC7CE\"/></patternFill></fill></dxf>"
    ), "{styles}");
    let read = xlsx::read_workbook_bytes(&bytes).unwrap();
    assert_eq!(rules_of(&read.workbook), rules_of(&workbook));
}

#[test]
fn xlsx_highlight_rules_keep_their_own_colours_and_share_equal_looks() {
    let workbook = workbook_with(vec![
        highlight("greater", &["20"]),
        highlight("less", &["50"]),
        highlight("between", &["20", "40"]),
        CondRule { fill: Some("#FFEB9C".into()), color: None, ..highlight("equal", &["30"]) },
    ]);
    let bytes = xlsx::write_xlsx(&workbook).unwrap();
    let reader = ZipReader::open(bytes.clone()).unwrap();
    let styles = reader.read_text("xl/styles.xml").unwrap();
    assert!(styles.contains("<dxfs count=\"2\">"), "equal looks share one dxf: {styles}");
    let xml = reader.read_text("xl/worksheets/sheet1.xml").unwrap();
    assert!(xml.contains("operator=\"between\"><formula>20</formula><formula>40</formula></cfRule>"), "{xml}");
    assert!(xml.contains("operator=\"greaterThan\"><formula>20</formula>"), "bare operand, no operator text: {xml}");
    let read = xlsx::read_workbook_bytes(&bytes).unwrap();
    assert_eq!(rules_of(&read.workbook), rules_of(&workbook));
}

#[test]
fn xlsx_rule_order_is_the_priority_order() {
    let workbook = workbook_with(vec![
        data_bar(),
        highlight("top", &[]),
        three_color_scale(),
        expression(),
        highlight("duplicate", &[]),
        icon_set("3Arrows", 3),
    ]);
    let read = xlsx::read_workbook_bytes(&xlsx::write_xlsx(&workbook).unwrap()).unwrap();
    let kinds: Vec<&str> = rules_of(&read.workbook).iter().map(|rule| rule.kind.as_str()).collect();
    assert_eq!(kinds, ["dataBar", "top", "colorScale", "expression", "duplicate", "iconSet"]);
    assert_eq!(rules_of(&read.workbook), rules_of(&workbook));
}

#[test]
fn xlsx_rules_that_cannot_be_written_stay_in_the_oswk_with_a_warning() {
    let mut one_stop = three_color_scale();
    one_stop.thresholds.truncate(1);
    let mut no_formula = expression();
    no_formula.formula = Some("  ".into());
    let unknown_icons = icon_set("9Planets", 3);
    let unknown_kind = CondRule { range: "A1".into(), kind: "sparkle".into(), ..Default::default() };
    let workbook = workbook_with(vec![one_stop, no_formula, unknown_icons, unknown_kind, three_color_scale()]);
    let result = xlsx::write_xlsx_package(&workbook).unwrap();
    assert_eq!(
        result.warnings.iter().filter(|warning| warning.contains("kept in the .oswk file only")).count(),
        4,
        "{:?}",
        result.warnings
    );
    let read = xlsx::read_workbook_bytes(&result.bytes).unwrap();
    assert_eq!(rules_of(&read.workbook).len(), 1, "only the valid color scale was written");
}

// ---------------------------------------------------------------------------
// XLSX import of foreign rules
// ---------------------------------------------------------------------------

/// Excel's own presets: the red-yellow-green scale, a blue data bar with the
/// Excel 2010 extension, a traffic-light icon set, plus highlight rules whose
/// dxf ids point at Excel's built-in red/yellow/green looks.
const EXCEL_RULES: &str = concat!(
    "<conditionalFormatting sqref=\"A1:A6\"><cfRule type=\"colorScale\" priority=\"4\"><colorScale>",
    "<cfvo type=\"min\"/><cfvo type=\"percentile\" val=\"50\"/><cfvo type=\"max\"/>",
    "<color rgb=\"FFF8696B\"/><color rgb=\"FFFFEB84\"/><color rgb=\"FF63BE7B\"/></colorScale></cfRule></conditionalFormatting>",
    "<conditionalFormatting sqref=\"B1:B6\"><cfRule type=\"dataBar\" priority=\"3\"><dataBar>",
    "<cfvo type=\"min\"/><cfvo type=\"max\"/><color rgb=\"FF638EC6\"/></dataBar>",
    "<extLst><ext uri=\"{B025F937-C7B1-47D3-B67F-A62EFF666E3E}\" xmlns:x14=\"http://schemas.microsoft.com/office/spreadsheetml/2009/9/main\"><x14:id>{6B1D9D5F-0000-4000-8000-000000000001}</x14:id></ext></extLst>",
    "</cfRule></conditionalFormatting>",
    "<conditionalFormatting sqref=\"A1:A6\"><cfRule type=\"iconSet\" priority=\"5\"><iconSet iconSet=\"3TrafficLights1\">",
    "<cfvo type=\"percent\" val=\"0\"/><cfvo type=\"percent\" val=\"33\"/><cfvo type=\"percent\" val=\"67\"/></iconSet></cfRule></conditionalFormatting>",
    "<conditionalFormatting sqref=\"A1:A6\"><cfRule type=\"expression\" dxfId=\"0\" priority=\"1\" stopIfTrue=\"1\"><formula>$A1&gt;30</formula></cfRule></conditionalFormatting>",
    "<conditionalFormatting sqref=\"B1:B6\"><cfRule type=\"cellIs\" dxfId=\"1\" priority=\"2\" operator=\"notBetween\"><formula>2</formula><formula>4</formula></cfRule></conditionalFormatting>"
);

const EXCEL_DXFS: &str = concat!(
    "<dxfs count=\"2\">",
    "<dxf><font><color rgb=\"FF9C0006\"/></font><fill><patternFill><bgColor rgb=\"FFFFC7CE\"/></patternFill></fill></dxf>",
    "<dxf><font><b/><color rgb=\"FF9C5700\"/></font><fill><patternFill patternType=\"solid\"><fgColor rgb=\"FFFFEB9C\"/><bgColor rgb=\"FFFFEB9C\"/></patternFill></fill></dxf>",
    "</dxfs>"
);

#[test]
fn imports_the_rules_excel_writes_and_keeps_every_type() {
    let read = with_foreign_rules(EXCEL_RULES, EXCEL_DXFS);
    let rules = rules_of(&read.workbook);
    assert_eq!(rules.len(), 5, "{:?}", read.warnings);
    // Evaluated by priority: expression (1), notBetween (2), bar (3), scale (4), icons (5).
    let kinds: Vec<&str> = rules.iter().map(|rule| rule.kind.as_str()).collect();
    assert_eq!(kinds, ["expression", "expression", "dataBar", "colorScale", "iconSet"]);

    assert_eq!(rules[0].formula.as_deref(), Some("$A1>30"));
    assert!(rules[0].stop_if_true);
    assert_eq!((rules[0].fill.as_deref(), rules[0].color.as_deref()), (Some("#FFC7CE"), Some("#9C0006")));

    // `notBetween` has no model kind; it keeps its meaning as a formula.
    assert_eq!(rules[1].formula.as_deref(), Some("OR(B1<2,B1>4)"));
    assert!(rules[1].bold);
    assert_eq!(rules[1].fill.as_deref(), Some("#FFEB9C"));

    assert_eq!(rules[2].fill.as_deref(), Some("#638EC6"));
    assert!(rules[2].thresholds.is_empty(), "automatic bounds are the default");

    assert_eq!(rules[3].thresholds, three_color_scale().thresholds);
    assert_eq!(rules[4].icon_set.as_deref(), Some("3TrafficLights1"));
    assert_eq!(rules[4].thresholds.len(), 3);
    assert_eq!(rules[4].thresholds[2], bound("percent", "67"));

    assert!(read.warnings.iter().all(|warning| !warning.contains("dropped")), "{:?}", read.warnings);
}

#[test]
fn excel_rules_without_a_model_kind_become_formula_rules() {
    let cf = concat!(
        "<conditionalFormatting sqref=\"A1:A6\">",
        "<cfRule type=\"beginsWith\" dxfId=\"0\" priority=\"1\" operator=\"beginsWith\" text=\"ab\"><formula>LEFT(A1,LEN(\"ab\"))=\"ab\"</formula></cfRule>",
        "<cfRule type=\"containsBlanks\" dxfId=\"0\" priority=\"2\"><formula>LEN(TRIM(A1))=0</formula></cfRule>",
        "<cfRule type=\"uniqueValues\" dxfId=\"0\" priority=\"3\"/>",
        "<cfRule type=\"aboveAverage\" dxfId=\"0\" priority=\"4\" equalAverage=\"1\"/>",
        "<cfRule type=\"aboveAverage\" dxfId=\"0\" priority=\"5\" aboveAverage=\"0\"/>",
        "<cfRule type=\"top10\" dxfId=\"0\" priority=\"6\" percent=\"1\" rank=\"10\"/>",
        "<cfRule type=\"top10\" dxfId=\"0\" priority=\"7\" bottom=\"1\" percent=\"1\" rank=\"20\"/>",
        "<cfRule type=\"top10\" dxfId=\"0\" priority=\"8\" rank=\"5\"/>",
        "<cfRule type=\"cellIs\" dxfId=\"0\" priority=\"9\" operator=\"greaterThanOrEqual\"><formula>20</formula></cfRule>",
        "<cfRule type=\"cellIs\" dxfId=\"0\" priority=\"10\" operator=\"notEqual\"><formula>\"x\"</formula></cfRule>",
        "<cfRule type=\"timePeriod\" dxfId=\"0\" priority=\"11\" timePeriod=\"lastWeek\"><formula>AND(TODAY()-ROUNDDOWN(A1,0)&gt;=(WEEKDAY(TODAY())),TODAY()-ROUNDDOWN(A1,0)&lt;(WEEKDAY(TODAY())+7))</formula></cfRule>",
        "<cfRule type=\"notContainsText\" dxfId=\"0\" priority=\"12\" operator=\"notContains\" text=\"q\"/>",
        "</conditionalFormatting>"
    );
    let read = with_foreign_rules(cf, EXCEL_DXFS);
    let rules = rules_of(&read.workbook);
    assert_eq!(rules.len(), 12, "{:?}", read.warnings);
    let formula = |index: usize| rules[index].formula.clone().unwrap_or_default();
    assert_eq!(formula(0), "LEFT(A1,LEN(\"ab\"))=\"ab\"");
    assert_eq!(formula(1), "LEN(TRIM(A1))=0");
    assert_eq!(formula(2), "COUNTIF($A$1:$A$6,A1)=1");
    assert_eq!(formula(3), "A1>=AVERAGE($A$1:$A$6)");
    assert_eq!(formula(4), "A1<AVERAGE($A$1:$A$6)");
    assert_eq!(formula(5), "A1>=PERCENTILE($A$1:$A$6,1-10/100)");
    assert_eq!(formula(6), "A1<=PERCENTILE($A$1:$A$6,20/100)");
    assert_eq!((rules[7].kind.as_str(), rules[7].top_n), ("top", Some(5)));
    assert_eq!(formula(8), "A1>=20");
    assert_eq!(formula(9), "A1<>\"x\"");
    assert!(formula(10).starts_with("AND(TODAY()-ROUNDDOWN(A1,0)>="));
    assert_eq!(formula(11), "ISERROR(SEARCH(\"q\",A1))");
    assert!(rules.iter().all(|rule| rule.fill.as_deref() == Some("#FFC7CE")));
    assert!(read.warnings.iter().all(|warning| !warning.contains("dropped")), "{:?}", read.warnings);
}

#[test]
fn excel_rules_that_cannot_be_kept_are_reported_not_silently_dropped() {
    let cf = concat!(
        "<conditionalFormatting sqref=\"A1:A6\">",
        "<cfRule type=\"aboveAverage\" dxfId=\"0\" priority=\"1\" stdDev=\"1\"/>",
        "<cfRule type=\"colorScale\" priority=\"2\"><colorScale><cfvo type=\"min\"/><color rgb=\"FFF8696B\"/></colorScale></cfRule>",
        "<cfRule type=\"iconSet\" priority=\"3\"><iconSet iconSet=\"3Arrows\"><cfvo type=\"percent\" val=\"0\"/></iconSet></cfRule>",
        "<cfRule type=\"expression\" dxfId=\"0\" priority=\"4\"/>",
        "<cfRule type=\"dataBar\" priority=\"5\"><dataBar><cfvo type=\"min\"/><cfvo type=\"max\"/><color rgb=\"FF638EC6\"/></dataBar></cfRule>",
        "</conditionalFormatting>"
    );
    let read = with_foreign_rules(cf, EXCEL_DXFS);
    assert_eq!(rules_of(&read.workbook).len(), 1, "only the data bar is valid");
    assert!(
        read.warnings.iter().any(|warning| warning.starts_with("4 conditional formatting rule(s)")),
        "{:?}",
        read.warnings
    );
}

#[test]
fn excel_2010_data_bar_extensions_are_reported() {
    let read = with_foreign_rules(EXCEL_RULES, EXCEL_DXFS);
    assert!(read.warnings.iter().all(|warning| !warning.contains("2010")), "inline x14 ids are not extensions");
    let ext = concat!(
        "<extLst><ext uri=\"{78C0D931-6437-407d-A8EE-F0AAD7539E65}\" xmlns:x14=\"http://schemas.microsoft.com/office/spreadsheetml/2009/9/main\">",
        "<x14:conditionalFormattings><x14:conditionalFormatting><x14:cfRule type=\"dataBar\" id=\"{6B1D9D5F-0000-4000-8000-000000000001}\">",
        "<x14:dataBar minLength=\"0\" maxLength=\"100\" border=\"1\" negativeBarBorderColorSameAsPositive=\"0\"><x14:cfvo type=\"autoMin\"/><x14:cfvo type=\"autoMax\"/>",
        "<x14:borderColor rgb=\"FF638EC6\"/><x14:negativeFillColor rgb=\"FFFF0000\"/></x14:dataBar></x14:cfRule><xm:sqref xmlns:xm=\"http://schemas.microsoft.com/office/excel/2006/main\">B1:B6</xm:sqref></x14:conditionalFormatting></x14:conditionalFormattings></ext></extLst>"
    );
    let read = with_foreign_rules(&format!("{EXCEL_RULES}{ext}"), EXCEL_DXFS);
    assert_eq!(rules_of(&read.workbook).len(), 5, "the basic rules still import");
    assert!(read.warnings.iter().any(|warning| warning.contains("Excel 2010")), "{:?}", read.warnings);
}

#[test]
fn old_files_of_this_editor_still_import_their_operator_encoding() {
    // Earlier builds wrote `>20` into the formula and `between` as `a~b`.
    let cf = concat!(
        "<conditionalFormatting sqref=\"A1:A6\"><cfRule type=\"cellIs\" dxfId=\"0\" priority=\"1\" operator=\"greaterThan\"><formula>&gt;20</formula></cfRule></conditionalFormatting>",
        "<conditionalFormatting sqref=\"A1:A6\"><cfRule type=\"cellIs\" dxfId=\"0\" priority=\"2\" operator=\"between\"><formula>1~9</formula></cfRule></conditionalFormatting>"
    );
    let read = with_foreign_rules(cf, EXCEL_DXFS);
    let rules = rules_of(&read.workbook);
    assert_eq!((rules[0].kind.as_str(), rules[0].values.clone()), ("greater", vec!["20".to_string()]));
    assert_eq!((rules[1].kind.as_str(), rules[1].values.clone()), ("between", vec!["1".to_string(), "9".to_string()]));
}

#[test]
fn a_multi_area_range_anchors_formulas_at_its_first_cell() {
    let mut rule = expression();
    rule.range = "B2:B4 D2:D4".into();
    rule.formula = Some("B2>3".into());
    let read = xlsx::read_workbook_bytes(&xlsx::write_xlsx(&workbook_with(vec![rule.clone()])).unwrap()).unwrap();
    assert_eq!(rules_of(&read.workbook)[0].range, "B2:B4 D2:D4");
    let mut text = highlight("textContains", &["ab"]);
    text.range = "B2:B4 D2:D4".into();
    let bytes = xlsx::write_xlsx(&workbook_with(vec![text])).unwrap();
    let xml = ZipReader::open(bytes).unwrap().read_text("xl/worksheets/sheet1.xml").unwrap();
    assert!(xml.contains("SEARCH(&quot;ab&quot;,B2)"), "{xml}");
}

// ---------------------------------------------------------------------------
// ODS
// ---------------------------------------------------------------------------

fn all_rules() -> Vec<CondRule> {
    vec![
        three_color_scale(),
        two_color_scale(),
        data_bar(),
        custom_data_bar(),
        icon_set("3Arrows", 3),
        CondRule { reverse_icons: true, hide_value: true, range: "B1:B6".into(), ..icon_set("3Flags", 3) },
        expression(),
        highlight("greater", &["20"]),
        highlight("less", &["50"]),
        highlight("equal", &["30"]),
        highlight("between", &["20", "40"]),
        highlight("textContains", &["ab \"c\""]),
        highlight("duplicate", &[]),
        highlight("top", &[]),
        highlight("bottom", &[]),
    ]
}

#[test]
fn ods_conditional_formats_round_trip_every_kind() {
    let mut workbook = workbook_with(all_rules());
    // Stop-if-true has no ODS equivalent.
    workbook.sheets[0].conditional.iter_mut().for_each(|rule| rule.stop_if_true = false);
    let bytes = odf::write_ods(&workbook).unwrap();
    let read = odf::read_ods(&bytes).unwrap();
    assert!(read.warnings.iter().all(|warning| !warning.contains("conditional")), "{:?}", read.warnings);
    assert_eq!(rules_of(&read.workbook), rules_of(&workbook));
    // A second cycle changes nothing.
    let again = odf::read_ods(&odf::write_ods(&read.workbook).unwrap()).unwrap();
    assert_eq!(rules_of(&again.workbook), rules_of(&read.workbook));
    // The cells are untouched.
    assert_eq!(read.workbook.sheets[0].get("A3").map(|cell| cell.value.clone()), Some(CellValue::Number(30.0)));
}

#[test]
fn ods_writes_calcext_formats_between_the_columns_and_the_rows() {
    let workbook = workbook_with(vec![three_color_scale(), data_bar(), icon_set("3Arrows", 3), expression()]);
    let reader = ZipReader::open(odf::write_ods(&workbook).unwrap()).unwrap();
    let content = reader.read_text("content.xml").unwrap();
    assert!(content.contains("xmlns:calcext=\"urn:org:documentfoundation:names:experimental:calc:xmlns:calcext:1.0\""));
    let columns = content.rfind("<table:table-column ").unwrap();
    let formats = content.find("<calcext:conditional-formats>").unwrap();
    let rows = content.find("<table:table-row>").unwrap();
    assert!(columns < formats && formats < rows, "formats sit between the columns and the rows");
    assert!(content.contains("<calcext:conditional-format calcext:target-range-address=\"Data.A1:Data.A6\">"));
    assert!(content.contains(
        "<calcext:color-scale><calcext:color-scale-entry calcext:value=\"0\" calcext:type=\"minimum\" calcext:color=\"#F8696B\"/><calcext:color-scale-entry calcext:value=\"50\" calcext:type=\"percentile\" calcext:color=\"#FFEB84\"/><calcext:color-scale-entry calcext:value=\"0\" calcext:type=\"maximum\" calcext:color=\"#63BE7B\"/></calcext:color-scale>"
    ), "{content}");
    assert!(content.contains("calcext:positive-color=\"#638EC6\""));
    assert!(content.contains("calcext:type=\"auto-minimum\"") && content.contains("calcext:type=\"auto-maximum\""));
    assert!(content.contains("<calcext:icon-set calcext:icon-set-type=\"3Arrows\""));
    // Rule formulas use ODF's bracketed references and `;`, as LibreOffice writes them.
    assert!(content.contains("calcext:value=\"formula-is(AND([.$A2]&gt;20;[.$B2]&lt;4))\""), "{content}");
    assert!(content.contains("calcext:base-cell-address=\"Data.A2\""));
    // The look is a named cell style in styles.xml.
    let styles = reader.read_text("styles.xml").unwrap();
    assert!(styles.contains(
        "<style:style style:name=\"OmniCF1\" style:family=\"table-cell\"><style:text-properties fo:color=\"#9C0006\" fo:font-weight=\"bold\" fo:font-style=\"italic\"/><style:table-cell-properties fo:background-color=\"#FFC7CE\"/></style:style>"
    ), "{styles}");
}

#[test]
fn ods_rules_that_cannot_be_written_are_skipped_and_reported() {
    let mut one_stop = three_color_scale();
    one_stop.thresholds.truncate(1);
    let workbook = workbook_with(vec![one_stop, data_bar()]);
    let read = odf::read_ods(&odf::write_ods(&workbook).unwrap()).unwrap();
    assert_eq!(rules_of(&read.workbook).len(), 1);
    let report = compat::workbook_feature_report(&workbook, "ods");
    let item = report.items.iter().find(|item| item.feature == "conditionalFormatting").expect("conditional item");
    assert_eq!(item.status, "lost");
    assert!(item.message.contains("colour stops"), "{}", item.message);
    let clean = compat::workbook_feature_report(&workbook_with(all_rules()), "ods");
    assert!(!clean.items.iter().any(|item| item.feature == "conditionalFormatting"), "{:?}", clean.items);
}

/// What LibreOffice 7 writes: one conditional-format per range with several
/// entries, a named style per look, and ODF condition strings.
fn libreoffice_package() -> Vec<u8> {
    let content = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
        "<office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
        "xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" ",
        "xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" ",
        "xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" ",
        "xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" ",
        "xmlns:calcext=\"urn:org:documentfoundation:names:experimental:calc:xmlns:calcext:1.0\" office:version=\"1.3\">",
        "<office:automatic-styles>",
        "<style:style style:name=\"ConditionalStyle_5f_1\" style:display-name=\"ConditionalStyle_1\" style:family=\"table-cell\"><style:text-properties fo:color=\"#9c0006\" fo:font-weight=\"bold\"/><style:table-cell-properties fo:background-color=\"#ffc7ce\"/></style:style>",
        "</office:automatic-styles><office:body><office:spreadsheet><table:table table:name=\"Sheet1\">",
        "<table:table-column table:number-columns-repeated=\"2\"/>",
        "<calcext:conditional-formats>",
        "<calcext:conditional-format calcext:target-range-address=\"Sheet1.A1:Sheet1.A4\">",
        "<calcext:color-scale>",
        "<calcext:color-scale-entry calcext:value=\"0\" calcext:type=\"minimum\" calcext:color=\"#ff0000\"/>",
        "<calcext:color-scale-entry calcext:value=\"50\" calcext:type=\"percentile\" calcext:color=\"#ffff00\"/>",
        "<calcext:color-scale-entry calcext:value=\"0\" calcext:type=\"maximum\" calcext:color=\"#00ff00\"/>",
        "</calcext:color-scale></calcext:conditional-format>",
        "<calcext:conditional-format calcext:target-range-address=\"Sheet1.B1:Sheet1.B4\">",
        "<calcext:data-bar calcext:positive-color=\"#638ec6\" calcext:gradient=\"true\" calcext:axis-position=\"automatic\" calcext:show-value=\"false\" calcext:axis-color=\"#000000\" calcext:negative-color=\"#ff0000\" calcext:min-length=\"0\" calcext:max-length=\"100\">",
        "<calcext:formatting-entry calcext:value=\"0\" calcext:type=\"auto-minimum\"/>",
        "<calcext:formatting-entry calcext:value=\"0\" calcext:type=\"auto-maximum\"/>",
        "</calcext:data-bar></calcext:conditional-format>",
        "<calcext:conditional-format calcext:target-range-address=\"Sheet1.A1:Sheet1.A4 Sheet1.B1\">",
        "<calcext:icon-set calcext:icon-set-type=\"3Arrows\">",
        "<calcext:formatting-entry calcext:show-value=\"false\" calcext:value=\"0\" calcext:type=\"percent\"/>",
        "<calcext:formatting-entry calcext:value=\"33\" calcext:type=\"percent\"/>",
        "<calcext:formatting-entry calcext:value=\"67\" calcext:type=\"percent\"/>",
        "</calcext:icon-set></calcext:conditional-format>",
        "<calcext:conditional-format calcext:target-range-address=\"Sheet1.A1:Sheet1.A4\">",
        "<calcext:condition calcext:apply-style-name=\"ConditionalStyle_1\" calcext:value=\"&gt;=20\" calcext:base-cell-address=\"Sheet1.A1\"/>",
        "<calcext:condition calcext:apply-style-name=\"ConditionalStyle_1\" calcext:value=\"between(1,9)\" calcext:base-cell-address=\"Sheet1.A1\"/>",
        "<calcext:condition calcext:apply-style-name=\"ConditionalStyle_1\" calcext:value=\"formula-is(AND([.A1]&gt;2;[$Sheet1.$B1]&lt;3))\" calcext:base-cell-address=\"Sheet1.A1\"/>",
        "<calcext:condition calcext:apply-style-name=\"ConditionalStyle_1\" calcext:value=\"begins-with(&quot;ab&quot;)\" calcext:base-cell-address=\"Sheet1.A1\"/>",
        "<calcext:condition calcext:apply-style-name=\"ConditionalStyle_1\" calcext:value=\"duplicate\" calcext:base-cell-address=\"Sheet1.A1\"/>",
        "<calcext:condition calcext:apply-style-name=\"ConditionalStyle_1\" calcext:value=\"top-elements(2)\" calcext:base-cell-address=\"Sheet1.A1\"/>",
        "<calcext:condition calcext:apply-style-name=\"ConditionalStyle_1\" calcext:value=\"sparkle()\" calcext:base-cell-address=\"Sheet1.A1\"/>",
        "</calcext:conditional-format>",
        "<calcext:conditional-format calcext:target-range-address=\"Sheet1.C1:Sheet1.C1\"><calcext:date calcext:date=\"today\" calcext:style=\"ConditionalStyle_1\"/></calcext:conditional-format>",
        "<calcext:conditional-format calcext:target-range-address=\"Sheet1.D1:Sheet1.D1\">",
        "<calcext:color-scale><calcext:color-scale-entry calcext:value=\"1\" calcext:type=\"number\" calcext:color=\"#ffffff\"/>",
        "<calcext:color-scale-entry calcext:value=\"90\" calcext:type=\"percent\" calcext:color=\"#5a8ac6\"/></calcext:color-scale>",
        "</calcext:conditional-format>",
        "</calcext:conditional-formats>",
        "<table:table-row><table:table-cell office:value-type=\"float\" office:value=\"1\"><text:p>1</text:p></table:table-cell>",
        "<table:table-cell office:value-type=\"float\" office:value=\"2\"><text:p>2</text:p></table:table-cell></table:table-row>",
        "</table:table></office:spreadsheet></office:body></office:document-content>"
    );
    let mut zip = ZipWriter::new();
    zip.add_text("mimetype", "application/vnd.oasis.opendocument.spreadsheet");
    zip.add_text("content.xml", content);
    zip.finish()
}

#[test]
fn reads_the_conditional_formats_libreoffice_writes() {
    let read = odf::read_ods(&libreoffice_package()).unwrap();
    let rules = rules_of(&read.workbook);
    let kinds: Vec<&str> = rules.iter().map(|rule| rule.kind.as_str()).collect();
    assert_eq!(
        kinds,
        [
            "colorScale",
            "dataBar",
            "iconSet",
            "expression",
            "between",
            "expression",
            "expression",
            "duplicate",
            "top",
            "colorScale"
        ],
        "{:?}",
        read.warnings
    );
    assert_eq!(rules[0].range, "A1:A4");
    assert_eq!(rules[0].thresholds[1], stop("percentile", "50", "#FFFF00"));
    assert_eq!(rules[0].thresholds[2], stop("max", "", "#00FF00"));

    assert_eq!(rules[1].fill.as_deref(), Some("#638EC6"));
    assert!(rules[1].hide_value && rules[1].thresholds.is_empty());

    assert_eq!(rules[2].range, "A1:A4 B1", "a range list keeps every area");
    assert_eq!(rules[2].icon_set.as_deref(), Some("3Arrows"));
    assert_eq!(rules[2].thresholds[1], bound("percent", "33"));
    assert!(rules[2].hide_value && !rules[2].reverse_icons, "LibreOffice hides the value on the first entry");

    // `>=` has no model kind; the formula keeps its meaning, anchored at the base cell.
    assert_eq!(rules[3].formula.as_deref(), Some("A1>=20"));
    assert_eq!(rules[4].values, vec!["1".to_string(), "9".to_string()]);
    assert_eq!(rules[5].formula.as_deref(), Some("AND(A1>2,Sheet1!$B1<3)"));
    assert_eq!(rules[6].formula.as_deref(), Some("LEFT(A1,LEN(\"ab\"))=\"ab\""));
    assert_eq!(rules[8].top_n, Some(2));
    // LibreOffice spells the number threshold `number` and a single cell as a range.
    assert_eq!(rules[9].range, "D1");
    assert_eq!(rules[9].thresholds, vec![stop("num", "1", "#FFFFFF"), stop("percent", "90", "#5A8AC6")]);
    // The named style supplies the look of every condition.
    for rule in &rules[3..9] {
        assert_eq!(rule.fill.as_deref(), Some("#FFC7CE"), "{rule:?}");
        assert_eq!(rule.color.as_deref(), Some("#9C0006"));
        assert!(rule.bold && !rule.italic);
    }
    // The unknown condition and the date format are counted, not silently dropped.
    assert!(
        read.warnings.iter().any(|warning| warning.starts_with("2 conditional formatting rule(s)")),
        "{:?}",
        read.warnings
    );
    assert_eq!(read.workbook.sheets[0].get("B1").map(|cell| cell.value.clone()), Some(CellValue::Number(2.0)));
}
