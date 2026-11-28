//! Column and horizontal bar chart renderers.

use super::cartesian::{draw_value_label, value_to_y, AxisScale, AXIS_COLOR, GRID_COLOR};
use super::resolve::{format_compact, ResolvedChart};
use super::{truncate_label, ChartRect};
use crate::model::ChartData;
use crate::pdfcanvas::{Canvas, FontSet, Rgb, TextStyle};

pub(crate) fn draw_bar_grid(
    canvas: &mut Canvas<'_>,
    fonts: &FontSet,
    area: ChartRect,
    x_scale: AxisScale,
    style: &TextStyle,
) {
    for tick in 0..=x_scale.steps {
        let value = x_scale.min + x_scale.step * tick as f64;
        let x = x_scale.map_to_range(value, area.x, area.w);
        let is_zero = value.abs() < x_scale.step * 0.01;
        canvas.line(x, area.y, x, area.y + area.h, if is_zero { AXIS_COLOR } else { GRID_COLOR }, 0.6, "");
        let label = format_compact(value);
        let lw = fonts.pick(false).advance_pt(&label, style.size_pt);
        canvas.text((x - lw / 2.0).max(area.x - 10.0), area.y + area.h + 11.0, &label, style);
    }
    canvas.line(area.x, area.y + area.h, area.x + area.w, area.y + area.h, AXIS_COLOR, 0.7, "");
}

pub(crate) fn draw_columns(
    canvas: &mut Canvas<'_>,
    fonts: &FontSet,
    area: ChartRect,
    chart: &ChartData,
    data: &ResolvedChart,
    y_scale: AxisScale,
) {
    let count = data.max_points();
    if count == 0 || data.series.is_empty() {
        return;
    }
    let slot_w = area.w / count as f64;
    let group_w = slot_w * 0.72;
    let group_left = (slot_w - group_w) / 2.0;
    let zero_y = value_to_y(y_scale, area, 0.0);

    for cat in 0..count {
        let mut pos_acc = 0.0;
        let mut neg_acc = 0.0;
        let n_series = data.series.len() as f64;
        let bar_w = if chart.stacked { group_w } else { (group_w / n_series).max(2.0) };
        let draw_w = (bar_w - if chart.stacked { 0.0 } else { 0.8 }).max(1.5);
        for (s_idx, series) in data.series.iter().enumerate() {
            let value = series.values.get(cat).copied().unwrap_or(0.0);
            let x = if chart.stacked {
                area.x + cat as f64 * slot_w + group_left
            } else {
                area.x + cat as f64 * slot_w + group_left + s_idx as f64 * bar_w
            };
            let (base, tip) = if chart.stacked {
                if value >= 0.0 {
                    let prev = pos_acc;
                    pos_acc += value;
                    (prev, pos_acc)
                } else {
                    let prev = neg_acc;
                    neg_acc += value;
                    (prev, neg_acc)
                }
            } else {
                (0.0, value)
            };
            let y1 = value_to_y(y_scale, area, base);
            let y2 = value_to_y(y_scale, area, tip);
            let top = y1.min(y2);
            let height = (y1 - y2).abs().max(if value != 0.0 { 1.0 } else { 0.0 });
            if height > 0.0 {
                canvas.fill_rect(x, top, draw_w, height, series.color, 1.0);
            }
            if chart.show_labels && value != 0.0 && (!chart.stacked || height >= 10.0) {
                let ly = if chart.stacked {
                    top + height / 2.0 + 2.5
                } else if value >= 0.0 {
                    top - 2.5
                } else {
                    top + height + 7.5
                };
                draw_value_label(canvas, fonts, x + bar_w / 2.0, ly, value);
            }
        }
    }
    canvas.line(area.x, zero_y, area.x + area.w, zero_y, AXIS_COLOR, 0.7, "");
}

pub(crate) fn draw_bars_horizontal(
    canvas: &mut Canvas<'_>,
    fonts: &FontSet,
    area: ChartRect,
    chart: &ChartData,
    data: &ResolvedChart,
    x_scale: AxisScale,
    tick_style: &TextStyle,
) {
    let count = data.max_points();
    if count == 0 || data.series.is_empty() {
        return;
    }
    let slot_h = area.h / count as f64;
    let group_h = slot_h * 0.72;
    let group_top = (slot_h - group_h) / 2.0;
    let zero_x = x_scale.map_to_range(0.0, area.x, area.w);
    let label_style = TextStyle { size_pt: 6.5, color: Rgb(51, 65, 85), ..Default::default() };

    for cat in 0..count {
        let raw = data.categories.get(cat).map(String::as_str).unwrap_or("");
        let max_cat_w = (area.x - 10.0).max(16.0);
        let label = truncate_label(fonts, raw, tick_style, max_cat_w);
        let lw = fonts.pick(false).advance_pt(&label, tick_style.size_pt);
        let cy = area.y + (cat as f64 + 0.5) * slot_h;
        canvas.text((area.x - lw - 5.0).max(2.0), cy + 2.5, &label, tick_style);

        let mut pos_acc = 0.0;
        let mut neg_acc = 0.0;
        let bar_h = if chart.stacked { group_h } else { (group_h / data.series.len() as f64).max(2.0) };
        let draw_h = (bar_h - if chart.stacked { 0.0 } else { 0.8 }).max(1.5);
        for (s_idx, series) in data.series.iter().enumerate() {
            let value = series.values.get(cat).copied().unwrap_or(0.0);
            let y = if chart.stacked {
                area.y + cat as f64 * slot_h + group_top
            } else {
                area.y + cat as f64 * slot_h + group_top + s_idx as f64 * bar_h
            };
            let (base, tip) = if chart.stacked {
                if value >= 0.0 {
                    let prev = pos_acc;
                    pos_acc += value;
                    (prev, pos_acc)
                } else {
                    let prev = neg_acc;
                    neg_acc += value;
                    (prev, neg_acc)
                }
            } else {
                (0.0, value)
            };
            let x1 = x_scale.map_to_range(base, area.x, area.w);
            let x2 = x_scale.map_to_range(tip, area.x, area.w);
            let left = x1.min(x2);
            let width = (x1 - x2).abs().max(if value != 0.0 { 1.0 } else { 0.0 });
            if width > 0.0 {
                canvas.fill_rect(left, y, width, draw_h, series.color, 1.0);
            }
            if chart.show_labels && value != 0.0 {
                let text = format_compact(value);
                canvas.text(left + width + 3.0, y + bar_h / 2.0 + 2.0, &text, &label_style);
            }
        }
    }
    canvas.line(zero_x, area.y, zero_x, area.y + area.h, AXIS_COLOR, 0.7, "");
}
