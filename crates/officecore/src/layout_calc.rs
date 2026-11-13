//! Calc spreadsheet PDF layout and tagged-PDF table export.

use crate::model::{CellValue, Sheet, Workbook};
use crate::pdfcanvas::{parse_hex, write_pdf, BuiltPage, Canvas, FontSet, Rgb, TextStyle};

fn column_width_px(sheet: &Sheet, column: u32) -> f64 {
    sheet.col_widths.get(&column).copied().unwrap_or(96.0)
}

fn row_height_px(sheet: &Sheet, row: u32) -> f64 {
    sheet.row_heights.get(&row).copied().unwrap_or(22.0)
}

fn format_cell_value(value: &CellValue) -> String {
    match value {
        CellValue::Empty => String::new(),
        CellValue::Number(number) => {
            if number.fract() == 0.0 && number.abs() < 1e15 {
                format!("{}", *number as i64)
            } else {
                let text = format!("{number:.4}");
                text.trim_end_matches('0').trim_end_matches('.').to_string()
            }
        }
        CellValue::Text(text) => text.clone(),
        CellValue::Bool(value) => if *value { "TRUE" } else { "FALSE" }.to_string(),
        CellValue::Error(error) => error.clone(),
    }
}

pub(crate) fn truncate_to_width(fonts: &FontSet, text: &str, style: &TextStyle, width: f64) -> String {
    if fonts.pick(style.bold).advance_pt(text, style.size_pt) <= width {
        return text.to_string();
    }
    let mut out = String::new();
    for ch in text.chars() {
        let candidate = format!("{out}{ch}");
        if fonts.pick(style.bold).advance_pt(&candidate, style.size_pt) > width - 8.0 {
            break;
        }
        out.push(ch);
    }
    out.push('…');
    out
}

/// Renders a workbook: every sheet becomes one or more landscape pages,
/// sliced into readable column strips with tagged table structure.
pub fn workbook_to_pdf(workbook: &Workbook, sheet_limit: usize) -> Vec<u8> {
    let fonts = FontSet::new();
    let mut pages: Vec<BuiltPage> = Vec::new();
    for sheet in workbook.sheets.iter().take(sheet_limit.max(1)) {
        pages.extend(render_sheet(&fonts, sheet, workbook));
    }
    if pages.is_empty() {
        let mut canvas = Canvas::new(842.0, 595.0, &fonts);
        canvas.begin_marked("P", None);
        canvas.text(40.0, 60.0, "Empty workbook", &TextStyle { size_pt: 16.0, ..Default::default() });
        canvas.end_marked();
        pages.push(canvas.finish());
    }
    write_pdf(&pages, &fonts, &workbook.title, "tr-TR")
}

fn render_sheet(fonts: &FontSet, sheet: &Sheet, workbook: &Workbook) -> Vec<BuiltPage> {
    let page_width = 842.0;
    let page_height = 595.0;
    let margin = 28.0;
    let content_top = margin + 40.0;
    let content_bottom = page_height - margin - 16.0;

    let mut max_row = 0u32;
    let mut max_col = 0u32;
    for address in sheet.cells.keys() {
        if let Some((row, column)) = crate::address::parse(address) {
            max_row = max_row.max(row);
            max_col = max_col.max(column);
        }
    }
    max_row = max_row.min(2_000);
    max_col = max_col.min(60);

    let col_widths: Vec<f64> = (0..=max_col).map(|column| column_width_px(sheet, column).min(220.0)).collect();
    let available_width = page_width - margin * 2.0;
    let mut strips: Vec<(u32, u32)> = Vec::new();
    let mut start = 0u32;
    let mut width = 0.0;
    for column in 0..=max_col {
        let column_width = col_widths[column as usize];
        if width + column_width > available_width && column > start {
            strips.push((start, column - 1));
            start = column;
            width = 0.0;
        }
        width += column_width;
    }
    strips.push((start, max_col));

    let title_style = TextStyle { size_pt: 13.0, bold: true, ..Default::default() };
    let small = TextStyle { size_pt: 8.5, color: Rgb(90, 100, 115), ..Default::default() };
    let header_style = TextStyle { size_pt: 8.5, bold: true, color: Rgb(60, 70, 85), ..Default::default() };
    let row_label_style = TextStyle { size_pt: 8.0, color: Rgb(100, 116, 139), ..Default::default() };

    let mut pages = Vec::new();
    for (strip_start, strip_end) in strips {
        let strip_width: f64 = (strip_start..=strip_end).map(|column| col_widths[column as usize]).sum();
        let mut row = 0u32;
        loop {
            let mut canvas = Canvas::new(page_width, page_height, fonts);
            canvas.begin_pagination_artifact("Header");
            canvas.text(margin, margin + 12.0, &sheet.name, &title_style);
            let sub = format!(
                "{} | columns {}-{}",
                workbook.title,
                crate::address::column_name(strip_start),
                crate::address::column_name(strip_end)
            );
            canvas.text(margin, margin + 26.0, &sub, &small);
            canvas.end_artifact();

            canvas.begin_struct("Table");
            canvas.begin_struct("TR");
            let mut x = margin;
            canvas.fill_rect(margin, content_top - 14.0, strip_width, 14.0, Rgb(241, 245, 249), 1.0);
            for column in strip_start..=strip_end {
                let width = col_widths[column as usize];
                if width > 0.0 {
                    canvas.begin_struct("TH");
                    canvas.begin_marked("P", None);
                    canvas.text(x + 3.0, content_top - 3.0, &crate::address::column_name(column), &header_style);
                    canvas.end_marked();
                    canvas.end_struct();
                }
                x += width;
            }
            canvas.end_struct();

            let mut y = content_top;
            let mut last_row = row;
            while y + row_height_px(sheet, last_row) <= content_bottom && last_row <= max_row {
                let row_height = row_height_px(sheet, last_row);
                if row_height <= 0.0 {
                    last_row += 1;
                    continue;
                }
                canvas.begin_struct("TR");
                let mut x = margin;
                if last_row % 2 == 1 {
                    canvas.fill_rect(margin, y, strip_width, row_height, Rgb(248, 250, 252), 1.0);
                }
                canvas.begin_struct("TH");
                canvas.begin_marked("P", None);
                canvas.text(margin - 20.0, y + row_height * 0.65, &(last_row + 1).to_string(), &row_label_style);
                canvas.end_marked();
                canvas.end_struct();

                for column in strip_start..=strip_end {
                    let width = col_widths[column as usize];
                    if width <= 0.0 {
                        continue;
                    }
                    canvas.begin_struct("TD");
                    let address = crate::address::format(last_row, column);
                    if let Some(cell) = sheet.cells.get(&address) {
                        let text = format_cell_value(&cell.value);
                        if !text.is_empty() {
                            let bold = cell.style.bold;
                            let color = cell.style.color.as_deref().and_then(parse_hex).unwrap_or(Rgb::BLACK);
                            let style = TextStyle {
                                size_pt: cell.style.size_pt.unwrap_or(9.0),
                                bold,
                                color,
                                ..Default::default()
                            };
                            let text_width = fonts.pick(bold).advance_pt(&text, style.size_pt);
                            let text_x = match cell.style.align.as_str() {
                                "center" => x + (width - text_width) / 2.0,
                                "right" => x + width - text_width - 3.0,
                                _ => x + 3.0,
                            };
                            canvas.begin_marked("P", None);
                            canvas.text(
                                text_x,
                                y + row_height * 0.68,
                                &truncate_to_width(fonts, &text, &style, width - 6.0),
                                &style,
                            );
                            canvas.end_marked();
                        } else if let Some(fill) = cell.style.fill.as_deref().and_then(parse_hex) {
                            canvas.fill_rect(x, y, width, row_height, fill, 1.0);
                        }
                    }
                    canvas.end_struct();
                    x += width;
                }
                canvas.end_struct();
                y += row_height;
                last_row += 1;
            }
            canvas.end_struct();

            canvas.begin_pagination_artifact("Footer");
            canvas.text(page_width - margin - 60.0, page_height - margin, &format!("Page {}", pages.len() + 1), &small);
            canvas.end_artifact();

            pages.push(canvas.finish());
            if last_row > max_row {
                break;
            }
            row = last_row;
        }
    }
    pages
}
