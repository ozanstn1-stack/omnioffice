//! OpenDocument Format support: ODT (Writer), ODS (Calc) and ODP (Impress).
//!
//! Written directly as ODF 1.2 packages so LibreOffice, OpenOffice and
//! OnlyOffice open the results. Import is tolerant: unknown constructs are
//! skipped with warnings, and nothing is ever executed from the package.

use crate::error::{OfficeError, OfficeResult};
use crate::model::*;
use crate::xml::{parse_xml, XmlNode, XmlWriter};
use crate::zip::{ZipReader, ZipWriter};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

const NS: &str = concat!(
    "xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
    "xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" ",
    "xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" ",
    "xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" ",
    "xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" ",
    "xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" ",
    "xmlns:xlink=\"http://www.w3.org/1999/xlink\" ",
    "xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\" ",
    "xmlns:number=\"urn:oasis:names:tc:opendocument:xmlns:datastyle:1.0\" ",
    "xmlns:meta=\"urn:oasis:names:tc:opendocument:xmlns:meta:1.0\" ",
    "xmlns:dc=\"http://purl.org/dc/elements/1.1/\" ",
    "xmlns:presentation=\"urn:oasis:names:tc:opendocument:xmlns:presentation:1.0\""
);
mod sheets_read;
use sheets_read::{ods_annotation, ods_href, read_ods_chart};
mod text_read;
use text_read::{collect_styles, node_text_runs, paragraph_props, wrap_trailing_text, NoteReadState};
mod text_write;
pub use sheets_read::{read_ods, read_ods_file};
pub use text_read::{read_odt, read_odt_file};
pub(crate) use text_write::{
    cm, escape, manifest_for, meta_xml, paragraph_style_xml, parse_cm, write_runs, write_table, AutoStyles, Media,
    NoteContext,
};
pub use text_write::{write_odt, write_odt_file, TextRead};

/// A named style's graphic formatting, resolved from `style:graphic-properties`
/// so frames can report their text wrapping and the watermark can be detected.
#[derive(Default, Clone)]
struct GraphicStyle {
    /// The `style:wrap` value (`parallel`, `none`, `run-through`, ...).
    wrap: String,
    /// Whether the style puts the frame behind the text.
    run_through: bool,
    /// `draw:opacity`, 0..1.
    opacity: f64,
}

/// Named styles resolved from content.xml's automatic styles and styles.xml:
/// paragraph formatting, run formatting and graphic styles.
#[derive(Default)]
struct ReadStyles {
    paragraphs: HashMap<String, ParaProps>,
    texts: HashMap<String, Run>,
    graphics: HashMap<String, GraphicStyle>,
}

/// Overlays the attributes of one `style:text-properties` element onto `run`.
/// Attributes that are absent leave the inherited value alone.

// ---------------------------------------------------------------------------
// ODS
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct SheetRead {
    pub workbook: Workbook,
    pub warnings: Vec<String>,
}

fn odf_formula_to_ours(formula: &str) -> String {
    crate::formula::from_odf(formula)
}

fn our_formula_to_odf(formula: &str) -> String {
    crate::formula::to_odf(formula)
}

fn cell_style_xml(style: &CellStyle) -> String {
    let mut text = String::new();
    if style.bold {
        text.push_str(" fo:font-weight=\"bold\"");
    }
    if style.italic {
        text.push_str(" fo:font-style=\"italic\"");
    }
    if style.underline {
        text.push_str(" style:text-underline-style=\"solid\"");
    }
    if let Some(size) = style.size_pt {
        text.push_str(&format!(" fo:font-size=\"{size}pt\""));
    }
    if let Some(color) = &style.color {
        text.push_str(&format!(" fo:color=\"{}\"", escape(color)));
    }
    let mut cell = String::new();
    if let Some(fill) = &style.fill {
        cell.push_str(&format!(" fo:background-color=\"{}\"", escape(fill)));
    }
    match style.align.as_str() {
        "center" => cell.push_str(" style:text-align=\"center\""),
        "right" => cell.push_str(" style:text-align=\"right\""),
        _ => {}
    }
    if style.wrap {
        cell.push_str(" fo:wrap-option=\"wrap\"");
    }
    if let Some(border) = &style.borders.top {
        if border.style != "none" {
            cell.push_str(&format!(" fo:border-top=\"0.05pt solid {}\"", escape(&border.color)));
        }
    }
    if let Some(border) = &style.borders.bottom {
        if border.style != "none" {
            cell.push_str(&format!(" fo:border-bottom=\"0.05pt solid {}\"", escape(&border.color)));
        }
    }
    format!("<style:table-cell-properties{cell}/><style:text-properties{text}/>")
}

// ---------------------------------------------------------------------------
// ODS charts (embedded chart objects) and pivot output
// ---------------------------------------------------------------------------

/// Namespaces a chart sub-document needs on top of [`NS`]. `loext` carries the
/// literal series names; ODF 1.2 can only name a series through a cell.
const CHART_NS: &str = concat!(
    "xmlns:chart=\"urn:oasis:names:tc:opendocument:xmlns:chart:1.0\" ",
    "xmlns:loext=\"urn:org:documentfoundation:names:experimental:office:xmlns:loext:1.0\""
);

/// LibreOffice's default column width (2.258cm) and row height (0.452cm) in
/// points. Only used to find an anchor cell for a chart that sits on the sheet
/// (`table:shapes`) instead of inside a cell.
const ODS_DEFAULT_COLUMN_PT: f64 = 64.0;
const ODS_DEFAULT_ROW_PT: f64 = 12.8;

/// Upper bound on the rows read from a chart's local table.
const ODS_MAX_CHART_ROWS: usize = 100_000;

/// Limits that keep a small hostile package from exhausting memory: series per
/// chart, cached values per chart (series x rows) and charts per sheet.
const ODS_MAX_CHART_SERIES: usize = 255;
const ODS_MAX_CHART_VALUES: usize = 1_000_000;
const ODS_MAX_CHARTS_PER_SHEET: usize = 64;

/// The sheet area `read_ods` imports cells from; a chart anchor outside it is
/// clamped to it, so an import never asks the writer for millions of rows.
const ODS_MAX_READ_ROW: u32 = 100_000;
const ODS_MAX_READ_COLUMN: u32 = 1_000;

/// A chart written as the `Object N/` sub-document of an ODS package.
struct OdsChartObject {
    name: String,
    content: String,
}

/// Why a sheet chart cannot be written to ODS, if it cannot. The writer skips
/// such a chart; the compatibility report names it before the save.
pub(crate) fn ods_chart_problem(chart: &ChartData) -> Option<String> {
    if !matches!(chart.kind.as_str(), "column" | "bar" | "line" | "pie" | "area" | "scatter" | "doughnut") {
        return Some(format!("the chart type \"{}\" is not exportable", chart.kind));
    }
    if chart.series.is_empty() {
        return Some("the chart has no data series".into());
    }
    if !chart.categories.trim().is_empty() && crate::address::parse_range(&chart.categories).is_none() {
        return Some("the category range could not be read".into());
    }
    chart
        .series
        .iter()
        .find(|series| crate::address::parse_range(&series.range).is_none())
        .map(|series| format!("the range for series \"{}\" could not be read", series.name))
}

/// A sheet name as it appears in an ODF cell address; names that are not plain
/// words are quoted with `'` doubled inside.
fn ods_sheet_ref(name: &str) -> String {
    if !name.is_empty() && name.chars().all(|ch| ch.is_alphanumeric() || ch == '_') {
        name.to_string()
    } else {
        format!("'{}'", name.replace('\'', "''"))
    }
}

/// `A2:A5` on `sheet` as an absolute ODF cell range address
/// (`Sheet1.$A$2:Sheet1.$A$5`); a single cell stays a single address.
fn ods_range_address(sheet: &str, range: &str) -> Option<String> {
    let range = range.trim();
    if range.is_empty() {
        return None;
    }
    let ((start_row, start_col), (end_row, end_col)) = crate::address::parse_range(range)?;
    let sheet = ods_sheet_ref(sheet);
    let start = format!("{sheet}.${}${}", crate::address::column_name(start_col), start_row + 1);
    if (start_row, start_col) == (end_row, end_col) {
        return Some(start);
    }
    Some(format!("{start}:{sheet}.${}${}", crate::address::column_name(end_col), end_row + 1))
}

/// The first range of an ODF cell range address as a sheet-local `A2:A5`
/// (sheet names and `$` dropped, like the XLSX importer does).
fn ods_range_to_ours(address: &str) -> String {
    let mut parts = vec![String::new()];
    let mut quoted = false;
    for ch in address.trim().chars() {
        match ch {
            '\'' => {
                quoted = !quoted;
                parts.last_mut().unwrap().push(ch);
            }
            ':' if !quoted => parts.push(String::new()),
            ch if ch.is_whitespace() && !quoted => break,
            ch => parts.last_mut().unwrap().push(ch),
        }
    }
    parts
        .iter()
        .map(|part| part.rsplit('.').next().unwrap_or(part).replace('$', ""))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(":")
}

/// A model colour as the `#rrggbb` ODF requires, or `None`.
fn ods_color(color: &str) -> Option<String> {
    let hex = color.trim().strip_prefix('#')?;
    (hex.len() == 6 && hex.chars().all(|ch| ch.is_ascii_hexdigit())).then(|| format!("#{hex}"))
}

fn ods_cell_label(value: &CellValue) -> String {
    match value {
        CellValue::Empty => String::new(),
        CellValue::Number(number) => format!("{number}"),
        CellValue::Bool(flag) => if *flag { "TRUE" } else { "FALSE" }.into(),
        CellValue::Text(text) | CellValue::Error(text) => text.clone(),
    }
}

fn ods_cell_number(value: &CellValue) -> Option<f64> {
    match value {
        CellValue::Number(number) => Some(*number),
        CellValue::Bool(flag) => Some(if *flag { 1.0 } else { 0.0 }),
        CellValue::Text(text) => text.trim().parse::<f64>().ok(),
        _ => None,
    }
    .filter(|number| number.is_finite())
}

/// The `table:table` LibreOffice keeps inside every chart object: a header row
/// with the series names, then one row per category with the cached values.
/// The model caches are written when present; otherwise the values come from
/// the sheet cells so the object renders on its own.
fn ods_chart_local_table(chart: &ChartData, cells: &std::collections::BTreeMap<String, Cell>) -> String {
    let cell_values = |range: &str| -> Vec<CellValue> {
        crate::address::expand_range(range, ODS_MAX_CHART_ROWS)
            .iter()
            .map(|address| cells.get(address).map(|cell| cell.value.clone()).unwrap_or_default())
            .collect()
    };
    let categories: Vec<String> = if chart.categories_cache.is_empty() {
        cell_values(&chart.categories).iter().map(ods_cell_label).collect()
    } else {
        chart.categories_cache.clone()
    };
    let series: Vec<Vec<Option<f64>>> = chart
        .series
        .iter()
        .enumerate()
        .map(|(index, series)| match chart.series_values_cache.get(index).filter(|values| !values.is_empty()) {
            Some(values) => values.iter().map(|value| Some(*value).filter(|value| value.is_finite())).collect(),
            None => cell_values(&series.range).iter().map(ods_cell_number).collect(),
        })
        .collect();
    let rows = series.iter().map(Vec::len).chain(std::iter::once(categories.len())).max().unwrap_or(0);
    let mut out = format!(
        "<table:table table:name=\"local-table\"><table:table-header-columns><table:table-column/></table:table-header-columns><table:table-columns><table:table-column table:number-columns-repeated=\"{}\"/></table:table-columns><table:table-header-rows><table:table-row><table:table-cell><text:p/></table:table-cell>",
        chart.series.len().max(1)
    );
    for entry in &chart.series {
        out.push_str(&format!(
            "<table:table-cell office:value-type=\"string\"><text:p>{}</text:p></table:table-cell>",
            crate::xml::escape_text(&entry.name)
        ));
    }
    out.push_str("</table:table-row></table:table-header-rows><table:table-rows>");
    for row in 0..rows {
        let label = categories.get(row).map(String::as_str).unwrap_or("");
        // The first column of a scatter chart holds X values, which are numbers.
        match label.trim().parse::<f64>().ok().filter(|number| chart.kind == "scatter" && number.is_finite()) {
            Some(number) => out.push_str(&format!(
                "<table:table-row><table:table-cell office:value-type=\"float\" office:value=\"{number}\"><text:p>{}</text:p></table:table-cell>",
                crate::xml::escape_text(label)
            )),
            None => out.push_str(&format!(
                "<table:table-row><table:table-cell office:value-type=\"string\"><text:p>{}</text:p></table:table-cell>",
                crate::xml::escape_text(label)
            )),
        }
        for values in &series {
            match values.get(row).copied().flatten() {
                Some(value) => out.push_str(&format!(
                    "<table:table-cell office:value-type=\"float\" office:value=\"{value}\"><text:p>{value}</text:p></table:table-cell>"
                )),
                None => out.push_str("<table:table-cell><text:p/></table:table-cell>"),
            }
        }
        out.push_str("</table:table-row>");
    }
    out.push_str("</table:table-rows></table:table>");
    out
}

/// `Object N/content.xml` for one sheet chart, following LibreOffice's layout:
/// `chart:chart` with title, legend and plot area (axes, series pointing at the
/// sheet ranges) plus the local table with the cached values. A column chart
/// is `chart:bar`; a bar chart is the same class with `chart:vertical`.
fn ods_chart_content(
    placement: &ChartPlacement,
    sheet_name: &str,
    cells: &std::collections::BTreeMap<String, Cell>,
) -> String {
    let chart = &placement.chart;
    let kind = chart.kind.as_str();
    let pie = matches!(kind, "pie" | "doughnut");
    let scatter = kind == "scatter";
    // LibreOffice's donut is the `chart:ring` class. ODF has no hole size, so a
    // custom `hole_size` stays in the .oswk and XLSX files only.
    let class = match kind {
        "line" => "chart:line",
        "pie" => "chart:circle",
        "doughnut" => "chart:ring",
        "area" => "chart:area",
        "scatter" => "chart:scatter",
        _ => "chart:bar",
    };
    let (scatter_lines, scatter_markers, scatter_smooth) = crate::xlsx::scatter_flavour(chart.scatter_style.as_deref());
    let mut plot_properties = String::new();
    if matches!(kind, "column" | "bar") {
        plot_properties.push_str(&format!(" chart:vertical=\"{}\"", kind == "bar"));
    }
    if chart.stacked && !pie && !scatter {
        plot_properties.push_str(" chart:stacked=\"true\"");
    }
    if scatter && scatter_smooth {
        plot_properties.push_str(" chart:interpolation=\"cubic-spline\"");
    }
    let mut styles = format!(
        "<style:style style:name=\"chPlot\" style:family=\"chart\"><style:chart-properties{plot_properties}/></style:style>"
    );
    // A scatter chart reads its X values from the first series' domain.
    let domain = if scatter { ods_range_address(sheet_name, &chart.categories) } else { None };
    let mut series_xml = String::new();
    for (index, series) in chart.series.iter().enumerate() {
        let style_name = format!("chS{}", index + 1);
        let mut chart_properties = String::new();
        if chart.show_labels {
            chart_properties.push_str(" chart:data-label-number=\"value\"");
        }
        if scatter {
            let symbol = if scatter_markers { "automatic" } else { "none" };
            chart_properties.push_str(&format!(" chart:symbol-type=\"{symbol}\""));
        }
        let color = series.color.as_deref().and_then(ods_color);
        let stroke = if scatter {
            format!(" draw:stroke=\"{}\"", if scatter_lines { "solid" } else { "none" })
        } else {
            String::new()
        };
        let graphic = match color {
            Some(color) => format!(
                "<style:graphic-properties{stroke} draw:fill=\"solid\" draw:fill-color=\"{color}\" svg:stroke-color=\"{color}\"/>"
            ),
            None if scatter => format!("<style:graphic-properties{stroke}/>"),
            None => String::new(),
        };
        styles.push_str(&format!(
            "<style:style style:name=\"{style_name}\" style:family=\"chart\"><style:chart-properties{chart_properties}/>{graphic}</style:style>"
        ));
        // LibreOffice stores a literal series name as a quoted string literal.
        let domain_xml = domain
            .as_deref()
            .map(|range| format!("<chart:domain table:cell-range-address=\"{}\"/>", escape(range)))
            .unwrap_or_default();
        let series_head = format!(
            "<chart:series chart:style-name=\"{style_name}\" chart:class=\"{class}\" chart:values-cell-range-address=\"{}\" loext:label-string=\"{}\"",
            escape(&ods_range_address(sheet_name, &series.range).unwrap_or_default()),
            escape(&format!("\"{}\"", series.name.replace('"', "\"\"")))
        );
        if domain_xml.is_empty() {
            series_xml.push_str(&format!("{series_head}/>"));
        } else {
            series_xml.push_str(&format!("{series_head}>{domain_xml}</chart:series>"));
        }
    }
    let categories = ods_range_address(sheet_name, &chart.categories);
    let categories_xml = categories
        .as_deref()
        .filter(|_| !scatter)
        .map(|range| format!("<chart:categories table:cell-range-address=\"{}\"/>", escape(range)))
        .unwrap_or_default();
    // Like the XLSX export, a pie chart has no axes worth a title.
    let axis_title = |text: &str| {
        if text.is_empty() || pie {
            String::new()
        } else {
            format!("<chart:title><text:p>{}</text:p></chart:title>", crate::xml::escape_text(text))
        }
    };
    let grid = if pie { "" } else { "<chart:grid chart:class=\"major\"/>" };
    // LibreOffice hides the tick labels of an axis that has no style.
    let axis_style = if pie {
        ""
    } else {
        styles.push_str(
            "<style:style style:name=\"chAxis\" style:family=\"chart\"><style:chart-properties chart:display-label=\"true\"/></style:style>",
        );
        " chart:style-name=\"chAxis\""
    };
    let axes = format!(
        "<chart:axis chart:dimension=\"x\" chart:name=\"primary-x\"{axis_style}>{}{categories_xml}</chart:axis><chart:axis chart:dimension=\"y\" chart:name=\"primary-y\"{axis_style}>{}{grid}</chart:axis>",
        axis_title(&chart.x_title),
        axis_title(&chart.y_title)
    );
    let ranges: Vec<String> = categories
        .into_iter()
        .chain(chart.series.iter().filter_map(|series| ods_range_address(sheet_name, &series.range)))
        .collect();
    let title = if chart.title.is_empty() {
        String::new()
    } else {
        format!("<chart:title><text:p>{}</text:p></chart:title>", crate::xml::escape_text(&chart.title))
    };
    let legend = if chart.legend { "<chart:legend chart:legend-position=\"bottom\"/>" } else { "" };
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-content {NS} {CHART_NS} office:version=\"1.2\"><office:automatic-styles>{styles}</office:automatic-styles><office:body><office:chart><chart:chart svg:width=\"{}\" svg:height=\"{}\" chart:class=\"{class}\">{title}{legend}<chart:plot-area chart:style-name=\"chPlot\" table:cell-range-address=\"{}\">{axes}{series_xml}<chart:wall/><chart:floor/></chart:plot-area>{}</chart:chart></office:chart></office:body></office:document-content>",
        cm(placement.width_px.max(64.0) * 0.75),
        cm(placement.height_px.max(64.0) * 0.75),
        escape(&ranges.join(" ")),
        ods_chart_local_table(chart, cells)
    )
}

/// The chart frames of one sheet keyed by their anchor cell (zero-based row,
/// column). Each frame is anchored to its cell, so its position is the offset
/// inside that cell; the chart itself goes to `objects` as `Object N`.
fn ods_chart_frames(
    sheet: &Sheet,
    cells: &std::collections::BTreeMap<String, Cell>,
    objects: &mut Vec<OdsChartObject>,
) -> HashMap<(u32, u32), String> {
    let mut frames: HashMap<(u32, u32), String> = HashMap::new();
    for (index, placement) in sheet.charts.iter().enumerate() {
        if ods_chart_problem(&placement.chart).is_some() {
            continue;
        }
        let name = format!("Object {}", objects.len() + 1);
        let ranges: Vec<String> = std::iter::once(&placement.chart.categories)
            .chain(placement.chart.series.iter().map(|series| &series.range))
            .filter_map(|range| ods_range_address(&sheet.name, range))
            .collect();
        let anchor = crate::address::parse(&placement.anchor).unwrap_or((0, 0));
        frames.entry(anchor).or_default().push_str(&format!(
            "<draw:frame draw:z-index=\"{index}\" draw:name=\"{}\" svg:width=\"{}\" svg:height=\"{}\" svg:x=\"0cm\" svg:y=\"0cm\"><draw:object draw:notify-on-update-of-ranges=\"{}\" xlink:href=\"./{name}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"/></draw:frame>",
            escape(if placement.id.is_empty() { &name } else { &placement.id }),
            cm(placement.width_px.max(64.0) * 0.75),
            cm(placement.height_px.max(64.0) * 0.75),
            escape(&ranges.join(" "))
        ));
        objects.push(OdsChartObject { content: ods_chart_content(placement, &sheet.name, cells), name });
    }
    frames
}

/// The sheet cells plus the computed output of the sheet's editor pivot
/// tables, which ODS (like the XLSX export) gets as plain values. A cell that
/// already holds a value or formula wins over the pivot output.
fn ods_cells_with_pivots<'a>(
    workbook: &Workbook,
    sheet: &'a Sheet,
) -> std::borrow::Cow<'a, std::collections::BTreeMap<String, Cell>> {
    let mut cells = std::borrow::Cow::Borrowed(&sheet.cells);
    for (address, value) in crate::pivot::materialize(workbook, sheet) {
        let taken =
            sheet.get(&address).map(|cell| cell.formula.is_some() || cell.value != CellValue::Empty).unwrap_or(false);
        if !taken {
            cells.to_mut().entry(address).or_default().value = value;
        }
    }
    cells
}

// ---------------------------------------------------------------------------
// ODS conditional formatting (LibreOffice's `calcext` extension)
// ---------------------------------------------------------------------------

const CALCEXT_NS: &str = "xmlns:calcext=\"urn:org:documentfoundation:names:experimental:calc:xmlns:calcext:1.0\"";
/// The namespace behind the `of:` in `of:=SUM(...)`; LibreOffice reads a formula
/// whose prefix is not declared as `Err:510`.
const OF_NS: &str = "xmlns:of=\"urn:oasis:names:tc:opendocument:xmlns:of:1.2\"";

/// Rules of a sheet that cannot be written to ODS, as the reason each was left
/// out. Mirrors `ods_chart_problem` so the compatibility report can name them.
pub(crate) fn ods_conditional_problem(rule: &CondRule) -> Option<String> {
    let known = |threshold: &CondThreshold| crate::xlsx::CFVO_KINDS.contains(&threshold.kind.as_str());
    match rule.kind.as_str() {
        "colorScale" if !(2..=3).contains(&rule.thresholds.len()) || !rule.thresholds.iter().all(known) => {
            Some("a color scale needs two or three valid colour stops".into())
        }
        "iconSet" if rule.icon_set.as_deref().map(crate::xlsx::icon_set_count).is_some_and(|count| count.is_none()) => {
            Some("the icon set is unknown".into())
        }
        "expression" if rule.formula.as_deref().map(str::trim).unwrap_or("").is_empty() => {
            Some("the formula rule has no formula".into())
        }
        "greater" | "less" | "equal" | "between" | "text" | "textContains" | "duplicate" | "duplicates" | "top"
        | "bottom" | "expression" | "colorScale" | "dataBar" | "iconSet" => {
            rule.range.trim().is_empty().then(|| "the rule has no range".to_string())
        }
        other => Some(format!("the rule type \"{other}\" is not exportable")),
    }
}

/// `A1:B5 D2` on `sheet` as the space separated cell range list ODF wants
/// (`Sheet1.A1:Sheet1.B5 Sheet1.D2`); parts that are not ranges are dropped.
fn ods_target_ranges(sheet: &str, range: &str) -> String {
    let sheet = ods_sheet_ref(sheet);
    range
        .split(|ch: char| ch.is_whitespace() || ch == ',')
        .filter_map(|part| {
            let ((start_row, start_col), (end_row, end_col)) = crate::address::parse_range(part)?;
            let start = format!("{sheet}.{}", crate::address::format(start_row, start_col));
            Some(if (start_row, start_col) == (end_row, end_col) {
                start
            } else {
                format!("{start}:{sheet}.{}", crate::address::format(end_row, end_col))
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether `chars[index..]` starts a cell reference the way a rule formula
/// writes it (`A1`, `$B$2`, `A1:C5`, `Sheet2!A1`, `'My Sheet'!A1:B2`), and if so
/// the number of characters it spans and its ODF spelling (`[.A1]`,
/// `[.A1:.C5]`, `[$Sheet2.A1]`).
fn ods_reference_at(chars: &[char], index: usize) -> Option<(usize, String)> {
    let cell_at = |start: usize| -> Option<(usize, String)> {
        let mut cursor = start;
        let mut text = String::new();
        if chars.get(cursor) == Some(&'$') {
            text.push('$');
            cursor += 1;
        }
        let letters = chars[cursor..].iter().take_while(|ch| ch.is_ascii_alphabetic()).count();
        if !(1..=3).contains(&letters) {
            return None;
        }
        text.extend(chars[cursor..cursor + letters].iter().map(char::to_ascii_uppercase));
        cursor += letters;
        if chars.get(cursor) == Some(&'$') {
            text.push('$');
            cursor += 1;
        }
        let digits = chars[cursor..].iter().take_while(|ch| ch.is_ascii_digit()).count();
        if digits == 0 {
            return None;
        }
        text.extend(&chars[cursor..cursor + digits]);
        Some((cursor + digits - start, text))
    };
    // An optional sheet prefix: `Name!` or `'Quoted name'!`.
    let mut cursor = index;
    let mut sheet = None;
    if chars.get(cursor) == Some(&'\'') {
        let mut end = cursor + 1;
        let mut name = String::new();
        while end < chars.len() {
            match (chars[end], chars.get(end + 1)) {
                ('\'', Some('\'')) => {
                    name.push('\'');
                    end += 2;
                }
                ('\'', _) => break,
                (ch, _) => {
                    name.push(ch);
                    end += 1;
                }
            }
        }
        if chars.get(end) != Some(&'\'') || chars.get(end + 1) != Some(&'!') {
            return None;
        }
        sheet = Some(ods_sheet_ref(&name));
        cursor = end + 2;
    } else {
        let word = chars[cursor..].iter().take_while(|ch| ch.is_alphanumeric() || **ch == '_').count();
        if word > 0 && chars.get(cursor + word) == Some(&'!') {
            sheet = Some(chars[cursor..cursor + word].iter().collect());
            cursor += word + 1;
        }
    }
    let (first_len, first) = cell_at(cursor)?;
    cursor += first_len;
    let mut reference = first;
    if chars.get(cursor) == Some(&':') {
        if let Some((second_len, second)) = cell_at(cursor + 1) {
            reference = format!("{reference}:.{second}");
            cursor += 1 + second_len;
        }
    }
    // `LOG10(` and `Rate1` are not references.
    if chars.get(cursor).is_some_and(|ch| ch.is_alphanumeric() || matches!(ch, '_' | '(' | '.')) {
        return None;
    }
    let text = match sheet {
        Some(sheet) => format!("[${sheet}.{reference}]"),
        None => format!("[.{reference}]"),
    };
    Some((cursor - index, text))
}

/// A rule formula in the editor's spelling (`AND($A2>20,B2<4)`) as ODF writes
/// it in conditions: bracketed references and `;` between arguments.
fn formula_to_ods(formula: &str) -> String {
    let chars: Vec<char> = formula.chars().collect();
    let mut out = String::new();
    let (mut index, mut quoted) = (0usize, false);
    while index < chars.len() {
        let ch = chars[index];
        if ch == '"' {
            quoted = !quoted;
        }
        if quoted || ch == '"' {
            out.push(ch);
            index += 1;
            continue;
        }
        if ch == ',' {
            out.push(';');
            index += 1;
            continue;
        }
        let after_word = index > 0 && (chars[index - 1].is_alphanumeric() || matches!(chars[index - 1], '_' | '.'));
        if !after_word {
            if let Some((length, text)) = ods_reference_at(&chars, index) {
                out.push_str(&text);
                index += length;
                continue;
            }
        }
        out.push(ch);
        index += 1;
    }
    out
}

/// The inverse of [`formula_to_ods`]: `[.$D1]` -> `$D1`, `[$Data.$B1]` ->
/// `Data!$B1`, `;` -> `,`.
fn formula_from_ods(formula: &str) -> String {
    let chars: Vec<char> = formula.chars().collect();
    let mut out = String::new();
    let (mut index, mut quoted) = (0usize, false);
    while index < chars.len() {
        let ch = chars[index];
        if ch == '"' {
            quoted = !quoted;
        }
        if quoted || ch == '"' {
            out.push(ch);
            index += 1;
            continue;
        }
        match ch {
            ';' => out.push(','),
            '[' => {
                let end = chars[index..].iter().position(|ch| *ch == ']').map_or(chars.len(), |offset| index + offset);
                let inner: String = chars[index + 1..end.min(chars.len())].iter().collect();
                out.push_str(&ods_reference_to_ours(&inner));
                index = end + 1;
                continue;
            }
            _ => out.push(ch),
        }
        index += 1;
    }
    out
}

/// `.A1:.B2`, `$Data.$B1` or `$'My Sheet'.A1:.B2` to the editor's spelling.
fn ods_reference_to_ours(inner: &str) -> String {
    // Split at ':' and '.' outside quoted sheet names.
    let mut parts: Vec<(String, String)> = vec![(String::new(), String::new())];
    let (mut quoted, mut after_dot) = (false, false);
    for ch in inner.chars() {
        match ch {
            '\'' => {
                quoted = !quoted;
                let part = parts.last_mut().unwrap();
                if after_dot {
                    part.1.push(ch)
                } else {
                    part.0.push(ch)
                }
            }
            ':' if !quoted => {
                parts.push((String::new(), String::new()));
                after_dot = false;
            }
            '.' if !quoted && !after_dot => after_dot = true,
            ch => {
                let part = parts.last_mut().unwrap();
                if after_dot {
                    part.1.push(ch)
                } else {
                    part.0.push(ch)
                }
            }
        }
    }
    let sheet = parts[0].0.trim_start_matches('$').to_string();
    let cells: Vec<&str> = parts.iter().map(|(_, cell)| cell.as_str()).collect();
    let range = cells.join(":");
    if sheet.is_empty() {
        range
    } else {
        format!("{sheet}!{range}")
    }
}

/// A rule operand as a condition literal: numbers stay, text is quoted.
fn ods_condition_literal(value: &str) -> String {
    let value = value.trim();
    if value.parse::<f64>().is_ok() || value.starts_with('"') {
        value.to_string()
    } else {
        format!("\"{}\"", value.replace('"', "\"\""))
    }
}

/// The `calcext:value` of a highlight rule, in the grammar LibreOffice writes:
/// an operator and operand (`>5`), `between(1,9)`, `contains-text("x")`,
/// `top-elements(3)` or `formula-is(...)`.
fn ods_condition_value(rule: &CondRule) -> Option<String> {
    let first = rule.values.first().map(String::as_str).unwrap_or("0");
    let second = rule.values.get(1).map(String::as_str).unwrap_or("0");
    Some(match rule.kind.as_str() {
        "greater" => format!(">{}", ods_condition_literal(first)),
        "less" => format!("<{}", ods_condition_literal(first)),
        "equal" => format!("={}", ods_condition_literal(first)),
        "between" => format!("between({},{})", ods_condition_literal(first), ods_condition_literal(second)),
        "text" | "textContains" => format!("contains-text(\"{}\")", first.replace('"', "\"\"")),
        "duplicate" | "duplicates" => "duplicate".to_string(),
        "top" => format!("top-elements({})", rule.top_n.unwrap_or(10)),
        "bottom" => format!("bottom-elements({})", rule.top_n.unwrap_or(10)),
        "expression" => {
            let formula = rule.formula.as_deref()?.trim().trim_start_matches('=').trim();
            format!("formula-is({})", formula_to_ods(formula))
        }
        _ => return None,
    })
}

/// A threshold type in the words of `calcext:type`.
fn ods_threshold_type(kind: &str) -> &'static str {
    match kind {
        "min" => "minimum",
        "max" => "maximum",
        "percent" => "percent",
        "percentile" => "percentile",
        "formula" => "formula",
        _ => "number",
    }
}

/// One threshold entry. An icon set's first entry also says whether the cell
/// value stays visible, where LibreOffice reads it.
fn ods_entry(tag: &str, threshold: &CondThreshold, hide_value: bool) -> String {
    let value = if matches!(threshold.kind.as_str(), "min" | "max") { "0" } else { threshold.value.trim() };
    let color = threshold
        .color
        .as_deref()
        .and_then(ods_color)
        .map(|color| format!(" calcext:color=\"{color}\""))
        .unwrap_or_default();
    let show = if hide_value { " calcext:show-value=\"false\"" } else { "" };
    format!(
        "<calcext:{tag}{show} calcext:value=\"{}\" calcext:type=\"{}\"{color}/>",
        escape(if value.is_empty() { "0" } else { value }),
        ods_threshold_type(&threshold.kind)
    )
}

/// The `calcext:conditional-formats` block of one sheet. Highlight looks are
/// collected into `look_styles` (named table-cell styles for `styles.xml`).
fn ods_conditional_formats(sheet: &Sheet, look_styles: &mut Vec<(String, String)>) -> String {
    let mut out = String::new();
    for rule in &sheet.conditional {
        if ods_conditional_problem(rule).is_some() {
            continue;
        }
        let target = ods_target_ranges(&sheet.name, &rule.range);
        if target.is_empty() {
            continue;
        }
        let base =
            format!("{}.{}", ods_sheet_ref(&sheet.name), crate::xlsx::anchor_of(&rule.range).trim_start_matches('$'));
        let body = match rule.kind.as_str() {
            "colorScale" => {
                let entries: String =
                    rule.thresholds.iter().map(|stop| ods_entry("color-scale-entry", stop, false)).collect();
                format!("<calcext:color-scale>{entries}</calcext:color-scale>")
            }
            "dataBar" => {
                let color = rule.fill.as_deref().and_then(ods_color).unwrap_or_else(|| "#638EC6".into());
                let (low, high) = match rule.thresholds.as_slice() {
                    [low, high] => {
                        (ods_entry("formatting-entry", low, false), ods_entry("formatting-entry", high, false))
                    }
                    _ => (
                        "<calcext:formatting-entry calcext:value=\"0\" calcext:type=\"auto-minimum\"/>".to_string(),
                        "<calcext:formatting-entry calcext:value=\"0\" calcext:type=\"auto-maximum\"/>".to_string(),
                    ),
                };
                format!(
                    "<calcext:data-bar calcext:positive-color=\"{color}\" calcext:gradient=\"true\" calcext:axis-position=\"automatic\" calcext:show-value=\"{}\" calcext:axis-color=\"#000000\" calcext:negative-color=\"#ff0000\" calcext:min-length=\"0\" calcext:max-length=\"100\">{low}{high}</calcext:data-bar>",
                    !rule.hide_value
                )
            }
            "iconSet" => {
                let name = rule
                    .icon_set
                    .as_deref()
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .unwrap_or("3TrafficLights1");
                let count = crate::xlsx::icon_set_count(name).unwrap_or(3);
                let thresholds = if rule.thresholds.len() == count {
                    rule.thresholds.clone()
                } else {
                    crate::xlsx::default_icon_thresholds(count)
                };
                let entries: String = thresholds
                    .iter()
                    .enumerate()
                    .map(|(position, entry)| ods_entry("formatting-entry", entry, rule.hide_value && position == 0))
                    .collect();
                // LibreOffice has no reversed icon sets; the flag is ours, kept
                // so the editor's own files round-trip.
                let reverse = if rule.reverse_icons { " calcext:reverse=\"true\"" } else { "" };
                format!(
                    "<calcext:icon-set calcext:icon-set-type=\"{}\"{reverse}>{entries}</calcext:icon-set>",
                    escape(name)
                )
            }
            _ => {
                let Some(value) = ods_condition_value(rule) else { continue };
                // The look is a named cell style; a rule with none gets the
                // shared highlight, as in the XLSX export.
                let fill = rule.fill.as_deref().and_then(ods_color);
                let color = rule.color.as_deref().and_then(ods_color);
                let fill = if fill.is_none() && color.is_none() && !rule.bold && !rule.italic {
                    ods_color(crate::xlsx::CONDITIONAL_FILL)
                } else {
                    fill
                };
                let mut text = String::new();
                if let Some(color) = &color {
                    text.push_str(&format!(" fo:color=\"{color}\""));
                }
                if rule.bold {
                    text.push_str(" fo:font-weight=\"bold\"");
                }
                if rule.italic {
                    text.push_str(" fo:font-style=\"italic\"");
                }
                let cell = fill.map(|fill| format!(" fo:background-color=\"{fill}\"")).unwrap_or_default();
                let style_xml = format!("<style:text-properties{text}/><style:table-cell-properties{cell}/>");
                let position = match look_styles.iter().position(|(_, existing)| existing == &style_xml) {
                    Some(position) => position,
                    None => {
                        look_styles.push((format!("OmniCF{}", look_styles.len() + 1), style_xml));
                        look_styles.len() - 1
                    }
                };
                format!(
                    "<calcext:condition calcext:apply-style-name=\"{}\" calcext:value=\"{}\" calcext:base-cell-address=\"{}\"/>",
                    look_styles[position].0,
                    escape(&value),
                    escape(&base)
                )
            }
        };
        out.push_str(&format!(
            "<calcext:conditional-format calcext:target-range-address=\"{}\">{body}</calcext:conditional-format>",
            escape(&target)
        ));
    }
    if out.is_empty() {
        out
    } else {
        format!("<calcext:conditional-formats>{out}</calcext:conditional-formats>")
    }
}

/// The indexes whose stored size is 0: the rows or columns the editor hides.
fn ods_hidden(sizes: &BTreeMap<u32, f64>) -> BTreeSet<u32> {
    sizes.iter().filter(|(_, size)| **size <= 0.0).map(|(index, _)| *index).collect()
}

pub fn write_ods(workbook: &Workbook) -> OfficeResult<Vec<u8>> {
    let mut styles = AutoStyles::default();
    let mut body = String::new();
    let mut chart_objects: Vec<OdsChartObject> = Vec::new();
    // Highlight looks of conditional-formatting rules, shared by every sheet.
    let mut look_styles: Vec<(String, String)> = Vec::new();
    for sheet in &workbook.sheets {
        body.push_str(&format!("<table:table table:name=\"{}\">", escape(&sheet.name)));
        let cells = ods_cells_with_pivots(workbook, sheet);
        let frames = ods_chart_frames(sheet, &cells, &mut chart_objects);
        let mut max_col = 0u32;
        for address in cells.keys() {
            if let Some((_, column)) = crate::address::parse(address) {
                max_col = max_col.max(column);
            }
        }
        for (_, column) in frames.keys() {
            max_col = max_col.max(*column);
        }
        // The editor hides a column or a row by storing size 0; ODF says so with
        // `table:visibility="collapse"`. A hidden column past the last cell is
        // still written. (`filter` is for rows an AutoFilter hid and needs the
        // filter's database range, which this writer does not export.)
        let hidden_columns = ods_hidden(&sheet.col_widths);
        let hidden_rows = ods_hidden(&sheet.row_heights);
        let last_column = hidden_columns.last().map_or(max_col, |last| max_col.max(*last)).min(200);
        for column in 0..=last_column {
            let width = sheet.col_widths.get(&column).copied().unwrap_or(90.0);
            let collapse = if hidden_columns.contains(&column) { " table:visibility=\"collapse\"" } else { "" };
            body.push_str(&format!(
                "<table:table-column table:style-name=\"co{}\"{collapse} style:column-width=\"{}\"/>",
                column,
                cm(width * 0.75)
            ));
        }
        // LibreOffice writes the sheet's conditional formats between the
        // columns and the rows.
        body.push_str(&ods_conditional_formats(sheet, &mut look_styles));
        // Only rows holding a cell or a chart are written one by one; the gaps
        // between them become repeated empty rows, so a far-away anchor costs
        // one element instead of a row per index.
        let mut occupied: BTreeSet<u32> =
            cells.keys().filter_map(|address| crate::address::parse(address).map(|(row, _)| row)).collect();
        occupied.extend(frames.keys().map(|(row, _)| *row));
        occupied.extend(hidden_rows.iter().copied());
        occupied.insert(0);
        let mut next_row = 0u32;
        for row in occupied {
            if row > next_row {
                body.push_str(&format!(
                    "<table:table-row table:number-rows-repeated=\"{}\"><table:table-cell table:number-columns-repeated=\"{}\"/></table:table-row>",
                    row - next_row,
                    max_col + 1
                ));
            }
            next_row = row + 1;
            let mut row_output = String::new();
            let mut column = 0u32;
            let mut empty_run = 0u32;
            while column <= max_col {
                let address = crate::address::format(row, column);
                let cell = cells.get(&address).filter(|cell| !cell.is_empty());
                let frame = frames.get(&(row, column)).map(String::as_str);
                if cell.is_none() && frame.is_none() {
                    empty_run += 1;
                    column += 1;
                    continue;
                }
                if empty_run > 0 {
                    row_output
                        .push_str(&format!("<table:table-cell table:number-columns-repeated=\"{}\"/>", empty_run));
                    empty_run = 0;
                }
                // Cell-anchored charts sit inside their anchor cell, before its text.
                let frame = frame.unwrap_or("");
                let Some(cell) = cell else {
                    row_output.push_str(&format!("<table:table-cell>{frame}</table:table-cell>"));
                    column += 1;
                    continue;
                };
                let style_name = styles.cell(cell_style_xml(&cell.style));
                let mut attributes = format!(" table:style-name=\"{style_name}\"");
                if let Some(merge) = sheet.merges.iter().find(|merge| merge.start == address) {
                    if let (Some((start_row, start_col)), Some((_, end_col))) =
                        (crate::address::parse(&merge.start), crate::address::parse(&merge.end))
                    {
                        let _ = start_row;
                        attributes.push_str(&format!(" table:number-columns-spanned=\"{}\"", end_col - start_col + 1));
                    }
                }
                let label = match &cell.value {
                    CellValue::Empty => String::new(),
                    CellValue::Number(number) => format!("{number}"),
                    CellValue::Bool(value) => if *value { "TRUE" } else { "FALSE" }.to_string(),
                    CellValue::Text(text) | CellValue::Error(text) => text.clone(),
                };
                // A hyperlink is a `text:a` around the cell text. A cell with no
                // text shows the link's display text (or its target) instead, since
                // ODF has no link without a label. Targets outside the allowed
                // schemes are not written.
                let link_target = cell.link.as_deref().and_then(safe_link_target);
                let paragraph = match &link_target {
                    Some(target) => {
                        let shown = if label.is_empty() {
                            cell.link_display.clone().filter(|text| !text.is_empty()).unwrap_or_else(|| target.clone())
                        } else {
                            label
                        };
                        let tip = cell
                            .link_tooltip
                            .as_deref()
                            .filter(|text| !text.is_empty())
                            .map(|text| format!(" office:title=\"{}\"", escape(text)))
                            .unwrap_or_default();
                        format!(
                            "<text:p><text:a xlink:type=\"simple\" xlink:href=\"{}\"{tip}>{}</text:a></text:p>",
                            escape(&ods_href(target)),
                            crate::xml::escape_text(&shown)
                        )
                    }
                    None if label.is_empty() => "<text:p/>".to_string(),
                    None => format!("<text:p>{}</text:p>", crate::xml::escape_text(&label)),
                };
                let annotation = cell
                    .comment
                    .as_deref()
                    .filter(|text| !text.trim().is_empty())
                    .map(|text| ods_annotation(text, cell.comment_author.as_deref(), cell.comment_visible))
                    .unwrap_or_default();
                let value_xml = format!("{annotation}{paragraph}");
                let value_type = match &cell.value {
                    CellValue::Number(_) => "float",
                    CellValue::Bool(_) => "boolean",
                    _ => "string",
                };
                // LibreOffice marks an error cell as a string with its own flag, which is
                // what tells an error from text that merely looks like one.
                let error_flag =
                    if matches!(cell.value, CellValue::Error(_)) { " calcext:value-type=\"error\"" } else { "" };
                let formula = cell
                    .formula
                    .as_deref()
                    .map(|formula| format!(" table:formula=\"{}\"", escape(&our_formula_to_odf(formula))))
                    .unwrap_or_default();
                let value_attr = match &cell.value {
                    CellValue::Number(number) => format!(" office:value=\"{number}\""),
                    CellValue::Bool(value) => {
                        format!(" office:boolean-value=\"{}\"", if *value { "true" } else { "false" })
                    }
                    // LibreOffice takes a cell's text from `office:string-value` when
                    // it is there and then never sees the `text:a` link, so a linked
                    // cell carries its text in the paragraph only.
                    CellValue::Text(text) if link_target.is_none() => {
                        format!(" office:string-value=\"{}\"", escape(text))
                    }
                    _ => String::new(),
                };
                row_output.push_str(&format!("<table:table-cell office:value-type=\"{value_type}\"{error_flag}{value_attr}{attributes}{formula}>{frame}{value_xml}</table:table-cell>"));
                column += 1;
            }
            if empty_run > 0 {
                row_output.push_str(&format!("<table:table-cell table:number-columns-repeated=\"{}\"/>", empty_run));
            }
            let collapse = if hidden_rows.contains(&row) { " table:visibility=\"collapse\"" } else { "" };
            body.push_str(&format!("<table:table-row{collapse}>{row_output}</table:table-row>"));
        }
        body.push_str("</table:table>");
    }
    let content = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-content {NS} {CALCEXT_NS} {OF_NS} office:version=\"1.2\">{}{}<office:body><office:spreadsheet>{body}</office:spreadsheet></office:body></office:document-content>",
        styles.xml(),
        "<office:styles/>"
    );
    // The looks a conditional format applies are named cell styles.
    let look_styles_xml: String = look_styles
        .iter()
        .map(|(name, xml)| {
            format!("<style:style style:name=\"{name}\" style:family=\"table-cell\">{xml}</style:style>")
        })
        .collect();
    // Every chart object is a sub-document with its own manifest entries.
    let mut files = vec!["content.xml".to_string(), "styles.xml".to_string(), "meta.xml".to_string()];
    for object in &chart_objects {
        files.push(format!("{}/", object.name));
        files.push(format!("{}/content.xml", object.name));
        files.push(format!("{}/styles.xml", object.name));
    }
    let manifest = manifest_for("application/vnd.oasis.opendocument.spreadsheet", &files);
    let mut zip = ZipWriter::new();
    zip.add_text("mimetype", "application/vnd.oasis.opendocument.spreadsheet");
    zip.add_text("META-INF/manifest.xml", &manifest);
    zip.add_text("content.xml", &content);
    zip.add_text(
        "styles.xml",
        &if look_styles_xml.is_empty() {
            format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-styles {NS} office:version=\"1.2\"/>")
        } else {
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-styles {NS} office:version=\"1.2\"><office:styles>{look_styles_xml}</office:styles></office:document-styles>"
            )
        },
    );
    zip.add_text("meta.xml", &meta_xml(&workbook.title, "OmniOffice", None));
    for object in &chart_objects {
        zip.add_text(&format!("{}/content.xml", object.name), &object.content);
        zip.add_text(
            &format!("{}/styles.xml", object.name),
            &format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-styles {NS} {CHART_NS} office:version=\"1.2\"><office:styles/></office:document-styles>"
            ),
        );
    }
    Ok(zip.finish())
}

pub fn write_ods_file(path: &Path, workbook: &Workbook) -> OfficeResult<()> {
    crate::io::write_atomic(path, &write_ods(workbook)?)
}

/// A chart's local table: the series names from the header row and, per body
/// row, the category label followed by one value per series.
struct OdsLocalTable {
    header: Vec<String>,
    rows: Vec<Vec<(String, Option<f64>)>>,
}

// ---------------------------------------------------------------------------
// ODP
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct DeckRead {
    pub deck: Deck,
    pub warnings: Vec<String>,
}

/// Namespaces the slide timing (ODF 1.2 SMIL animations) needs on top of [`NS`].
const ANIM_NS: &str = concat!(
    "xmlns:anim=\"urn:oasis:names:tc:opendocument:xmlns:animation:1.0\" ",
    "xmlns:smil=\"urn:oasis:names:tc:opendocument:xmlns:smil-compatible:1.0\""
);

/// True when `value` is usable as an `xml:id` (an NCName).
fn is_ncname(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(first) if first.is_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_alphanumeric() || matches!(ch, '.' | '-' | '_'))
}

/// `xml:id`/`draw:id` allocation for ODP shapes, which animations target.
/// Ids must be unique in the whole content.xml: a model id that is an NCName
/// and still free is kept, so a re-import restores it; anything else (a UUID
/// starting with a digit, an id repeated on another slide) gets `idN`.
#[derive(Default)]
struct OdpShapeIds {
    used: std::collections::HashSet<String>,
    /// Model object id -> written id, for the slide being written.
    slide: HashMap<String, String>,
    /// Model ids whose planned id has already been handed out on this slide.
    written: std::collections::HashSet<String>,
    next: usize,
}

impl OdpShapeIds {
    /// A fresh id that is unique in the whole content.xml, preferring an NCName
    /// model id that is still free.
    fn unique(&mut self, object: &SlideObject) -> String {
        let id = if is_ncname(&object.id) && !self.used.contains(&object.id) {
            object.id.clone()
        } else {
            loop {
                self.next += 1;
                let candidate = format!("id{}", self.next);
                if !self.used.contains(&candidate) {
                    break candidate;
                }
            }
        };
        self.used.insert(id.clone());
        id
    }

    /// Plans the id of one shape without consuming it, so a connector written
    /// before its target already knows the target's id. The first object with a
    /// given model id owns the mapping.
    fn plan(&mut self, object: &SlideObject) -> String {
        let id = self.unique(object);
        if !object.id.is_empty() {
            self.slide.entry(object.id.clone()).or_insert_with(|| id.clone());
        }
        id
    }

    /// The id to write for `object`: its planned id when there is one, otherwise
    /// a fresh one. A repeated model id falls back to a fresh id so every shape
    /// stays addressable.
    fn assign(&mut self, object: &SlideObject) -> String {
        if !object.id.is_empty() {
            if let Some(planned) = self.slide.get(&object.id).cloned() {
                if self.written.insert(object.id.clone()) {
                    return planned;
                }
            }
        }
        let id = self.unique(object);
        if !object.id.is_empty() {
            self.slide.entry(object.id.clone()).or_insert_with(|| id.clone());
        }
        id
    }

    /// Starts a new slide; the planned and written maps are per page.
    fn reset_slide(&mut self) {
        self.slide.clear();
        self.written.clear();
    }
}

/// The editor's animation effects and the LibreOffice presets they are written
/// as (LibreOffice's `simpress/effects.xml`): kind, effect, preset id and
/// preset sub-type. Grow and shrink share a preset and differ by scale.
const ODP_PRESETS: [(&str, &str, &str, &str); 11] = [
    ("entrance", "appear", "ooo-entrance-appear", ""),
    ("entrance", "fade", "ooo-entrance-fade-in", ""),
    ("entrance", "flyIn", "ooo-entrance-fly-in", "from-bottom"),
    ("entrance", "zoom", "ooo-entrance-zoom", "in"),
    ("emphasis", "pulse", "ooo-emphasis-flash-bulb", ""),
    ("emphasis", "spin", "ooo-emphasis-spin", ""),
    ("emphasis", "grow", "ooo-emphasis-grow-and-shrink", ""),
    ("emphasis", "shrink", "ooo-emphasis-grow-and-shrink", ""),
    ("exit", "disappear", "ooo-exit-disappear", ""),
    ("exit", "fadeOut", "ooo-exit-fade-out", ""),
    ("exit", "flyOut", "ooo-exit-fly-out", "to-top"),
];

/// The editor effect closest to a foreign effect or preset name (a PPTX filter
/// such as `wipe(down)`, or a LibreOffice preset without an editor twin).
fn closest_odp_effect(kind: &str, name: &str) -> &'static str {
    let name = name.to_ascii_lowercase();
    let has = |words: &[&str]| words.iter().any(|word| name.contains(word));
    let flies = has(&["fly", "peek", "rise", "ascend", "descend", "float", "glide", "sling", "credits", "sink"]);
    match kind {
        "entrance" if flies => "flyIn",
        "entrance" if has(&["zoom", "magnify", "expand", "grow", "stretch", "compress", "unfold"]) => "zoom",
        "entrance" if name.is_empty() || has(&["appear", "flash"]) => "appear",
        "entrance" => "fade",
        "exit" if flies => "flyOut",
        "exit" if has(&["disappear", "flash"]) => "disappear",
        "exit" => "fadeOut",
        _ if has(&["shrink", "compress"]) => "shrink",
        _ if has(&["grow", "magnify", "zoom", "scale", "size", "expand"]) => "grow",
        _ if has(&["spin", "rotate", "teeter", "swivel", "wave"]) => "spin",
        _ => "pulse",
    }
}

/// The editor effect an animation is written as, and whether that is exact.
fn odp_export_effect(kind: &str, effect: &str) -> (&'static str, bool) {
    let exact = match (kind, effect) {
        ("entrance", "fadeIn") => Some("fade"),
        // PPTX imports name the exit fade `fade`.
        ("exit", "fade") => Some("fadeOut"),
        _ => ODP_PRESETS.iter().find(|preset| preset.0 == kind && preset.1 == effect).map(|preset| preset.1),
    };
    match exact {
        Some(effect) => (effect, true),
        None => (closest_odp_effect(kind, effect), false),
    }
}

/// A SMIL clock value in seconds (`0.5s`).
fn smil_time(ms: u64) -> String {
    format!("{}s", ms as f64 / 1000.0)
}

/// Seconds from a SMIL clock value (`0.5s`, `500ms`, `2`, `00:00:01.5`);
/// `None` for event values such as `next` or `indefinite`.
fn parse_smil_seconds(value: &str) -> Option<f64> {
    let value = value.trim();
    let seconds = if let Some(ms) = value.strip_suffix("ms") {
        ms.trim().parse::<f64>().ok()? / 1000.0
    } else if let Some(minutes) = value.strip_suffix("min") {
        minutes.trim().parse::<f64>().ok()? * 60.0
    } else if let Some(hours) = value.strip_suffix('h') {
        hours.trim().parse::<f64>().ok()? * 3600.0
    } else if let Some(seconds) = value.strip_suffix('s') {
        seconds.trim().parse::<f64>().ok()?
    } else if value.contains(':') {
        let mut total = 0.0;
        for part in value.split(':') {
            total = total * 60.0 + part.trim().parse::<f64>().ok()?;
        }
        total
    } else {
        value.parse::<f64>().ok()?
    };
    seconds.is_finite().then_some(seconds.max(0.0))
}

fn seconds_to_ms(seconds: f64) -> u32 {
    (seconds * 1000.0).round().clamp(0.0, u32::MAX as f64) as u32
}

/// The animation nodes of one effect, following the LibreOffice preset of the
/// same name with its durations scaled to `duration_ms`.
fn odp_effect_nodes(effect: &str, target: &str, duration_ms: u64) -> String {
    let target = escape(target);
    let dur = smil_time(duration_ms);
    let visibility = |visible: bool, begin_ms: u64, dur: &str| {
        format!(
            "<anim:set smil:begin=\"{}\" smil:dur=\"{dur}\" smil:fill=\"hold\" smil:targetElement=\"{target}\" smil:attributeName=\"visibility\" smil:to=\"{}\"/>",
            smil_time(begin_ms),
            if visible { "visible" } else { "hidden" }
        )
    };
    let show = visibility(true, 0, "0.001s");
    let hide_at_end = visibility(false, duration_ms.saturating_sub(1), "0.001s");
    let animate = |attribute: &str, values: &str| {
        format!(
            "<anim:animate smil:dur=\"{dur}\" smil:fill=\"hold\" smil:targetElement=\"{target}\" smil:attributeName=\"{attribute}\" smil:values=\"{values}\" smil:keyTimes=\"0;1\"/>"
        )
    };
    let fade = |mode: &str| {
        format!(
            "<anim:transitionFilter smil:dur=\"{dur}\" smil:targetElement=\"{target}\" smil:type=\"fade\" smil:subtype=\"crossfade\"{mode}/>"
        )
    };
    let transform = |kind: &str, by: &str| {
        format!(
            "<anim:animateTransform smil:dur=\"{dur}\" smil:fill=\"hold\" smil:targetElement=\"{target}\" smil:by=\"{by}\" svg:type=\"{kind}\"/>"
        )
    };
    match effect {
        "fade" => format!("{show}{}", fade("")),
        "flyIn" => format!("{show}{}{}", animate("x", "x;x"), animate("y", "1+height/2;y")),
        "zoom" => format!("{show}{}{}", animate("width", "0;width"), animate("height", "0;height")),
        "disappear" => visibility(false, 0, &dur),
        "fadeOut" => format!("{}{hide_at_end}", fade(" smil:mode=\"out\"")),
        "flyOut" => format!("{}{}{hide_at_end}", animate("x", "x;x"), animate("y", "y;0-height/2")),
        "spin" => transform("rotate", "360"),
        "grow" => transform("scale", "1.5,1.5"),
        "shrink" => transform("scale", "0.5,0.5"),
        "pulse" => format!(
            "<anim:transitionFilter smil:dur=\"{dur}\" smil:targetElement=\"{target}\" smil:keySplines=\"0,0;0.2,0.5;0.8,0.5;1,0\" smil:type=\"fade\" smil:subtype=\"crossfade\" smil:mode=\"out\"/><anim:animateTransform smil:dur=\"{}\" smil:fill=\"hold\" smil:autoReverse=\"true\" smil:targetElement=\"{target}\" smil:by=\"1.05,1.05\" svg:type=\"scale\"/>",
            smil_time(duration_ms / 2)
        ),
        // "appear": visible for the whole effect so its duration survives.
        _ => visibility(true, 0, &dur),
    }
}

/// One "after previous" step of a click group in the main sequence.
struct OdpTimingStep {
    begin_ms: u64,
    end_ms: u64,
    effects: String,
}

/// The slide's main sequence as ODF SMIL timing, the structure LibreOffice
/// writes: timing root -> main sequence -> one `anim:par` per click -> one per
/// "after previous" step -> one per effect. The effect `anim:par` carries the
/// trigger (`presentation:node-type`), class, preset and delay; its children
/// carry the duration and the target shape.
fn odp_timing_xml(slide: &Slide, ids: &HashMap<String, String>, warnings: &mut Vec<String>) -> String {
    let mut animations: Vec<&Animation> = slide.animations.iter().collect();
    animations.sort_by_key(|animation| animation.order);
    let mut clicks: Vec<(&str, Vec<OdpTimingStep>)> = Vec::new();
    for animation in animations {
        let Some(target) = ids.get(&animation.object_id) else {
            warnings.push(format!(
                "An animation targeting \"{}\" was not written because the object is not on the exported slide.",
                animation.object_id
            ));
            continue;
        };
        if !matches!(animation.kind.as_str(), "entrance" | "exit" | "emphasis") {
            warnings.push(format!(
                "An animation of kind \"{}\" was kept in the native .oswk file and not written to the ODP timing.",
                animation.kind
            ));
            continue;
        }
        let (effect, exact) = odp_export_effect(&animation.kind, &animation.effect);
        let Some(&(_, _, preset_id, sub_type)) =
            ODP_PRESETS.iter().find(|preset| preset.0 == animation.kind && preset.1 == effect)
        else {
            continue;
        };
        if !exact {
            warnings.push(format!(
                "The {} effect \"{}\" has no LibreOffice preset and was written as \"{preset_id}\".",
                animation.kind, animation.effect
            ));
        }
        let node_type = match animation.trigger.as_str() {
            "withPrevious" => "with-previous",
            "afterPrevious" => "after-previous",
            _ => "on-click",
        };
        let first_step = || vec![OdpTimingStep { begin_ms: 0, end_ms: 0, effects: String::new() }];
        match (node_type, clicks.last_mut()) {
            ("on-click", _) => clicks.push(("indefinite", first_step())),
            // Effects before the first click start with the slide.
            (_, None) => clicks.push(("0s", first_step())),
            ("after-previous", Some((_, steps))) => {
                let begin_ms = steps.last().map(|step| step.end_ms).unwrap_or(0);
                steps.push(OdpTimingStep { begin_ms, end_ms: begin_ms, effects: String::new() });
            }
            _ => {}
        }
        let Some(step) = clicks.last_mut().and_then(|(_, steps)| steps.last_mut()) else { continue };
        let duration = u64::from(animation.duration_ms);
        let delay = u64::from(animation.delay_ms);
        let sub_type =
            if sub_type.is_empty() { String::new() } else { format!(" presentation:preset-sub-type=\"{sub_type}\"") };
        step.effects.push_str(&format!(
            "<anim:par smil:begin=\"{}\" smil:fill=\"hold\" presentation:node-type=\"{node_type}\" presentation:preset-class=\"{}\" presentation:preset-id=\"{preset_id}\"{sub_type}>{}</anim:par>",
            smil_time(delay),
            animation.kind,
            odp_effect_nodes(effect, target, duration)
        ));
        step.end_ms = step.end_ms.max(step.begin_ms + delay + duration);
    }
    if clicks.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "<anim:par presentation:node-type=\"timing-root\"><anim:seq presentation:node-type=\"main-sequence\">",
    );
    for (begin, steps) in clicks {
        out.push_str(&format!("<anim:par smil:begin=\"{begin}\">"));
        for step in steps {
            out.push_str(&format!("<anim:par smil:begin=\"{}\">{}</anim:par>", smil_time(step.begin_ms), step.effects));
        }
        out.push_str("</anim:par>");
    }
    out.push_str("</anim:seq></anim:par>");
    out
}

/// Writes a `draw:text-box`'s paragraphs the way the ODT writer writes body
/// text: automatic paragraph and text styles shared through `AutoStyles`,
/// bullets collected into `text:list` elements, and `Standard` for a plain
/// paragraph. Runs that no longer add up to `paragraph.text` fall back to the
/// paragraph text itself, so an edit can never lose content.
fn write_odp_text_frame(writer: &mut XmlWriter, text: &TextFrame, styles: &mut AutoStyles) {
    let mut index = 0usize;
    while index < text.paragraphs.len() {
        let paragraph = &text.paragraphs[index];
        if !paragraph.bullet {
            write_odp_paragraph(writer, paragraph, styles);
            index += 1;
            continue;
        }
        // A run of consecutive bullets becomes one list, nested by level.
        let start = index;
        while index < text.paragraphs.len() && text.paragraphs[index].bullet {
            index += 1;
        }
        write_odp_list(writer, &text.paragraphs[start..index], styles, 0);
    }
    if text.paragraphs.is_empty() {
        writer.raw("<text:p/>");
    }
}

/// One `text:list` of a bullet run at `level`; items deeper than `level` become
/// nested lists inside their item, so the reader can recover the level.
fn write_odp_list(writer: &mut XmlWriter, items: &[TextParagraph], styles: &mut AutoStyles, level: u32) {
    writer.raw("<text:list text:style-name=\"LB\">");
    let mut index = 0usize;
    while index < items.len() {
        writer.raw("<text:list-item>");
        write_odp_paragraph(writer, &items[index], styles);
        let mut end = index + 1;
        while end < items.len() && items[end].level > level {
            end += 1;
        }
        if end > index + 1 {
            write_odp_list(writer, &items[index + 1..end], styles, level + 1);
        }
        writer.raw("</text:list-item>");
        index = end;
    }
    writer.raw("</text:list>");
}

fn write_odp_paragraph(writer: &mut XmlWriter, paragraph: &TextParagraph, styles: &mut AutoStyles) {
    let props = ParaProps {
        align: paragraph.align.clone(),
        list: paragraph.bullet.then(|| ListInfo {
            kind: "bullet".into(),
            level: paragraph.level,
            start: 1,
            marker: "•".into(),
        }),
        ..Default::default()
    };
    let properties = paragraph_style_xml(&props, false);
    let name = if properties == "<style:paragraph-properties/>" {
        "Standard".to_string()
    } else {
        styles.paragraph(properties)
    };
    writer.raw(&format!("<text:p text:style-name=\"{name}\">"));
    let runs_match = paragraph.runs.iter().map(|run| run.text.as_str()).collect::<String>() == paragraph.text;
    if paragraph.runs.is_empty() || !runs_match {
        let fallback = Run {
            text: paragraph.text.clone(),
            bold: paragraph.bold,
            italic: paragraph.italic,
            underline: paragraph.underline,
            size_pt: paragraph.size_pt,
            color: paragraph.color.clone(),
            lang: paragraph.lang.clone(),
            ..Default::default()
        };
        write_runs(writer, &[fallback], styles, &NoteContext::default());
    } else {
        write_runs(writer, &paragraph.runs, styles, &NoteContext::default());
    }
    writer.raw("</text:p>");
}

/// One shape of a slide as `draw:*` XML. `charts` maps a chart object's model
/// id to the `Object N` sub-document the writer stored for it.
fn slide_object_xml(
    object: &SlideObject,
    ids: &mut OdpShapeIds,
    styles: &mut AutoStyles,
    charts: &HashMap<String, String>,
) -> String {
    let id = ids.assign(object);
    let id_attrs = format!(" draw:id=\"{0}\" xml:id=\"{0}\"", escape(&id));
    if object.kind == "group" {
        // Children keep their absolute page coordinates; draw:g has no box of
        // its own, which matches the editor's group = bounding box of children.
        let mut children: Vec<&SlideObject> = object.children.iter().collect();
        children.sort_by_key(|child| child.z);
        let inner: String = children.into_iter().map(|child| slide_object_xml(child, ids, styles, charts)).collect();
        return format!(
            "<draw:g draw:name=\"{}\"{id_attrs} draw:z-index=\"{}\">{inner}</draw:g>",
            escape(&object.name),
            object.z
        );
    }
    let frame_start = format!("<draw:frame draw:name=\"{}\"{id_attrs} text:anchor-type=\"page\"", escape(&object.name));
    let style = object.style.clone().unwrap_or_default();
    let mut inner = String::new();
    match object.kind.as_str() {
        "image" => {
            if let Some(image) = &object.image {
                // `fo:clip` trims the rendered frame; the fractions are of the
                // rendered size, spelled in cm with the same precision as every
                // other length in this writer.
                let clip = image
                    .crop
                    .as_ref()
                    .filter(|crop| crop.left > 0.0 || crop.top > 0.0 || crop.right > 0.0 || crop.bottom > 0.0)
                    .map(|crop| {
                        format!(
                            " fo:clip=\"rect({} {} {} {})\"",
                            cm(crop.top * object.h.max(1.0)),
                            cm(crop.right * object.w.max(1.0)),
                            cm(crop.bottom * object.h.max(1.0)),
                            cm(crop.left * object.w.max(1.0))
                        )
                    })
                    .unwrap_or_default();
                inner.push_str(&format!("<draw:image xlink:href=\"Pictures/{}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"{clip}/>", escape(&image.name)));
            }
        }
        "line" | "arrow" => {
            let line = object.line.clone().unwrap_or_default();
            let (x1, y1, x2, y2) = (object.x, object.y, object.x + line.x2, object.y + line.y2);
            if line.begin_object.is_some() || line.end_object.is_some() {
                // A glued line is a real connector: LibreOffice needs the
                // written shape ids (planned before writing) and glue points.
                let mut glue = String::new();
                if let Some(target) = line.begin_object.as_deref().and_then(|target| ids.slide.get(target)) {
                    glue.push_str(&format!(
                        " draw:start-shape=\"{}\" draw:start-glue-point=\"{}\"",
                        escape(target),
                        line.begin_site
                    ));
                }
                if let Some(target) = line.end_object.as_deref().and_then(|target| ids.slide.get(target)) {
                    glue.push_str(&format!(
                        " draw:end-shape=\"{}\" draw:end-glue-point=\"{}\"",
                        escape(target),
                        line.end_site
                    ));
                }
                let stroke = object.style.as_ref().and_then(|style| style.stroke.clone());
                let width = object.style.as_ref().map(|style| style.stroke_width_pt).unwrap_or(2.0).max(0.5);
                let properties = format!(
                    "<style:graphic-properties draw:stroke=\"solid\" svg:stroke-width=\"{}\"{} draw:fill=\"none\"/>",
                    cm(width),
                    stroke.map(|color| format!(" svg:stroke-color=\"{}\"", escape(&color))).unwrap_or_default()
                );
                let style_name = styles.graphic(properties);
                inner.push_str(&format!(
                    "<draw:connector svg:x1=\"{}\" svg:y1=\"{}\" svg:x2=\"{}\" svg:y2=\"{}\"{glue} draw:style-name=\"{style_name}\"/>",
                    cm(x1),
                    cm(y1),
                    cm(x2),
                    cm(y2)
                ));
            } else {
                inner.push_str(&format!(
                    "<draw:line svg:x1=\"{}\" svg:y1=\"{}\" svg:x2=\"{}\" svg:y2=\"{}\" draw:style-name=\"gr1\"/>",
                    cm(x1),
                    cm(y1),
                    cm(x2),
                    cm(y2)
                ));
            }
        }
        "table" => {
            if let Some(table) = &object.table {
                let mut writer = XmlWriter::new();
                let mut styles = AutoStyles::default();
                let mut media = Media::default();
                write_table(&mut writer, table, &mut styles, &mut media, &NoteContext::default());
                inner.push_str(&writer.finish());
            }
        }
        "chart" => match charts.get(&object.id) {
            Some(name) => inner.push_str(&format!(
                "<draw:object xlink:href=\"./{}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"/>",
                escape(name)
            )),
            None => {
                // A chart this writer cannot export keeps the old title
                // placeholder so nothing is dropped silently.
                if let Some(chart) = &object.chart {
                    let title =
                        if chart.title.trim().is_empty() { format!("{} chart", chart.kind) } else { chart.title.clone() };
                    inner.push_str(&format!(
                        "<draw:text-box><text:p text:style-name=\"Standard\">{}</text:p></draw:text-box>",
                        crate::xml::escape_text(&title)
                    ));
                }
            }
        },
        _ => {
            if let Some(text) = &object.text {
                let mut writer = XmlWriter::new();
                writer.raw("<draw:text-box>");
                write_odp_text_frame(&mut writer, text, styles);
                writer.raw("</draw:text-box>");
                inner.push_str(&writer.finish());
            }
        }
    }
    let shape = match object.kind.as_str() {
        "ellipse" => "draw:ellipse",
        "line" | "arrow" => return format!("{frame_start} draw:z-index=\"{}\">{inner}</draw:frame>", object.z),
        _ => "draw:frame",
    };
    if shape == "draw:frame" {
        let fill = style
            .fill
            .as_deref()
            .map(|fill| format!(" draw:fill=\"solid\" draw:fill-color=\"{}\"", escape(fill)))
            .unwrap_or_else(|| " draw:fill=\"none\"".into());
        format!(
            "{frame_start} svg:x=\"{}\" svg:y=\"{}\" svg:width=\"{}\" svg:height=\"{}\" draw:z-index=\"{}\"{}>{}<draw:glue-points/><draw:enhanced-geometry/></draw:frame>",
            cm(object.x),
            cm(object.y),
            cm(object.w.max(4.0)),
            cm(object.h.max(4.0)),
            object.z,
            fill,
            inner
        )
        .replace("<draw:glue-points/><draw:enhanced-geometry/>", "")
    } else {
        format!(
            "{frame_start} svg:x=\"{}\" svg:y=\"{}\" svg:width=\"{}\" svg:height=\"{}\" draw:z-index=\"{}\">{}</draw:frame>",
            cm(object.x),
            cm(object.y),
            cm(object.w.max(4.0)),
            cm(object.h.max(4.0)),
            object.z,
            inner
        )
    }
}

#[derive(Debug, Clone)]
pub struct DeckWrite {
    pub bytes: Vec<u8>,
    pub warnings: Vec<String>,
}

/// Assigns ids to the shapes of one slide in the order `slide_object_xml`
/// visits them, so a connector can name a shape written later in the z order.
fn plan_odp_ids(objects: &[SlideObject], ids: &mut OdpShapeIds) {
    let mut ordered: Vec<&SlideObject> = objects.iter().collect();
    ordered.sort_by_key(|object| object.z);
    for object in ordered {
        if !object.id.is_empty() {
            ids.plan(object);
        }
        plan_odp_ids(&object.children, ids);
    }
}

/// The chart objects anywhere on a slide, in the order the writer visits them.
fn collect_odp_charts<'a>(objects: &'a [SlideObject], charts: &mut Vec<&'a SlideObject>) {
    let mut ordered: Vec<&SlideObject> = objects.iter().collect();
    ordered.sort_by_key(|object| object.z);
    for object in ordered {
        if object.chart.is_some() {
            charts.push(object);
        }
        collect_odp_charts(&object.children, charts);
    }
}

fn collect_odp_pictures(objects: &[SlideObject], pictures: &mut Vec<(String, Vec<u8>)>) {
    for object in objects {
        if let Some(image) = &object.image {
            if !image.is_empty() && !pictures.iter().any(|(name, _)| name == &image.name) {
                pictures.push((image.name.clone(), image.bytes()));
            }
        }
        collect_odp_pictures(&object.children, pictures);
    }
}

pub fn write_odp_package(deck: &Deck) -> OfficeResult<DeckWrite> {
    let mut warnings = Vec::new();
    let bytes = write_odp_bytes(deck, &mut warnings)?;
    let mut seen = std::collections::HashSet::new();
    warnings.retain(|warning| seen.insert(warning.clone()));
    Ok(DeckWrite { bytes, warnings })
}

pub fn write_odp(deck: &Deck) -> OfficeResult<Vec<u8>> {
    Ok(write_odp_package(deck)?.bytes)
}

/// The `text:list` style the ODP content declares for the bullet lists the
/// writer emits (`LB`, the same name the ODT writer uses).
fn odp_list_style_xml() -> String {
    let mut out = String::from("<text:list-style style:name=\"LB\">");
    for level in 1..=9 {
        out.push_str(&format!("<text:list-level-style-bullet text:level=\"{level}\" text:bullet-char=\"•\"><style:list-level-properties text:space-before=\"{}cm\" text:min-label-width=\"0.6cm\"/></text:list-level-style-bullet>", (level as f64 - 1.0) * 0.6));
    }
    out.push_str("</text:list-style>");
    out
}

/// One placeholder frame of the ODF master page for the footer, date and slide
/// number; `body` is already escaped (or a field element).
fn odp_master_frame(class: &str, x: f64, y: f64, w: f64, h: f64, body: &str) -> String {
    format!(
        "<draw:frame presentation:class=\"{class}\" svg:x=\"{}\" svg:y=\"{}\" svg:width=\"{}\" svg:height=\"{}\"><draw:text-box><text:p>{body}</text:p></draw:text-box></draw:frame>",
        cm(x),
        cm(y),
        cm(w),
        cm(h)
    )
}

/// The placeholder frames of the default master page for an enabled footer,
/// laid out along the bottom of the slide like the PPTX writer lays them out.
fn odp_footer_shapes(deck: &Deck, footer: &SlideFooter) -> String {
    let width = deck.size.width_pt;
    let bottom = (deck.size.height_pt - 34.0).max(0.0);
    let mut out = String::new();
    if footer.show_date {
        out.push_str(&odp_master_frame(
            "date-time",
            width * 0.06,
            bottom,
            width * 0.25,
            24.0,
            &crate::xml::escape_text(&footer.date_text),
        ));
    }
    if footer.show_text {
        out.push_str(&odp_master_frame(
            "footer",
            width * 0.35,
            bottom,
            width * 0.30,
            24.0,
            &crate::xml::escape_text(&footer.text),
        ));
    }
    if footer.show_slide_number {
        out.push_str(&odp_master_frame(
            "page-number",
            width * 0.88,
            bottom,
            width * 0.06,
            24.0,
            "<text:page-number>1</text:page-number>",
        ));
    }
    out
}

fn write_odp_bytes(deck: &Deck, warnings: &mut Vec<String>) -> OfficeResult<Vec<u8>> {
    let mut body = String::new();
    let mut pictures: Vec<(String, Vec<u8>)> = Vec::new();
    let mut ids = OdpShapeIds::default();
    // Charts become `Object N` sub-documents numbered across the deck.
    let mut chart_objects: Vec<OdsChartObject> = Vec::new();
    let mut auto = AutoStyles::default();
    for (index, slide) in deck.slides.iter().enumerate() {
        let style_attr = if slide.hidden { " draw:style-name=\"dp-hidden\"" } else { "" };
        body.push_str(&format!(
            "<draw:page draw:name=\"Slide{}\" draw:master-page-name=\"Default\"{style_attr}>",
            index + 1
        ));
        ids.reset_slide();
        plan_odp_ids(&slide.objects, &mut ids);
        collect_odp_pictures(&slide.objects, &mut pictures);
        let mut chart_map: HashMap<String, String> = HashMap::new();
        let mut charts = Vec::new();
        collect_odp_charts(&slide.objects, &mut charts);
        for chart_object in charts {
            let Some(chart) = &chart_object.chart else { continue };
            match ods_chart_problem(chart) {
                Some(reason) => warnings.push(format!("Chart data is kept in the native .oswk file: {reason}.")),
                None => {
                    let name = format!("Object {}", chart_objects.len() + 1);
                    let placement = ChartPlacement {
                        id: chart_object.id.clone(),
                        chart: chart.clone(),
                        anchor: String::new(),
                        width_px: chart_object.w.max(48.0) / 0.75,
                        height_px: chart_object.h.max(48.0) / 0.75,
                    };
                    chart_objects.push(OdsChartObject {
                        content: ods_chart_content(&placement, "Sheet1", &BTreeMap::new()),
                        name: name.clone(),
                    });
                    if !chart_object.id.is_empty() {
                        chart_map.insert(chart_object.id.clone(), name);
                    }
                }
            }
        }
        let mut objects: Vec<&SlideObject> = slide.objects.iter().collect();
        objects.sort_by_key(|object| object.z);
        for object in objects {
            body.push_str(&slide_object_xml(object, &mut ids, &mut auto, &chart_map));
        }
        // draw:page content order: shapes, then the timing root, then notes.
        body.push_str(&odp_timing_xml(slide, &ids.slide, warnings));
        if !slide.notes.is_empty() {
            body.push_str(&format!("<presentation:notes><draw:frame presentation:class=\"notes\"><draw:text-box><text:p>{}</text:p></draw:text-box></draw:frame></presentation:notes>", crate::xml::escape_text(&slide.notes)));
        }
        body.push_str("</draw:page>");
    }
    let page_layout = format!(
        "<style:page-layout style:name=\"pl1\"><style:page-layout-properties fo:page-width=\"{}\" fo:page-height=\"{}\"/></style:page-layout>",
        cm(deck.size.width_pt),
        cm(deck.size.height_pt)
    );
    // A hidden slide is a drawing-page style with `visibility="hidden"`
    // (LibreOffice's convention); visible-only output stays exactly as before.
    let hidden_style = if deck.slides.iter().any(|slide| slide.hidden) {
        "<style:style style:name=\"dp-hidden\" style:family=\"drawing-page\"><style:drawing-page-properties presentation:visibility=\"hidden\"/></style:style>"
    } else {
        ""
    };
    let automatic = auto.xml();
    let automatic =
        automatic.trim_start_matches("<office:automatic-styles>").trim_end_matches("</office:automatic-styles>");
    let content = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-content {NS} {ANIM_NS} office:version=\"1.2\"><office:styles>{}</office:styles><office:automatic-styles>{page_layout}{hidden_style}{automatic}</office:automatic-styles><office:body><office:presentation>{body}</office:presentation></office:body></office:document-content>",
        odp_list_style_xml()
    );
    let background = match deck.theme.as_str() {
        "dark" => "#0F172A",
        "business" => "#F8FAFC",
        "education" => "#FEFCE8",
        "modern" => "#FFFFFF",
        _ => "#FFFFFF",
    };
    let mut drawing_props = format!("draw:fill=\"solid\" draw:fill-color=\"{background}\"");
    let mut master_shapes = String::new();
    if let Some(footer) = deck.footer.as_ref().filter(|footer| footer.enabled) {
        if footer.show_text {
            drawing_props.push_str(" presentation:display-footer=\"true\"");
        }
        if footer.show_slide_number {
            drawing_props.push_str(" presentation:display-page-number=\"true\"");
        }
        if footer.show_date {
            drawing_props.push_str(" presentation:display-date-time=\"true\"");
        }
        master_shapes = odp_footer_shapes(deck, footer);
    }
    let styles = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-styles {NS} office:version=\"1.2\"><office:styles/><office:automatic-styles>{page_layout}</office:automatic-styles><office:master-styles><style:master-page style:name=\"Default\" style:page-layout-name=\"pl1\"><style:drawing-page-properties {drawing_props}/>{master_shapes}</style:master-page></office:master-styles></office:document-styles>"
    );
    // Every chart object is a sub-document with its own manifest entries.
    let mut files = vec!["content.xml".to_string(), "styles.xml".to_string(), "meta.xml".to_string()];
    for object in &chart_objects {
        files.push(format!("{}/", object.name));
        files.push(format!("{}/content.xml", object.name));
        files.push(format!("{}/styles.xml", object.name));
    }
    for (name, _) in &pictures {
        files.push(format!("Pictures/{name}"));
    }
    let manifest = manifest_for("application/vnd.oasis.opendocument.presentation", &files);
    let mut zip = ZipWriter::new();
    zip.add_text("mimetype", "application/vnd.oasis.opendocument.presentation");
    zip.add_text("META-INF/manifest.xml", &manifest);
    zip.add_text("content.xml", &content);
    zip.add_text("styles.xml", &styles);
    zip.add_text("meta.xml", &meta_xml(&deck.title, "OmniOffice", None));
    for object in &chart_objects {
        zip.add_text(&format!("{}/content.xml", object.name), &object.content);
        zip.add_text(
            &format!("{}/styles.xml", object.name),
            &format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-styles {NS} {CHART_NS} office:version=\"1.2\"><office:styles/></office:document-styles>"
            ),
        );
    }
    for (name, data) in &pictures {
        zip.add(&format!("Pictures/{name}"), data);
    }
    Ok(zip.finish())
}

pub fn write_odp_file(path: &Path, deck: &Deck) -> OfficeResult<()> {
    crate::io::write_atomic(path, &write_odp(deck)?)
}

/// Read-side state of one ODP package: named styles, crops declared by graphic
/// styles and the embedded objects already read.
#[derive(Default)]
struct OdpImport {
    styles: ReadStyles,
    clips: HashMap<String, String>,
    seen_objects: HashSet<String>,
}

/// `style:graphic-properties/fo:clip` by style name, for images that carry
/// their crop on a graphic style instead of the `draw:image` element.
fn odp_graphic_clips(roots: &[&XmlNode]) -> HashMap<String, String> {
    let mut clips = HashMap::new();
    for root in roots {
        let mut styles = Vec::new();
        root.find_all("style", &mut styles);
        for style in styles {
            let Some(name) = style.attr_any_ns("name") else { continue };
            let mut properties = Vec::new();
            style.find_all("graphic-properties", &mut properties);
            if let Some(clip) = properties.iter().find_map(|node| node.attr_any_ns("clip")) {
                clips.insert(name.to_string(), clip.to_string());
            }
        }
    }
    clips
}

/// The crop of an ODP image, read from `fo:clip="rect(top right bottom left)"`
/// on the `draw:image`, the frame or its graphic style. The lengths are the
/// trims of the rendered frame and become fractions of the frame size. Clip
/// lengths round-trip through the writer's three-decimal cm spelling, so the
/// fractions are accurate to roughly a thousandth of the frame size; the tests
/// allow 0.01.
fn read_odp_crop(image: &XmlNode, frame: &XmlNode, import: &OdpImport, w: f64, h: f64) -> Option<ImageCrop> {
    let clip = image
        .attr_any_ns("clip")
        .or_else(|| frame.attr_any_ns("clip"))
        .map(str::to_string)
        .or_else(|| frame.attr_any_ns("style-name").and_then(|name| import.clips.get(name)).cloned())?;
    let lengths = clip.trim().strip_prefix("rect(")?.strip_suffix(')')?;
    let lengths: Option<Vec<f64>> = lengths
        .split(|ch: char| ch.is_whitespace() || ch == ',')
        .filter(|part| !part.is_empty())
        .map(parse_cm)
        .collect();
    let lengths = lengths?;
    let [top, right, bottom, left] = lengths.as_slice() else { return None };
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let crop = ImageCrop {
        left: (left / w).abs().clamp(0.0, 0.95),
        top: (top / h).abs().clamp(0.0, 0.95),
        right: (right / w).abs().clamp(0.0, 0.95),
        bottom: (bottom / h).abs().clamp(0.0, 0.95),
    };
    (crop.left > 0.0 || crop.top > 0.0 || crop.right > 0.0 || crop.bottom > 0.0).then_some(crop)
}

/// A `draw:line` or `draw:connector` shape: geometry from the svg endpoints and
/// the glue references (`draw:start-shape` and friends) of a connector.
fn read_odp_line(node: &XmlNode) -> Option<SlideObject> {
    let x1 = node.attr_any_ns("x1").and_then(parse_cm)?;
    let y1 = node.attr_any_ns("y1").and_then(parse_cm)?;
    let x2 = node.attr_any_ns("x2").and_then(parse_cm).unwrap_or(x1);
    let y2 = node.attr_any_ns("y2").and_then(parse_cm).unwrap_or(y1);
    let mut object = SlideObject::new("line", x1, y1, (x2 - x1).abs(), (y2 - y1).abs());
    object.line = Some(LineSpec {
        x2: x2 - x1,
        y2: y2 - y1,
        begin_arrow: node.attr_any_ns("marker-start").is_some(),
        end_arrow: node.attr_any_ns("marker-end").is_some(),
        dash: String::new(),
        begin_object: node.attr_any_ns("start-shape").map(str::to_string),
        end_object: node.attr_any_ns("end-shape").map(str::to_string),
        begin_site: node.attr_any_ns("start-glue-point").and_then(|value| value.parse::<u32>().ok()).unwrap_or(0),
        end_site: node.attr_any_ns("end-glue-point").and_then(|value| value.parse::<u32>().ok()).unwrap_or(0),
    });
    Some(object)
}

/// One `text:p` of a shape: runs with named styles resolved, alignment from
/// the paragraph style, and paragraph-level formatting aggregated from the
/// runs like the PPTX import does.
fn read_odp_paragraph(paragraph: &XmlNode, level: u32, bullet: bool, styles: &ReadStyles) -> TextParagraph {
    let mut runs = Vec::new();
    let mut notes = NoteReadState::default();
    if !paragraph.text.is_empty() {
        runs.push(Run { text: paragraph.text.clone(), ..Default::default() });
    }
    for child in &paragraph.children {
        node_text_runs(child, &mut runs, &mut notes, styles);
    }
    let props = paragraph_props(paragraph, styles);
    let mut model = TextParagraph { level, bullet, align: props.align, ..Default::default() };
    for run in &runs {
        model.text.push_str(&run.text);
        model.bold |= run.bold;
        model.italic |= run.italic;
        model.underline |= run.underline;
        if model.size_pt.is_none() {
            model.size_pt = run.size_pt;
        }
        if model.color.is_none() {
            model.color = run.color.clone();
        }
    }
    model.lang = runs.iter().find_map(|run| run.lang.clone());
    model.runs = runs;
    model
}

/// The paragraphs of a `draw:text-box`: plain paragraphs and lists, the latter
/// nested so the nesting depth is the bullet level.
fn read_odp_paragraphs(text_box: &XmlNode, styles: &ReadStyles) -> Vec<TextParagraph> {
    fn walk(node: &XmlNode, depth: u32, styles: &ReadStyles, out: &mut Vec<TextParagraph>) {
        for child in &node.children {
            match child.local_name() {
                "p" | "h" => out.push(read_odp_paragraph(child, depth.saturating_sub(1), depth > 0, styles)),
                "list" => {
                    for item in child.children_named("list-item") {
                        walk(item, depth + 1, styles, out);
                    }
                }
                _ => {}
            }
        }
    }
    let mut paragraphs = Vec::new();
    walk(text_box, 0, styles, &mut paragraphs);
    if paragraphs.is_empty() {
        paragraphs.push(TextParagraph::default());
    }
    paragraphs
}

/// One `draw:frame` of a slide or group: picture, embedded chart, text box,
/// table, connector, or a plain shape kept as a rectangle. Frames without a
/// size (lines) are skipped.
fn read_odp_frame(
    frame: &XmlNode,
    zip: &ZipReader,
    import: &mut OdpImport,
    warnings: &mut Vec<String>,
) -> Option<SlideObject> {
    let x = frame.attr_any_ns("x").and_then(parse_cm).unwrap_or(40.0);
    let y = frame.attr_any_ns("y").and_then(parse_cm).unwrap_or(40.0);
    let w = frame.attr_any_ns("width").and_then(parse_cm).unwrap_or(320.0);
    let h = frame.attr_any_ns("height").and_then(parse_cm).unwrap_or(180.0);
    let mut images = Vec::new();
    frame.find_all("image", &mut images);
    if let Some(image_node) = images.first() {
        if let Some(href) = image_node.attr_any_ns("href") {
            let path = href.trim_start_matches("./");
            if let Ok(data) = zip.read(path) {
                let name = path.rsplit('/').next().unwrap_or("image.png").to_string();
                let mut object = SlideObject::new("image", x, y, w, h);
                let mut image = ImageData::from_bytes(&name, &data);
                image.crop = read_odp_crop(image_node, frame, import, w, h);
                object.image = Some(image);
                return Some(object);
            }
        }
    }
    // An embedded object is a chart when its sub-document parses as one; an
    // unrecognized one falls through to the rectangle fallback below.
    if frame.child("object").is_some() {
        let sheet = Sheet::new("Sheet1");
        if let Some(placement) = read_ods_chart(zip, frame, String::new(), &sheet, &mut import.seen_objects, warnings) {
            let mut object = SlideObject::new("chart", x, y, w, h);
            object.chart = Some(placement.chart);
            return Some(object);
        }
    }
    if let Some(line) = frame
        .children_of("connector")
        .into_iter()
        .next()
        .or_else(|| frame.children_of("line").into_iter().next())
        .and_then(read_odp_line)
    {
        return Some(line);
    }
    let mut text_boxes = Vec::new();
    frame.find_all("text-box", &mut text_boxes);
    if let Some(text_box) = text_boxes.first() {
        let paragraphs = read_odp_paragraphs(text_box, &import.styles);
        let mut object = SlideObject::new("text", x, y, w, h);
        if paragraphs.iter().any(|paragraph| !paragraph.text.trim().is_empty()) {
            object.text = Some(TextFrame { paragraphs, ..Default::default() });
        }
        return Some(object);
    }
    let mut tables = Vec::new();
    frame.find_all("table", &mut tables);
    if let Some(table) = tables.first() {
        let mut rows = Vec::new();
        for row_node in table.children_named("table-row") {
            let mut cells = Vec::new();
            for cell in row_node.children_named("table-cell") {
                let mut inner = Vec::new();
                cell.find_all("p", &mut inner);
                let cell_text = inner.iter().map(|node| node.deep_text()).collect::<Vec<_>>().join("\n");
                cells.push(TableCell { blocks: vec![Block::paragraph(&cell_text)], ..Default::default() });
            }
            rows.push(TableRow { cells, ..Default::default() });
        }
        let mut object = SlideObject::new("table", x, y, w, h);
        object.table = Some(TableData { rows, ..Default::default() });
        return Some(object);
    }
    // Shapes we cannot map precisely are preserved as rectangles.
    if frame.attr_any_ns("width").is_some() {
        let fill = match frame.attr_any_ns("fill") {
            Some("none") => None,
            _ => Some(frame.attr_any_ns("fill-color").unwrap_or("#E2E8F0").to_string()),
        };
        let mut object = SlideObject::new("rect", x, y, w, h);
        object.style = Some(ShapeStyle { fill, ..Default::default() });
        warnings.push("Some shapes were imported as simple rectangles.".into());
        return Some(object);
    }
    None
}

/// The shapes directly below `parent` (a `draw:page` or a `draw:g`) in
/// document order. A `draw:g` becomes a `group` object whose box is the
/// bounding box of its children, as the editor sizes groups; children keep
/// their absolute coordinates. Shape ids come from `xml:id`/`draw:id` so the
/// slide timing can find its targets.
fn read_odp_shapes(
    parent: &XmlNode,
    zip: &ZipReader,
    import: &mut OdpImport,
    warnings: &mut Vec<String>,
) -> Vec<SlideObject> {
    let mut objects = Vec::new();
    for node in &parent.children {
        let object = match node.local_name() {
            "frame" => read_odp_frame(node, zip, import, warnings),
            // A line or connector may be a direct child of the page as well as
            // wrapped in a frame, as some producers write it.
            "line" | "connector" => read_odp_line(node),
            "g" => {
                let children = read_odp_shapes(node, zip, import, warnings);
                if children.is_empty() {
                    warnings.push("A group without importable shapes was skipped.".into());
                    None
                } else {
                    let left = children.iter().map(|child| child.x).fold(f64::INFINITY, f64::min);
                    let top = children.iter().map(|child| child.y).fold(f64::INFINITY, f64::min);
                    let right =
                        children.iter().map(|child| child.x + child.w.max(0.0)).fold(f64::NEG_INFINITY, f64::max);
                    let bottom =
                        children.iter().map(|child| child.y + child.h.max(0.0)).fold(f64::NEG_INFINITY, f64::max);
                    let mut group =
                        SlideObject::new("group", left, top, (right - left).max(1.0), (bottom - top).max(1.0));
                    group.children = children;
                    Some(group)
                }
            }
            _ => None,
        };
        let Some(mut object) = object else { continue };
        object.z = objects.len() as i32 + 1;
        if let Some(id) = node.attr("xml:id").or_else(|| node.attr("draw:id")).filter(|id| !id.trim().is_empty()) {
            object.id = id.to_string();
        }
        object.name = node.attr("draw:name").unwrap_or_default().to_string();
        objects.push(object);
    }
    objects
}

fn collect_odp_ids<'a>(objects: &'a [SlideObject], out: &mut std::collections::HashSet<&'a str>) {
    for object in objects {
        out.insert(object.id.as_str());
        collect_odp_ids(&object.children, out);
    }
}

/// The effect `anim:par` nodes below a sequence, in document order. Effects are
/// the nodes that carry a trigger or a preset class; the click and "after
/// previous" containers around them are walked through.
fn collect_odp_effects<'a>(node: &'a XmlNode, out: &mut Vec<&'a XmlNode>) {
    for child in &node.children {
        if !matches!(child.local_name(), "par" | "seq" | "iterate") {
            continue;
        }
        let trigger = matches!(child.attr_any_ns("node-type"), Some("on-click" | "with-previous" | "after-previous"));
        if trigger || child.attr_any_ns("preset-class").is_some() {
            out.push(child);
        } else {
            collect_odp_effects(child, out);
        }
    }
}

/// Reads the slide's main sequence back into animations: trigger from the
/// effect's node type, class and preset to kind and editor effect (a preset
/// without an editor twin maps to the closest effect with a warning), delay
/// from its begin and duration from its longest child.
fn read_odp_timing(page: &XmlNode, objects: &[SlideObject], warnings: &mut Vec<String>) -> Vec<Animation> {
    let Some(root) = page.children.iter().find(|child| child.local_name() == "par") else { return Vec::new() };
    let mut effects = Vec::new();
    for sequence in root.children_named("seq") {
        match sequence.attr_any_ns("node-type") {
            Some("main-sequence") => collect_odp_effects(sequence, &mut effects),
            Some("interactive-sequence") => {
                warnings.push("Animations triggered by clicking a shape were not imported.".into());
            }
            _ => {}
        }
    }
    let mut ids = std::collections::HashSet::new();
    collect_odp_ids(objects, &mut ids);
    let mut animations = Vec::new();
    for effect in effects {
        let mut nodes = Vec::new();
        effect.walk(&mut nodes);
        let nodes = &nodes[1..];
        let preset = effect.attr_any_ns("preset-id").unwrap_or("");
        let kind = match effect.attr_any_ns("preset-class") {
            Some(kind @ ("entrance" | "exit" | "emphasis")) => kind,
            Some(other) => {
                warnings.push(format!("A \"{other}\" animation has no editor equivalent and was skipped."));
                continue;
            }
            None if preset.starts_with("ooo-entrance-") => "entrance",
            None if preset.starts_with("ooo-exit-") => "exit",
            None => "emphasis",
        };
        let Some(target) = nodes.iter().find_map(|node| node.attr_any_ns("targetElement")) else {
            warnings.push("An animation without a target object was skipped.".into());
            continue;
        };
        if !ids.contains(target) {
            warnings.push("An animation whose target shape was not imported was skipped.".into());
            continue;
        }
        let scale = nodes
            .iter()
            .find(|node| node.local_name() == "animateTransform" && node.attr_any_ns("type") == Some("scale"))
            .and_then(|node| node.attr_any_ns("by").or_else(|| node.attr_any_ns("to")))
            .and_then(|value| value.split(',').next()?.trim().parse::<f64>().ok());
        let exact = ODP_PRESETS.iter().find(|entry| {
            entry.0 == kind
                && entry.2 == preset
                && match entry.1 {
                    "grow" => scale.map(|scale| scale >= 1.0).unwrap_or(true),
                    "shrink" => scale.map(|scale| scale < 1.0).unwrap_or(false),
                    _ => true,
                }
        });
        let effect_name = match exact {
            Some(entry) => entry.1,
            None => {
                // A custom effect has no preset; its transition type is the best hint.
                let hint = if preset.is_empty() {
                    nodes
                        .iter()
                        .find_map(|node| node.attr_any_ns("type").filter(|_| node.local_name() == "transitionFilter"))
                        .unwrap_or("")
                } else {
                    preset
                };
                let closest = closest_odp_effect(kind, hint.trim_start_matches(&format!("ooo-{kind}-")));
                let label = if preset.is_empty() { "without a preset" } else { preset };
                warnings.push(format!(
                    "The {kind} effect \"{label}\" was imported as the closest editor effect \"{closest}\"."
                ));
                closest
            }
        };
        let trigger = match effect.attr_any_ns("node-type") {
            Some("with-previous") => "withPrevious",
            Some("after-previous") => "afterPrevious",
            _ => "onClick",
        };
        let duration = nodes
            .iter()
            .filter_map(|node| {
                let duration = node.attr_any_ns("dur").and_then(parse_smil_seconds)?;
                Some(node.attr_any_ns("begin").and_then(parse_smil_seconds).unwrap_or(0.0) + duration)
            })
            .fold(0.0, f64::max);
        animations.push(Animation {
            id: uuid::Uuid::new_v4().to_string(),
            object_id: target.to_string(),
            kind: kind.to_string(),
            effect: effect_name.to_string(),
            trigger: trigger.to_string(),
            duration_ms: seconds_to_ms(duration),
            delay_ms: effect.attr_any_ns("begin").and_then(parse_smil_seconds).map(seconds_to_ms).unwrap_or(0),
            order: animations.len() as u32,
        });
    }
    animations
}

/// The names of the drawing-page styles that mark a slide hidden
/// (`presentation:visibility="hidden"` on their properties), LibreOffice's
/// convention for a hidden slide.
fn odp_hidden_page_styles(root: &XmlNode) -> HashSet<String> {
    let mut hidden = HashSet::new();
    let mut styles = Vec::new();
    root.find_all("style", &mut styles);
    for style in styles {
        if style.attr_any_ns("family") != Some("drawing-page") {
            continue;
        }
        let Some(name) = style.attr_any_ns("name") else { continue };
        let mut properties = Vec::new();
        style.find_all("drawing-page-properties", &mut properties);
        if properties.iter().any(|node| node.attr_any_ns("visibility") == Some("hidden")) {
            hidden.insert(name.to_string());
        }
    }
    hidden
}

/// The deck footer settings a master page declares: the
/// `presentation:display-*` flags on its drawing-page properties, plus the
/// placeholder frames (`presentation:class`) carrying the footer and date
/// text. `None` when the master page says nothing about a footer.
fn read_odp_footer(root: &XmlNode) -> Option<SlideFooter> {
    let mut footer = SlideFooter::default();
    let mut found = false;
    let mut properties = Vec::new();
    root.find_all("drawing-page-properties", &mut properties);
    for node in properties {
        if node.attr_any_ns("display-footer") == Some("true") {
            footer.show_text = true;
            found = true;
        }
        if node.attr_any_ns("display-page-number") == Some("true") {
            footer.show_slide_number = true;
            found = true;
        }
        if node.attr_any_ns("display-date-time") == Some("true") {
            footer.show_date = true;
            found = true;
        }
    }
    let mut frames = Vec::new();
    root.find_all("frame", &mut frames);
    for frame in frames {
        match frame.attr_any_ns("class") {
            Some("footer") => {
                footer.show_text = true;
                found = true;
                let text = frame.deep_text().trim().to_string();
                if footer.text.is_empty() {
                    footer.text = text;
                }
            }
            Some("page-number") => {
                footer.show_slide_number = true;
                found = true;
            }
            Some("date-time") => {
                footer.show_date = true;
                found = true;
                let text = frame.deep_text().trim().to_string();
                if footer.date_text.is_empty() {
                    footer.date_text = text;
                }
            }
            _ => {}
        }
    }
    found.then(|| {
        footer.enabled = true;
        footer
    })
}

pub fn read_odp(bytes: &[u8]) -> OfficeResult<DeckRead> {
    let reader = ZipReader::open(bytes.to_vec())?;
    if !reader.contains("content.xml") {
        return Err(OfficeError::corrupt("The package does not contain content.xml."));
    }
    let text = reader.read_text("content.xml")?;
    // Trailing text after an inline element is wrapped, so run order survives
    // the parse exactly like the ODT reader's parts.
    let root = parse_xml(&wrap_trailing_text(&text))?;
    let mut deck = Deck::new_blank("Imported presentation");
    deck.slides.clear();
    let mut warnings = Vec::new();
    // Slide size and master page (footer flags) from styles.xml when available.
    let styles_root =
        reader.read_text("styles.xml").ok().and_then(|styles| parse_xml(&wrap_trailing_text(&styles)).ok());
    if let Some(styles_root) = &styles_root {
        let mut layouts = Vec::new();
        styles_root.find_all("page-layout-properties", &mut layouts);
        if let Some(properties) = layouts.first() {
            if let Some(width) = properties.attr_any_ns("page-width").and_then(parse_cm) {
                deck.size.width_pt = width;
            }
            if let Some(height) = properties.attr_any_ns("page-height").and_then(parse_cm) {
                deck.size.height_pt = height;
            }
        }
        if let Some(footer) = read_odp_footer(styles_root) {
            deck.footer = Some(footer);
        }
    }
    let mut import = OdpImport::default();
    collect_styles(&root, &mut import.styles);
    let mut roots = vec![&root];
    if let Some(styles_root) = &styles_root {
        collect_styles(styles_root, &mut import.styles);
        roots.push(styles_root);
    }
    import.clips = odp_graphic_clips(&roots);
    let hidden_pages = odp_hidden_page_styles(&root);
    let mut pages = Vec::new();
    root.find_all("page", &mut pages);
    for page in pages {
        let objects = read_odp_shapes(page, &reader, &mut import, &mut warnings);
        let animations = read_odp_timing(page, &objects, &mut warnings);
        let hidden = page.attr_any_ns("visibility") == Some("hidden")
            || page.attr_any_ns("style-name").map(|name| hidden_pages.contains(name)).unwrap_or(false);
        let mut slide = Slide { objects, animations, hidden, ..Default::default() };
        let mut notes = Vec::new();
        page.find_all("notes", &mut notes);
        if let Some(notes) = notes.first() {
            slide.notes = notes.deep_text().trim().to_string();
        }
        deck.slides.push(slide);
    }
    if deck.slides.is_empty() {
        deck.slides.push(Slide::default());
    }
    warnings.sort();
    warnings.dedup();
    Ok(DeckRead { deck, warnings })
}

pub fn read_odp_file(path: &Path) -> OfficeResult<DeckRead> {
    let bytes = crate::io::read_bytes(path)?;
    let mut result = read_odp(&bytes)?;
    result.deck.title = crate::io::file_stem(path);
    Ok(result)
}

#[cfg(test)]
#[path = "odf_tests.rs"]
mod tests;
