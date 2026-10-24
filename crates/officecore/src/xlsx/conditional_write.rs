//! Export of conditional-formatting rules (`<conditionalFormatting>`/`<dxf>`).

use super::*;

/// Highlight colour used by a highlight rule that carries no look of its own.
pub(crate) const CONDITIONAL_FILL: &str = "#FFF3C4";

/// The threshold types of a color scale, data bar or icon set (`cfvo/@type`).
pub(crate) const CFVO_KINDS: [&str; 6] = ["min", "max", "num", "percent", "percentile", "formula"];

/// The number of icons in an OOXML icon set name (`3Arrows` -> 3, `5Rating` ->
/// 5); `None` for a name that is not an icon set.
pub(crate) fn icon_set_count(name: &str) -> Option<usize> {
    let count = name.chars().next()?.to_digit(10)? as usize;
    let tail = &name[1..];
    ((3..=5).contains(&count) && !tail.is_empty() && tail.chars().all(|ch| ch.is_ascii_alphanumeric())).then_some(count)
}

/// Even percent thresholds for an icon set: 0/33/67 for three icons, 0/25/50/75
/// for four, the way Excel fills them in.
pub(crate) fn default_icon_thresholds(count: usize) -> Vec<CondThreshold> {
    (0..count)
        .map(|step| CondThreshold {
            kind: "percent".into(),
            value: format!("{}", (step as f64 * 100.0 / count as f64).round() as u32),
            color: None,
        })
        .collect()
}

/// One `<cfvo>` element.
fn cfvo_xml(threshold: &CondThreshold) -> Result<String, String> {
    let kind = threshold.kind.as_str();
    if !CFVO_KINDS.contains(&kind) {
        return Err(format!("the threshold type \"{kind}\" is unknown"));
    }
    if matches!(kind, "min" | "max") {
        return Ok(format!("<cfvo type=\"{kind}\"/>"));
    }
    let value = threshold.value.trim();
    Ok(format!("<cfvo type=\"{kind}\" val=\"{}\"/>", escape_attr(if value.is_empty() { "0" } else { value })))
}

/// Builds a `<conditionalFormatting>` block for one rule.
///
/// `Err` carries the reason a rule cannot be written; the caller keeps the rule
/// in the .oswk file and warns, instead of emitting a package Excel would repair.
pub(super) fn conditional_formatting_xml(
    rule: &CondRule,
    index: usize,
    styles: &mut StyleTable,
) -> Result<String, String> {
    let priority = index + 1;
    let stop = if rule.stop_if_true { " stopIfTrue=\"1\"" } else { "" };
    if rule.range.trim().is_empty() {
        return Err("the rule has no range".into());
    }
    let range = escape_attr(rule.range.trim());
    let wrap = |cf_rule: String| format!("<conditionalFormatting sqref=\"{range}\">{cf_rule}</conditionalFormatting>");
    let anchor = anchor_of(&rule.range);

    // Color scales, data bars and icon sets carry their look in the rule; no dxf.
    match rule.kind.as_str() {
        "colorScale" => {
            if !(2..=3).contains(&rule.thresholds.len()) {
                return Err("a color scale needs two or three colour stops".into());
            }
            let mut values = String::new();
            let mut colors = String::new();
            for stop_point in &rule.thresholds {
                values.push_str(&cfvo_xml(stop_point)?);
                colors.push_str(&format!(
                    "<color rgb=\"{}\"/>",
                    argb_of(stop_point.color.as_deref().unwrap_or("#FFFFFF"))
                ));
            }
            return Ok(wrap(format!(
                "<cfRule type=\"colorScale\" priority=\"{priority}\"{stop}><colorScale>{values}{colors}</colorScale></cfRule>"
            )));
        }
        "dataBar" => {
            // Automatic minimum and maximum unless the rule names its own.
            let bounds = match rule.thresholds.as_slice() {
                [low, high] => format!("{}{}", cfvo_xml(low)?, cfvo_xml(high)?),
                _ => "<cfvo type=\"min\"/><cfvo type=\"max\"/>".to_string(),
            };
            let color = rule.fill.as_deref().map(argb_of).unwrap_or_else(|| "FF638EC6".into());
            let show = if rule.hide_value { " showValue=\"0\"" } else { "" };
            return Ok(wrap(format!(
                "<cfRule type=\"dataBar\" priority=\"{priority}\"{stop}><dataBar{show}>{bounds}<color rgb=\"{color}\"/></dataBar></cfRule>"
            )));
        }
        "iconSet" => {
            let name =
                rule.icon_set.as_deref().map(str::trim).filter(|name| !name.is_empty()).unwrap_or("3TrafficLights1");
            let count = icon_set_count(name).ok_or_else(|| format!("the icon set \"{name}\" is unknown"))?;
            let thresholds =
                if rule.thresholds.len() == count { rule.thresholds.clone() } else { default_icon_thresholds(count) };
            let mut values = String::new();
            for threshold in &thresholds {
                values.push_str(&cfvo_xml(threshold)?);
            }
            let mut attributes = format!(" iconSet=\"{}\"", escape_attr(name));
            if rule.hide_value {
                attributes.push_str(" showValue=\"0\"");
            }
            if rule.reverse_icons {
                attributes.push_str(" reverse=\"1\"");
            }
            return Ok(wrap(format!(
                "<cfRule type=\"iconSet\" priority=\"{priority}\"{stop}><iconSet{attributes}>{values}</iconSet></cfRule>"
            )));
        }
        _ => {}
    }

    let dxf = styles.dxf_id(rule.fill.as_deref(), rule.color.as_deref(), rule.bold, rule.italic);
    let cell_is = |operator: &str, formulas: &[&str]| -> String {
        let body: String = formulas.iter().map(|value| format!("<formula>{}</formula>", escape_text(value))).collect();
        format!(
            "<cfRule type=\"cellIs\" dxfId=\"{dxf}\" priority=\"{priority}\"{stop} operator=\"{operator}\">{body}</cfRule>"
        )
    };
    // The editor writes `textContains` / `duplicate`; older files may carry the
    // shorter spellings, so both are accepted.
    let cf_rule = match rule.kind.as_str() {
        "greater" => cell_is("greaterThan", &[first_value(rule, "0")]),
        "less" => cell_is("lessThan", &[first_value(rule, "0")]),
        "equal" => cell_is("equal", &[first_value(rule, "0")]),
        "between" => cell_is("between", &[first_value(rule, "0"), second_value(rule, "0")]),
        "text" | "textContains" => format!(
            "<cfRule type=\"containsText\" dxfId=\"{dxf}\" priority=\"{priority}\"{stop} operator=\"containsText\" text=\"{}\"><formula>NOT(ISERROR(SEARCH(&quot;{}&quot;,{})))</formula></cfRule>",
            escape_attr(first_value(rule, "")),
            escape_text(first_value(rule, "")),
            anchor
        ),
        "duplicates" | "duplicate" => format!(
            "<cfRule type=\"duplicateValues\" dxfId=\"{dxf}\" priority=\"{priority}\"{stop}/>"
        ),
        "top" | "bottom" => {
            let rank = rule.top_n.unwrap_or(10);
            let bottom = if rule.kind == "bottom" { " bottom=\"1\"" } else { "" };
            format!("<cfRule type=\"top10\" dxfId=\"{dxf}\" priority=\"{priority}\"{stop} rank=\"{rank}\"{bottom}/>")
        }
        "expression" => {
            let formula = rule.formula.as_deref().map(crate::formula::to_xlsx).unwrap_or_default();
            if formula.is_empty() {
                return Err("the formula rule has no formula".into());
            }
            format!(
                "<cfRule type=\"expression\" dxfId=\"{dxf}\" priority=\"{priority}\"{stop}><formula>{}</formula></cfRule>",
                escape_text(&formula)
            )
        }
        other => return Err(format!("the rule type \"{other}\" is not exportable")),
    };
    Ok(wrap(cf_rule))
}

fn first_value<'a>(rule: &'a CondRule, fallback: &'a str) -> &'a str {
    rule.values.first().map(String::as_str).unwrap_or(fallback)
}

fn second_value<'a>(rule: &'a CondRule, fallback: &'a str) -> &'a str {
    rule.values.get(1).map(String::as_str).unwrap_or(fallback)
}

/// The top-left cell of a range, used as the relative anchor in rule formulas.
/// A multi-area range (`A1:A5 C1:C5`) anchors at its first area.
pub(crate) fn anchor_of(range: &str) -> String {
    let first = range.split_whitespace().next().unwrap_or(range);
    first.split(':').next().unwrap_or(first).to_string()
}
