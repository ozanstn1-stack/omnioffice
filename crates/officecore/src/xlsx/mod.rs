//! XLSX export (written directly as OOXML) and spreadsheet import.
//!
//! Export covers values, formulas (functions Excel 2010 and later added carry
//! the `_xlfn.` prefix Excel needs, see `formula`), error values (`#CALC!`,
//! `#SPILL!`), styling, number formats, column widths, row heights, hidden rows
//! and columns, merges, freeze panes, gridline settings, defined names,
//! autofilters, tab colours, hyperlinks, cell comments, data validation, conditional
//! formatting (highlight and formula rules, color scales, data bars, icon
//! sets), structured tables, sheet protection, print layout (margins,
//! headers/footers, page breaks, print area/titles), charts (column, bar, line,
//! pie, area, scatter, doughnut) as real ChartML parts anchored to their cells
//! and sheet pictures in `xl/media` with drawing anchors. Hyperlinks are limited
//! to http, https, mailto and internal references. Imported pivot caches/tables
//! are re-exported from the raw parts they came in as.
//!
//! Import is two passes: the well-tested `calamine` parser reads values and
//! formulas from XLSX, XLS and ODS files from Excel and LibreOffice, then a
//! best-effort OOXML pass (only for XLSX/XLSM) reads styles, column widths, row
//! heights, hidden rows and columns, merges, freeze panes, validations,
//! conditional formatting, defined names, hyperlinks, comments, structured
//! tables, print settings, sheet protection, drawings (charts and pictures) and
//! pivot parts straight from the package through the hardened ZIP/XML readers. The second pass never fails the
//! import: a part that cannot be parsed adds a warning and the values from the
//! first pass are still returned. The one thing the first pass cannot survive is
//! an error code `calamine` has no name for (Excel 365's `#CALC!`); it is read
//! again from a copy of the package that hides the code, and the second pass puts
//! it back.

mod chart_write;
mod conditional_read;
mod conditional_write;
mod drawings_read;
mod import;
mod pivot_read;
mod reader;
mod sheet_read;
mod sheet_write;
mod styles_read;
mod styles_write;
mod writer;

pub(crate) use chart_write::scatter_flavour;
use chart_write::{
    absolute_ref, chart_xml, drawing_xml, print_titles_ref, PlannedChart, PlannedImage, DOUGHNUT_DEFAULT_HOLE,
};
pub(crate) use conditional_read::absolute_area;
use conditional_read::apply_conditional;
use conditional_write::conditional_formatting_xml;
pub(crate) use conditional_write::{anchor_of, default_icon_thresholds, icon_set_count, CFVO_KINDS, CONDITIONAL_FILL};
use drawings_read::{apply_drawings, descendant};
pub(crate) use import::import_limit_warning;
use import::{
    attr_on, import_ooxml_layout, parse_u32_attr, read_relationships, resolve_part, MAX_CHART_CACHE_POINTS,
    MAX_DETAIL_PART_BYTES, MAX_FILTER_VALUES, MAX_IMPORT_COLS, MAX_IMPORT_ROWS, MAX_PIVOT_FIELDS,
};
use pivot_read::apply_pivot_tables;
use reader::CALAMINE_ERRORS;
pub use reader::{read_workbook_bytes, read_workbook_file};
use sheet_read::{
    apply_columns, apply_comments, apply_freeze_panes, apply_hyperlinks, apply_merges, apply_print_settings,
    apply_sheet_cells, apply_sheet_filter, apply_sheet_protection, apply_tables, apply_validations, list_reference,
};
use sheet_write::{sheet_comments, sheet_xml, SheetPart, DEFAULT_NOTE_AUTHOR};
use styles_read::{color_of, parse_styles, style_worth_a_cell, ImportedStyles};
use styles_write::{argb_of, StyleTable};
use writer::filter_owned_by_table;
pub use writer::{write_xlsx, write_xlsx_file, write_xlsx_package};

use crate::error::{OfficeError, OfficeResult};
use crate::io::{normalize_hex, write_atomic};
use crate::model::*;
use crate::xml::{escape_attr, escape_text, parse_xml, XmlNode, XmlWriter};
use crate::zip::ZipWriter;
use base64::Engine as _;
use calamine::Reader as CalamineReader;
use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct SheetWrite {
    pub bytes: Vec<u8>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SheetRead {
    pub workbook: Workbook,
    pub warnings: Vec<String>,
}

#[cfg(test)]
#[path = "../xlsx_tests.rs"]
mod tests;
