//! N-up sheets and booklet imposition.
//!
//! Places two or four source pages on one output sheet by turning each source
//! page into a Form XObject and painting it into the new page's content
//! stream. Booklet mode orders the pages for saddle-stitch printing (the
//! first sheet carries the last and first page, and so on).

use crate::docutil::OverwritePolicy;
use crate::error::PdfResult;
use crate::progress::{CancelToken, ProgressCallback};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// How many pages go on one sheet and in which order.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NupOptions {
    /// Pages per sheet: 2 or 4.
    pub per_sheet: u32,
    /// Saddle-stitch booklet ordering instead of reading order.
    pub booklet: bool,
    /// `portrait` or `landscape`; the sheet is always landscape-ish for N-up.
    pub orientation: String,
    /// Sheet size: `source` keeps the source page size, or `a4` / `letter`.
    pub page_size: String,
    pub margin_pt: f64,
    pub gutter_pt: f64,
    /// Draw a light border around each placed page.
    pub border: bool,
    /// 1-based pages to include; empty means every page.
    pub pages: Vec<u32>,
}

impl Default for NupOptions {
    fn default() -> Self {
        Self {
            per_sheet: 2,
            booklet: false,
            orientation: "landscape".into(),
            page_size: "source".into(),
            margin_pt: 18.0,
            gutter_pt: 12.0,
            border: false,
            pages: Vec::new(),
        }
    }
}

/// Places the selected pages on N-up sheets. TODO(phase8): implementation.
pub fn nup_pdf(
    _input: &std::path::Path,
    _output: &std::path::Path,
    _options: &NupOptions,
    _policy: OverwritePolicy,
    _password: Option<&str>,
    _progress: &ProgressCallback,
    _cancel: &CancelToken,
) -> PdfResult<PathBuf> {
    Err(crate::error::PdfError::Internal("nup_pdf is not implemented yet".into()))
}
