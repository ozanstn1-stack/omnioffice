//! PresentationML (PPTX) import and export.
//!
//! The writer emits a complete, valid package (masters, layouts, themes,
//! slides, notes, media, ChartML and animation timing) so PowerPoint,
//! LibreOffice and OnlyOffice open it natively. The reader extracts text
//! boxes, pictures, shapes, groups, charts, tables, placeholders, animations,
//! notes and slide size, warning whenever something cannot be represented.

mod reader;
mod writer;

pub use reader::{read_pptx, read_pptx_file};
pub use writer::{write_pptx, write_pptx_file, write_pptx_package, DeckRead, DeckWrite};

use writer::{layout_kind, part_stem, placeholder_role, pt_from_emu};

use crate::error::{OfficeError, OfficeResult};
use crate::model::*;
use crate::xml::{escape_attr, escape_text, parse_xml, XmlNode};
use crate::zip::{ZipReader, ZipWriter};
use std::collections::HashMap;
use std::path::Path;

#[cfg(test)]
#[path = "../pptx_tests.rs"]
mod tests;
