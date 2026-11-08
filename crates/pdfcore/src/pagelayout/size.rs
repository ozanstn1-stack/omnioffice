//! Choosing the page size a resize targets.
//!
//! A named size, a custom one, or the page's own - and then an orientation that
//! either keeps the current shape or forces portrait or landscape.

use crate::error::{PdfError, PdfResult};
use crate::images::page_size_points;

use super::ResizeOptions;

pub(super) fn resolve_target_size(options: &ResizeOptions, current_w: f64, current_h: f64) -> PdfResult<(f64, f64)> {
    let (mut w, mut h) = match options.page_size.to_lowercase().as_str() {
        "custom" => (options.custom_width_pt.max(1.0), options.custom_height_pt.max(1.0)),
        other => {
            page_size_points(other).ok_or_else(|| PdfError::InvalidInput(format!("unknown page size '{other}'")))?
        }
    };
    let current_landscape = current_w > current_h;
    match options.orientation.to_lowercase().as_str() {
        "landscape" => {
            if w < h {
                std::mem::swap(&mut w, &mut h);
            }
        }
        "portrait" => {
            if w > h {
                std::mem::swap(&mut w, &mut h);
            }
        }
        _ => {
            // auto: follow the current orientation
            let target_landscape = w > h;
            if current_landscape != target_landscape {
                std::mem::swap(&mut w, &mut h);
            }
        }
    }
    Ok((w, h))
}
