//! Calc sheet chart page renderer (vector charts following each sheet's table strips).

use crate::model::{Sheet, Workbook};
use crate::pdfcanvas::{BuiltPage, Canvas, FontSet, Rgb, TextStyle};
use crate::pdfchart::{draw_chart, ChartRect};

pub(crate) fn render_sheet_charts(
    fonts: &FontSet,
    sheet: &Sheet,
    workbook: &Workbook,
    start_page: usize,
) -> Vec<BuiltPage> {
    if sheet.charts.is_empty() {
        return Vec::new();
    }
    let page_width = 842.0;
    let page_height = 595.0;
    let margin = 28.0;
    let content_top = margin + 44.0;
    let content_h = page_height - content_top - margin - 24.0;
    let available_w = page_width - margin * 2.0;

    let title_style = TextStyle { size_pt: 13.0, bold: true, ..Default::default() };
    let small = TextStyle { size_pt: 8.5, color: Rgb(90, 100, 115), ..Default::default() };

    let mut pages = Vec::new();
    for (chunk_idx, pair) in sheet.charts.chunks(2).enumerate() {
        let mut canvas = Canvas::new(page_width, page_height, fonts);
        canvas.begin_pagination_artifact("Header");
        canvas.text(margin, margin + 12.0, &sheet.name, &title_style);
        let sub = format!("{} | Charts", workbook.title);
        canvas.text(margin, margin + 26.0, &sub, &small);
        canvas.end_artifact();

        let gap = 18.0;
        let card_w = if pair.len() == 1 { available_w } else { (available_w - gap) / 2.0 };
        for (slot, placement) in pair.iter().enumerate() {
            let x = margin + slot as f64 * (card_w + gap);
            let rect = ChartRect { x, y: content_top, w: card_w, h: content_h };
            let title = placement.chart.title.trim();
            let alt = if title.is_empty() { format!("{} chart", placement.chart.kind) } else { title.to_string() };
            canvas.begin_marked("Figure", Some(&alt));
            draw_chart(&mut canvas, fonts, rect, &placement.chart, Some(sheet));
            canvas.end_marked();
        }

        canvas.begin_pagination_artifact("Footer");
        let page_num = start_page + chunk_idx + 1;
        canvas.text(page_width - margin - 60.0, page_height - margin, &format!("Page {page_num}"), &small);
        canvas.end_artifact();

        pages.push(canvas.finish());
    }
    pages
}
