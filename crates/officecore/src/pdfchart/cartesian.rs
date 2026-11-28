//! Cartesian chart vector renderer: axes, grid, lines, area, and scatter.

use crate::model::ChartData;
use crate::pdfcanvas::{Canvas, FontSet, Rgb, TextStyle};
use super::bars::{draw_bar_grid, draw_bars_horizontal, draw_columns};
use super::resolve::{format_compact, ResolvedChart};
use super::{blend_white, truncate_label, ChartRect};

pub(crate) const AXIS_COLOR: Rgb = Rgb(148, 163, 184);
pub(crate) const GRID_COLOR: Rgb = Rgb(226, 232, 240);
const LABEL_COLOR: Rgb = Rgb(71, 85, 105);
const TITLE_COLOR: Rgb = Rgb(51, 65, 85);

#[derive(Debug, Clone, Copy)]
pub(crate) struct AxisScale {
    pub(crate) min: f64,
    pub(crate) max: f64,
    pub(crate) step: f64,
    pub(crate) steps: usize,
}

impl AxisScale {
    pub(crate) fn map_to_range(self, value: f64, start: f64, span: f64) -> f64 {
        let total = (self.max - self.min).max(1e-9);
        start + ((value - self.min) / total) * span
    }
}

fn nice_number(value: f64, round: bool) -> f64 {
    let abs = value.abs().max(1e-9);
    let exponent = abs.log10().floor();
    let base = 10f64.powf(exponent);
    let fraction = abs / base;
    let nice = if round {
        if fraction < 1.5 {
            1.0
        } else if fraction < 3.0 {
            2.0
        } else if fraction < 7.0 {
            5.0
        } else {
            10.0
        }
    } else if fraction <= 1.0 {
        1.0
    } else if fraction <= 2.0 {
        2.0
    } else if fraction <= 5.0 {
        5.0
    } else {
        10.0
    };
    nice * base
}

fn compute_scale(raw_min: f64, raw_max: f64, include_zero: bool) -> AxisScale {
    let mut min = if raw_min.is_finite() { raw_min } else { 0.0 };
    let mut max = if raw_max.is_finite() { raw_max } else { 1.0 };
    if include_zero {
        min = min.min(0.0);
        max = max.max(0.0);
    }
    if (max - min).abs() < 1e-9 {
        if min > 0.0 {
            min = 0.0;
        } else if max < 0.0 {
            max = 0.0;
        } else {
            max = 1.0;
        }
    }
    let span = nice_number(max - min, false);
    let step = nice_number(span / 4.0, true).max(1e-9);
    let nice_min = (min / step).floor() * step;
    let nice_max = (max / step).ceil() * step;
    let steps = (((nice_max - nice_min) / step).round() as usize).clamp(1, 8);
    AxisScale { min: nice_min, max: nice_min + step * steps as f64, step, steps }
}

fn y_bounds(chart: &ChartData, data: &ResolvedChart) -> (f64, f64) {
    let is_stacked = chart.stacked && matches!(chart.kind.as_str(), "column" | "bar" | "area");
    if !is_stacked {
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for series in &data.series {
            for &value in &series.values {
                min = min.min(value);
                max = max.max(value);
            }
        }
        return (min, max);
    }
    let count = data.max_points();
    let mut min = 0.0f64;
    let mut max = 0.0f64;
    for index in 0..count {
        let mut pos = 0.0;
        let mut neg = 0.0;
        for series in &data.series {
            let value = series.values.get(index).copied().unwrap_or(0.0);
            if value >= 0.0 {
                pos += value;
            } else {
                neg += value;
            }
        }
        max = max.max(pos);
        min = min.min(neg);
    }
    (min, max)
}

pub(crate) fn value_to_y(y_scale: AxisScale, area: ChartRect, value: f64) -> f64 {
    area.y + area.h - y_scale.map_to_range(value, 0.0, area.h)
}

pub(crate) fn draw_value_label(canvas: &mut Canvas<'_>, fonts: &FontSet, cx: f64, baseline_y: f64, value: f64) {
    let style = TextStyle { size_pt: 6.5, color: TITLE_COLOR, ..Default::default() };
    let text = format_compact(value);
    let lw = fonts.pick(false).advance_pt(&text, style.size_pt);
    canvas.text(cx - lw / 2.0, baseline_y, &text, &style);
}

pub(crate) fn draw_cartesian(
    canvas: &mut Canvas<'_>,
    fonts: &FontSet,
    plot: ChartRect,
    chart: &ChartData,
    data: &ResolvedChart,
) {
    let is_bar = chart.kind == "bar";
    let (raw_min, raw_max) = y_bounds(chart, data);
    let y_scale = compute_scale(raw_min, raw_max, chart.kind != "scatter");
    let tick_style = TextStyle { size_pt: 7.5, color: LABEL_COLOR, ..Default::default() };
    let title_style = TextStyle { size_pt: 8.0, bold: true, color: TITLE_COLOR, ..Default::default() };

    let left_pad = if is_bar {
        let widest = data
            .categories
            .iter()
            .take(data.max_points())
            .map(|cat| fonts.pick(false).advance_pt(cat, 7.5))
            .fold(0.0, f64::max);
        (widest + 10.0).clamp(36.0, (plot.w * 0.28).max(36.0))
    } else {
        let mut widest = 22.0f64;
        for tick in 0..=y_scale.steps {
            let label = format_compact(y_scale.min + y_scale.step * tick as f64);
            widest = widest.max(fonts.pick(false).advance_pt(&label, 7.5));
        }
        (widest + 8.0).clamp(28.0, 62.0)
    };
    let top_pad = if chart.y_title.trim().is_empty() { 8.0 } else { 18.0 };
    let bottom_pad = 18.0 + if chart.x_title.trim().is_empty() { 0.0 } else { 13.0 };
    let area = ChartRect {
        x: plot.x + left_pad,
        y: plot.y + top_pad,
        w: (plot.w - left_pad - 10.0).max(20.0),
        h: (plot.h - top_pad - bottom_pad).max(20.0),
    };

    if !chart.y_title.trim().is_empty() {
        let label = truncate_label(fonts, chart.y_title.trim(), &title_style, area.w);
        canvas.text(area.x, plot.y + 10.0, &label, &title_style);
    }
    if !chart.x_title.trim().is_empty() {
        let label = truncate_label(fonts, chart.x_title.trim(), &title_style, area.w);
        let lw = fonts.pick(true).advance_pt(&label, title_style.size_pt);
        canvas.text(area.x + (area.w - lw) / 2.0, area.y + area.h + 24.0, &label, &title_style);
    }

    if is_bar {
        draw_bar_grid(canvas, fonts, area, y_scale, &tick_style);
        draw_bars_horizontal(canvas, fonts, area, chart, data, y_scale, &tick_style);
        return;
    }

    draw_vertical_grid(canvas, fonts, area, y_scale, &tick_style);
    if chart.kind == "scatter" {
        draw_scatter(canvas, fonts, area, chart, data, y_scale, &tick_style);
        return;
    }

    draw_x_categories(canvas, fonts, area, &data.categories, data.max_points(), &tick_style);
    match chart.kind.as_str() {
        "line" => draw_lines(canvas, fonts, area, chart, data, y_scale, false),
        "area" => draw_lines(canvas, fonts, area, chart, data, y_scale, true),
        _ => draw_columns(canvas, fonts, area, chart, data, y_scale),
    }
}

fn draw_vertical_grid(
    canvas: &mut Canvas<'_>,
    fonts: &FontSet,
    area: ChartRect,
    y_scale: AxisScale,
    style: &TextStyle,
) {
    for tick in 0..=y_scale.steps {
        let value = y_scale.min + y_scale.step * tick as f64;
        let y = value_to_y(y_scale, area, value);
        let is_zero = value.abs() < y_scale.step * 0.01;
        canvas.line(area.x, y, area.x + area.w, y, if is_zero { AXIS_COLOR } else { GRID_COLOR }, 0.6, "");
        let label = format_compact(value);
        let lw = fonts.pick(false).advance_pt(&label, style.size_pt);
        canvas.text((area.x - lw - 5.0).max(2.0), y + 2.5, &label, style);
    }
    canvas.line(area.x, area.y, area.x, area.y + area.h, AXIS_COLOR, 0.7, "");
}

fn draw_x_categories(
    canvas: &mut Canvas<'_>,
    fonts: &FontSet,
    area: ChartRect,
    categories: &[String],
    count: usize,
    style: &TextStyle,
) {
    if count == 0 {
        return;
    }
    let slot_w = area.w / count as f64;
    let stride = (24.0 / slot_w).ceil().max(1.0) as usize;
    let max_label_w = (slot_w * stride as f64 - 4.0).max(14.0);
    for index in (0..count).step_by(stride) {
        let raw = categories.get(index).map(String::as_str).unwrap_or("");
        let label = truncate_label(fonts, raw, style, max_label_w);
        let lw = fonts.pick(false).advance_pt(&label, style.size_pt);
        let cx = area.x + (index as f64 + 0.5) * slot_w;
        canvas.text((cx - lw / 2.0).max(area.x), area.y + area.h + 11.0, &label, style);
    }
}

fn draw_lines(
    canvas: &mut Canvas<'_>,
    fonts: &FontSet,
    area: ChartRect,
    chart: &ChartData,
    data: &ResolvedChart,
    y_scale: AxisScale,
    filled: bool,
) {
    let count = data.max_points();
    if count == 0 {
        return;
    }
    let slot_w = area.w / count as f64;
    let zero_y = value_to_y(y_scale, area, 0.0);
    let mut prev_stack = vec![0.0; count];

    canvas.clip_rect(area.x - 4.0, area.y - 4.0, area.w + 8.0, area.h + 8.0);
    for series in &data.series {
        let mut pts = Vec::with_capacity(series.values.len());
        let mut base_pts = Vec::with_capacity(series.values.len());
        let mut next_stack = prev_stack.clone();
        for (index, &raw) in series.values.iter().enumerate() {
            let x = area.x + (index as f64 + 0.5) * slot_w;
            let (base_v, top_v) = if filled && chart.stacked {
                let prev = prev_stack.get(index).copied().unwrap_or(0.0);
                let acc = prev + raw;
                if let Some(slot) = next_stack.get_mut(index) {
                    *slot = acc;
                }
                (prev, acc)
            } else {
                (0.0, raw)
            };
            pts.push((x, value_to_y(y_scale, area, top_v)));
            base_pts.push((x, value_to_y(y_scale, area, base_v)));
        }
        prev_stack = next_stack;

        if filled && pts.len() >= 2 {
            let mut poly = pts.clone();
            if chart.stacked {
                base_pts.reverse();
                poly.extend(base_pts);
            } else if let (Some(last), Some(first)) = (pts.last(), pts.first()) {
                poly.push((last.0, zero_y));
                poly.push((first.0, zero_y));
            }
            canvas.polygon(&poly, Some(blend_white(series.color, 0.28)), None);
        }
        for pair in pts.windows(2) {
            canvas.line(pair[0].0, pair[0].1, pair[1].0, pair[1].1, series.color, 1.6, "");
        }
        for (index, &(px, py)) in pts.iter().enumerate() {
            canvas.ellipse(px - 2.6, py - 2.6, 5.2, 5.2, Some(series.color), None);
            canvas.ellipse(px - 2.6, py - 2.6, 5.2, 5.2, None, Some((Rgb::WHITE, 0.7)));
            if chart.show_labels {
                let value = series.values.get(index).copied().unwrap_or(0.0);
                draw_value_label(canvas, fonts, px, py - 5.0, value);
            }
        }
    }
    canvas.restore();
}

fn draw_scatter(
    canvas: &mut Canvas<'_>,
    fonts: &FontSet,
    area: ChartRect,
    chart: &ChartData,
    data: &ResolvedChart,
    y_scale: AxisScale,
    tick_style: &TextStyle,
) {
    let x_vals = data.x_numbers.as_deref().unwrap_or(&[]);
    let (raw_xmin, raw_xmax) = if x_vals.is_empty() {
        (0.0, 1.0)
    } else {
        x_vals.iter().copied().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(v), b.max(v)))
    };
    let x_scale = compute_scale(raw_xmin, raw_xmax, false);
    draw_bar_grid(canvas, fonts, area, x_scale, tick_style);

    let (draw_line, draw_markers, smooth) = crate::xlsx::chart_write::scatter_flavour(chart.scatter_style.as_deref());
    canvas.clip_rect(area.x - 4.0, area.y - 4.0, area.w + 8.0, area.h + 8.0);
    for series in &data.series {
        let pts: Vec<(f64, f64)> = series
            .values
            .iter()
            .enumerate()
            .map(|(i, &vy)| {
                let vx = x_vals.get(i).copied().unwrap_or((i + 1) as f64);
                (x_scale.map_to_range(vx, area.x, area.w), value_to_y(y_scale, area, vy))
            })
            .collect();
        if draw_line && pts.len() >= 2 {
            if smooth {
                draw_smooth_polyline(canvas, &pts, series.color);
            } else {
                for pair in pts.windows(2) {
                    canvas.line(pair[0].0, pair[0].1, pair[1].0, pair[1].1, series.color, 1.5, "");
                }
            }
        }
        if draw_markers || !draw_line {
            for &(px, py) in &pts {
                canvas.ellipse(px - 2.8, py - 2.8, 5.6, 5.6, Some(series.color), None);
                canvas.ellipse(px - 2.8, py - 2.8, 5.6, 5.6, None, Some((Rgb::WHITE, 0.7)));
            }
        }
    }
    canvas.restore();
}

fn draw_smooth_polyline(canvas: &mut Canvas<'_>, pts: &[(f64, f64)], color: Rgb) {
    if pts.len() < 2 {
        return;
    }
    let mut curves = Vec::with_capacity(pts.len() - 1);
    for i in 0..(pts.len() - 1) {
        let p0 = if i == 0 { pts[0] } else { pts[i - 1] };
        let p1 = pts[i];
        let p2 = pts[i + 1];
        let p3 = if i + 2 < pts.len() { pts[i + 2] } else { p2 };
        let c1x = p1.0 + (p2.0 - p0.0) / 6.0;
        let c1y = p1.1 + (p2.1 - p0.1) / 6.0;
        let c2x = p2.0 - (p3.0 - p1.0) / 6.0;
        let c2y = p2.1 - (p3.1 - p1.1) / 6.0;
        curves.push((c1x, c1y, c2x, c2y, p2.0, p2.1));
    }
    canvas.bezier_path(pts[0], &curves, None, Some((color, 1.5)));
}
