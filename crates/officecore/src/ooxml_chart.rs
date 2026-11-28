//! Shared OOXML ChartML serialization, parsing, and text-summary helpers.

mod read;

pub(crate) use read::{first_descendant, read_chart_xml};

use crate::address::column_name;
use crate::error::OfficeResult;
use crate::model::{Cell, CellValue, ChartData, Sheet, Workbook};
use crate::xml::{escape_attr, escape_text};
use crate::xlsx::scatter_flavour;

const CHART_CATEGORY_AXIS: u64 = 111_111_111;
const CHART_VALUE_AXIS: u64 = 222_222_222;
const DOUGHNUT_DEFAULT_HOLE: u32 = 50;

pub(crate) fn chart_kind_supported(kind: &str) -> bool {
    matches!(kind, "column" | "bar" | "line" | "area" | "pie" | "doughnut" | "scatter")
}

pub(crate) fn chart_has_cache(chart: &ChartData) -> bool {
    !chart.categories_cache.is_empty() || chart.series_values_cache.iter().any(|values| !values.is_empty())
}

fn cache_number(value: f64) -> String {
    if !value.is_finite() {
        return "0".into();
    }
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

fn title_xml(text: &str) -> String {
    format!(
        "<c:title><c:tx><c:rich><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang=\"en-US\"/><a:t>{}</a:t></a:r></a:p></c:rich></c:tx><c:overlay val=\"0\"/></c:title>",
        escape_text(text)
    )
}

fn str_cache_xml(labels: &[String]) -> String {
    if labels.is_empty() {
        return String::new();
    }
    let mut out = format!("<c:strCache><c:ptCount val=\"{}\"/>", labels.len());
    for (index, label) in labels.iter().enumerate() {
        out.push_str(&format!("<c:pt idx=\"{index}\"><c:v>{}</c:v></c:pt>", escape_text(label)));
    }
    out.push_str("</c:strCache>");
    out
}

fn num_cache_xml(values: &[f64]) -> String {
    if values.is_empty() {
        return String::new();
    }
    let mut out = format!("<c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"{}\"/>", values.len());
    for (index, value) in values.iter().enumerate() {
        out.push_str(&format!("<c:pt idx=\"{index}\"><c:v>{}</c:v></c:pt>", cache_number(*value)));
    }
    out.push_str("</c:numCache>");
    out
}

fn axes_xml(chart: &ChartData, scatter: bool) -> String {
    let x_title = if chart.x_title.is_empty() { String::new() } else { title_xml(&chart.x_title) };
    let y_title = if chart.y_title.is_empty() { String::new() } else { title_xml(&chart.y_title) };
    if scatter {
        format!(
            "<c:valAx><c:axId val=\"{x}\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"b\"/>{x_title}<c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:crossAx val=\"{y}\"/><c:crossBetween val=\"midCat\"/></c:valAx><c:valAx><c:axId val=\"{y}\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"l\"/>{y_title}<c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:crossAx val=\"{x}\"/><c:crossBetween val=\"midCat\"/></c:valAx>",
            x = CHART_CATEGORY_AXIS,
            y = CHART_VALUE_AXIS,
        )
    } else {
        format!(
            "<c:catAx><c:axId val=\"{x}\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"b\"/>{x_title}<c:crossAx val=\"{y}\"/></c:catAx><c:valAx><c:axId val=\"{y}\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"l\"/>{y_title}<c:crossAx val=\"{x}\"/></c:valAx>",
            x = CHART_CATEGORY_AXIS,
            y = CHART_VALUE_AXIS,
        )
    }
}

fn sheet_col_range(col: u32, rows: usize) -> String {
    let col_name = column_name(col);
    let end_row = rows.max(1) + 1;
    format!("Sheet1!${col_name}$2:${col_name}${end_row}")
}

fn scatter_look_xml(color: Option<&str>, lines: bool, markers: bool) -> String {
    let rgb = color.map(|c| c.trim_start_matches('#').to_ascii_uppercase()).filter(|s| s.len() == 6);
    let line = match (&rgb, lines) {
        (_, false) => "<a:ln w=\"19050\"><a:noFill/></a:ln>".to_string(),
        (Some(hex), true) => {
            format!("<a:ln w=\"28575\" cap=\"rnd\"><a:solidFill><a:srgbClr val=\"{hex}\"/></a:solidFill><a:round/></a:ln>")
        }
        (None, true) => String::new(),
    };
    let shape = if line.is_empty() { String::new() } else { format!("<c:spPr>{line}</c:spPr>") };
    let marker = if !markers {
        "<c:marker><c:symbol val=\"none\"/></c:marker>".to_string()
    } else {
        let fill = rgb
            .map(|hex| {
                format!(
                    "<c:spPr><a:solidFill><a:srgbClr val=\"{hex}\"/></a:solidFill><a:ln w=\"9525\"><a:solidFill><a:srgbClr val=\"{hex}\"/></a:solidFill></a:ln></c:spPr>"
                )
            })
            .unwrap_or_default();
        format!("<c:marker><c:symbol val=\"circle\"/><c:size val=\"7\"/>{fill}</c:marker>")
    };
    format!("{shape}{marker}")
}

/// Builds the embedded `.xlsx` workbook backing a Writer/Impress chart's cache.
pub(crate) fn embedded_workbook(chart: &ChartData) -> OfficeResult<Vec<u8>> {
    let mut workbook = Workbook::new_blank("Chart data");
    workbook.sheets.clear();
    let mut sheet = Sheet::new("Sheet1");
    sheet.set("A1", Cell { value: CellValue::Text("Category".into()), ..Default::default() });
    for (index, series) in chart.series.iter().enumerate() {
        let address = crate::address::format(0, index as u32 + 1);
        sheet.set(&address, Cell { value: CellValue::Text(series.name.clone()), ..Default::default() });
    }
    for (row, label) in chart.categories_cache.iter().enumerate() {
        let address = crate::address::format(row as u32 + 1, 0);
        sheet.set(&address, Cell { value: CellValue::Text(label.clone()), ..Default::default() });
    }
    for (series_index, values) in chart.series_values_cache.iter().enumerate() {
        for (row, value) in values.iter().enumerate() {
            let address = crate::address::format(row as u32 + 1, series_index as u32 + 1);
            sheet.set(&address, Cell { value: CellValue::Number(*value), ..Default::default() });
        }
    }
    workbook.sheets.push(sheet);
    crate::xlsx::write_xlsx(&workbook)
}

/// Serializes one `ChartData` to ChartML XML across all 7 chart kinds.
pub(crate) fn chart_part_xml(chart: &ChartData, external_rid: Option<&str>) -> Result<String, String> {
    let kind = chart.kind.as_str();
    if !chart_kind_supported(kind) {
        return Err(format!("the chart type \"{kind}\" is not representable yet"));
    }
    if chart.series.is_empty() {
        return Err("the chart has no data series".into());
    }
    let max_rows = chart
        .series_values_cache
        .iter()
        .map(Vec::len)
        .chain(std::iter::once(chart.categories_cache.len()))
        .max()
        .unwrap_or(0);
    let categories_ref = if chart.categories.trim().is_empty() {
        if max_rows > 0 {
            sheet_col_range(0, max_rows)
        } else {
            String::new()
        }
    } else {
        chart.categories.trim().to_string()
    };
    let scatter = kind == "scatter";
    if categories_ref.is_empty() && !scatter {
        return Err("the chart has no category range".into());
    }

    let (scatter_lines, scatter_markers, scatter_smooth) = scatter_flavour(chart.scatter_style.as_deref());
    let mut series_xml = String::new();
    let label_cache = str_cache_xml(&chart.categories_cache);

    for (index, series) in chart.series.iter().enumerate() {
        let val_ref = if series.range.trim().is_empty() {
            sheet_col_range(index as u32 + 1, max_rows)
        } else {
            series.range.trim().to_string()
        };
        let value_cache = chart.series_values_cache.get(index).map(|v| num_cache_xml(v)).unwrap_or_default();
        series_xml.push_str(&format!(
            "<c:ser><c:idx val=\"{index}\"/><c:order val=\"{index}\"/><c:tx><c:v>{}</c:v></c:tx>",
            escape_text(&series.name)
        ));
        if scatter {
            series_xml.push_str(&scatter_look_xml(series.color.as_deref(), scatter_lines, scatter_markers));
            if !categories_ref.is_empty() {
                let nums: Vec<f64> =
                    chart.categories_cache.iter().filter_map(|l| l.trim().parse::<f64>().ok()).collect();
                if chart.categories_cache.is_empty() || nums.len() == chart.categories_cache.len() {
                    series_xml.push_str(&format!(
                        "<c:xVal><c:numRef><c:f>{}</c:f>{}</c:numRef></c:xVal>",
                        escape_text(&categories_ref),
                        num_cache_xml(&nums)
                    ));
                } else {
                    series_xml.push_str(&format!(
                        "<c:xVal><c:strRef><c:f>{}</c:f>{label_cache}</c:strRef></c:xVal>",
                        escape_text(&categories_ref)
                    ));
                }
            }
            series_xml.push_str(&format!(
                "<c:yVal><c:numRef><c:f>{}</c:f>{value_cache}</c:numRef></c:yVal><c:smooth val=\"{}\"/></c:ser>",
                escape_text(&val_ref),
                u8::from(scatter_smooth)
            ));
            continue;
        }
        if let Some(color) = series.color.as_deref() {
            series_xml.push_str(&format!(
                "<c:spPr><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill></c:spPr>",
                escape_attr(color.trim_start_matches('#'))
            ));
        }
        series_xml.push_str(&format!(
            "<c:cat><c:strRef><c:f>{}</c:f>{}</c:strRef></c:cat><c:val><c:numRef><c:f>{}</c:f>{value_cache}</c:numRef></c:val></c:ser>",
            escape_text(&categories_ref),
            if index == 0 { label_cache.as_str() } else { "" },
            escape_text(&val_ref)
        ));
    }

    let labels = if chart.show_labels {
        "<c:dLbls><c:showLegendKey val=\"0\"/><c:showVal val=\"1\"/><c:showCatName val=\"0\"/><c:showSerName val=\"0\"/><c:showPercent val=\"0\"/><c:showBubbleSize val=\"0\"/></c:dLbls>"
    } else {
        ""
    };
    let mut axes = String::new();
    let plot = match kind {
        "column" | "bar" => {
            axes = axes_xml(chart, false);
            let dir = if kind == "bar" { "bar" } else { "col" };
            let grp = if chart.stacked { "stacked" } else { "clustered" };
            let ov = if chart.stacked { "<c:overlap val=\"100\"/>" } else { "" };
            format!("<c:barChart><c:barDir val=\"{dir}\"/><c:grouping val=\"{grp}\"/><c:varyColors val=\"0\"/>{series_xml}{labels}<c:gapWidth val=\"150\"/>{ov}<c:axId val=\"{CHART_CATEGORY_AXIS}\"/><c:axId val=\"{CHART_VALUE_AXIS}\"/></c:barChart>")
        }
        "line" => {
            axes = axes_xml(chart, false);
            format!("<c:lineChart><c:grouping val=\"standard\"/><c:varyColors val=\"0\"/>{series_xml}{labels}<c:marker val=\"1\"/><c:axId val=\"{CHART_CATEGORY_AXIS}\"/><c:axId val=\"{CHART_VALUE_AXIS}\"/></c:lineChart>")
        }
        "area" => {
            axes = axes_xml(chart, false);
            let grp = if chart.stacked { "stacked" } else { "standard" };
            format!("<c:areaChart><c:grouping val=\"{grp}\"/><c:varyColors val=\"0\"/>{series_xml}{labels}<c:axId val=\"{CHART_CATEGORY_AXIS}\"/><c:axId val=\"{CHART_VALUE_AXIS}\"/></c:areaChart>")
        }
        "pie" => format!("<c:pieChart><c:varyColors val=\"1\"/>{series_xml}{labels}<c:firstSliceAng val=\"0\"/></c:pieChart>"),
        "doughnut" => {
            let hole = chart.hole_size.unwrap_or(DOUGHNUT_DEFAULT_HOLE).clamp(10, 90);
            format!("<c:doughnutChart><c:varyColors val=\"1\"/>{series_xml}{labels}<c:firstSliceAng val=\"0\"/><c:holeSize val=\"{hole}\"/></c:doughnutChart>")
        }
        "scatter" => {
            axes = axes_xml(chart, true);
            let style = if scatter_smooth { "smoothMarker" } else { "lineMarker" };
            format!("<c:scatterChart><c:scatterStyle val=\"{style}\"/><c:varyColors val=\"0\"/>{series_xml}{labels}<c:axId val=\"{CHART_CATEGORY_AXIS}\"/><c:axId val=\"{CHART_VALUE_AXIS}\"/></c:scatterChart>")
        }
        _ => return Err(format!("the chart type \"{kind}\" is not representable yet")),
    };

    let title = if chart.title.is_empty() { String::new() } else { title_xml(&chart.title) };
    let legend = if chart.legend { "<c:legend><c:legendPos val=\"b\"/><c:overlay val=\"0\"/></c:legend>" } else { "" };
    let external = external_rid
        .map(|rid| format!("<c:externalData r:id=\"{}\"><c:autoUpdate val=\"0\"/></c:externalData>", escape_attr(rid)))
        .unwrap_or_default();
    Ok(format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><c:roundedCorners val=\"0\"/><c:chart>{title}<c:plotArea><c:layout/>{plot}{axes}</c:plotArea>{legend}<c:plotVisOnly val=\"1\"/><c:dispBlanksAs val=\"gap\"/></c:chart>{external}</c:chartSpace>"
    ))
}

/// Concise text representation for formats without native chart embedding.
pub(crate) fn chart_text_summary(chart: &ChartData, caption: &str) -> String {
    let label = if !caption.trim().is_empty() {
        caption.trim()
    } else if !chart.title.trim().is_empty() {
        chart.title.trim()
    } else {
        chart.kind.as_str()
    };
    let preview: Vec<String> = chart
        .series
        .iter()
        .take(3)
        .enumerate()
        .map(|(i, ser)| {
            let pts: Vec<String> = chart
                .series_values_cache
                .get(i)
                .map(|vals| vals.iter().take(4).map(|v| cache_number(*v)).collect())
                .unwrap_or_default();
            if pts.is_empty() {
                ser.name.clone()
            } else {
                format!("{}: {}", ser.name, pts.join(", "))
            }
        })
        .collect();
    if preview.is_empty() {
        format!("[Chart ({}): {label}]", chart.kind)
    } else {
        format!("[Chart ({}): {label} — {}]", chart.kind, preview.join("; "))
    }
}
