//! Import of conditional-formatting rules from a worksheet part.

use super::*;

/// The first area of a range with absolute marks (`B2:B5` -> `$B$2:$B$5`), the
/// form a rule formula needs to refer to the whole range from every cell.
pub(crate) fn absolute_area(range: &str) -> String {
    let first = range.split_whitespace().next().unwrap_or(range);
    let cell = |row: u32, column: u32| format!("${}${}", crate::address::column_name(column), row + 1);
    match crate::address::parse_range(first) {
        Some(((start_row, start_col), (end_row, end_col))) if (start_row, start_col) != (end_row, end_col) => {
            format!("{}:{}", cell(start_row, start_col), cell(end_row, end_col))
        }
        Some(((row, column), _)) => cell(row, column),
        None => first.to_string(),
    }
}

/// Reads one `<cfRule>` into a model rule. `None` when the rule has a type or
/// shape the model cannot hold.
///
/// Types with a model kind of their own (cell value, text, duplicates, top and
/// bottom, color scale, data bar, icon set) map straight onto it. Excel states
/// every other highlight rule as a formula (`beginsWith`, `containsBlanks`,
/// `notBetween`, `aboveAverage`, ...), so those become `expression` rules and
/// keep their meaning.
fn read_cf_rule(rule: &XmlNode, range: &str, styles: &ImportedStyles) -> Option<CondRule> {
    let kind = rule.attr("type").unwrap_or("").trim();
    let dxf = parse_u32_attr(rule, "dxfId").map(|value| value as usize);
    let mut out = CondRule {
        range: range.to_string(),
        stop_if_true: attr_on(rule, "stopIfTrue"),
        fill: dxf.and_then(|index| styles.dxf_fills.get(index).cloned()).flatten(),
        color: dxf.and_then(|index| styles.dxf_colors.get(index).cloned()).flatten(),
        bold: dxf.and_then(|index| styles.dxf_bold.get(index).copied()).unwrap_or(false),
        italic: dxf.and_then(|index| styles.dxf_italic.get(index).copied()).unwrap_or(false),
        ..Default::default()
    };
    let formulas: Vec<String> =
        rule.children_of("formula").iter().map(|node| crate::formula::unprefix_xlsx(&node.deep_text())).collect();
    let formula = |index: usize| formulas.get(index).cloned().unwrap_or_default();
    let anchor = anchor_of(range);
    let all = absolute_area(range);
    let text = rule.attr("text").unwrap_or("").to_string();
    let quoted = text.replace('"', "\"\"");
    let expression = |out: &mut CondRule, formula: String| {
        out.kind = "expression".into();
        out.formula = Some(formula);
    };
    // A formula Excel stored with the rule wins over one rebuilt from its parts.
    let stated = |fallback: String| Some(formula(0)).filter(|stated| !stated.is_empty()).unwrap_or(fallback);
    match kind {
        "cellIs" => match rule.attr("operator").unwrap_or("") {
            "greaterThan" => {
                out.kind = "greater".into();
                out.values = vec![strip_operator(&formula(0))];
            }
            "lessThan" => {
                out.kind = "less".into();
                out.values = vec![strip_operator(&formula(0))];
            }
            "equal" => {
                out.kind = "equal".into();
                out.values = vec![strip_operator(&formula(0))];
            }
            "between" => {
                let first = formula(0);
                // Older files of this editor encode `between` as one formula
                // with a `~` separator; Excel uses two formula elements.
                let (low, high) = match first.split_once('~') {
                    Some((low, high)) => (low.to_string(), high.to_string()),
                    None => (first, formula(1)),
                };
                out.kind = "between".into();
                out.values = vec![strip_operator(&low), strip_operator(&high)];
            }
            "greaterThanOrEqual" => expression(&mut out, format!("{anchor}>={}", formula(0))),
            "lessThanOrEqual" => expression(&mut out, format!("{anchor}<={}", formula(0))),
            "notEqual" => expression(&mut out, format!("{anchor}<>{}", formula(0))),
            "notBetween" => expression(&mut out, format!("OR({anchor}<{},{anchor}>{})", formula(0), formula(1))),
            _ => return None,
        },
        "containsText" => {
            out.kind = "textContains".into();
            out.values = vec![if text.is_empty() { contains_text_value(&formula(0)) } else { text }];
        }
        "notContainsText" => expression(&mut out, stated(format!("ISERROR(SEARCH(\"{quoted}\",{anchor}))"))),
        "beginsWith" => expression(&mut out, stated(format!("LEFT({anchor},LEN(\"{quoted}\"))=\"{quoted}\""))),
        "endsWith" => expression(&mut out, stated(format!("RIGHT({anchor},LEN(\"{quoted}\"))=\"{quoted}\""))),
        "containsBlanks" => expression(&mut out, stated(format!("LEN(TRIM({anchor}))=0"))),
        "notContainsBlanks" => expression(&mut out, stated(format!("LEN(TRIM({anchor}))>0"))),
        "containsErrors" => expression(&mut out, stated(format!("ISERROR({anchor})"))),
        "notContainsErrors" => expression(&mut out, stated(format!("NOT(ISERROR({anchor}))"))),
        "timePeriod" => {
            // The period itself (`period="lastWeek"`) lives in the stated formula.
            let stated = formula(0);
            if stated.is_empty() {
                return None;
            }
            expression(&mut out, stated);
        }
        "duplicateValues" => out.kind = "duplicate".into(),
        "uniqueValues" => expression(&mut out, format!("COUNTIF({all},{anchor})=1")),
        "top10" => {
            let rank = parse_u32_attr(rule, "rank").unwrap_or(10);
            let bottom = attr_on(rule, "bottom");
            if attr_on(rule, "percent") {
                // "Top 10 percent" is not a count; as a formula it stays exact.
                expression(
                    &mut out,
                    if bottom {
                        format!("{anchor}<=PERCENTILE({all},{rank}/100)")
                    } else {
                        format!("{anchor}>=PERCENTILE({all},1-{rank}/100)")
                    },
                );
            } else {
                out.kind = if bottom { "bottom" } else { "top" }.into();
                out.top_n = Some(rank);
            }
        }
        "aboveAverage" => {
            if parse_u32_attr(rule, "stdDev").unwrap_or(0) > 0 {
                return None;
            }
            let above = rule.attr("aboveAverage").map(|value| !matches!(value.trim(), "0" | "false")).unwrap_or(true);
            let operator = match (above, attr_on(rule, "equalAverage")) {
                (true, false) => ">",
                (true, true) => ">=",
                (false, false) => "<",
                (false, true) => "<=",
            };
            expression(&mut out, format!("{anchor}{operator}AVERAGE({all})"));
        }
        "expression" => {
            let stated = formula(0);
            if stated.is_empty() {
                return None;
            }
            expression(&mut out, stated.trim_start_matches('=').trim().to_string());
        }
        "colorScale" => {
            let scale = rule.child("colorScale")?;
            let values = scale.children_of("cfvo");
            let colors = scale.children_of("color");
            if !(2..=3).contains(&values.len()) || values.len() != colors.len() {
                return None;
            }
            out.kind = "colorScale".into();
            for (value, color) in values.into_iter().zip(colors) {
                let mut threshold = read_cfvo(value)?;
                // A theme or indexed colour has no portable value; white keeps the stop.
                threshold.color = Some(color_of(color).unwrap_or_else(|| "#FFFFFF".into()));
                out.thresholds.push(threshold);
            }
            out.fill = None;
            out.color = None;
        }
        "dataBar" => {
            let bar = rule.child("dataBar")?;
            out.kind = "dataBar".into();
            out.fill = bar.child("color").and_then(color_of);
            out.color = None;
            out.hide_value = bar.attr("showValue").map(|value| matches!(value.trim(), "0" | "false")).unwrap_or(false);
            let values = bar.children_of("cfvo");
            if let [low, high] = values.as_slice() {
                let (low, high) = (read_cfvo(low)?, read_cfvo(high)?);
                // The automatic bounds are the default; only explicit ones are kept.
                if !(low.kind == "min" && high.kind == "max") {
                    out.thresholds = vec![low, high];
                }
            }
        }
        "iconSet" => {
            let set = rule.child("iconSet")?;
            let name = set.attr("iconSet").map(str::trim).filter(|name| !name.is_empty()).unwrap_or("3TrafficLights1");
            let count = icon_set_count(name)?;
            let values = set.children_of("cfvo");
            if values.len() != count {
                return None;
            }
            out.kind = "iconSet".into();
            out.icon_set = Some(name.to_string());
            out.reverse_icons = attr_on(set, "reverse");
            out.hide_value = set.attr("showValue").map(|value| matches!(value.trim(), "0" | "false")).unwrap_or(false);
            for value in values {
                out.thresholds.push(read_cfvo(value)?);
            }
            out.fill = None;
            out.color = None;
        }
        _ => return None,
    }
    Some(out)
}

/// One `<cfvo>`; `None` for a threshold type the model does not know.
fn read_cfvo(node: &XmlNode) -> Option<CondThreshold> {
    let kind = node.attr("type")?.trim();
    if !CFVO_KINDS.contains(&kind) {
        return None;
    }
    let value =
        if matches!(kind, "min" | "max") { String::new() } else { node.attr("val").unwrap_or("").trim().to_string() };
    Some(CondThreshold { kind: kind.to_string(), value, color: None })
}

pub(super) fn apply_conditional(
    root: &XmlNode,
    sheet: &mut Sheet,
    styles: &ImportedStyles,
    warnings: &mut Vec<String>,
) {
    let mut unsupported = 0usize;
    // Excel evaluates rules by `priority` across all blocks, not by position.
    let mut rules: Vec<(u32, CondRule)> = Vec::new();
    for block in root.children_of("conditionalFormatting") {
        let range = block.attr("sqref").unwrap_or("").trim().to_string();
        if range.is_empty() {
            continue;
        }
        for rule in block.children_of("cfRule") {
            match read_cf_rule(rule, &range, styles) {
                Some(model) => rules.push((parse_u32_attr(rule, "priority").unwrap_or(u32::MAX), model)),
                None => unsupported += 1,
            }
        }
    }
    rules.sort_by_key(|(priority, _)| *priority);
    for (_, mut rule) in rules {
        rule.id = format!("cf{}", sheet.conditional.len() + 1);
        sheet.conditional.push(rule);
    }
    // Excel 2010 keeps negative bar colours, borders and custom icons in
    // worksheet extensions; the basic rule above is imported, those are not.
    if let Some(extensions) = root.child("extLst") {
        let mut found = Vec::new();
        extensions.find_all("cfRule", &mut found);
        if !found.is_empty() {
            warnings.push(format!(
                "Sheet \"{}\" has {} Excel 2010 conditional formatting extension(s) (negative bar colours, custom icons); their extra options were not imported.",
                sheet.name,
                found.len()
            ));
        }
    }
    if unsupported > 0 {
        warnings.push(format!(
            "{unsupported} conditional formatting rule(s) on sheet \"{}\" use types this editor cannot import and were dropped.",
            sheet.name
        ));
    }
}

/// `>10`, `<=5` and the like become the bare value the editor stores.
fn strip_operator(formula: &str) -> String {
    formula.trim().trim_start_matches(['>', '<', '=']).trim().to_string()
}

fn contains_text_value(formula: &str) -> String {
    const SEARCH: &str = "SEARCH(\"";
    if let Some(start) = formula.find(SEARCH) {
        let rest = &formula[start + SEARCH.len()..];
        if let Some(end) = rest.find('"') {
            return rest[..end].to_string();
        }
    }
    String::new()
}
