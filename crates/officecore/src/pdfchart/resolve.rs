//! Data resolution and numeric formatting helpers for vector PDF charts.

use crate::model::{Cell, CellValue, ChartData, Sheet};
use crate::pdfcanvas::{parse_hex, Rgb};

pub(crate) const CHART_PALETTE: [Rgb; 8] = [
    Rgb(37, 99, 235),  // blue-600
    Rgb(22, 163, 74),  // green-600
    Rgb(234, 88, 12),  // orange-600
    Rgb(147, 51, 234), // purple-600
    Rgb(8, 145, 178),  // cyan-600
    Rgb(220, 38, 38),  // red-600
    Rgb(202, 138, 4),  // yellow-600
    Rgb(79, 70, 229),  // indigo-600
];

#[derive(Debug, Clone)]
pub(crate) struct ResolvedSeries {
    pub(crate) name: String,
    pub(crate) color: Rgb,
    pub(crate) values: Vec<f64>,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedChart {
    pub(crate) categories: Vec<String>,
    pub(crate) x_numbers: Option<Vec<f64>>,
    pub(crate) series: Vec<ResolvedSeries>,
}

impl ResolvedChart {
    pub(crate) fn has_drawable_data(&self, kind: &str) -> bool {
        if kind == "pie" || kind == "doughnut" {
            let Some(first) = self.series.first() else { return false };
            return !first.values.is_empty() && first.values.iter().map(|v| v.abs()).sum::<f64>() > 0.0;
        }
        self.series.iter().any(|series| !series.values.is_empty())
    }

    pub(crate) fn max_points(&self) -> usize {
        self.series.iter().map(|series| series.values.len()).max().unwrap_or(0)
    }
}

fn strip_sheet_prefix(range: &str) -> &str {
    let trimmed = range.trim();
    match trimmed.rsplit_once('!') {
        Some((_, local)) => local.trim(),
        None => trimmed,
    }
}

fn cell_as_label(cell: &Cell) -> String {
    match &cell.value {
        CellValue::Empty => String::new(),
        CellValue::Number(value) => format_compact(*value),
        CellValue::Text(text) => text.clone(),
        CellValue::Bool(flag) => if *flag { "TRUE" } else { "FALSE" }.to_string(),
        CellValue::Error(error) => error.clone(),
    }
}

fn read_sheet_labels(sheet: &Sheet, range: &str) -> Vec<String> {
    let local = strip_sheet_prefix(range);
    if local.is_empty() {
        return Vec::new();
    }
    let addresses = crate::address::expand_range(local, 512);
    if addresses.is_empty() {
        return Vec::new();
    }
    addresses.into_iter().map(|addr| sheet.cells.get(&addr).map(cell_as_label).unwrap_or_default()).collect()
}

fn read_sheet_numbers(sheet: &Sheet, range: &str) -> Vec<f64> {
    let local = strip_sheet_prefix(range);
    if local.is_empty() {
        return Vec::new();
    }
    let addresses = crate::address::expand_range(local, 512);
    if addresses.is_empty() {
        return Vec::new();
    }
    let mut saw_number = false;
    let mut values = Vec::with_capacity(addresses.len());
    for addr in addresses {
        match sheet.cells.get(&addr).map(|cell| &cell.value) {
            Some(CellValue::Number(value)) if value.is_finite() => {
                saw_number = true;
                values.push(*value);
            }
            _ => values.push(0.0),
        }
    }
    if !saw_number {
        return Vec::new();
    }
    values
}

pub(crate) fn resolve_chart(chart: &ChartData, sheet: Option<&Sheet>) -> ResolvedChart {
    let mut resolved_series = Vec::with_capacity(chart.series.len());
    for (index, entry) in chart.series.iter().enumerate() {
        let cached = chart.series_values_cache.get(index).filter(|vals| !vals.is_empty());
        let values = match cached {
            Some(vals) => vals.iter().map(|v| if v.is_finite() { *v } else { 0.0 }).collect(),
            None => sheet.map_or_else(Vec::new, |sheet_ref| read_sheet_numbers(sheet_ref, &entry.range)),
        };
        let color = entry.color.as_deref().and_then(parse_hex).unwrap_or(CHART_PALETTE[index % CHART_PALETTE.len()]);
        let name = if entry.name.trim().is_empty() { format!("Series {}", index + 1) } else { entry.name.clone() };
        resolved_series.push(ResolvedSeries { name, color, values });
    }

    let max_len = resolved_series.iter().map(|series| series.values.len()).max().unwrap_or(0);
    let mut categories = if !chart.categories_cache.is_empty() {
        chart.categories_cache.clone()
    } else if let Some(sheet_ref) = sheet {
        read_sheet_labels(sheet_ref, &chart.categories)
    } else {
        Vec::new()
    };
    while categories.len() < max_len {
        categories.push((categories.len() + 1).to_string());
    }

    let x_numbers = if chart.kind == "scatter" && max_len > 0 {
        let parsed: Vec<f64> = categories
            .iter()
            .take(max_len)
            .filter_map(|label| label.trim().parse::<f64>().ok().filter(|v| v.is_finite()))
            .collect();
        if parsed.len() == max_len {
            Some(parsed)
        } else {
            Some((1..=max_len).map(|i| i as f64).collect())
        }
    } else {
        None
    };

    ResolvedChart { categories, x_numbers, series: resolved_series }
}

pub(crate) fn format_compact(value: f64) -> String {
    if !value.is_finite() {
        return "0".into();
    }
    let clean = if value.abs() < 1e-9 { 0.0 } else { value };
    if clean.fract().abs() < 1e-6 && clean.abs() < 1e12 {
        return format!("{}", clean.round() as i64);
    }
    let formatted = format!("{clean:.2}");
    formatted.trim_end_matches('0').trim_end_matches('.').to_string()
}
