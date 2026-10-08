//! officecore - local-first document engine for OmniOffice.
//!
//! Handles the document model and the file formats behind the Writer, Calc,
//! Impress and Tools modules. Everything runs offline on the user's machine;
//! macros and embedded scripts are never executed, and ZIP/XML inputs are
//! bounded so hostile documents cannot exhaust memory.
//!
//! * `model`     - shared document model (the in-app source of truth)
//! * `address`   - A1 cell addressing helpers
//! * `formula`   - formula text and function names across XLSX / ODF
//! * `zip`       - hardened ZIP container reader/writer
//! * `xml`       - OOXML/ODF XML tree helpers
//! * `pdfcanvas` - vector PDF output with embedded OFL fonts

pub mod address;
pub mod cleaner;
pub mod compat;
pub mod csvio;
pub mod docx;
pub mod encoding;
pub mod error;
pub mod formula;
pub mod io;
pub mod layout;
pub mod legacy;
pub mod model;
pub mod odf;
pub mod pdfcanvas;
pub mod pivot;
pub mod pptx;
pub mod revisions;
pub mod rtf;
pub mod schema;
pub mod textio;
pub mod unit;
pub mod xlsx;
pub mod xml;
pub mod zip;

pub use error::{ErrorCode, OfficeError, OfficeResult};

/// Semantic version of the office engine.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Human readable summary used by the About/Settings screen.
pub fn engine_summary() -> String {
    format!("Office engine {VERSION} (docx, odt, rtf, xlsx, ods, csv, pptx, odp, pdf export)")
}
