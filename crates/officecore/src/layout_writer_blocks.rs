//! Writer image sizing and table grid layout helpers shared by PDF pagination.

use crate::model::{ImageData, TableCell, TableData};

pub(crate) fn image_box(
    image: &ImageData,
    width_pt: f64,
    height_pt: f64,
    max_w: f64,
    max_h: Option<f64>,
) -> (f64, f64) {
    let (pw, ph) = image.pixel_size();
    let aspect = if pw > 0 && ph > 0 { ph as f64 / pw as f64 } else { 0.66 };
    let mut w = width_pt.max(24.0);
    let mut h = height_pt.max(18.0);
    if w > max_w {
        w = max_w;
        h = w * aspect;
    }
    if let Some(cap) = max_h.filter(|cap| h > *cap) {
        h = cap;
        w = h / aspect.max(0.05);
    }
    (w, h)
}

pub(crate) fn table_grid_columns(table: &TableData) -> usize {
    let mut open: Vec<usize> = Vec::new();
    let mut columns = 0usize;
    for row in &table.rows {
        for remaining in open.iter_mut() {
            *remaining = remaining.saturating_sub(1);
        }
        let mut column = 0usize;
        for cell in &row.cells {
            while column < open.len() && open[column] > 0 {
                column += 1;
            }
            let span = cell.colspan.max(1) as usize;
            while open.len() < column + span {
                open.push(0);
            }
            if cell.rowspan > 1 {
                for slot in open.iter_mut().skip(column).take(span) {
                    *slot = cell.rowspan as usize;
                }
            }
            column += span;
        }
        while column < open.len() && open[column] > 0 {
            column += 1;
        }
        columns = columns.max(column);
    }
    columns.max(1)
}

pub(crate) fn table_column_widths(table: &TableData, columns: usize, available: f64) -> Vec<f64> {
    let mut widths: Vec<f64> = table.column_widths_pt.clone();
    let fallback =
        if widths.is_empty() { available / columns as f64 } else { widths.iter().sum::<f64>() / widths.len() as f64 };
    while widths.len() < columns {
        widths.push(fallback);
    }
    widths.truncate(columns);
    for width in widths.iter_mut() {
        if !width.is_finite() || *width < 1.0 {
            *width = 1.0;
        }
    }
    let total: f64 = widths.iter().sum();
    if total > 0.0 && (total - available).abs() > 1.0 {
        let scale = available / total;
        for width in widths.iter_mut() {
            *width *= scale;
        }
    }
    widths
}

pub(crate) fn table_cell_layout<'a>(table: &'a TableData, widths: &[f64]) -> Vec<Vec<(f64, f64, &'a TableCell)>> {
    let mut open: Vec<usize> = Vec::new();
    let mut rows = Vec::with_capacity(table.rows.len());
    for row in &table.rows {
        for remaining in open.iter_mut() {
            *remaining = remaining.saturating_sub(1);
        }
        let mut column = 0usize;
        let mut cells = Vec::with_capacity(row.cells.len());
        for cell in &row.cells {
            while column < open.len() && open[column] > 0 {
                column += 1;
            }
            let span = cell.colspan.max(1) as usize;
            while open.len() < column + span {
                open.push(0);
            }
            if cell.rowspan > 1 {
                for slot in open.iter_mut().skip(column).take(span) {
                    *slot = cell.rowspan as usize;
                }
            }
            let x: f64 = widths.iter().take(column).sum();
            let width: f64 = widths.iter().skip(column).take(span).sum();
            cells.push((x, width, cell));
            column += span;
        }
        rows.push(cells);
    }
    rows
}
