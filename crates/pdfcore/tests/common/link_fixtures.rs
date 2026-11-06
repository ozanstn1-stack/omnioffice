//! Source documents with link annotations for the N-up link tests.
//!
//! Shared by two test binaries, so each one only uses part of it.
#![allow(dead_code)]

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// A scratch directory removed on drop.
pub struct TestDir {
    path: PathBuf,
}

impl TestDir {
    pub fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "nup-links-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).expect("temp dir");
        TestDir { path }
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Size and rotation of one source page.
pub struct PageSpec {
    pub w: f64,
    pub h: f64,
    pub rotate: i64,
}

impl Default for PageSpec {
    fn default() -> Self {
        PageSpec { w: 600.0, h: 800.0, rotate: 0 }
    }
}

/// A document under construction: pages first, then their annotations.
pub struct Fixture {
    doc: Document,
    pages: Vec<ObjectId>,
}

impl Fixture {
    pub fn new(specs: &[PageSpec]) -> Self {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        let mut pages = Vec::with_capacity(specs.len());
        for (index, spec) in specs.iter().enumerate() {
            let body = format!("BT /F1 18 Tf 40 {:.2} Td (Page {}) Tj ET", (spec.h - 80.0).max(0.0), index + 1);
            let content_id = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), body.into_bytes())));
            let mut page = dictionary! {
                "Type" => "Page",
                "Parent" => Object::Reference(pages_id),
                "MediaBox" => rect_array([0.0, 0.0, spec.w, spec.h]),
                "Resources" => dictionary! { "Font" => dictionary! { "F1" => Object::Reference(font_id) } },
                "Contents" => Object::Reference(content_id),
            };
            if spec.rotate != 0 {
                page.set("Rotate", Object::Integer(spec.rotate));
            }
            pages.push(doc.add_object(Object::Dictionary(page)));
        }
        let kids: Vec<Object> = pages.iter().map(|id| Object::Reference(*id)).collect();
        let mut root = dictionary! {
            "Type" => "Pages",
            "Kids" => Object::Array(kids),
            "Count" => Object::Integer(pages.len() as i64),
        };
        root.set("MediaBox", rect_array([0.0, 0.0, specs[0].w, specs[0].h]));
        doc.objects.insert(pages_id, Object::Dictionary(root));
        let catalog_id = doc.new_object_id();
        doc.objects.insert(
            catalog_id,
            Object::Dictionary(dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages_id) }),
        );
        doc.trailer.set("Root", Object::Reference(catalog_id));
        Fixture { doc, pages }
    }

    /// Object id of a page, zero-based index.
    pub fn page(&self, index: usize) -> ObjectId {
        self.pages[index]
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Attaches an annotation dictionary to a page.
    pub fn annot(&mut self, page: usize, annot: Dictionary) -> ObjectId {
        let id = self.doc.add_object(Object::Dictionary(annot));
        self.annot_ref(page, Object::Reference(id));
        id
    }

    /// Attaches an already-built annotation object reference.
    pub fn annot_ref(&mut self, page: usize, annot: Object) {
        let page_id = self.pages[page];
        let mut items = self.annots(page);
        items.push(annot);
        self.doc.get_dictionary_mut(page_id).expect("page").set("Annots", Object::Array(items));
    }

    /// Stores a raw `/Annots` value (used for malformed input).
    pub fn raw_annots(&mut self, page: usize, value: Object) {
        let page_id = self.pages[page];
        self.doc.get_dictionary_mut(page_id).expect("page").set("Annots", value);
    }

    fn annots(&self, page: usize) -> Vec<Object> {
        let page = self.doc.get_dictionary(self.pages[page]).expect("page");
        match page.get(b"Annots").ok() {
            Some(Object::Array(items)) => items.clone(),
            _ => Vec::new(),
        }
    }

    /// Adds a `/Names /Dests` name tree (two levels, to exercise the kids walk)
    /// and the older catalog `/Dests` dictionary with one entry.
    pub fn named_destinations(&mut self, tree: &[(&str, Object)], legacy: Option<(&str, Object)>) {
        let catalog_id = self.doc.trailer.get(b"Root").expect("root").as_reference().expect("root ref");
        if !tree.is_empty() {
            let mid = tree.len() / 2;
            let leaf = |entries: &[(&str, Object)], doc: &mut Document| {
                let mut pairs: Vec<Object> = Vec::new();
                for (name, value) in entries {
                    pairs.push(Object::string_literal(*name));
                    pairs.push(value.clone());
                }
                let names = doc.add_object(Object::Array(pairs));
                doc.add_object(Object::Dictionary(dictionary! { "Names" => Object::Reference(names) }))
            };
            let left = leaf(&tree[..mid], &mut self.doc);
            let right = leaf(&tree[mid..], &mut self.doc);
            let node = self.doc.add_object(Object::Dictionary(dictionary! {
                "Kids" => Object::Array(vec![Object::Reference(left), Object::Reference(right)]),
            }));
            let names = self.doc.add_object(Object::Dictionary(dictionary! { "Dests" => Object::Reference(node) }));
            self.doc.get_dictionary_mut(catalog_id).expect("catalog").set("Names", Object::Reference(names));
        }
        if let Some((name, value)) = legacy {
            let mut entries = Dictionary::new();
            entries.set(name, value);
            let dests = self.doc.add_object(Object::Dictionary(entries));
            self.doc.get_dictionary_mut(catalog_id).expect("catalog").set("Dests", Object::Reference(dests));
        }
    }

    /// Catalogue object id, for tests that build name trees by hand.
    pub fn catalog_id(&self) -> ObjectId {
        self.doc.trailer.get(b"Root").expect("root").as_reference().expect("root ref")
    }

    /// Stores a bare object (an action stream, a malformed tree node, ...).
    pub fn add_object(&mut self, value: Object) -> ObjectId {
        self.doc.add_object(value)
    }

    /// Overwrites one key on an existing object of the fixture.
    pub fn patch(&mut self, id: ObjectId, key: &str, value: Object) {
        self.doc.get_dictionary_mut(id).expect("object").set(key, value);
    }

    pub fn write(&mut self, path: &Path) {
        self.doc.save(path).expect("save fixture");
    }
}

pub fn rect_array(rect: [f64; 4]) -> Object {
    Object::Array(rect.iter().map(|value| Object::Real(*value as f32)).collect())
}

/// A link annotation with a rectangle and no target yet.
pub fn link(rect: [f64; 4]) -> Dictionary {
    dictionary! {
        "Type" => "Annot",
        "Subtype" => "Link",
        "Rect" => rect_array(rect),
    }
}

pub fn with_action(mut annot: Dictionary, action: Dictionary) -> Dictionary {
    annot.set("A", Object::Dictionary(action));
    annot
}

pub fn with_dest(mut annot: Dictionary, dest: Object) -> Dictionary {
    annot.set("Dest", dest);
    annot
}

pub fn uri_action(url: &str) -> Dictionary {
    dictionary! { "S" => "URI", "URI" => Object::string_literal(url) }
}

pub fn goto_action(dest: Object) -> Dictionary {
    dictionary! { "S" => "GoTo", "D" => dest }
}

/// `[page /XYZ left top zoom]`.
pub fn xyz(page: ObjectId, left: Object, top: Object, zoom: Object) -> Object {
    Object::Array(vec![Object::Reference(page), Object::Name(b"XYZ".to_vec()), left, top, zoom])
}

/// `[page /FitH top]`.
pub fn fit_h(page: ObjectId, top: Object) -> Object {
    Object::Array(vec![Object::Reference(page), Object::Name(b"FitH".to_vec()), top])
}

/// `[page /Fit]`.
pub fn fit(page: ObjectId) -> Object {
    Object::Array(vec![Object::Reference(page), Object::Name(b"Fit".to_vec())])
}

/// `[page /FitR left bottom right top]`.
pub fn fit_r(page: ObjectId, rect: [f64; 4]) -> Object {
    let mut items = vec![Object::Reference(page), Object::Name(b"FitR".to_vec())];
    items.extend(rect.iter().map(|value| Object::Real(*value as f32)));
    Object::Array(items)
}

pub fn integer(value: i64) -> Object {
    Object::Integer(value)
}

pub fn real(value: f64) -> Object {
    Object::Real(value as f32)
}

/// A named destination string.
pub fn name(value: &str) -> Object {
    Object::string_literal(value)
}

// ---------------------------------------------------------------------------
// Reading the imposed output
// ---------------------------------------------------------------------------

use pdfcore::docutil::OverwritePolicy;
use pdfcore::nup::{nup_pdf, NupOptions};
use pdfcore::progress::{CancelToken, ProgressEvent};

pub fn no_progress(_event: ProgressEvent) {}

/// Runs the imposition, replacing the target.
pub fn impose(input: &Path, output: &Path, options: &NupOptions) {
    nup_pdf(input, output, options, OverwritePolicy::Replace, None, &no_progress, &CancelToken::new())
        .expect("nup_pdf");
}

/// The sheet pages in order.
pub fn sheets(doc: &Document) -> Vec<(u32, ObjectId)> {
    doc.get_pages().into_iter().collect()
}

/// The page's `/Annots` dictionaries, tolerating a direct array only.
pub fn annots(doc: &Document, page_id: ObjectId) -> Vec<Dictionary> {
    let page = doc.get_dictionary(page_id).expect("sheet page");
    let items = match page.get(b"Annots") {
        Ok(Object::Array(items)) => items.clone(),
        Ok(Object::Reference(id)) => {
            doc.get_object(*id).ok().and_then(|object| object.as_array().ok().cloned()).unwrap_or_default()
        }
        _ => return Vec::new(),
    };
    items
        .iter()
        .filter_map(|item| match item {
            Object::Reference(id) => doc.get_dictionary(*id).ok().cloned(),
            Object::Dictionary(dict) => Some(dict.clone()),
            _ => None,
        })
        .collect()
}

pub fn has_annots(doc: &Document, page_id: ObjectId) -> bool {
    doc.get_dictionary(page_id).expect("sheet page").get(b"Annots").is_ok()
}

pub fn number(value: &Object) -> Option<f64> {
    match value {
        Object::Integer(value) => Some(*value as f64),
        Object::Real(value) => Some(*value as f64),
        _ => None,
    }
}

pub fn rect_of(value: &Object) -> Option<[f64; 4]> {
    let items = value.as_array().ok()?;
    if items.len() != 4 {
        return None;
    }
    let mut rect = [0.0; 4];
    for (slot, item) in items.iter().enumerate() {
        rect[slot] = number(item)?;
    }
    Some(rect)
}

/// The `/Dest` array of an annotation, empty when it carries none.
pub fn dest_of(annot: &Dictionary) -> Vec<Object> {
    match annot.get(b"Dest").ok().and_then(|value| value.as_array().ok()) {
        Some(items) => items.clone(),
        None => Vec::new(),
    }
}

/// The decoded content stream of a sheet page.
pub fn page_content(doc: &Document, page_id: ObjectId) -> String {
    String::from_utf8_lossy(&doc.get_page_content(page_id)).to_string()
}

/// The `cm` operands of a sheet content stream, in slot order.
pub fn paint_matrices(content: &str) -> Vec<[f64; 6]> {
    content
        .lines()
        .filter_map(|line| {
            let operands = line.trim().strip_suffix(" cm")?;
            let values: Vec<f64> =
                operands.split_whitespace().map(|value| value.parse().ok()).collect::<Option<_>>()?;
            (values.len() == 6).then(|| [values[0], values[1], values[2], values[3], values[4], values[5]])
        })
        .collect()
}

pub fn apply(matrix: &[f64; 6], x: f64, y: f64) -> (f64, f64) {
    (matrix[0] * x + matrix[2] * y + matrix[4], matrix[1] * x + matrix[3] * y + matrix[5])
}

/// The axis-aligned rectangle a placement matrix gives a `w` x `h` page.
pub fn placed_rect(matrix: &[f64; 6], w: f64, h: f64) -> [f64; 4] {
    map_rect(matrix, [0.0, 0.0, w, h])
}

/// Maps a page-space rectangle through a placement matrix.
pub fn map_rect(matrix: &[f64; 6], rect: [f64; 4]) -> [f64; 4] {
    let corners = [(rect[0], rect[1]), (rect[2], rect[1]), (rect[0], rect[3]), (rect[2], rect[3])];
    let mapped = corners.map(|(x, y)| apply(matrix, x, y));
    let min_x = mapped.iter().map(|point| point.0).fold(f64::MAX, f64::min);
    let max_x = mapped.iter().map(|point| point.0).fold(f64::MIN, f64::max);
    let min_y = mapped.iter().map(|point| point.1).fold(f64::MAX, f64::min);
    let max_y = mapped.iter().map(|point| point.1).fold(f64::MIN, f64::max);
    [min_x, min_y, max_x, max_y]
}

/// Area of the intersection of two rectangles (zero when they touch or miss).
pub fn overlap(a: [f64; 4], b: [f64; 4]) -> f64 {
    let width = a[2].min(b[2]) - a[0].max(b[0]);
    let height = a[3].min(b[3]) - a[1].max(b[1]);
    if width <= 0.0 || height <= 0.0 {
        return 0.0;
    }
    width * height
}

pub fn close(a: [f64; 4], b: [f64; 4], tolerance: f64) -> bool {
    (0..4).all(|index| (a[index] - b[index]).abs() <= tolerance)
}
