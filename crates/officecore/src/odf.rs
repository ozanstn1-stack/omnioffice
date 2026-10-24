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
mod sheets_write;
mod slides_read;
mod slides_read_timing;
use sheets_read::{ods_annotation, ods_href, read_ods_chart};
mod text_read;
use text_read::{collect_styles, node_text_runs, paragraph_props, wrap_trailing_text, NoteReadState};
mod text_write;
pub use sheets_read::{read_ods, read_ods_file};
pub use sheets_write::{write_ods, write_ods_file};
pub use slides_read_timing::{read_odp, read_odp_file};
pub use slides_write_file::{write_odp, write_odp_file, write_odp_package};
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
mod slides_write;
mod slides_write_file;
mod slides_write_objects;
#[cfg(test)]
#[path = "odf_tests.rs"]
mod tests;
