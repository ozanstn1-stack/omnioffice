#![forbid(unsafe_code)]
// Refuse panicking shortcuts in production code (E1.7 ratchet): a library
// crate that parses untrusted documents must return its errors, not unwind.
// Tests keep their freedom; the gate is `cargo clippy --all-targets`.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

//! pdfcore - local-first PDF processing engine for OmniOffice.
//!
//! All operations run on the user's machine. Nothing is uploaded anywhere;
//! passwords are never logged or persisted.
//!
//! Structure:
//! * `docutil`   - loading/saving, page trees, content injection, matrices
//! * `merge`     - document merging
//! * `organize`  - page plans, delete/extract/rotate, splitting
//! * `pages`     - page selection parsing and split planning
//! * `security`  - AES-256 protection and password removal
//! * `metadata`  - Info dictionary editing
//! * `info`      - document information/statistics
//! * `render`    - pdfium rasterization and text extraction
//! * `images`    - image encoding/decoding, images -> PDF
//! * `compress`  - lossless and raster compression
//! * `textimg`   - Unicode text rendering for stamps/watermarks
//! * `watermark` - text/image watermarks
//! * `numbering` - page numbers (base14 Helvetica)
//! * `annotate`  - text/image/rect/highlight/line annotations
//! * `page_detect` - automatic page corners (Canny/Hough) and scanner filters
//! * `pagelayout`- page size conversion and cropping
//! * `ocr`       - Tesseract-based OCR pipeline
//! * `engines`   - engine discovery (pdfium.dll, qpdf.exe, tesseract.exe)
//! * `sanitize`  - full-object removal of scripts, actions, attachments
//! * `repair`    - recovery (qpdf, or the built-in rebuild) and linearization
//! * `rebuild`   - built-in object-scanning repair that needs no qpdf
//! * `pdfa`      - PDF/A validation and best-effort conversion
//! * `flatten`   - annotation and form flattening
//! * `forms`     - AcroForm fields (list/fill/validate) and page object editing
//! * `pdf2doc`   - layout recovery (paragraphs, headings, lists) for PDF to Word
//! * `nup`       - N-up sheets and booklet imposition
//! * `stamp`     - header/footer text and Bates numbering stamps

pub mod annotate;
pub mod attachment;
pub mod certificate;
pub mod compare;
pub mod compress;
pub mod content;
pub mod convert;
pub mod docutil;
pub mod engines;
pub mod error;
pub mod evidence;
pub mod flatten;
pub mod fontembed;
mod fontusage;
pub mod forms;
pub mod gibqr;
pub mod images;
pub mod incremental;
pub mod info;
pub mod inspect;
pub mod ltv;
pub mod merge;
pub mod metadata;
pub mod numbering;
pub mod nup;
pub mod ocr;
pub mod organize;
pub mod page_detect;
pub mod pagelayout;
pub mod pages;
pub mod pdf2doc;
pub mod pdfa;
pub mod progress;
pub mod provenance;
pub mod rebuild;
pub mod redact;
pub mod render;
pub mod repair;
pub mod revocation;
pub mod sanitize;
pub mod scan;
pub mod security;
pub mod sign;
pub mod stamp;
pub mod textbox;
pub mod textfont;
pub mod textimg;
pub mod timestamp;
pub mod ttfsubset;
pub mod watermark;

pub use error::{ErrorCode, PdfError, PdfResult};
pub use progress::{CancelToken, ProgressEvent};

/// Semantic version of the processing core.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
