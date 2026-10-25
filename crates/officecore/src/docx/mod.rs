//! WordprocessingML (DOCX) import and export.
//!
//! The writer produces a self-contained OOXML package (styles, numbering,
//! headers/footers, tables, images, hyperlinks, page setup) that opens in Word,
//! LibreOffice and OnlyOffice. The reader is deliberately tolerant: unknown
//! constructs are skipped with a user-facing warning instead of failing, and
//! macros/embedded scripts are never executed or imported.

mod reader;
mod writer;

pub use reader::{read_docx, read_docx_file, DocxRead};
pub use writer::{write_docx, write_docx_file};

#[cfg(test)]
pub(crate) use reader::read_run_properties;

use crate::error::{OfficeError, OfficeResult};
use crate::io::{bare_hex, normalize_hex};
use crate::model::*;
use crate::xml::{parse_xml, XmlNode, XmlWriter};
use crate::zip::{ZipReader, ZipWriter};
use std::collections::HashMap;
use std::path::Path;

#[cfg(test)]
#[path = "../docx_tests.rs"]
mod tests;
