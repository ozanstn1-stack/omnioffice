//! Vector PDF chart renderer shared by Impress, Calc, and Writer PDF exports.

mod bars;
mod cartesian;
mod pie;
mod resolve;

use crate::layout_calc::truncate_to_width;
use crate::model::{ChartData, Sheet};
use crate::pdfcanvas::{Canvas, FontSet, Rgb, TextStyle};
use resolve::{resolve_chart, ResolvedChart, CHART_PALETTE};

#[derive(Debug, Clone, Copy)]
pub struct ChartRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

pub(crate) fn blend_white(color: Rgb, alpha: f64) -> Rgb {
    let a = alpha.clamp(0.0, 1.0);
    let mix = |channel: u8| ((channel as f64) * a + 255.0 * (1.0 - a)).round().clamp(0.0, 255.0) as u8;
    Rgb(mix(color.0), mix(color.1), mix(color.2))
}

pub(crate) fn truncate_label(fonts: &FontSet, text: &str, style: &TextStyle, max_width: f64) -> String {
    if max_width <= 8.0 {
        return String::new();
    }
    truncate_to_width(fonts, text, style, max_width)
}

fn legend_entries(chart: &ChartData, data: &ResolvedChart) -> Vec<(String, Rgb)> {
    if chart.kind == "pie" || chart.kind == "doughnut" {
        let count = data.series.first().map(|s| s.values.len()).unwrap_or(0);
        return (0..count)
            .map(|i| {
                let label = data.categories.get(i).cloned().unwrap_or_else(|| (i + 1).to_string());
                (label, CHART_PALETTE[i % CHART_PALETTE.len()])
            })
            .collect();
    }
    data.series.iter().map(|series| (series.name.clone(), series.color)).collect()
}

fn draw_legend(canvas: &mut Canvas<'_>, fonts: &FontSet, rect: ChartRect, entries: &[(String, Rgb)]) -> f64 {
    if entries.is_empty() {
        return 0.0;
    }
    let style = TextStyle { size_pt: 7.5, color: Rgb(71, 85, 105), ..Default::default() };
    let face = fonts.pick(false);
    let max_x = rect.x + rect.w - 10.0;

    let mut rows = 1usize;
    let mut cx = rect.x + 10.0;
    let mut placements: Vec<(usize, f64, String, Rgb)> = Vec::new();
    for (label, color) in entries {
        let short = truncate_label(fonts, label, &style, (rect.w * 0.4).max(40.0));
        let item_w = 10.0 + face.advance_pt(&short, style.size_pt) + 12.0;
        if cx + item_w > max_x && cx > rect.x + 10.0 {
            if rows >= 2 {
                break;
            }
            rows += 1;
            cx = rect.x + 10.0;
        }
        placements.push((rows - 1, cx, short, *color));
        cx += item_w;
    }

    let legend_h = rows as f64 * 12.0 + 6.0;
    let start_y = rect.y + rect.h - legend_h + 2.0;
    for (row_idx, px, label, color) in placements {
        let y = start_y + row_idx as f64 * 12.0;
        canvas.fill_rect(px, y, 6.5, 6.5, color, 1.0);
        canvas.text(px + 9.5, y + 6.0, &label, &style);
    }
    legend_h
}

/// Renders `chart` inside `rect` as vector PDF geometry. Callers wrap each
/// chart in a Tagged-PDF `Figure` marked-content block when tagged output applies.
pub fn draw_chart(canvas: &mut Canvas<'_>, fonts: &FontSet, rect: ChartRect, chart: &ChartData, sheet: Option<&Sheet>) {
    canvas.fill_rect(rect.x, rect.y, rect.w, rect.h, Rgb::WHITE, 1.0);
    canvas.stroke_rect(rect.x, rect.y, rect.w, rect.h, Rgb(226, 232, 240), 0.6);

    let title = chart.title.trim();
    let title_h = if title.is_empty() {
        8.0
    } else {
        let title_style = TextStyle { size_pt: 11.0, bold: true, color: Rgb(15, 23, 42), ..Default::default() };
        let clipped = truncate_label(fonts, title, &title_style, (rect.w - 20.0).max(20.0));
        let tw = fonts.pick(true).advance_pt(&clipped, title_style.size_pt);
        let tx = rect.x + ((rect.w - tw) / 2.0).max(10.0);
        canvas.text(tx, rect.y + 18.0, &clipped, &title_style);
        26.0
    };

    let data = resolve_chart(chart, sheet);
    let legend_h = if chart.legend && data.has_drawable_data(&chart.kind) {
        draw_legend(canvas, fonts, rect, &legend_entries(chart, &data))
    } else {
        6.0
    };

    let plot = ChartRect {
        x: rect.x + 8.0,
        y: rect.y + title_h,
        w: (rect.w - 16.0).max(0.0),
        h: (rect.h - title_h - legend_h).max(0.0),
    };

    if !data.has_drawable_data(&chart.kind) || plot.w < 36.0 || plot.h < 28.0 {
        let note_style = TextStyle { size_pt: 9.0, color: Rgb(100, 116, 139), ..Default::default() };
        let empty_text = format!("{} — no numerical data", chart.kind);
        let note = truncate_label(fonts, &empty_text, &note_style, (rect.w - 20.0).max(20.0));
        let nw = fonts.pick(false).advance_pt(&note, note_style.size_pt);
        canvas.text(rect.x + ((rect.w - nw) / 2.0).max(10.0), rect.y + rect.h / 2.0, &note, &note_style);
        return;
    }

    match chart.kind.as_str() {
        "pie" => pie::draw_pie(canvas, fonts, plot, chart, &data, false),
        "doughnut" => pie::draw_pie(canvas, fonts, plot, chart, &data, true),
        _ => cartesian::draw_cartesian(canvas, fonts, plot, chart, &data),
    }
}
