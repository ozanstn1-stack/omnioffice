//! Shared document helpers: loading with graceful error mapping, atomic
//! saving, output path policies, page-tree rebuilds, inherited attribute
//! materialization, content stream injection, resource management, image
//! XObjects and 2D affine math for rotation-aware placement.

use crate::error::{PdfError, PdfResult};
use lopdf::{
    dictionary, Dictionary, Document, Object, ObjectId, Stream, StringFormat,
};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const INHERITED_ATTRS: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];

// ---------------------------------------------------------------------------
// Loading / saving
// ---------------------------------------------------------------------------

/// Loads a PDF, mapping malformed/encrypted inputs to friendly errors.
pub fn load_document(path: &Path, password: Option<&str>) -> PdfResult<Document> {
    if !path.exists() {
        return Err(PdfError::NotFound(path.display().to_string()));
    }
    let result = match password {
        Some(pw) => Document::load_with_password(path, pw),
        None => Document::load(path),
    };
    match result {
        Ok(doc) => {
            // lopdf parses the outer structure of encrypted files without a
            // password but leaves every stream/string encrypted, which would
            // silently produce empty operations. Treat that as "needs a
            // password" instead.
            if doc.is_encrypted() && password.is_none() {
                return Err(PdfError::PasswordRequired);
            }
            Ok(doc)
        }
        Err(err) => {
            if password.is_some() {
                let msg = err.to_string().to_lowercase();
                if msg.contains("password") || msg.contains("decrypt") {
                    return Err(PdfError::WrongPassword);
                }
            }
            if !password.is_some() && looks_encrypted(path) {
                return Err(PdfError::PasswordRequired);
            }
            Err(PdfError::from_lopdf(err, Some(path)))
        }
    }
}

/// Cheap scan of the raw file header/trailer for an /Encrypt entry. Used only
/// to disambiguate errors when no password was supplied.
pub fn looks_encrypted(path: &Path) -> bool {
    match fs::read(path) {
        Ok(bytes) => {
            let needle = b"/Encrypt";
            bytes
                .windows(needle.len())
                .rev()
                .take(4096)
                .any(|w| w == needle)
        }
        Err(_) => false,
    }
}

pub fn is_encrypted_document(path: &Path) -> bool {
    match Document::load(path) {
        Ok(doc) => doc.is_encrypted() || doc.was_encrypted(),
        Err(_) => looks_encrypted(path),
    }
}

/// Saves a document atomically: writes to a sibling temp file first and only
/// then moves it over the target, so a cancelled or failed run never leaves a
/// half-written PDF behind.
pub fn save_document(doc: &mut Document, output: &Path, compress_streams: bool) -> PdfResult<()> {
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent).map_err(PdfError::from_io)?;
        }
    }
    if compress_streams {
        doc.compress();
    }
    doc.prune_objects();
    doc.renumber_objects();

    let tmp_path = temp_sibling(output);
    {
        let mut file = fs::File::create(&tmp_path).map_err(PdfError::from_io)?;
        doc.save_to(&mut file).map_err(PdfError::from_io)?;
        file.flush().map_err(PdfError::from_io)?;
    }
    // Windows rename is not atomic over an existing file; remove first.
    if output.exists() {
        fs::remove_file(output).map_err(PdfError::from_io)?;
    }
    fs::rename(&tmp_path, output).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        PdfError::from_io(e)
    })
}

/// Writes raw bytes atomically, the same way documents are saved (temp
/// sibling first, then a move over the target).
pub fn write_bytes_atomic(output: &Path, bytes: &[u8]) -> PdfResult<()> {
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent).map_err(PdfError::from_io)?;
        }
    }
    let tmp_path = temp_sibling(output);
    fs::write(&tmp_path, bytes).map_err(PdfError::from_io)?;
    // Windows rename is not atomic over an existing file; remove first.
    if output.exists() {
        fs::remove_file(output).map_err(PdfError::from_io)?;
    }
    fs::rename(&tmp_path, output).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        PdfError::from_io(e)
    })
}

/// Same as `save_document` but disables cross-reference/object streams, which
/// keeps maximum compatibility with third-party parsers (used by tests and
/// for encrypted outputs).
pub fn save_document_classic(doc: &mut Document, output: &Path) -> PdfResult<()> {
    save_document(doc, output, false)
}

pub fn temp_sibling(path: &Path) -> PathBuf {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "output.pdf".to_string());
    path.with_file_name(format!(".{name}.{pid}{nanos}.tmp"))
}

// ---------------------------------------------------------------------------
// Output path policies
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverwritePolicy {
    /// Fail with `OutputExists` so the UI can ask the user.
    Error,
    /// Replace the existing file.
    Replace,
    /// Write alongside with a " (1)", " (2)" suffix.
    UniqueName,
}

/// Resolves the final output path according to the overwrite policy and
/// verifies that an existing target can actually be replaced (locked files
/// produce `FileLocked` instead of a confusing write error later).
pub fn resolve_output_path(output: &Path, policy: OverwritePolicy) -> PdfResult<PathBuf> {
    if !output.exists() {
        return Ok(output.to_path_buf());
    }
    match policy {
        OverwritePolicy::Error => Err(PdfError::OutputExists(output.display().to_string())),
        OverwritePolicy::Replace => {
            fs::OpenOptions::new()
                .write(true)
                .open(output)
                .map_err(PdfError::from_io)?;
            Ok(output.to_path_buf())
        }
        OverwritePolicy::UniqueName => {
            let parent = output.parent().unwrap_or_else(|| Path::new("."));
            let stem = output
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "output".to_string());
            let ext = output
                .extension()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "pdf".to_string());
            for i in 1..10_000 {
                let candidate = parent.join(format!("{stem} ({i}).{ext}"));
                if !candidate.exists() {
                    return Ok(candidate);
                }
            }
            Err(PdfError::OutputExists(output.display().to_string()))
        }
    }
}

/// Default output path: next to the input file, with a suffix and .pdf ext.
pub fn default_output_for(input: &Path, suffix: &str) -> PathBuf {
    let parent = input.parent().unwrap_or_else(|| Path::new("."));
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "document".to_string());
    parent.join(format!("{stem}{suffix}.pdf"))
}

/// Creates a unique working directory for a single job, deleted on drop.
pub struct JobDir {
    inner: Option<tempfile::TempDir>,
}

impl JobDir {
    pub fn new(prefix: &str) -> PdfResult<Self> {
        let dir = tempfile::Builder::new()
            .prefix(&format!("pdfsak-{prefix}-"))
            .tempdir()
            .map_err(PdfError::from_io)?;
        Ok(Self { inner: Some(dir) })
    }

    pub fn path(&self) -> &Path {
        self.inner.as_ref().map(|d| d.path()).unwrap_or(Path::new("."))
    }

    pub fn file(&self, name: &str) -> PathBuf {
        self.path().join(name)
    }

    /// Explicit cleanup (Drop also does this).
    pub fn cleanup(mut self) {
        if let Some(dir) = self.inner.take() {
            let _ = dir.close();
        }
    }
}

// ---------------------------------------------------------------------------
// Page tree operations
// ---------------------------------------------------------------------------

/// Copies any inherited page attribute (Resources, MediaBox, CropBox, Rotate)
/// from ancestor /Pages nodes down onto the page object itself, so pages can
/// be moved between documents or re-parented safely.
pub fn materialize_inherited_attrs(doc: &mut Document, page_id: ObjectId) -> PdfResult<()> {
    let mut collected: Vec<(Vec<u8>, Object)> = Vec::new();
    {
        let page = doc.get_dictionary(page_id)?;
        let mut missing: Vec<&[u8]> = INHERITED_ATTRS
            .iter()
            .copied()
            .filter(|k| !page.has(k))
            .collect();
        if missing.is_empty() {
            return Ok(());
        }
        let mut current = page.get(b"Parent").ok().and_then(|o| o.as_reference().ok());
        let mut hops = 0;
        while let Some(parent_id) = current {
            hops += 1;
            if hops > 64 || missing.is_empty() {
                break;
            }
            let parent = doc.get_dictionary(parent_id)?;
            let mut remove_keys: Vec<&[u8]> = Vec::new();
            for key in missing.iter() {
                if let Ok(value) = parent.get(key) {
                    collected.push((key.to_vec(), value.clone()));
                    remove_keys.push(key);
                }
            }
            missing.retain(|k| !remove_keys.contains(k));
            current = parent.get(b"Parent").ok().and_then(|o| o.as_reference().ok());
        }
    }
    if !collected.is_empty() {
        let page = doc.get_object_mut(page_id)?.as_dict_mut()?;
        for (key, value) in collected {
            if !page.has(&key) {
                page.set(key, value);
            }
        }
    }
    Ok(())
}

/// Materializes inherited attributes for every page in the document.
pub fn materialize_all_pages(doc: &mut Document) -> PdfResult<()> {
    let pages: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    for page_id in pages {
        materialize_inherited_attrs(doc, page_id)?;
    }
    Ok(())
}

/// Returns the object id of the root /Pages node.
pub fn pages_root_id(doc: &Document) -> PdfResult<ObjectId> {
    doc.catalog()?
        .get(b"Pages")?
        .as_reference()
        .map_err(|_| PdfError::CorruptPdf("catalog /Pages is not a reference".into()))
}

/// Rewrites the page tree to contain exactly `order`, in order. Duplicated
/// entries are cloned so per-instance edits (e.g. rotation) stay independent.
/// All pages must already be direct children materialized onto themselves.
pub fn rebuild_page_tree(doc: &mut Document, plan: &[PagePlanItem]) -> PdfResult<()> {
    let root_id = pages_root_id(doc)?;
    let mut counts: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    for item in plan {
        *counts.entry(item.source_page).or_insert(0) += 1;
    }
    // Snapshot the original rotation of every referenced page *before* any
    // edits, so duplicated instances each start from the original rotation.
    let mut rotation_snapshot: std::collections::HashMap<u32, i32> = std::collections::HashMap::new();
    let current_pages = doc.get_pages();
    for item in plan {
        if let std::collections::hash_map::Entry::Vacant(e) = rotation_snapshot.entry(item.source_page) {
            let page_id = current_pages
                .get(&item.source_page)
                .copied()
                .ok_or(PdfError::RangeOutOfBounds)?;
            e.insert(page_rotation(doc, page_id)?);
        }
    }
    let mut used: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    let mut new_kids: Vec<Object> = Vec::with_capacity(plan.len());
    for item in plan {
        let page_id = doc.get_pages().get(&item.source_page).copied().ok_or(PdfError::RangeOutOfBounds)?;
        let seen = used.entry(item.source_page).or_insert(0);
        let object_id = if *seen == 0 && counts[&item.source_page] == 1 {
            page_id
        } else if *seen == 0 {
            *seen += 1;
            page_id
        } else {
            *seen += 1;
            clone_page_object(doc, page_id)?
        };
        // apply rotation delta relative to the original rotation
        if item.rotation_delta % 360 != 0 {
            let base = *rotation_snapshot.get(&item.source_page).unwrap_or(&0);
            let next = ((base + item.rotation_delta) % 360 + 360) % 360;
            doc.get_object_mut(object_id)?
                .as_dict_mut()?
                .set("Rotate", Object::Integer(next as i64));
        }
        new_kids.push(Object::Reference(object_id));
    }
    {
        let root = doc.get_object_mut(root_id)?.as_dict_mut()?;
        root.set("Type", Object::Name(b"Pages".to_vec()));
        root.set("Kids", Object::Array(new_kids.clone()));
        root.set("Count", Object::Integer(new_kids.len() as i64));
        root.remove(b"Parent");
    }
    for kid in &new_kids {
        if let Object::Reference(id) = kid {
            if let Ok(dict) = doc.get_object_mut(*id).and_then(Object::as_dict_mut) {
                dict.set("Parent", Object::Reference(root_id));
            }
        }
    }
    Ok(())
}

/// A single entry of a pages plan: source page number (1-based, referring to
/// the input document) plus a rotation delta applied to that instance.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PagePlanItem {
    pub source_page: u32,
    #[serde(default)]
    pub rotation_delta: i32,
}

/// Deep-ish copy of a page dictionary (one level, references shared) used for
/// duplicated pages in a plan.
fn clone_page_object(doc: &mut Document, page_id: ObjectId) -> PdfResult<ObjectId> {
    let dict = doc.get_dictionary(page_id)?.clone();
    let mut dict = dict;
    dict.remove(b"Parent");
    // Annotations reference their page; strip those references so duplicates
    // do not contain dangling /P pointers to the original page.
    if let Ok(annots) = dict.get(b"Annots") {
        let keep: Vec<Object> = match annots {
            Object::Array(items) => items.to_vec(),
            Object::Reference(_) => vec![annots.clone()],
            _ => Vec::new(),
        };
        if !keep.is_empty() {
            dict.set("Annots", Object::Array(keep));
        }
    }
    Ok(doc.add_object(Object::Dictionary(dict)))
}

pub fn page_rotation(doc: &Document, page_id: ObjectId) -> PdfResult<i32> {
    let page = doc.get_dictionary(page_id)?;
    Ok(page
        .get(b"Rotate")
        .ok()
        .and_then(|o| o.as_i64().ok())
        .map(|v| v.rem_euclid(360) as i32)
        .unwrap_or(0))
}

pub fn page_mediabox(doc: &Document, page_id: ObjectId) -> PdfResult<[f64; 4]> {
    let page = doc.get_dictionary(page_id)?;
    let mb = page
        .get(b"MediaBox")
        .map_err(|_| PdfError::CorruptPdf("page has no MediaBox".into()))?;
    let arr = mb
        .as_array()
        .map_err(|_| PdfError::CorruptPdf("MediaBox is not an array".into()))?;
    if arr.len() != 4 {
        return Err(PdfError::CorruptPdf("MediaBox must have 4 entries".into()));
    }
    let mut out = [0f64; 4];
    for (i, obj) in arr.iter().enumerate() {
        out[i] = object_to_f64(obj).ok_or_else(|| PdfError::CorruptPdf("MediaBox entry not numeric".into()))?;
    }
    Ok(out)
}

pub fn page_cropbox(doc: &Document, page_id: ObjectId) -> PdfResult<Option<[f64; 4]>> {
    let page = doc.get_dictionary(page_id)?;
    match page.get(b"CropBox") {
        Ok(cb) => {
            let arr = cb
                .as_array()
                .map_err(|_| PdfError::CorruptPdf("CropBox is not an array".into()))?;
            if arr.len() != 4 {
                return Ok(None);
            }
            let mut out = [0f64; 4];
            for (i, obj) in arr.iter().enumerate() {
                out[i] = object_to_f64(obj).unwrap_or(0.0);
            }
            Ok(Some(out))
        }
        Err(_) => Ok(None),
    }
}

pub fn object_to_f64(obj: &Object) -> Option<f64> {
    match obj {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(f) => Some(*f as f64),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Content stream injection
// ---------------------------------------------------------------------------

/// Appends a content stream to a page (drawn on top of existing content).
pub fn append_page_content(doc: &mut Document, page_id: ObjectId, content: Vec<u8>) -> PdfResult<()> {
    doc.add_page_contents(page_id, content)
        .map_err(|e| PdfError::from_lopdf(e, None))
}

/// Wraps all existing page content in a `q ... cm ... Q` transform, used for
/// scaling/translating pages (resize, custom page sizes).
pub fn wrap_page_content_transform(
    doc: &mut Document,
    page_id: ObjectId,
    matrix: Matrix,
) -> PdfResult<()> {
    let existing = doc.get_page_content(page_id);
    let mut wrapped = Vec::with_capacity(existing.len() + 64);
    wrapped.extend_from_slice(b"q\n");
    wrapped.extend_from_slice(matrix.to_cm().as_bytes());
    wrapped.push(b'\n');
    wrapped.extend_from_slice(&existing);
    wrapped.extend_from_slice(b"\nQ\n");
    let content_id = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), wrapped)));
    let page = doc.get_object_mut(page_id)?.as_dict_mut()?;
    page.set("Contents", Object::Reference(content_id));
    Ok(())
}

// ---------------------------------------------------------------------------
// Resources
// ---------------------------------------------------------------------------

/// The image XObjects a page draws from, whichever way they are stored.
///
/// A page's `/Resources` may be inline on the page, behind a reference, or
/// inherited from an ancestor page node, and the `/XObject` sub-dictionary may
/// itself be behind a reference. Every consumer that walks a page's images -
/// the inspector, the compressor, redaction - needs all four cases handled, and
/// getting it wrong means silently seeing zero images.
pub fn page_xobjects(doc: &Document, page_id: ObjectId) -> Vec<ObjectId> {
    let resolve = |value: Option<&Object>| -> Option<Dictionary> {
        match value? {
            Object::Reference(id) => match doc.get_object(*id) {
                Ok(Object::Dictionary(dictionary)) => Some(dictionary.clone()),
                _ => None,
            },
            Object::Dictionary(dictionary) => Some(dictionary.clone()),
            _ => None,
        }
    };
    let resources = match doc.get_page_resources(page_id) {
        Ok((Some(resources), _)) => resources.clone(),
        _ => match doc.get_object(page_id) {
            Ok(Object::Dictionary(page)) => match resolve(page.get(b"Resources").ok()) {
                Some(value) => value,
                None => return Vec::new(),
            },
            _ => return Vec::new(),
        },
    };
    let list = match resolve(resources.get(b"XObject").ok()) {
        Some(value) => value,
        None => return Vec::new(),
    };
    let mut out: Vec<ObjectId> = Vec::new();
    for (_, value) in list.iter() {
        if let Object::Reference(id) = value {
            if !out.contains(id) {
                out.push(*id);
            }
        }
    }
    out
}

pub fn own_page_resources(doc: &mut Document, page_id: ObjectId) -> PdfResult<ObjectId> {
    materialize_inherited_attrs(doc, page_id)?;
    let existing = {
        let page = doc.get_dictionary(page_id)?;
        page.get(b"Resources").ok().cloned()
    };
    match existing {
        Some(Object::Reference(id)) => {
            // Only share when this reference is used by exactly this page.
            let usage = doc
                .objects
                .values()
                .filter(|o| {
                    o.as_dict()
                        .ok()
                        .and_then(|d| d.get(b"Resources").ok())
                        .and_then(|r| r.as_reference().ok())
                        .map(|r| r == id)
                        .unwrap_or(false)
                })
                .count();
            if usage <= 1 {
                Ok(id)
            } else {
                let dict = doc.get_dictionary(id)?.clone();
                let new_id = doc.add_object(Object::Dictionary(dict));
                doc.get_object_mut(page_id)?
                    .as_dict_mut()?
                    .set("Resources", Object::Reference(new_id));
                Ok(new_id)
            }
        }
        Some(Object::Dictionary(dict)) => {
            let new_id = doc.add_object(Object::Dictionary(dict));
            doc.get_object_mut(page_id)?
                .as_dict_mut()?
                .set("Resources", Object::Reference(new_id));
            Ok(new_id)
        }
        _ => {
            let new_id = doc.add_object(Object::Dictionary(Dictionary::new()));
            doc.get_object_mut(page_id)?
                .as_dict_mut()?
                .set("Resources", Object::Reference(new_id));
            Ok(new_id)
        }
    }
}

/// Inserts a named entry into a sub-dictionary of the page resources
/// (e.g. `/XObject /Im0`, `/Font /Helv`, `/ExtGState /GS0`).
pub fn add_resource_entry(
    doc: &mut Document,
    page_id: ObjectId,
    category: &[u8],
    name: &str,
    value: Object,
) -> PdfResult<()> {
    let resources_id = own_page_resources(doc, page_id)?;
    let sub_id = {
        let resources = doc.get_dictionary(resources_id)?;
        match resources.get(category) {
            Ok(Object::Reference(id)) => Some(*id),
            _ => None,
        }
    };
    let sub_id = match sub_id {
        Some(id) => id,
        None => {
            let existing_dict = {
                let resources = doc.get_dictionary(resources_id)?;
                match resources.get(category) {
                    Ok(Object::Dictionary(d)) => Some(d.clone()),
                    _ => None,
                }
            };
            let new_id = doc.add_object(Object::Dictionary(existing_dict.unwrap_or_default()));
            doc.get_object_mut(resources_id)?
                .as_dict_mut()?
                .set(category, Object::Reference(new_id));
            new_id
        }
    };
    doc.get_object_mut(sub_id)?
        .as_dict_mut()?
        .set(name, value);
    Ok(())
}

// ---------------------------------------------------------------------------
// Image XObjects
// ---------------------------------------------------------------------------

/// Raw, non-premultiplied RGBA image data.
pub struct RawImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Adds an RGBA image as an image XObject with a soft mask for transparency.
pub fn add_rgba_image_xobject(doc: &mut Document, img: &RawImage) -> PdfResult<ObjectId> {
    let (w, h) = (img.width, img.height);
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    let mut alpha = Vec::with_capacity((w * h) as usize);
    for px in img.rgba.as_chunks::<4>().0 {
        rgb.extend_from_slice(&px[0..3]);
        alpha.push(px[3]);
    }
    let smask_dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Image",
        "Width" => w as i64,
        "Height" => h as i64,
        "ColorSpace" => "DeviceGray",
        "BitsPerComponent" => 8i64,
    };
    let mut smask = Stream::new(smask_dict, alpha);
    smask.compress().ok();
    let smask_id = doc.add_object(Object::Stream(smask));

    let img_dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Image",
        "Width" => w as i64,
        "Height" => h as i64,
        "ColorSpace" => "DeviceRGB",
        "BitsPerComponent" => 8i64,
        "SMask" => smask_id,
    };
    let mut stream = Stream::new(img_dict, rgb);
    stream.compress().ok();
    Ok(doc.add_object(Object::Stream(stream)))
}

/// Adds an RGB (or gray) image XObject with flate compression.
pub fn add_rgb_image_xobject(
    doc: &mut Document,
    width: u32,
    height: u32,
    rgb: Vec<u8>,
    grayscale: bool,
) -> PdfResult<ObjectId> {
    let cs = if grayscale { "DeviceGray" } else { "DeviceRGB" };
    let dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Image",
        "Width" => width as i64,
        "Height" => height as i64,
        "ColorSpace" => cs,
        "BitsPerComponent" => 8i64,
    };
    let mut stream = Stream::new(dict, rgb);
    stream.compress().ok();
    Ok(doc.add_object(Object::Stream(stream)))
}

/// Embeds raw JPEG bytes without re-encoding (DCTDecode passthrough).
pub fn add_jpeg_image_xobject(
    doc: &mut Document,
    width: u32,
    height: u32,
    jpeg: Vec<u8>,
    grayscale: bool,
) -> PdfResult<ObjectId> {
    let cs = if grayscale { "DeviceGray" } else { "DeviceRGB" };
    let dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Image",
        "Width" => width as i64,
        "Height" => height as i64,
        "ColorSpace" => cs,
        "BitsPerComponent" => 8i64,
        "Filter" => "DCTDecode",
    };
    let stream = Stream::new(dict, jpeg);
    Ok(doc.add_object(Object::Stream(stream)))
}

// ---------------------------------------------------------------------------
// Text objects
// ---------------------------------------------------------------------------

/// Encodes a Rust string as a PDF text string (ASCII literal or UTF-16BE with
/// BOM for anything else).
pub fn pdf_text_object(value: &str) -> Object {
    if value.is_ascii() {
        Object::String(value.as_bytes().to_vec(), StringFormat::Literal)
    } else {
        let mut bytes = vec![0xFE, 0xFF];
        for unit in value.encode_utf16() {
            bytes.extend_from_slice(&unit.to_be_bytes());
        }
        Object::String(bytes, StringFormat::Literal)
    }
}

/// Decodes a PDF text string object into a Rust string.
pub fn pdf_text_value(obj: &Object) -> Option<String> {
    match obj {
        Object::String(bytes, _) => {
            if bytes.starts_with(&[0xFE, 0xFF]) {
                let units: Vec<u16> = bytes[2..]
                    .as_chunks::<2>().0.iter()
                    .map(|c| u16::from_be_bytes([c[0], c[1]]))
                    .collect();
                Some(String::from_utf16_lossy(&units))
            } else if bytes.starts_with(&[0xFF, 0xFE]) {
                let units: Vec<u16> = bytes[2..]
                    .as_chunks::<2>().0.iter()
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect();
                Some(String::from_utf16_lossy(&units))
            } else {
                Some(String::from_utf8_lossy(bytes).to_string())
            }
        }
        Object::Name(bytes) => Some(String::from_utf8_lossy(bytes).to_string()),
        _ => None,
    }
}

pub fn escape_pdf_literal(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            '\\' => out.push_str("\\\\"),
            '\r' => out.push_str("\\r"),
            _ => out.push(c),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Affine math (rotation-aware placement)
// ---------------------------------------------------------------------------

/// Row-major 2D affine transform: [a b c d e f] maps (x,y) -> (a x + c y + e,
/// b x + d y + f), matching the PDF `cm` operator.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Matrix(pub [f64; 6]);

impl Matrix {
    pub const IDENTITY: Matrix = Matrix([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    pub fn translate(tx: f64, ty: f64) -> Matrix {
        Matrix([1.0, 0.0, 0.0, 1.0, tx, ty])
    }

    pub fn scale(sx: f64, sy: f64) -> Matrix {
        Matrix([sx, 0.0, 0.0, sy, 0.0, 0.0])
    }

    pub fn rotate_deg(deg: f64) -> Matrix {
        let r = deg.to_radians();
        let (s, c) = r.sin_cos();
        Matrix([c, s, -s, c, 0.0, 0.0])
    }

    /// self ∘ other: apply `other` first, then `self`.
    pub fn mul(self, other: Matrix) -> Matrix {
        let a = self.0;
        let b = other.0;
        Matrix([
            a[0] * b[0] + a[2] * b[1],
            a[1] * b[0] + a[3] * b[1],
            a[0] * b[2] + a[2] * b[3],
            a[1] * b[2] + a[3] * b[3],
            a[0] * b[4] + a[2] * b[5] + a[4],
            a[1] * b[4] + a[3] * b[5] + a[5],
        ])
    }

    pub fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        (
            self.0[0] * x + self.0[2] * y + self.0[4],
            self.0[1] * x + self.0[3] * y + self.0[5],
        )
    }

    /// Inverse of an affine transform (None when degenerate).
    pub fn inverse(&self) -> Option<Matrix> {
        let [a, b, c, d, e, f] = self.0;
        let det = a * d - b * c;
        if det.abs() < 1e-12 {
            return None;
        }
        Some(Matrix([
            d / det,
            -b / det,
            -c / det,
            a / det,
            (c * f - d * e) / det,
            (b * e - a * f) / det,
        ]))
    }

    pub fn to_cm(&self) -> String {
        format!(
            "{:.4} {:.4} {:.4} {:.4} {:.4} {:.4} cm",
            self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5]
        )
    }

    /// Maps coordinates in the *displayed* page space (after /Rotate is
    /// applied, origin bottom-left) back into the page's own coordinate
    /// system. Use as the outer `cm` for rotation-aware drawing.
    pub fn display_to_page(rotation: i32, page_w: f64, page_h: f64) -> Matrix {
        match rotation.rem_euclid(360) {
            90 => Matrix([0.0, 1.0, -1.0, 0.0, page_w, 0.0]),
            180 => Matrix([-1.0, 0.0, 0.0, -1.0, page_w, page_h]),
            270 => Matrix([0.0, -1.0, 1.0, 0.0, 0.0, page_h]),
            _ => Matrix::IDENTITY,
        }
    }

    /// The placement matrix for an image drawn into the display-space rect
    /// (x, y, w, h) on a page that will be shown with `rotation` degrees.
    /// Returns a matrix valid in the page's own coordinate system.
    pub fn for_rotated_page(rotation: i32, page_w: f64, page_h: f64, x: f64, y: f64, w: f64, h: f64) -> Matrix {
        let display = Matrix::translate(x, y).mul(Matrix::scale(w, h));
        Self::display_to_page(rotation, page_w, page_h).mul(display)
    }

    /// Size of the displayed page for a rotation value.
    pub fn displayed_size(rotation: i32, page_w: f64, page_h: f64) -> (f64, f64) {
        match rotation.rem_euclid(360) {
            90 | 270 => (page_h, page_w),
            _ => (page_w, page_h),
        }
    }
}

/// Converts a display-space rectangle back to page coordinates (used by crop
/// preview -> CropBox conversion).
pub fn display_rect_to_page_rect(
    rotation: i32,
    page_w: f64,
    page_h: f64,
    rect: [f64; 4],
) -> [f64; 4] {
    let to_page = Matrix::display_to_page(rotation, page_w, page_h);
    let corners = [
        to_page.apply(rect[0], rect[1]),
        to_page.apply(rect[0] + rect[2], rect[1]),
        to_page.apply(rect[0], rect[1] + rect[3]),
        to_page.apply(rect[0] + rect[2], rect[1] + rect[3]),
    ];
    let xs: Vec<f64> = corners.iter().map(|c| c.0).collect();
    let ys: Vec<f64> = corners.iter().map(|c| c.1).collect();
    let min_x = xs.iter().cloned().fold(f64::MAX, f64::min);
    let max_x = xs.iter().cloned().fold(f64::MIN, f64::max);
    let min_y = ys.iter().cloned().fold(f64::MAX, f64::min);
    let max_y = ys.iter().cloned().fold(f64::MIN, f64::max);
    [min_x, min_y, max_x - min_x, max_y - min_y]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_composition_is_correct() {
        let m = Matrix::translate(10.0, 20.0).mul(Matrix::scale(2.0, 3.0));
        assert_eq!(m.apply(1.0, 1.0), (12.0, 23.0));
        let id = Matrix::IDENTITY;
        assert_eq!(id.mul(id), id);
    }

    #[test]
    fn rotated_placement_maps_back_to_display_rect() {
        // A rect at display (50, 60) size 100x20 on a 200x300 page rotated 90.
        let rot = 90;
        let (w, h) = (200.0, 300.0);
        let m = Matrix::for_rotated_page(rot, w, h, 50.0, 60.0, 100.0, 20.0);
        // The page->display transform must map the placed corners back onto
        // the same display rect.
        let to_display = Matrix::display_to_page(rot, w, h)
            .inverse()
            .expect("invertible");
        let corners_display = [(50.0, 60.0), (150.0, 60.0), (50.0, 80.0), (150.0, 80.0)];
        for (i, (dx, dy)) in corners_display.iter().enumerate() {
            // unit square corner for this display corner
            let (u, v) = match i {
                0 => (0.0, 0.0),
                1 => (1.0, 0.0),
                2 => (0.0, 1.0),
                _ => (1.0, 1.0),
            };
            let (px, py) = m.apply(u, v);
            let (bx, by) = to_display.apply(px, py);
            assert!((bx - dx).abs() < 1e-6, "x mismatch at {i}: {bx} vs {dx}");
            assert!((by - dy).abs() < 1e-6, "y mismatch at {i}: {by} vs {dy}");
        }
    }

    #[test]
    fn display_rect_roundtrip_for_all_rotations() {
        for rotation in [0, 90, 180, 270] {
            for (w, h) in [(200.0, 300.0), (300.0, 200.0)] {
                let (dw, dh) = Matrix::displayed_size(rotation, w, h);
                let rect = [40.0, 50.0, 100.0, 60.0];
                let page_rect = display_rect_to_page_rect(rotation, w, h, rect);
                // The page-space rect must fit inside the page box.
                assert!(page_rect[0] >= -0.5 && page_rect[1] >= -0.5);
                assert!(page_rect[0] + page_rect[2] <= w.max(dh).max(dw).max(h) + 0.5);
                // Area is preserved under rotation.
                let area = page_rect[2] * page_rect[3];
                assert!((area - rect[2] * rect[3]).abs() < 1.0, "area changed for rot {rotation}");
            }
        }
    }

    #[test]
    fn text_objects_roundtrip() {
        let obj = pdf_text_object("Rapor: şğüöç 2026");
        assert_eq!(pdf_text_value(&obj).unwrap(), "Rapor: şğüöç 2026");
        let ascii = pdf_text_object("plain");
        assert_eq!(pdf_text_value(&ascii).unwrap(), "plain");
    }
}