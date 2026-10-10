//! N-up sheets and booklet imposition.
//!
//! Places two or four source pages on one output sheet by turning each source
//! page into a Form XObject and painting it into the new page's content
//! stream. Booklet mode orders the pages for saddle-stitch printing (the
//! first sheet carries the last and first page, and so on).
//!
//! Sheet pages are fully self-contained (own `/MediaBox` and `/Resources`), so
//! the fresh `/Pages` root deliberately carries no inherited attributes: the
//! source tree's inherited `/Rotate` must not be re-applied on top of the
//! rotation already baked into each form's placement matrix. Rebuilding the
//! tree also drops links, outlines and named destinations that pointed at the
//! old pages; the old page objects themselves are unreferenced and are removed
//! by `prune_objects` on save (any still referenced from surviving catalog
//! branches, e.g. outlines, remain as harmless orphans).

use crate::docutil::{
    load_document, materialize_all_pages, own_page_resources, page_mediabox, page_rotation, resolve_output_path,
    save_document, Matrix, OverwritePolicy,
};
use crate::error::{PdfError, PdfResult};
use crate::progress::{CancelToken, ProgressCallback, ProgressEvent};
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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

/// A4 portrait in points.
const A4_PORTRAIT: (f64, f64) = (595.28, 841.89);
/// US Letter portrait in points.
const LETTER_PORTRAIT: (f64, f64) = (612.0, 792.0);

/// Places the selected pages on N-up sheets according to `options`.
///
/// Encrypted input is handled through `password` (see `docutil::load_document`);
/// the result replaces/creates `output` following `policy`.
pub fn nup_pdf(
    input: &Path,
    output: &Path,
    options: &NupOptions,
    policy: OverwritePolicy,
    password: Option<&str>,
    progress: &ProgressCallback,
    cancel: &CancelToken,
) -> PdfResult<PathBuf> {
    let mut doc = load_document(input, password)?;
    build_nup(&mut doc, options, progress, cancel)?;
    let final_path = resolve_output_path(output, policy)?;
    save_document(&mut doc, &final_path, true)?;
    Ok(final_path)
}

/// In-memory variant of [`nup_pdf`] for callers that already hold the bytes
/// (and for tests). Encrypted input must be decrypted by the caller first.
pub fn nup_pdf_bytes(input: &[u8], options: &NupOptions) -> PdfResult<Vec<u8>> {
    let mut doc = Document::load_mem(input).map_err(|error| PdfError::from_lopdf(error, None))?;
    if doc.is_encrypted() {
        return Err(PdfError::PasswordRequired);
    }
    let cancel = CancelToken::new();
    let no_progress = |_event: ProgressEvent| {};
    build_nup(&mut doc, options, &no_progress, &cancel)?;
    doc.compress();
    doc.prune_objects();
    doc.renumber_objects();
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).map_err(PdfError::from_io)?;
    Ok(bytes)
}

// ---------------------------------------------------------------------------
// Planning
// ---------------------------------------------------------------------------

/// A source page turned into a Form XObject, with the geometry later needed to
/// place it on a sheet.
struct PageForm {
    id: ObjectId,
    /// Unrotated page size (the form's BBox).
    page_w: f64,
    page_h: f64,
    /// Size the page occupies once `/Rotate` is applied.
    display_w: f64,
    display_h: f64,
    rotation: i32,
}

/// Sheet geometry resolved once per job.
struct Layout {
    sheet_w: f64,
    sheet_h: f64,
    margin: f64,
    gutter: f64,
    cols: usize,
    cell_w: f64,
    cell_h: f64,
}

impl Layout {
    fn new(size: &str, orientation: &str, source: (f64, f64), per_sheet: u32, options: &NupOptions) -> Layout {
        let (base_w, base_h) = match size {
            "a4" => A4_PORTRAIT,
            "letter" => LETTER_PORTRAIT,
            _ => source,
        };
        // `orientation` decides which side of the sheet is the long one.
        let (sheet_w, sheet_h) = if orientation == "portrait" {
            (base_w.min(base_h), base_w.max(base_h))
        } else {
            (base_w.max(base_h), base_w.min(base_h))
        };
        let (cols, rows) = if per_sheet == 2 { (2, 1) } else { (2, 2) };
        // Margins that would collapse the cells are capped just short of half
        // the sheet, so the placement matrices stay valid for any input.
        let margin_cap = (sheet_w.min(sheet_h) / 2.0 - 1.0).max(0.0);
        let margin = positive_or_zero(options.margin_pt).min(margin_cap);
        let gutter = positive_or_zero(options.gutter_pt);
        let usable_w = (sheet_w - 2.0 * margin - (cols - 1) as f64 * gutter).max(1.0);
        let usable_h = (sheet_h - 2.0 * margin - (rows - 1) as f64 * gutter).max(1.0);
        Layout {
            sheet_w,
            sheet_h,
            margin,
            gutter,
            cols,
            cell_w: usable_w / cols as f64,
            cell_h: usable_h / rows as f64,
        }
    }

    /// Left edge of column `col`.
    fn cell_x(&self, col: usize) -> f64 {
        self.margin + col as f64 * (self.cell_w + self.gutter)
    }

    /// Bottom edge of row `row` (row 0 is the top row).
    fn cell_y(&self, row: usize) -> f64 {
        self.sheet_h - self.margin - self.cell_h - row as f64 * (self.cell_h + self.gutter)
    }
}

fn positive_or_zero(value: f64) -> f64 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        0.0
    }
}

/// Saddle-stitch order over the selected pages, padded to a multiple of four
/// with blanks (`None`). Each physical sheet contributes its front side and
/// then its back side to the output, two cells each.
fn booklet_order(selected: &[u32]) -> Vec<Option<u32>> {
    let mut padded: Vec<Option<u32>> = selected.iter().copied().map(Some).collect();
    while !padded.len().is_multiple_of(4) {
        padded.push(None);
    }
    let last = padded.len();
    let mut out = Vec::with_capacity(last);
    let mut sheet = 0usize;
    while sheet * 4 < last {
        let front_left = padded[last - 2 * sheet - 1];
        let front_right = padded[2 * sheet];
        let back_left = padded[2 * sheet + 1];
        let back_right = padded[last - 2 * sheet - 2];
        out.extend([front_left, front_right, back_left, back_right]);
        sheet += 1;
    }
    out
}

// ---------------------------------------------------------------------------
// Document construction
// ---------------------------------------------------------------------------

fn build_nup(
    doc: &mut Document,
    options: &NupOptions,
    progress: &ProgressCallback,
    cancel: &CancelToken,
) -> PdfResult<()> {
    let per_sheet = options.per_sheet;
    if !matches!(per_sheet, 2 | 4) {
        return Err(PdfError::InvalidInput("pages per sheet must be 2 or 4".into()));
    }
    if options.booklet && per_sheet != 2 {
        return Err(PdfError::InvalidInput("booklet imposition requires 2 pages per sheet".into()));
    }
    let orientation = options.orientation.trim().to_ascii_lowercase();
    if orientation != "portrait" && orientation != "landscape" {
        return Err(PdfError::InvalidInput(format!("unknown sheet orientation: {}", options.orientation)));
    }
    let page_size = options.page_size.trim().to_ascii_lowercase();
    if !matches!(page_size.as_str(), "source" | "a4" | "letter") {
        return Err(PdfError::InvalidInput(format!("unknown sheet size: {}", options.page_size)));
    }

    let page_ids: HashMap<u32, ObjectId> = doc.get_pages().into_iter().collect();
    if page_ids.is_empty() {
        return Err(PdfError::InvalidPdf("document has no pages".into()));
    }
    let total_pages = page_ids.len() as u32;
    let selected: Vec<u32> = if options.pages.is_empty() { (1..=total_pages).collect() } else { options.pages.clone() };
    for page in &selected {
        if *page == 0 || *page > total_pages {
            return Err(PdfError::RangeOutOfBounds);
        }
    }
    cancel.check()?;

    materialize_all_pages(doc)?;

    // `None` marks a padding blank page inserted only in booklet mode.
    let slots: Vec<Option<u32>> =
        if options.booklet { booklet_order(&selected) } else { selected.iter().copied().map(Some).collect() };

    // One Form XObject per distinct selected source page; a page listed twice
    // shares its form.
    let mut forms: Vec<PageForm> = Vec::new();
    let mut form_index: HashMap<u32, usize> = HashMap::new();
    for page in &selected {
        if form_index.contains_key(page) {
            continue;
        }
        let page_id = page_ids.get(page).copied().ok_or(PdfError::RangeOutOfBounds)?;
        let info = add_page_form(doc, page_id)?;
        form_index.insert(*page, forms.len());
        forms.push(info);
    }

    // `source` sheet size derives from the first selected page.
    let first_box = page_mediabox(doc, page_ids[&selected[0]])?;
    let first_size = ((first_box[2] - first_box[0]).abs(), (first_box[3] - first_box[1]).abs());
    let layout = Layout::new(&page_size, &orientation, first_size, per_sheet, options);

    let output_pages = slots.len().div_ceil(per_sheet as usize);
    let mut sheet_ids: Vec<ObjectId> = Vec::with_capacity(output_pages);
    for (index, chunk) in slots.chunks(per_sheet as usize).enumerate() {
        cancel.check()?;
        let sheet_id = build_sheet_page(doc, &forms, &form_index, chunk, &layout, options.border)?;
        sheet_ids.push(sheet_id);
        progress(ProgressEvent::new("nup.sheet", (index + 1) as u64, output_pages as u64));
    }

    install_page_tree(doc, &sheet_ids)?;
    Ok(())
}

/// Wraps the page's content in a Form XObject with its own resources. A
/// non-zero MediaBox origin is translated away so the BBox is `[0 0 w h]`.
fn add_page_form(doc: &mut Document, page_id: ObjectId) -> PdfResult<PageForm> {
    let media_box = page_mediabox(doc, page_id)?;
    let origin_x = media_box[0].min(media_box[2]);
    let origin_y = media_box[1].min(media_box[3]);
    let page_w = (media_box[2] - media_box[0]).abs().max(1.0);
    let page_h = (media_box[3] - media_box[1]).abs().max(1.0);
    let rotation = page_rotation(doc, page_id)?;
    let group = doc.get_dictionary(page_id).ok().and_then(|page| page.get(b"Group").ok().cloned());
    let resources_id = own_page_resources(doc, page_id)?;
    let content = doc.get_page_content(page_id);
    let body = if origin_x.abs() > 1e-6 || origin_y.abs() > 1e-6 {
        let mut body = Vec::with_capacity(content.len() + 64);
        body.extend_from_slice(format!("q\n1 0 0 1 {:.4} {:.4} cm\n", -origin_x, -origin_y).as_bytes());
        body.extend_from_slice(&content);
        body.extend_from_slice(b"\nQ\n");
        body
    } else {
        content
    };
    let mut form_dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Form",
        "FormType" => 1i64,
        "BBox" => vec![
            Object::Real(0.0),
            Object::Real(0.0),
            Object::Real(page_w as f32),
            Object::Real(page_h as f32),
        ],
        "Resources" => Object::Reference(resources_id),
    };
    if let Some(group) = group {
        // Carry a transparency group over so blend modes keep grouping the
        // way they did on the source page.
        form_dict.set("Group", group);
    }
    let mut stream = Stream::new(form_dict, body);
    stream.compress().ok();
    let (display_w, display_h) = Matrix::displayed_size(rotation, page_w, page_h);
    Ok(PageForm { id: doc.add_object(Object::Stream(stream)), page_w, page_h, display_w, display_h, rotation })
}

/// Builds one output sheet page: a content stream that paints the forms in
/// slot order plus an optional light border, and a `/Resources` dictionary
/// naming them.
fn build_sheet_page(
    doc: &mut Document,
    forms: &[PageForm],
    form_index: &HashMap<u32, usize>,
    slots: &[Option<u32>],
    layout: &Layout,
    border: bool,
) -> PdfResult<ObjectId> {
    let mut xobjects = Dictionary::new();
    let mut content: Vec<u8> = Vec::new();
    for (slot, page) in slots.iter().enumerate() {
        let Some(page) = page else { continue };
        let form = &forms[form_index[page]];
        let scale = (layout.cell_w / form.display_w).min(layout.cell_h / form.display_h);
        let col = slot % layout.cols;
        let row = slot / layout.cols;
        let placed_w = form.display_w * scale;
        let placed_h = form.display_h * scale;
        let x = layout.cell_x(col) + (layout.cell_w - placed_w) / 2.0;
        let y = layout.cell_y(row) + (layout.cell_h - placed_h) / 2.0;
        let name = format!("Fm{}", form_index[page]);
        xobjects.set(name.as_str(), Object::Reference(form.id));
        // The page -> display transform for the source rotation: applying its
        // inverse paints the page as it was displayed, pre-rotating the
        // placement instead of relying on a `/Rotate` on the sheet.
        let to_display =
            Matrix::display_to_page(form.rotation, form.page_w, form.page_h).inverse().unwrap_or(Matrix::IDENTITY);
        let matrix = Matrix::translate(x, y).mul(Matrix::scale(scale, scale)).mul(to_display);
        content.extend_from_slice(format!("q\n{}\n/{} Do\nQ\n", matrix.to_cm(), name).as_bytes());
        if border {
            content.extend_from_slice(
                format!("0.5 w\n0.5 G\n{x:.4} {y:.4} {placed_w:.4} {placed_h:.4} re S\n").as_bytes(),
            );
        }
    }
    let resources_id = doc.add_object(Object::Dictionary(dictionary! {
        "XObject" => Object::Dictionary(xobjects),
    }));
    let content_id = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), content)));
    Ok(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Page",
        "MediaBox" => vec![
            Object::Real(0.0),
            Object::Real(0.0),
            Object::Real(layout.sheet_w as f32),
            Object::Real(layout.sheet_h as f32),
        ],
        "Resources" => Object::Reference(resources_id),
        "Contents" => Object::Reference(content_id),
    })))
}

/// Points the catalog at a fresh `/Pages` root holding exactly the sheets.
fn install_page_tree(doc: &mut Document, sheet_ids: &[ObjectId]) -> PdfResult<()> {
    let mut root = Dictionary::new();
    root.set("Type", Object::Name(b"Pages".to_vec()));
    root.set("Kids", Object::Array(sheet_ids.iter().map(|id| Object::Reference(*id)).collect::<Vec<Object>>()));
    root.set("Count", Object::Integer(sheet_ids.len() as i64));
    let root_id = doc.add_object(Object::Dictionary(root));
    for id in sheet_ids {
        doc.get_object_mut(*id)?.as_dict_mut()?.set("Parent", Object::Reference(root_id));
    }
    doc.catalog_mut()?.set("Pages", Object::Reference(root_id));
    Ok(())
}
