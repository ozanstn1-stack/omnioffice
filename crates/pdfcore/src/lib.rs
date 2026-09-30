//! pdfcore - local-first PDF processing engine for PDF Swiss Army Knife.
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
//! * `pagelayout`- page size conversion and cropping
//! * `ocr`       - Tesseract-based OCR pipeline
//! * `engines`   - engine discovery (pdfium.dll, qpdf.exe, tesseract.exe)
//! * `sanitize`  - full-object removal of scripts, actions, attachments
//! * `pdfa`      - PDF/A validation and best-effort conversion
//! * `flatten`   - annotation and form flattening
//! * `forms`     - AcroForm fields (list/fill/validate) and page object editing

pub mod annotate;
pub mod compare;
pub mod compress;
pub mod convert;
pub mod docutil;
pub mod engines;
pub mod error;
pub mod flatten;
pub mod fontembed;
pub mod forms;
pub mod images;
pub mod ltv;
pub mod info;
pub mod inspect;
pub mod merge;
pub mod metadata;
pub mod numbering;
pub mod ocr;
pub mod organize;
pub mod pagelayout;
pub mod pages;
pub mod pdfa;
pub mod progress;
pub mod redact;
pub mod render;
pub mod sanitize;
pub mod security;
pub mod sign;
pub mod textbox;
pub mod textimg;
pub mod watermark;

pub use error::{ErrorCode, PdfError, PdfResult};
pub use progress::{CancelToken, ProgressEvent};

/// Semantic version of the processing core.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
