//! Writer PDF vector chart layout and caption rendering.

use crate::model::ChartData;
use crate::pdfcanvas::{Canvas, FontSet, Rgb, TextStyle};
use crate::pdfchart::{draw_chart, ChartRect};

pub(crate) fn clamp_chart_box(width_pt: f64, height_pt: f64, max_w: f64, max_h: f64) -> (f64, f64) {
    let mut w = width_pt.max(72.0);
    let mut h = height_pt.max(48.0);
    let aspect = h / w;
    if w > max_w && max_w > 16.0 {
        w = max_w;
        h = (w * aspect).max(40.0);
    }
    if h > max_h && max_h > 16.0 {
        h = max_h;
        w = (h / aspect.max(0.1)).min(max_w.max(16.0));
    }
    (w, h)
}

pub(crate) fn caption_extra_height(caption: &str) -> f64 {
    if caption.trim().is_empty() {
        0.0
    } else {
        15.0
    }
}

pub(crate) fn chart_x(align: &str, col_x: f64, col_w: f64, chart_w: f64) -> f64 {
    match align {
        "center" => col_x + (col_w - chart_w) / 2.0,
        "right" => col_x + col_w - chart_w,
        _ => col_x,
    }
}

pub(crate) fn measure_chart_box(
    width_pt: f64,
    height_pt: f64,
    max_w: f64,
    max_page_h: f64,
    caption: &str,
) -> (f64, f64, f64) {
    let extra = caption_extra_height(caption);
    let (w, h) = clamp_chart_box(width_pt, height_pt, max_w, (max_page_h - extra).max(40.0));
    (w, h, h + extra)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_cell_chart(
    canvas: &mut Canvas<'_>,
    fonts: &FontSet,
    chart: &ChartData,
    x: f64,
    y: f64,
    cell_w: f64,
    width_pt: f64,
    height_pt: f64,
    caption: &str,
) -> f64 {
    let (w, h, total_h) = measure_chart_box(width_pt, height_pt, cell_w.max(36.0), 400.0, caption);
    render_writer_chart(canvas, fonts, chart, x, y, w.min(cell_w), h, caption);
    total_h + 6.0
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_writer_chart(
    canvas: &mut Canvas<'_>,
    fonts: &FontSet,
    chart: &ChartData,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    caption: &str,
) {
    let title = chart.title.trim();
    let alt = if !caption.trim().is_empty() {
        caption.trim().to_string()
    } else if !title.is_empty() {
        title.to_string()
    } else {
        format!("{} chart", chart.kind)
    };
    canvas.begin_marked("Figure", Some(&alt));
    draw_chart(canvas, fonts, ChartRect { x, y, w: width, h: height }, chart, None);
    canvas.end_marked();

    let trimmed = caption.trim();
    if !trimmed.is_empty() {
        let style = TextStyle { size_pt: 9.0, italic: true, color: Rgb(100, 116, 139), ..Default::default() };
        let clipped = crate::layout_calc::truncate_to_width(fonts, trimmed, &style, width);
        let tw = fonts.pick(false).advance_pt(&clipped, style.size_pt);
        let cx = x + ((width - tw) / 2.0).max(0.0);
        canvas.begin_marked("Caption", None);
        canvas.text(cx, y + height + 11.0, &clipped, &style);
        canvas.end_marked();
    }
}
