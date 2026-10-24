//! Export of ChartML chart parts, their caches, and the drawing XML that anchors them.

use super::*;

// ---------------------------------------------------------------------------
// Charts
// ---------------------------------------------------------------------------

/// A chart that made it into the package, ready to be written.
pub(super) struct PlannedChart {
    /// Position in the package-wide `xl/charts/chartN.xml` numbering.
    pub(super) chart_number: usize,
    pub(super) xml: String,
    pub(super) anchor: String,
    pub(super) width_px: f64,
    pub(super) height_px: f64,
}

/// A sheet picture that made it into the package, ready to be written.
pub(super) struct PlannedImage {
    /// File name inside `xl/media/`, e.g. `image1.png`.
    pub(super) part_name: String,
    pub(super) bytes: Vec<u8>,
    pub(super) image: SheetImage,
}

/// The chart kinds the exporter writes as real ChartML parts.
fn chart_kind_supported(kind: &str) -> bool {
    matches!(kind, "column" | "bar" | "line" | "pie" | "area" | "scatter" | "doughnut")
}

/// Quotes a sheet name when a formula reference needs it (`'My Sheet'!`).
fn sheet_ref(name: &str) -> String {
    let simple =
        !name.is_empty() && name.chars().all(|character| character.is_ascii_alphanumeric() || character == '_');
    if simple {
        name.to_string()
    } else {
        format!("'{}'", name.replace('\'', "''"))
    }
}

/// `A1:B5` (relative, no sheet) as `Sheet!$A$1:$B$5`. `None` when malformed.
pub(super) fn absolute_ref(range: &str, sheet: &str) -> Option<String> {
    let range = range.trim();
    if range.is_empty() {
        return None;
    }
    if let Some((start, end)) = range.split_once(':') {
        let (start_row, start_col) = crate::address::parse(start)?;
        let (end_row, end_col) = crate::address::parse(end)?;
        Some(format!(
            "{}!${}${}:${}${}",
            sheet_ref(sheet),
            crate::address::column_name(start_col),
            start_row + 1,
            crate::address::column_name(end_col),
            end_row + 1
        ))
    } else {
        let (row, column) = crate::address::parse(range)?;
        Some(format!("{}!${}${}", sheet_ref(sheet), crate::address::column_name(column), row + 1))
    }
}

/// `1:3` or `A:B` as the `Sheet!$1:$3` / `Sheet!$A:$B` form Excel stores in
/// `_xlnm.Print_Titles`. `None` when the range is not a row or column span.
pub(super) fn print_titles_ref(range: &str, sheet: &str, rows: bool) -> Option<String> {
    let (start, end) = range.trim().split_once(':')?;
    let (start, end) = (start.trim(), end.trim());
    if start.is_empty() || end.is_empty() {
        return None;
    }
    if rows {
        let start_row = start.parse::<u32>().ok()?;
        let end_row = end.parse::<u32>().ok()?;
        Some(format!("{}!${}:${}", sheet_ref(sheet), start_row, end_row))
    } else {
        if !start.chars().all(|character| character.is_ascii_alphabetic())
            || !end.chars().all(|character| character.is_ascii_alphabetic())
        {
            return None;
        }
        Some(format!("{}!${}:${}", sheet_ref(sheet), start.to_ascii_uppercase(), end.to_ascii_uppercase()))
    }
}

/// A rich-text chart or axis title.
fn chart_title_xml(text: &str) -> String {
    format!(
        "<c:title><c:tx><c:rich><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang=\"en-US\"/><a:t>{}</a:t></a:r></a:p></c:rich></c:tx><c:overlay val=\"0\"/></c:title>",
        escape_text(text)
    )
}

const CHART_CATEGORY_AXIS: u64 = 111_111_111;
const CHART_VALUE_AXIS: u64 = 222_222_222;
/// Excel's default doughnut hole, the value `ChartData::hole_size == None` means.
pub(super) const DOUGHNUT_DEFAULT_HOLE: u32 = 50;

/// One cached value the way Excel writes `c:v`: plain decimal where possible.
fn chart_cache_number(value: f64) -> String {
    if !value.is_finite() {
        return "0".into();
    }
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

/// `<c:strCache>` for cached category labels; empty when there is no cache.
fn chart_str_cache_xml(labels: &[String]) -> String {
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

/// `<c:numCache>` for cached series values; empty when there is no cache.
fn chart_num_cache_xml(values: &[f64]) -> String {
    if values.is_empty() {
        return String::new();
    }
    let mut out = format!("<c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"{}\"/>", values.len());
    for (index, value) in values.iter().enumerate() {
        out.push_str(&format!("<c:pt idx=\"{index}\"><c:v>{}</c:v></c:pt>", chart_cache_number(*value)));
    }
    out.push_str("</c:numCache>");
    out
}

/// The category + value axis pair shared by every cartesian chart kind.
fn chart_axes_xml(chart: &ChartData) -> String {
    let category_title = if chart.x_title.is_empty() { String::new() } else { chart_title_xml(&chart.x_title) };
    let value_title = if chart.y_title.is_empty() { String::new() } else { chart_title_xml(&chart.y_title) };
    format!(
        "<c:catAx><c:axId val=\"{cat}\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"b\"/>{category_title}<c:crossAx val=\"{val}\"/></c:catAx><c:valAx><c:axId val=\"{val}\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"l\"/>{value_title}<c:crossAx val=\"{cat}\"/></c:valAx>",
        cat = CHART_CATEGORY_AXIS,
        val = CHART_VALUE_AXIS,
    )
}

/// The two value axes of a scatter chart: X along the bottom, Y on the left.
fn scatter_axes_xml(chart: &ChartData) -> String {
    let x_title = if chart.x_title.is_empty() { String::new() } else { chart_title_xml(&chart.x_title) };
    let y_title = if chart.y_title.is_empty() { String::new() } else { chart_title_xml(&chart.y_title) };
    format!(
        "<c:valAx><c:axId val=\"{x}\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"b\"/>{x_title}<c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:crossAx val=\"{y}\"/><c:crossBetween val=\"midCat\"/></c:valAx><c:valAx><c:axId val=\"{y}\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"l\"/>{y_title}<c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:crossAx val=\"{x}\"/><c:crossBetween val=\"midCat\"/></c:valAx>",
        x = CHART_CATEGORY_AXIS,
        y = CHART_VALUE_AXIS,
    )
}

/// What a scatter style draws: (lines, markers, smoothed lines). `None` and the
/// unknown spellings are markers only, which is Excel's plain "Scatter".
pub(crate) fn scatter_flavour(style: Option<&str>) -> (bool, bool, bool) {
    match style {
        Some("lineMarker") => (true, true, false),
        Some("line") => (true, false, false),
        Some("smoothMarker") => (true, true, true),
        Some("smooth") => (true, false, true),
        _ => (false, true, false),
    }
}

/// The `c:xVal` of a scatter series. X values that are all numbers are written
/// as a number reference with their cache; anything else stays a string
/// reference, which Excel plots as 1, 2, 3, ...
fn scatter_x_values_xml(reference: &str, cache: &[String]) -> String {
    let numbers: Vec<f64> = cache.iter().filter_map(|label| label.trim().parse::<f64>().ok()).collect();
    if cache.is_empty() || numbers.len() == cache.len() {
        format!(
            "<c:xVal><c:numRef><c:f>{}</c:f>{}</c:numRef></c:xVal>",
            escape_text(reference),
            chart_num_cache_xml(&numbers)
        )
    } else {
        format!(
            "<c:xVal><c:strRef><c:f>{}</c:f>{}</c:strRef></c:xVal>",
            escape_text(reference),
            chart_str_cache_xml(cache)
        )
    }
}

/// The marker and line look of one scatter series, before its data.
fn scatter_series_look_xml(color: Option<&str>, lines: bool, markers: bool) -> String {
    let rgb = color.map(|color| argb_of(color).get(2..).unwrap_or("000000").to_string());
    let line = match (&rgb, lines) {
        (_, false) => "<a:ln w=\"19050\"><a:noFill/></a:ln>".to_string(),
        (Some(rgb), true) => {
            format!(
                "<a:ln w=\"28575\" cap=\"rnd\"><a:solidFill><a:srgbClr val=\"{rgb}\"/></a:solidFill><a:round/></a:ln>"
            )
        }
        (None, true) => String::new(),
    };
    let shape = if line.is_empty() { String::new() } else { format!("<c:spPr>{line}</c:spPr>") };
    let marker = if !markers {
        "<c:marker><c:symbol val=\"none\"/></c:marker>".to_string()
    } else {
        let fill = rgb
            .map(|rgb| {
                format!(
                    "<c:spPr><a:solidFill><a:srgbClr val=\"{rgb}\"/></a:solidFill><a:ln w=\"9525\"><a:solidFill><a:srgbClr val=\"{rgb}\"/></a:solidFill></a:ln></c:spPr>"
                )
            })
            .unwrap_or_default();
        format!("<c:marker><c:symbol val=\"circle\"/><c:size val=\"7\"/>{fill}</c:marker>")
    };
    format!("{shape}{marker}")
}

/// Builds one `xl/charts/chartN.xml`. `Err` carries the user-facing reason the
/// chart cannot be represented; the caller keeps it in `.oswk` and warns.
pub(super) fn chart_xml(placement: &ChartPlacement, sheet_name: &str) -> Result<String, String> {
    let chart = &placement.chart;
    let kind = chart.kind.as_str();
    if !chart_kind_supported(kind) {
        return Err(format!("the chart type \"{kind}\" is not exportable"));
    }
    let scatter = kind == "scatter";
    // A scatter chart may leave the X range empty (Excel then plots 1, 2, 3, ...);
    // every other kind needs its category range.
    let categories = absolute_ref(&chart.categories, sheet_name);
    if categories.is_none() && !(scatter && chart.categories.trim().is_empty()) {
        return Err("the category range could not be read".to_string());
    }
    let categories = categories.unwrap_or_default();
    if chart.series.is_empty() {
        return Err("the chart has no data series".into());
    }
    let (scatter_lines, scatter_markers, scatter_smooth) = scatter_flavour(chart.scatter_style.as_deref());

    let mut series_xml = String::new();
    for (index, series) in chart.series.iter().enumerate() {
        let values = absolute_ref(&series.range, sheet_name)
            .ok_or_else(|| format!("the range for series \"{}\" could not be read", series.name))?;
        series_xml.push_str(&format!(
            "<c:ser><c:idx val=\"{index}\"/><c:order val=\"{index}\"/><c:tx><c:v>{}</c:v></c:tx>",
            escape_text(&series.name)
        ));
        // Cached labels/values from an imported ChartML part are written back
        // so a chart whose source range lives outside the package still renders
        // and a second import sees the same model.
        let value_cache =
            chart.series_values_cache.get(index).map(|values| chart_num_cache_xml(values)).unwrap_or_default();
        if scatter {
            series_xml.push_str(&scatter_series_look_xml(series.color.as_deref(), scatter_lines, scatter_markers));
            if !categories.is_empty() {
                series_xml.push_str(&scatter_x_values_xml(&categories, &chart.categories_cache));
            }
            series_xml.push_str(&format!(
                "<c:yVal><c:numRef><c:f>{}</c:f>{value_cache}</c:numRef></c:yVal><c:smooth val=\"{}\"/></c:ser>",
                escape_text(&values),
                u8::from(scatter_smooth)
            ));
            continue;
        }
        if let Some(color) = series.color.as_deref() {
            let argb = argb_of(color);
            series_xml.push_str(&format!(
                "<c:spPr><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill></c:spPr>",
                argb.get(2..).unwrap_or("000000")
            ));
        }
        let category_cache = if index == 0 { chart_str_cache_xml(&chart.categories_cache) } else { String::new() };
        series_xml.push_str(&format!(
            "<c:cat><c:strRef><c:f>{}</c:f>{category_cache}</c:strRef></c:cat><c:val><c:numRef><c:f>{}</c:f>{value_cache}</c:numRef></c:val></c:ser>",
            // A sheet name can contain `&` or `<`; the reference has to be
            // XML-escaped or the chart part stops being well-formed XML.
            escape_text(&categories),
            escape_text(&values)
        ));
    }

    let labels = if chart.show_labels {
        "<c:dLbls><c:showLegendKey val=\"0\"/><c:showVal val=\"1\"/><c:showCatName val=\"0\"/><c:showSerName val=\"0\"/><c:showPercent val=\"0\"/><c:showBubbleSize val=\"0\"/></c:dLbls>"
    } else {
        ""
    };

    let mut plot = String::new();
    let mut axes = String::new();
    match kind {
        "column" | "bar" => {
            let direction = if kind == "bar" { "bar" } else { "col" };
            let grouping = if chart.stacked { "stacked" } else { "clustered" };
            let overlap = if chart.stacked { "<c:overlap val=\"100\"/>" } else { "" };
            plot.push_str(&format!(
                "<c:barChart><c:barDir val=\"{direction}\"/><c:grouping val=\"{grouping}\"/><c:varyColors val=\"0\"/>{series_xml}{labels}<c:gapWidth val=\"150\"/>{overlap}<c:axId val=\"{cat}\"/><c:axId val=\"{val}\"/></c:barChart>",
                cat = CHART_CATEGORY_AXIS,
                val = CHART_VALUE_AXIS,
            ));
            axes = chart_axes_xml(chart);
        }
        "line" => {
            plot.push_str(&format!(
                "<c:lineChart><c:grouping val=\"standard\"/><c:varyColors val=\"0\"/>{series_xml}{labels}<c:marker val=\"1\"/><c:axId val=\"{cat}\"/><c:axId val=\"{val}\"/></c:lineChart>",
                cat = CHART_CATEGORY_AXIS,
                val = CHART_VALUE_AXIS,
            ));
            axes = chart_axes_xml(chart);
        }
        "area" => {
            let grouping = if chart.stacked { "stacked" } else { "standard" };
            plot.push_str(&format!(
                "<c:areaChart><c:grouping val=\"{grouping}\"/><c:varyColors val=\"0\"/>{series_xml}{labels}<c:axId val=\"{cat}\"/><c:axId val=\"{val}\"/></c:areaChart>",
                cat = CHART_CATEGORY_AXIS,
                val = CHART_VALUE_AXIS,
            ));
            axes = chart_axes_xml(chart);
        }
        "pie" => {
            plot.push_str(&format!(
                "<c:pieChart><c:varyColors val=\"1\"/>{series_xml}{labels}<c:firstSliceAng val=\"0\"/></c:pieChart>"
            ));
        }
        "doughnut" => {
            let hole = chart.hole_size.unwrap_or(DOUGHNUT_DEFAULT_HOLE).clamp(10, 90);
            plot.push_str(&format!(
                "<c:doughnutChart><c:varyColors val=\"1\"/>{series_xml}{labels}<c:firstSliceAng val=\"0\"/><c:holeSize val=\"{hole}\"/></c:doughnutChart>"
            ));
        }
        "scatter" => {
            // Excel spells every drawn flavour `lineMarker` (or `smoothMarker`);
            // whether lines and markers show is decided per series above.
            let style = if scatter_smooth { "smoothMarker" } else { "lineMarker" };
            plot.push_str(&format!(
                "<c:scatterChart><c:scatterStyle val=\"{style}\"/><c:varyColors val=\"0\"/>{series_xml}{labels}<c:axId val=\"{cat}\"/><c:axId val=\"{val}\"/></c:scatterChart>",
                cat = CHART_CATEGORY_AXIS,
                val = CHART_VALUE_AXIS,
            ));
            axes = scatter_axes_xml(chart);
        }
        _ => return Err(format!("the chart type \"{kind}\" is not exportable")),
    }

    let title = if chart.title.is_empty() { String::new() } else { chart_title_xml(&chart.title) };
    let legend = if chart.legend { "<c:legend><c:legendPos val=\"b\"/><c:overlay val=\"0\"/></c:legend>" } else { "" };
    Ok(format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><c:roundedCorners val=\"0\"/><c:chart>{title}<c:plotArea><c:layout/>{plot}{axes}</c:plotArea>{legend}<c:plotVisOnly val=\"1\"/><c:dispBlanksAs val=\"gap\"/></c:chart><c:printSettings><c:headerFooter/><c:pageMargins b=\"0.75\" l=\"0.7\" r=\"0.7\" t=\"0.75\" header=\"0.3\" footer=\"0.3\"/><c:pageSetup/></c:printSettings></c:chartSpace>"
    ))
}

/// The drawing part that anchors a sheet's charts and pictures at their model
/// positions. Chart relationships come first so the rIds stay stable when only
/// one of the two kinds is present.
pub(super) fn drawing_xml(charts: &[PlannedChart], images: &[PlannedImage]) -> String {
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<xdr:wsDr xmlns:xdr=\"http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">",
    );
    for (index, chart) in charts.iter().enumerate() {
        let (row, column) = crate::address::parse(&chart.anchor).unwrap_or((0, 0));
        let cx = (chart.width_px.max(64.0) * 9525.0).round() as i64;
        let cy = (chart.height_px.max(64.0) * 9525.0).round() as i64;
        let rid = format!("rId{}", index + 1);
        xml.push_str(&format!(
            "<xdr:oneCellAnchor><xdr:from><xdr:col>{column}</xdr:col><xdr:colOff>190500</xdr:colOff><xdr:row>{row}</xdr:row><xdr:rowOff>95250</xdr:rowOff></xdr:from><xdr:ext cx=\"{cx}\" cy=\"{cy}\"/><xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr><xdr:cNvPr id=\"{id}\" name=\"Chart {id}\"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/></xdr:xfrm><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" r:id=\"{rid}\"/></a:graphicData></a:graphic></xdr:graphicFrame><xdr:clientData/></xdr:oneCellAnchor>",
            id = index + 1
        ));
    }
    for (index, planned) in images.iter().enumerate() {
        let image = &planned.image;
        let (row, column) = crate::address::parse(&image.anchor.address).unwrap_or((0, 0));
        let cx = (image.width_px.max(8.0) * 9525.0).round() as i64;
        let cy = (image.height_px.max(8.0) * 9525.0).round() as i64;
        let rid = format!("rId{}", charts.len() + index + 1);
        // `rot` is stored in 60000ths of a degree, mirrored by the importer.
        let rotation = if image.rotation_deg.abs() > 0.001 {
            format!(" rot=\"{}\"", (image.rotation_deg * 60_000.0).round() as i64)
        } else {
            String::new()
        };
        let alt = escape_attr(&image.image.alt);
        let id = charts.len() + index + 1;
        let picture = format!(
            "<xdr:pic><xdr:nvPicPr><xdr:cNvPr id=\"{id}\" name=\"Image {id}\" descr=\"{alt}\"/><xdr:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></xdr:cNvPicPr></xdr:nvPicPr><xdr:blipFill><a:blip xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" r:embed=\"{rid}\"/><a:stretch><a:fillRect/></a:stretch></xdr:blipFill><xdr:spPr><a:xfrm{rotation}><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></xdr:spPr></xdr:pic>"
        );
        let from = format!(
            "<xdr:from><xdr:col>{column}</xdr:col><xdr:colOff>{}</xdr:colOff><xdr:row>{row}</xdr:row><xdr:rowOff>{}</xdr:rowOff></xdr:from>",
            image.anchor.col_off_emu, image.anchor.row_off_emu
        );
        // A picture imported with a two-cell anchor keeps both corners; a model
        // picture without one is written as a one-cell anchor with its extent.
        match image.anchor.to_address.as_deref().and_then(crate::address::parse) {
            Some((to_row, to_column)) => {
                xml.push_str(&format!(
                    "<xdr:twoCellAnchor editAs=\"oneCell\">{from}<xdr:to><xdr:col>{to_column}</xdr:col><xdr:colOff>{}</xdr:colOff><xdr:row>{to_row}</xdr:row><xdr:rowOff>{}</xdr:rowOff></xdr:to>{picture}<xdr:clientData/></xdr:twoCellAnchor>",
                    image.anchor.to_col_off_emu, image.anchor.to_row_off_emu
                ));
            }
            None => {
                xml.push_str(&format!(
                    "<xdr:oneCellAnchor>{from}<xdr:ext cx=\"{cx}\" cy=\"{cy}\"/>{picture}<xdr:clientData/></xdr:oneCellAnchor>"
                ));
            }
        }
    }
    xml.push_str("</xdr:wsDr>");
    xml
}
