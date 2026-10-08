//! Calc scatter and doughnut charts: model, XLSX (ChartML) and ODS (chart
//! objects), written by us and read from the shapes Excel and LibreOffice
//! produce.
//!
//! The workflow is the one a user performs: build a sheet with a chart, save,
//! reopen, compare the model. The hand-written parts below are trimmed copies of
//! what Excel 365 and LibreOffice 7 write for the same charts.

use officecore::compat;
use officecore::model::*;
use officecore::odf;
use officecore::xlsx;
use officecore::zip::{ZipLimits, ZipReader, ZipWriter};

fn number(value: f64) -> Cell {
    Cell { value: CellValue::Number(value), ..Default::default() }
}

fn text(value: &str) -> Cell {
    Cell { value: CellValue::Text(value.to_string()), ..Default::default() }
}

/// X values in column A, two Y series in B and C.
fn scatter_workbook() -> Workbook {
    let mut workbook = Workbook::new_blank("Scatter");
    let sheet = &mut workbook.sheets[0];
    sheet.name = "Data".into();
    sheet.set("A1", text("Height"));
    sheet.set("B1", text("Weight"));
    sheet.set("C1", text("Age"));
    for (row, (x, y, z)) in
        [(150.0, 52.5, 31.0), (160.0, 60.0, 35.0), (172.5, 71.0, 41.0), (181.0, 80.0, 29.0)].iter().enumerate()
    {
        let row = row + 2;
        sheet.set(&format!("A{row}"), number(*x));
        sheet.set(&format!("B{row}"), number(*y));
        sheet.set(&format!("C{row}"), number(*z));
    }
    workbook
}

fn scatter_chart(style: Option<&str>) -> ChartPlacement {
    ChartPlacement {
        id: "chart-scatter".into(),
        chart: ChartData {
            kind: "scatter".into(),
            title: "Weight by height".into(),
            categories: "A2:A5".into(),
            series: vec![
                ChartSeries { name: "Weight".into(), range: "B2:B5".into(), color: Some("#1D4ED8".into()) },
                ChartSeries { name: "Age".into(), range: "C2:C5".into(), color: None },
            ],
            legend: true,
            x_title: "Height (cm)".into(),
            y_title: "Weight (kg)".into(),
            show_labels: false,
            scatter_style: style.map(str::to_string),
            ..Default::default()
        },
        anchor: "E2".into(),
        width_px: 480.0,
        height_px: 300.0,
    }
}

fn doughnut_chart(hole: Option<u32>) -> ChartPlacement {
    ChartPlacement {
        id: "chart-doughnut".into(),
        chart: ChartData {
            kind: "doughnut".into(),
            title: "Share".into(),
            categories: "A2:A4".into(),
            series: vec![ChartSeries { name: "Weight".into(), range: "B2:B4".into(), color: None }],
            legend: true,
            show_labels: true,
            hole_size: hole,
            ..Default::default()
        },
        anchor: "E20".into(),
        width_px: 320.0,
        height_px: 240.0,
    }
}

/// Rebuilds a package with some parts replaced, so the importer can be fed
/// parts our own writer would never produce.
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

fn first_chart(workbook: &Workbook) -> &ChartData {
    &workbook.sheets[0].charts.first().expect("a chart").chart
}

// ---------------------------------------------------------------------------
// XLSX
// ---------------------------------------------------------------------------

#[test]
fn xlsx_scatter_chart_writes_xval_yval_and_two_value_axes() {
    let mut workbook = scatter_workbook();
    workbook.sheets[0].charts.push(scatter_chart(Some("lineMarker")));
    let result = xlsx::write_xlsx_package(&workbook).unwrap();
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    let chart = ZipReader::open(result.bytes).unwrap().read_text("xl/charts/chart1.xml").unwrap();
    assert!(chart.contains("<c:scatterChart><c:scatterStyle val=\"lineMarker\"/>"), "{chart}");
    assert_eq!(chart.matches("<c:xVal>").count(), 2);
    assert_eq!(chart.matches("<c:yVal>").count(), 2);
    assert!(chart.contains("<c:xVal><c:numRef><c:f>Data!$A$2:$A$5</c:f>"));
    assert!(chart.contains("<c:yVal><c:numRef><c:f>Data!$C$2:$C$5</c:f>"));
    assert!(!chart.contains("<c:cat>") && !chart.contains("<c:catAx>"));
    assert_eq!(chart.matches("<c:valAx>").count(), 2);
    assert!(chart.contains("<c:axPos val=\"b\"/><c:title>"), "the X axis carries its title");
    assert!(chart.contains("Height (cm)") && chart.contains("Weight (kg)"));
    // Lines and markers are both drawn for this flavour.
    assert!(chart.contains("<c:symbol val=\"circle\"/>"));
    assert!(chart.contains("<a:ln w=\"28575\" cap=\"rnd\"><a:solidFill><a:srgbClr val=\"1D4ED8\"/>"));
    assert!(chart.contains("<c:smooth val=\"0\"/>"));
}

#[test]
fn xlsx_scatter_chart_round_trips_every_style() {
    for style in [None, Some("lineMarker"), Some("line"), Some("smoothMarker"), Some("smooth")] {
        let mut workbook = scatter_workbook();
        workbook.sheets[0].charts.push(scatter_chart(style));
        let bytes = xlsx::write_xlsx(&workbook).unwrap();
        let read = xlsx::read_workbook_bytes(&bytes).unwrap();
        assert!(read.warnings.iter().all(|warning| !warning.contains("limited support")), "{:?}", read.warnings);
        let placement = read.workbook.sheets[0].charts.first().expect("scatter chart survives");
        assert_eq!(placement.anchor, "E2");
        assert_eq!(placement.chart, workbook.sheets[0].charts[0].chart, "style {style:?}");
        // A second cycle changes nothing.
        let again = xlsx::read_workbook_bytes(&xlsx::write_xlsx(&read.workbook).unwrap()).unwrap();
        assert_eq!(first_chart(&again.workbook), first_chart(&read.workbook));
    }
}

#[test]
fn xlsx_scatter_chart_keeps_x_and_y_caches() {
    let mut workbook = scatter_workbook();
    let mut placement = scatter_chart(Some("lineMarker"));
    placement.chart.categories_cache = vec!["150".into(), "160".into(), "172.5".into(), "181".into()];
    placement.chart.series_values_cache = vec![vec![52.5, 60.0, 71.0, 80.0], vec![31.0, 35.0, 41.0, 29.0]];
    workbook.sheets[0].charts.push(placement.clone());
    let read = xlsx::read_workbook_bytes(&xlsx::write_xlsx(&workbook).unwrap()).unwrap();
    assert_eq!(first_chart(&read.workbook), &placement.chart);
}

#[test]
fn xlsx_scatter_chart_without_an_x_range_omits_xval() {
    let mut workbook = scatter_workbook();
    let mut placement = scatter_chart(None);
    placement.chart.categories = String::new();
    workbook.sheets[0].charts.push(placement);
    let result = xlsx::write_xlsx_package(&workbook).unwrap();
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    let chart = ZipReader::open(result.bytes.clone()).unwrap().read_text("xl/charts/chart1.xml").unwrap();
    assert!(!chart.contains("<c:xVal>") && chart.contains("<c:yVal>"));
    let read = xlsx::read_workbook_bytes(&result.bytes).unwrap();
    assert_eq!(first_chart(&read.workbook).categories, "");
    assert_eq!(first_chart(&read.workbook).series.len(), 2);
}

#[test]
fn xlsx_scatter_with_a_broken_x_range_is_kept_in_the_oswk_with_a_warning() {
    let mut workbook = scatter_workbook();
    let mut placement = scatter_chart(None);
    placement.chart.categories = "not a range".into();
    workbook.sheets[0].charts.push(placement);
    let result = xlsx::write_xlsx_package(&workbook).unwrap();
    assert!(result.warnings.iter().any(|warning| warning.contains("kept in the .oswk")), "{:?}", result.warnings);
}

#[test]
fn xlsx_doughnut_chart_writes_hole_size_and_round_trips() {
    for hole in [None, Some(30), Some(75)] {
        let mut workbook = scatter_workbook();
        workbook.sheets[0].charts.push(doughnut_chart(hole));
        let bytes = xlsx::write_xlsx(&workbook).unwrap();
        let chart_xml = ZipReader::open(bytes.clone()).unwrap().read_text("xl/charts/chart1.xml").unwrap();
        assert!(chart_xml.contains("<c:doughnutChart><c:varyColors val=\"1\"/>"), "{chart_xml}");
        assert!(chart_xml.contains(&format!("<c:holeSize val=\"{}\"/>", hole.unwrap_or(50))), "{chart_xml}");
        assert!(!chart_xml.contains("<c:catAx>"), "a doughnut has no axes");
        let read = xlsx::read_workbook_bytes(&bytes).unwrap();
        assert_eq!(first_chart(&read.workbook), &workbook.sheets[0].charts[0].chart, "hole {hole:?}");
    }
}

#[test]
fn xlsx_doughnut_hole_size_is_clamped_to_the_valid_range() {
    let mut workbook = scatter_workbook();
    workbook.sheets[0].charts.push(doughnut_chart(Some(100)));
    let bytes = xlsx::write_xlsx(&workbook).unwrap();
    let chart_xml = ZipReader::open(bytes).unwrap().read_text("xl/charts/chart1.xml").unwrap();
    assert!(chart_xml.contains("<c:holeSize val=\"90\"/>"), "{chart_xml}");
}

/// What Excel writes for "Scatter" (markers only): `lineMarker`, a hidden line,
/// theme-coloured markers, per-series X and Y references with caches.
const EXCEL_SCATTER: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n",
    "<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" ",
    "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" ",
    "xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">",
    "<c:date1904 val=\"0\"/><c:roundedCorners val=\"0\"/><c:chart>",
    "<c:title><c:tx><c:rich><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang=\"en-US\"/><a:t>Weight by height</a:t></a:r></a:p></c:rich></c:tx><c:overlay val=\"0\"/></c:title>",
    "<c:autoTitleDeleted val=\"0\"/><c:plotArea><c:layout/>",
    "<c:scatterChart><c:scatterStyle val=\"lineMarker\"/><c:varyColors val=\"0\"/>",
    "<c:ser><c:idx val=\"0\"/><c:order val=\"0\"/>",
    "<c:tx><c:strRef><c:f>Data!$B$1</c:f><c:strCache><c:ptCount val=\"1\"/><c:pt idx=\"0\"><c:v>Weight</c:v></c:pt></c:strCache></c:strRef></c:tx>",
    "<c:spPr><a:ln w=\"19050\" cap=\"rnd\"><a:noFill/><a:round/></a:ln><a:effectLst/></c:spPr>",
    "<c:marker><c:symbol val=\"circle\"/><c:size val=\"5\"/><c:spPr><a:solidFill><a:schemeClr val=\"accent1\"/></a:solidFill><a:ln w=\"9525\"><a:solidFill><a:schemeClr val=\"accent1\"/></a:solidFill></a:ln><a:effectLst/></c:spPr></c:marker>",
    "<c:xVal><c:numRef><c:f>Data!$A$2:$A$5</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"4\"/><c:pt idx=\"0\"><c:v>150</c:v></c:pt><c:pt idx=\"1\"><c:v>160</c:v></c:pt><c:pt idx=\"2\"><c:v>172.5</c:v></c:pt><c:pt idx=\"3\"><c:v>181</c:v></c:pt></c:numCache></c:numRef></c:xVal>",
    "<c:yVal><c:numRef><c:f>Data!$B$2:$B$5</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"4\"/><c:pt idx=\"0\"><c:v>52.5</c:v></c:pt><c:pt idx=\"1\"><c:v>60</c:v></c:pt><c:pt idx=\"2\"><c:v>71</c:v></c:pt><c:pt idx=\"3\"><c:v>80</c:v></c:pt></c:numCache></c:numRef></c:yVal>",
    "<c:smooth val=\"0\"/></c:ser>",
    "<c:ser><c:idx val=\"1\"/><c:order val=\"1\"/>",
    "<c:tx><c:strRef><c:f>Data!$C$1</c:f><c:strCache><c:ptCount val=\"1\"/><c:pt idx=\"0\"><c:v>Age</c:v></c:pt></c:strCache></c:strRef></c:tx>",
    "<c:spPr><a:ln w=\"19050\" cap=\"rnd\"><a:noFill/><a:round/></a:ln></c:spPr>",
    "<c:marker><c:symbol val=\"square\"/><c:size val=\"5\"/><c:spPr><a:solidFill><a:srgbClr val=\"ED7D31\"/></a:solidFill></c:spPr></c:marker>",
    "<c:xVal><c:numRef><c:f>Data!$A$2:$A$5</c:f></c:numRef></c:xVal>",
    "<c:yVal><c:numRef><c:f>Data!$C$2:$C$5</c:f></c:numRef></c:yVal><c:smooth val=\"0\"/></c:ser>",
    "<c:dLbls><c:showLegendKey val=\"0\"/><c:showVal val=\"0\"/><c:showCatName val=\"0\"/><c:showSerName val=\"0\"/><c:showPercent val=\"0\"/><c:showBubbleSize val=\"0\"/></c:dLbls>",
    "<c:axId val=\"501\"/><c:axId val=\"502\"/></c:scatterChart>",
    "<c:valAx><c:axId val=\"501\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"b\"/>",
    "<c:title><c:tx><c:rich><a:bodyPr/><a:p><a:r><a:t>Height (cm)</a:t></a:r></a:p></c:rich></c:tx><c:overlay val=\"0\"/></c:title>",
    "<c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/><c:crossAx val=\"502\"/><c:crosses val=\"autoZero\"/><c:crossBetween val=\"midCat\"/></c:valAx>",
    "<c:valAx><c:axId val=\"502\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"l\"/><c:majorGridlines/>",
    "<c:title><c:tx><c:rich><a:bodyPr/><a:p><a:r><a:t>Weight (kg)</a:t></a:r></a:p></c:rich></c:tx><c:overlay val=\"0\"/></c:title>",
    "<c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/><c:crossAx val=\"501\"/><c:crosses val=\"autoZero\"/><c:crossBetween val=\"midCat\"/></c:valAx>",
    "</c:plotArea><c:legend><c:legendPos val=\"b\"/><c:overlay val=\"0\"/></c:legend><c:plotVisOnly val=\"1\"/><c:dispBlanksAs val=\"gap\"/></c:chart></c:chartSpace>"
);

/// Excel's doughnut with the 2013+ default hole of 75 percent.
const EXCEL_DOUGHNUT: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n",
    "<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" ",
    "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" ",
    "xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">",
    "<c:roundedCorners val=\"0\"/><c:chart><c:autoTitleDeleted val=\"1\"/><c:plotArea><c:layout/>",
    "<c:doughnutChart><c:varyColors val=\"1\"/>",
    "<c:ser><c:idx val=\"0\"/><c:order val=\"0\"/>",
    "<c:tx><c:strRef><c:f>Data!$B$1</c:f><c:strCache><c:ptCount val=\"1\"/><c:pt idx=\"0\"><c:v>Weight</c:v></c:pt></c:strCache></c:strRef></c:tx>",
    "<c:dPt><c:idx val=\"0\"/><c:bubble3D val=\"0\"/><c:spPr><a:solidFill><a:schemeClr val=\"accent1\"/></a:solidFill><a:ln w=\"19050\"><a:solidFill><a:schemeClr val=\"lt1\"/></a:solidFill></a:ln></c:spPr></c:dPt>",
    "<c:cat><c:strRef><c:f>Data!$A$2:$A$4</c:f><c:strCache><c:ptCount val=\"3\"/><c:pt idx=\"0\"><c:v>150</c:v></c:pt><c:pt idx=\"1\"><c:v>160</c:v></c:pt><c:pt idx=\"2\"><c:v>172.5</c:v></c:pt></c:strCache></c:strRef></c:cat>",
    "<c:val><c:numRef><c:f>Data!$B$2:$B$4</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"3\"/><c:pt idx=\"0\"><c:v>52.5</c:v></c:pt><c:pt idx=\"1\"><c:v>60</c:v></c:pt><c:pt idx=\"2\"><c:v>71</c:v></c:pt></c:numCache></c:numRef></c:val></c:ser>",
    "<c:dLbls><c:showLegendKey val=\"0\"/><c:showVal val=\"1\"/><c:showCatName val=\"0\"/><c:showSerName val=\"0\"/><c:showPercent val=\"0\"/><c:showBubbleSize val=\"0\"/><c:showLeaderLines val=\"1\"/></c:dLbls>",
    "<c:firstSliceAng val=\"0\"/><c:holeSize val=\"75\"/></c:doughnutChart>",
    "</c:plotArea><c:legend><c:legendPos val=\"r\"/><c:overlay val=\"0\"/></c:legend><c:plotVisOnly val=\"1\"/><c:dispBlanksAs val=\"gap\"/></c:chart></c:chartSpace>"
);

#[test]
fn reads_the_scatter_chart_excel_writes() {
    let mut workbook = scatter_workbook();
    workbook.sheets[0].charts.push(scatter_chart(None));
    let bytes = xlsx::write_xlsx(&workbook).unwrap();
    let patched = rebuild(&bytes, &[("xl/charts/chart1.xml", EXCEL_SCATTER)]);
    let read = xlsx::read_workbook_bytes(&patched).unwrap();
    assert!(read.warnings.iter().all(|warning| !warning.contains("limited support")), "{:?}", read.warnings);
    let chart = first_chart(&read.workbook);
    assert_eq!(chart.kind, "scatter");
    assert_eq!(chart.title, "Weight by height");
    assert_eq!(chart.categories, "A2:A5", "the X values are the shared range");
    assert_eq!(chart.categories_cache, vec!["150", "160", "172.5", "181"]);
    assert_eq!(chart.series.len(), 2);
    assert_eq!((chart.series[0].name.as_str(), chart.series[0].range.as_str()), ("Weight", "B2:B5"));
    assert_eq!((chart.series[1].name.as_str(), chart.series[1].range.as_str()), ("Age", "C2:C5"));
    // A theme colour has no portable value; the explicit one comes through.
    assert_eq!(chart.series[0].color, None);
    assert_eq!(chart.series[1].color.as_deref(), Some("#ED7D31"));
    assert_eq!(chart.series_values_cache, vec![vec![52.5, 60.0, 71.0, 80.0], vec![]]);
    assert_eq!(chart.x_title, "Height (cm)");
    assert_eq!(chart.y_title, "Weight (kg)");
    assert!(chart.legend);
    assert_eq!(chart.scatter_style, None, "hidden lines are a markers-only scatter");
}

#[test]
fn reads_scatter_series_that_bring_their_own_x_values_with_a_warning() {
    let mut workbook = scatter_workbook();
    workbook.sheets[0].charts.push(scatter_chart(None));
    let bytes = xlsx::write_xlsx(&workbook).unwrap();
    let foreign = EXCEL_SCATTER.replacen(
        "Data!$A$2:$A$5</c:f></c:numRef></c:xVal>",
        "Data!$C$2:$C$5</c:f></c:numRef></c:xVal>",
        1,
    );
    let read = xlsx::read_workbook_bytes(&rebuild(&bytes, &[("xl/charts/chart1.xml", &foreign)])).unwrap();
    assert_eq!(first_chart(&read.workbook).categories, "A2:A5");
    assert!(read.warnings.iter().any(|warning| warning.contains("own X values")), "{:?}", read.warnings);
}

#[test]
fn reads_scatter_lines_and_smoothing_from_the_series() {
    let mut workbook = scatter_workbook();
    workbook.sheets[0].charts.push(scatter_chart(None));
    let bytes = xlsx::write_xlsx(&workbook).unwrap();
    // "Scatter with smooth lines": visible lines, no markers, smoothing on.
    let smooth = EXCEL_SCATTER
        .replace(
            "<a:ln w=\"19050\" cap=\"rnd\"><a:noFill/><a:round/></a:ln>",
            "<a:ln w=\"28575\" cap=\"rnd\"><a:solidFill><a:srgbClr val=\"4472C4\"/></a:solidFill><a:round/></a:ln>",
        )
        .replace("<c:marker><c:symbol val=\"circle\"/>", "<c:marker><c:symbol val=\"none\"/>")
        .replace("<c:marker><c:symbol val=\"square\"/>", "<c:marker><c:symbol val=\"none\"/>")
        .replace("<c:scatterStyle val=\"lineMarker\"/>", "<c:scatterStyle val=\"smoothMarker\"/>")
        .replace("<c:smooth val=\"0\"/>", "<c:smooth val=\"1\"/>");
    let read = xlsx::read_workbook_bytes(&rebuild(&bytes, &[("xl/charts/chart1.xml", &smooth)])).unwrap();
    let chart = first_chart(&read.workbook);
    assert_eq!(chart.scatter_style.as_deref(), Some("smooth"));
    assert_eq!(chart.series[0].color.as_deref(), Some("#4472C4"));
}

#[test]
fn reads_the_doughnut_chart_excel_writes() {
    let mut workbook = scatter_workbook();
    workbook.sheets[0].charts.push(doughnut_chart(None));
    let bytes = xlsx::write_xlsx(&workbook).unwrap();
    let patched = rebuild(&bytes, &[("xl/charts/chart1.xml", EXCEL_DOUGHNUT)]);
    let read = xlsx::read_workbook_bytes(&patched).unwrap();
    assert!(read.warnings.iter().all(|warning| !warning.contains("limited support")), "{:?}", read.warnings);
    let chart = first_chart(&read.workbook);
    assert_eq!(chart.kind, "doughnut");
    assert_eq!(chart.hole_size, Some(75));
    assert_eq!(chart.categories, "A2:A4");
    assert_eq!(chart.series.len(), 1);
    assert_eq!(chart.series[0].range, "B2:B4");
    assert!(chart.show_labels && chart.legend);
    assert_eq!(chart.categories_cache.len(), 3);
}

// ---------------------------------------------------------------------------
// ODS
// ---------------------------------------------------------------------------

#[test]
fn ods_scatter_chart_round_trips_every_style() {
    for style in [None, Some("lineMarker"), Some("line"), Some("smoothMarker"), Some("smooth")] {
        let mut workbook = scatter_workbook();
        workbook.sheets[0].charts.push(scatter_chart(style));
        let bytes = odf::write_ods(&workbook).unwrap();
        let read = odf::read_ods(&bytes).unwrap();
        assert!(read.warnings.iter().all(|warning| !warning.contains("imported as")), "{:?}", read.warnings);
        let placement = read.workbook.sheets[0].charts.first().expect("scatter chart survives");
        assert_eq!(placement.anchor, "E2");
        let mut expected = workbook.sheets[0].charts[0].chart.clone();
        // The local table caches come back from the sheet cells.
        expected.categories_cache = vec!["150".into(), "160".into(), "172.5".into(), "181".into()];
        expected.series_values_cache = vec![vec![52.5, 60.0, 71.0, 80.0], vec![31.0, 35.0, 41.0, 29.0]];
        assert_eq!(placement.chart, expected, "style {style:?}");
    }
}

#[test]
fn ods_scatter_chart_uses_the_scatter_class_with_a_domain_for_the_x_values() {
    let mut workbook = scatter_workbook();
    workbook.sheets[0].charts.push(scatter_chart(Some("smoothMarker")));
    let reader = ZipReader::open(odf::write_ods(&workbook).unwrap()).unwrap();
    let chart = reader.read_text("Object 1/content.xml").unwrap();
    assert!(chart.contains("chart:class=\"chart:scatter\""), "{chart}");
    assert!(chart.contains("<chart:domain table:cell-range-address=\"Data.$A$2:Data.$A$5\"/>"), "{chart}");
    assert!(!chart.contains("<chart:categories"), "the X range is the domain, not categories");
    assert!(chart.contains("chart:interpolation=\"cubic-spline\""));
    assert!(chart.contains("chart:symbol-type=\"automatic\""));
    // The local table carries the X values as numbers.
    assert!(chart.contains("office:value-type=\"float\" office:value=\"172.5\""));
}

#[test]
fn ods_doughnut_chart_round_trips_as_a_ring() {
    let mut workbook = scatter_workbook();
    workbook.sheets[0].charts.push(doughnut_chart(None));
    let bytes = odf::write_ods(&workbook).unwrap();
    let chart = ZipReader::open(bytes.clone()).unwrap().read_text("Object 1/content.xml").unwrap();
    assert!(chart.contains("chart:class=\"chart:ring\""), "{chart}");
    let read = odf::read_ods(&bytes).unwrap();
    assert!(read.warnings.iter().all(|warning| !warning.contains("imported as")), "{:?}", read.warnings);
    let back = first_chart(&read.workbook);
    assert_eq!(back.kind, "doughnut");
    assert_eq!(back.title, "Share");
    assert_eq!(back.categories, "A2:A4");
    assert_eq!(back.series, workbook.sheets[0].charts[0].chart.series);
    assert!(back.show_labels && back.legend);
}

#[test]
fn ods_custom_hole_size_is_an_xlsx_and_oswk_feature_only() {
    let mut workbook = scatter_workbook();
    workbook.sheets[0].charts.push(doughnut_chart(Some(80)));
    let read = odf::read_ods(&odf::write_ods(&workbook).unwrap()).unwrap();
    assert_eq!(first_chart(&read.workbook).hole_size, None);
}

/// A scatter chart object as LibreOffice 7 writes it: styles with the stroke
/// and symbol, the X values as the series domain and a label cell per series.
fn libreoffice_scatter_package() -> Vec<u8> {
    let content = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
        "<office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
        "xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" ",
        "xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" ",
        "xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" ",
        "xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\" ",
        "xmlns:xlink=\"http://www.w3.org/1999/xlink\" office:version=\"1.3\">",
        "<office:body><office:spreadsheet><table:table table:name=\"Sheet1\">",
        "<table:table-row><table:table-cell office:value-type=\"string\"><text:p>x</text:p></table:table-cell>",
        "<table:table-cell office:value-type=\"string\"><text:p>y</text:p></table:table-cell></table:table-row>",
        "<table:table-row><table:table-cell office:value-type=\"float\" office:value=\"1\"><text:p>1</text:p></table:table-cell>",
        "<table:table-cell office:value-type=\"float\" office:value=\"2\"><text:p>2</text:p></table:table-cell>",
        "<table:table-cell><draw:frame draw:name=\"Object 1\" svg:width=\"14cm\" svg:height=\"8cm\" svg:x=\"0cm\" svg:y=\"0cm\">",
        "<draw:object xlink:href=\"./Object 1\"/></draw:frame></table:table-cell></table:table-row>",
        "<table:table-row><table:table-cell office:value-type=\"float\" office:value=\"2\"><text:p>2</text:p></table:table-cell>",
        "<table:table-cell office:value-type=\"float\" office:value=\"4\"><text:p>4</text:p></table:table-cell></table:table-row>",
        "</table:table></office:spreadsheet></office:body></office:document-content>"
    );
    let object = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
        "<office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
        "xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" ",
        "xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" ",
        "xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" ",
        "xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\" ",
        "xmlns:chart=\"urn:oasis:names:tc:opendocument:xmlns:chart:1.0\" ",
        "xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" office:version=\"1.3\">",
        "<office:automatic-styles>",
        "<style:style style:name=\"ch1\" style:family=\"chart\"><style:chart-properties chart:interpolation=\"none\"/></style:style>",
        "<style:style style:name=\"ch6\" style:family=\"chart\"><style:chart-properties chart:symbol-type=\"automatic\"/>",
        "<style:graphic-properties draw:stroke=\"none\" svg:stroke-color=\"#004586\" draw:fill-color=\"#004586\"/></style:style>",
        "</office:automatic-styles><office:body><office:chart>",
        "<chart:chart svg:width=\"14cm\" svg:height=\"8cm\" chart:class=\"chart:scatter\">",
        "<chart:title><text:p>Points</text:p></chart:title><chart:legend chart:legend-position=\"end\"/>",
        "<chart:plot-area chart:style-name=\"ch1\" table:cell-range-address=\"Sheet1.A1:Sheet1.B3\">",
        "<chart:axis chart:dimension=\"x\" chart:name=\"primary-x\"><chart:title><text:p>Input</text:p></chart:title></chart:axis>",
        "<chart:axis chart:dimension=\"y\" chart:name=\"primary-y\"><chart:title><text:p>Output</text:p></chart:title><chart:grid chart:class=\"major\"/></chart:axis>",
        "<chart:series chart:style-name=\"ch6\" chart:values-cell-range-address=\"Sheet1.$B$2:.$B$3\" chart:label-cell-address=\"Sheet1.$B$1\" chart:class=\"chart:scatter\">",
        "<chart:domain table:cell-range-address=\"Sheet1.$A$2:.$A$3\"/><chart:data-point chart:repeated=\"2\"/></chart:series>",
        "</chart:plot-area></chart:chart></office:chart></office:body></office:document-content>"
    );
    let mut zip = ZipWriter::new();
    zip.add_text("mimetype", "application/vnd.oasis.opendocument.spreadsheet");
    zip.add_text("content.xml", content);
    zip.add_text("Object 1/content.xml", object);
    zip.finish()
}

#[test]
fn reads_the_scatter_chart_libreoffice_writes() {
    let read = odf::read_ods(&libreoffice_scatter_package()).unwrap();
    assert!(read.warnings.iter().all(|warning| !warning.contains("imported as")), "{:?}", read.warnings);
    let sheet = &read.workbook.sheets[0];
    assert_eq!(sheet.charts.len(), 1, "{:?}", read.warnings);
    let placement = &sheet.charts[0];
    let chart = &placement.chart;
    assert_eq!(chart.kind, "scatter");
    assert_eq!(chart.title, "Points");
    assert_eq!(chart.categories, "A2:A3", "the series domain is the X range");
    assert_eq!(chart.series.len(), 1);
    assert_eq!(chart.series[0].range, "B2:B3");
    assert_eq!(chart.series[0].name, "y", "named by its label cell");
    assert_eq!(chart.series[0].color.as_deref(), Some("#004586"));
    assert_eq!(chart.x_title, "Input");
    assert_eq!(chart.y_title, "Output");
    assert_eq!(chart.scatter_style, None, "a stroke of none is markers only");
    assert_eq!(placement.anchor, "C2", "the frame sits in the third cell of the second row");
}

// ---------------------------------------------------------------------------
// Compatibility report
// ---------------------------------------------------------------------------

#[test]
fn compatibility_report_no_longer_loses_scatter_and_doughnut_charts_in_ods() {
    let mut workbook = scatter_workbook();
    workbook.sheets[0].charts.push(scatter_chart(None));
    workbook.sheets[0].charts.push(doughnut_chart(None));
    let report = compat::workbook_feature_report(&workbook, "ods");
    let charts = report.items.iter().find(|item| item.feature == "charts").expect("charts item");
    assert_eq!(charts.status, "unchanged", "{}", charts.message);
    assert!(!report.lossy());
    for format in ["xlsx", "ods"] {
        let capabilities = compat::format_capabilities(format);
        let note = &capabilities.features.iter().find(|feature| feature.feature == "charts").unwrap().note;
        assert!(note.contains("scatter") && note.contains("doughnut"), "{format}: {note}");
    }
}
