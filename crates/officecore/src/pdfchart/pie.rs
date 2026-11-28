//! Pie and doughnut chart vector renderer.

use crate::model::ChartData;
use crate::pdfcanvas::{Canvas, FontSet, Rgb, TextStyle};
use std::f64::consts::{FRAC_PI_2, TAU};
use super::ChartRect;
use super::resolve::{ResolvedChart, CHART_PALETTE};

fn is_dark(color: Rgb) -> bool {
    (0.299 * color.0 as f64 + 0.587 * color.1 as f64 + 0.114 * color.2 as f64) < 150.0
}

pub(crate) fn draw_pie(
    canvas: &mut Canvas<'_>,
    fonts: &FontSet,
    plot: ChartRect,
    chart: &ChartData,
    data: &ResolvedChart,
    is_doughnut: bool,
) {
    let Some(first_series) = data.series.first() else { return };
    let values: Vec<f64> = first_series.values.iter().map(|v| v.abs()).collect();
    let total: f64 = values.iter().sum();
    if total <= 0.0 {
        return;
    }

    let cx = plot.x + plot.w / 2.0;
    let cy = plot.y + plot.h / 2.0;
    let radius = (plot.w.min(plot.h) * 0.43).max(14.0);
    let inner_ratio = if is_doughnut { chart.hole_size.unwrap_or(50).clamp(10, 90) as f64 / 100.0 } else { 0.0 };
    let inner_r = radius * inner_ratio;

    let mut angle = -FRAC_PI_2;
    let mut label_positions: Vec<(f64, f64, String, Rgb)> = Vec::new();

    for (index, &value) in values.iter().enumerate() {
        if value <= 0.0 {
            continue;
        }
        let fraction = value / total;
        let sweep = fraction * TAU;
        let color = CHART_PALETTE[index % CHART_PALETTE.len()];

        if fraction >= 0.9999 {
            canvas.ellipse(cx - radius, cy - radius, radius * 2.0, radius * 2.0, Some(color), None);
        } else {
            let steps = ((fraction * 72.0).ceil() as usize).max(6);
            let mut polygon = Vec::with_capacity(steps + 2);
            polygon.push((cx, cy));
            for step in 0..=steps {
                let theta = angle + sweep * (step as f64 / steps as f64);
                polygon.push((cx + radius * theta.cos(), cy + radius * theta.sin()));
            }
            canvas.polygon(&polygon, Some(color), None);
            canvas.polygon(&polygon, None, Some((Rgb::WHITE, 1.0)));
        }

        if chart.show_labels && fraction >= 0.05 {
            let mid = angle + sweep / 2.0;
            let label_r = if is_doughnut { (radius + inner_r) / 2.0 } else { radius * 0.64 };
            let text_color = if is_dark(color) { Rgb::WHITE } else { Rgb(15, 23, 42) };
            let pct = format!("{}%", (fraction * 100.0).round() as u32);
            label_positions.push((cx + label_r * mid.cos(), cy + label_r * mid.sin(), pct, text_color));
        }
        angle += sweep;
    }

    if is_doughnut && inner_r > 2.0 {
        let ring = (Rgb(226, 232, 240), 0.6);
        canvas.ellipse(cx - inner_r, cy - inner_r, inner_r * 2.0, inner_r * 2.0, Some(Rgb::WHITE), None);
        canvas.ellipse(cx - inner_r, cy - inner_r, inner_r * 2.0, inner_r * 2.0, None, Some(ring));
    }

    for (lx, ly, text, color) in label_positions {
        let style = TextStyle { size_pt: 7.5, bold: true, color, ..Default::default() };
        let lw = fonts.pick(true).advance_pt(&text, style.size_pt);
        canvas.text(lx - lw / 2.0, ly + 2.5, &text, &style);
    }
}
