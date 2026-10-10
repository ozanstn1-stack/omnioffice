//! Header/footer text and Bates numbering stamps.
//!
//! Both stamps append a new content revision to every selected page, so
//! signed documents keep their signatures. Text is drawn with base14
//! Helvetica; `{page}` and `{pages}` placeholders are substituted per page.

use serde::{Deserialize, Serialize};

/// Header and footer templates; empty strings skip that slot.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HeaderFooterOptions {
    pub header_left: String,
    pub header_center: String,
    pub header_right: String,
    pub footer_left: String,
    pub footer_center: String,
    pub footer_right: String,
    pub font_size_pt: f64,
    pub color: String,
    pub margin_pt: f64,
    /// 1-based pages; empty means every page.
    pub pages: Vec<u32>,
    /// Visible number of the first selected page (`{page}` on it).
    pub start_number: u32,
    /// Number pages by their position in the selection instead of the document.
    pub count_from_start: bool,
}

impl Default for HeaderFooterOptions {
    fn default() -> Self {
        Self {
            header_left: String::new(),
            header_center: String::new(),
            header_right: String::new(),
            footer_left: String::new(),
            footer_center: String::new(),
            footer_right: String::new(),
            font_size_pt: 9.0,
            color: "#334155".into(),
            margin_pt: 24.0,
            pages: Vec::new(),
            start_number: 1,
            count_from_start: true,
        }
    }
}

/// Bates numbering: `{prefix}{number}{suffix}` with zero padding.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BatesOptions {
    pub prefix: String,
    pub suffix: String,
    pub start: u32,
    /// Zero-padded digit count for the number part.
    pub digits: u32,
    /// `bottomLeft` | `bottomCenter` | `bottomRight` | `topLeft` | `topCenter` | `topRight`.
    pub position: String,
    pub font_size_pt: f64,
    pub color: String,
    pub margin_pt: f64,
    /// 1-based pages; empty means every page.
    pub pages: Vec<u32>,
}

impl Default for BatesOptions {
    fn default() -> Self {
        Self {
            prefix: String::new(),
            suffix: String::new(),
            start: 1,
            digits: 6,
            position: "bottomRight".into(),
            font_size_pt: 8.0,
            color: "#334155".into(),
            margin_pt: 24.0,
            pages: Vec::new(),
        }
    }
}
