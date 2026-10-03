//! Real PDF sanitizer.
//!
//! Auditing a PDF that came from somewhere else and "cleaning" it by editing
//! the catalog is theatre: the JavaScript name tree, a page's additional
//! actions, a file attachment annotation or an XMP packet all carry hazards,
//! and they hide behind indirect references. This module walks every object in
//! the file, removes the requested categories from dictionaries and arrays in
//! place, and lets the save path prune whatever is left unreferenced.
//!
//! The traversal works on the object graph rather than the page tree:
//!
//! 1. every object is classified once (JavaScript action, dangerous action,
//!    file specification, unsafe annotation, link) so references to them can
//!    be dropped from any container;
//! 2. every dictionary and array in the file is cleaned recursively, including
//!    the dictionaries of streams and the dictionaries nested inside them, so
//!    a `/JS` hiding inside a form field or an appearance stream is found;
//! 3. the pass repeats until nothing changes, which covers objects that only
//!    became reachable classifications after a reference was rewritten.
//!
//! What is removed is counted in [`SanitizeReport`], because a sanitizer that
//! silently does nothing is indistinguishable from one that worked.

use std::collections::BTreeMap;
use std::path::Path;

use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::{Deserialize, Serialize};

use crate::docutil;
use crate::error::PdfResult;
use crate::progress::{CancelToken, ProgressCallback, ProgressReporter};

/// Which classes of content the sanitizer removes. Every switch defaults to
/// `true`; the user turns off only what they intentionally want to keep.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SanitizeOptions {
    /// Removes `/JavaScript` name trees and every `/JS` / `/JavaScript` key.
    pub remove_javascript: bool,
    /// Removes the `/EmbeddedFiles` name tree and every file specification.
    pub remove_embedded_files: bool,
    /// Removes Launch, URI, SubmitForm, ImportData, GoToR and GoToE actions,
    /// plus the `/AA` additional-action dictionaries.
    pub remove_actions: bool,
    /// Removes the Info dictionary and every XMP `/Metadata` stream.
    pub remove_metadata: bool,
    /// Removes the catalog `/OpenAction`.
    pub remove_open_action: bool,
    /// Removes `/A` and `/AA` entries from form fields and widgets.
    pub remove_form_actions: bool,
    /// Removes annotations with a hazardous subtype (FileAttachment, Sound,
    /// Movie, Screen, RichMedia) or with an attached action.
    pub remove_annotations_unsafe: bool,
    /// Removes every link annotation, including its target action.
    pub remove_links: bool,
}

impl Default for SanitizeOptions {
    fn default() -> Self {
        Self {
            remove_javascript: true,
            remove_embedded_files: true,
            remove_actions: true,
            remove_metadata: true,
            remove_open_action: true,
            remove_form_actions: true,
            remove_annotations_unsafe: true,
            remove_links: true,
        }
    }
}

/// What the document carried before sanitization, so the UI can show what was
/// at stake even after every trace is gone.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SanitizeFindingsBefore {
    pub javascript_entries: u32,
    pub embedded_files: u32,
    pub actions: u32,
    pub unsafe_annotations: u32,
    pub link_annotations: u32,
    pub metadata_present: bool,
    pub open_action_present: bool,
}

/// Exactly what the sanitizer removed.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SanitizeReport {
    pub javascript_removed: u32,
    pub embedded_files_removed: u32,
    pub actions_removed: u32,
    pub metadata_removed: u32,
    pub annotations_removed: u32,
    pub links_removed: u32,
    pub warnings: Vec<String>,
    pub findings_before: SanitizeFindingsBefore,
}

const UNSAFE_ANNOTATION_SUBTYPES: [&[u8]; 5] = [b"FileAttachment", b"Sound", b"Movie", b"Screen", b"RichMedia"];

const DANGEROUS_ACTIONS: [&[u8]; 6] = [b"Launch", b"URI", b"SubmitForm", b"ImportData", b"GoToR", b"GoToE"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DropKind {
    Javascript,
    Action,
    EmbeddedFile,
    Annotation,
    Link,
}

fn dict_name<'a>(dict: &'a Dictionary, key: &[u8]) -> Option<&'a [u8]> {
    dict.get(key).ok().and_then(|value| value.as_name().ok())
}

fn is_javascript_action(dict: &Dictionary) -> bool {
    dict_name(dict, b"S") == Some(b"JavaScript".as_slice()) || dict.has(b"JS")
}

fn is_dangerous_action(dict: &Dictionary) -> bool {
    match dict_name(dict, b"S") {
        Some(kind) => DANGEROUS_ACTIONS.contains(&kind),
        None => false,
    }
}

fn is_file_spec(dict: &Dictionary) -> bool {
    dict.has_type(b"Filespec") || dict.has(b"EF")
}

fn is_link_annotation(dict: &Dictionary) -> bool {
    dict_name(dict, b"Subtype") == Some(b"Link".as_slice())
}

fn is_unsafe_annotation(dict: &Dictionary) -> bool {
    let subtype = match dict_name(dict, b"Subtype") {
        Some(value) => value,
        None => return false,
    };
    if subtype == b"Link" {
        return false;
    }
    UNSAFE_ANNOTATION_SUBTYPES.contains(&subtype) || dict.has(b"A") || dict.has(b"AA") || dict.has(b"FS")
}

fn is_form_field(dict: &Dictionary) -> bool {
    dict.has(b"FT") || dict_name(dict, b"Subtype") == Some(b"Widget".as_slice())
}

fn classify_dictionary(dict: &Dictionary, options: &SanitizeOptions) -> Option<DropKind> {
    if options.remove_links && is_link_annotation(dict) {
        return Some(DropKind::Link);
    }
    if options.remove_annotations_unsafe && is_unsafe_annotation(dict) {
        return Some(DropKind::Annotation);
    }
    if options.remove_javascript && is_javascript_action(dict) {
        return Some(DropKind::Javascript);
    }
    if options.remove_actions && is_dangerous_action(dict) {
        return Some(DropKind::Action);
    }
    if options.remove_embedded_files && is_file_spec(dict) {
        return Some(DropKind::EmbeddedFile);
    }
    None
}

fn classify_object(object: &Object, options: &SanitizeOptions) -> Option<DropKind> {
    match object {
        Object::Dictionary(dict) => classify_dictionary(dict, options),
        Object::Stream(stream) => classify_dictionary(&stream.dict, options),
        _ => None,
    }
}

/// Counts the hazardous structures reachable in the document, without changing
/// anything. Used for the report's `findingsBefore` section.
fn scan_findings(doc: &Document) -> SanitizeFindingsBefore {
    let mut findings = SanitizeFindingsBefore::default();
    for object in doc.objects.values() {
        let dict = match object {
            Object::Dictionary(dict) => dict,
            Object::Stream(stream) => &stream.dict,
            _ => continue,
        };
        if is_javascript_action(dict) || dict.has(b"JavaScript") || dict.has(b"JS") {
            findings.javascript_entries += 1;
        }
        if is_file_spec(dict) || dict.has(b"EmbeddedFiles") {
            findings.embedded_files += 1;
        }
        if is_dangerous_action(dict) || dict.has(b"AA") {
            findings.actions += 1;
        }
        if dict.has(b"OpenAction") {
            findings.open_action_present = true;
        }
        if is_unsafe_annotation(dict) {
            findings.unsafe_annotations += 1;
        }
        if is_link_annotation(dict) {
            findings.link_annotations += 1;
        }
        if dict.has(b"Metadata") {
            findings.metadata_present = true;
        }
    }
    if doc.trailer.get(b"Info").is_ok() {
        findings.metadata_present = true;
    }
    findings
}

struct Sanitizer<'a> {
    options: &'a SanitizeOptions,
    drop: BTreeMap<ObjectId, DropKind>,
    report: SanitizeReport,
    changed: bool,
}

impl Sanitizer<'_> {
    fn bump(&mut self, kind: DropKind) {
        match kind {
            DropKind::Javascript => self.report.javascript_removed += 1,
            DropKind::Action => self.report.actions_removed += 1,
            DropKind::EmbeddedFile => self.report.embedded_files_removed += 1,
            DropKind::Annotation => self.report.annotations_removed += 1,
            DropKind::Link => self.report.links_removed += 1,
        }
    }

    /// Cleans one value, returning the category when the value itself has to
    /// be removed from its container.
    fn clean_object(&mut self, object: &mut Object, depth: usize) -> Option<DropKind> {
        if depth > 64 {
            return None;
        }
        match object {
            Object::Dictionary(dict) => self.clean_dictionary(dict, depth),
            Object::Stream(stream) => self.clean_dictionary(&mut stream.dict, depth),
            Object::Array(items) => {
                let taken = std::mem::take(items);
                let mut kept = Vec::with_capacity(taken.len());
                for mut item in taken {
                    match self.clean_object(&mut item, depth + 1) {
                        Some(kind) => {
                            self.bump(kind);
                            self.changed = true;
                        }
                        None => kept.push(item),
                    }
                }
                *items = kept;
                None
            }
            Object::Reference(id) => self.drop.get(id).copied(),
            _ => None,
        }
    }

    fn clean_dictionary(&mut self, dict: &mut Dictionary, depth: usize) -> Option<DropKind> {
        let keys: Vec<Vec<u8>> = dict.iter().map(|(key, _)| key.clone()).collect();
        for key in &keys {
            let kind = match dict.get_mut(key) {
                Ok(value) => self.clean_object(value, depth + 1),
                Err(_) => None,
            };
            if let Some(kind) = kind {
                dict.remove(key);
                self.bump(kind);
                self.changed = true;
            }
        }

        if self.options.remove_javascript {
            for key in [b"JavaScript".as_slice(), b"JS"] {
                if dict.has(key) {
                    dict.remove(key);
                    self.bump(DropKind::Javascript);
                    self.changed = true;
                }
            }
        }
        if self.options.remove_open_action && dict.has(b"OpenAction") {
            dict.remove(b"OpenAction");
            self.bump(DropKind::Action);
            self.changed = true;
        }
        if self.options.remove_actions && dict.has(b"AA") {
            dict.remove(b"AA");
            self.bump(DropKind::Action);
            self.changed = true;
        }
        if self.options.remove_embedded_files && dict.has(b"EmbeddedFiles") {
            dict.remove(b"EmbeddedFiles");
            self.bump(DropKind::EmbeddedFile);
            self.changed = true;
        }
        if self.options.remove_metadata && dict.has(b"Metadata") {
            dict.remove(b"Metadata");
            self.report.metadata_removed += 1;
            self.changed = true;
        }
        if self.options.remove_form_actions && is_form_field(dict) {
            for key in [b"A".as_slice(), b"AA"] {
                if dict.has(key) {
                    dict.remove(key);
                    self.bump(DropKind::Action);
                    self.changed = true;
                }
            }
        }

        classify_dictionary(dict, self.options)
    }
}

/// Removes the requested hazardous structures from an in-memory document.
///
/// Shared with the PDF/A converter, which has to strip JavaScript, actions and
/// embedded files before it can claim an archival conversion. The caller owns
/// loading and saving so the same traversal serves both entry points.
pub(crate) fn sanitize_document(
    doc: &mut Document,
    options: &SanitizeOptions,
    cancel: &CancelToken,
) -> PdfResult<SanitizeReport> {
    let mut sanitizer = Sanitizer {
        options,
        drop: BTreeMap::new(),
        report: SanitizeReport { findings_before: scan_findings(doc), ..Default::default() },
        changed: false,
    };
    for (id, object) in &doc.objects {
        if let Some(kind) = classify_object(object, options) {
            sanitizer.drop.insert(*id, kind);
        }
    }

    let xfa = doc
        .catalog()
        .ok()
        .is_some_and(|catalog| resolve_dict(doc, catalog.get(b"AcroForm").ok()).is_some_and(|acro| acro.has(b"XFA")));
    if xfa {
        sanitizer
            .report
            .warnings
            .push("The form uses XFA; dynamic form content cannot be fully inspected by this sanitizer.".into());
    }

    for _round in 0..4 {
        sanitizer.changed = false;
        let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
        for (index, id) in ids.iter().enumerate() {
            if index % 64 == 0 {
                cancel.check()?;
            }
            if let Some(object) = doc.objects.get_mut(id) {
                let _ = sanitizer.clean_object(object, 0);
            }
        }
        if options.remove_metadata && doc.trailer.remove(b"Info").is_some() {
            sanitizer.report.metadata_removed += 1;
            sanitizer.changed = true;
        }
        if !sanitizer.changed {
            break;
        }
    }

    Ok(sanitizer.report)
}

fn resolve_dict(doc: &Document, value: Option<&Object>) -> Option<Dictionary> {
    match value? {
        Object::Reference(id) => doc.get_dictionary(*id).ok().cloned(),
        Object::Dictionary(dict) => Some(dict.clone()),
        _ => None,
    }
}

/// Loads `input`, removes everything selected by `options`, and writes the
/// result atomically to `output`.
pub fn sanitize_pdf(
    input: &Path,
    output: &Path,
    options: &SanitizeOptions,
    progress: &ProgressCallback,
    cancel: &CancelToken,
) -> PdfResult<SanitizeReport> {
    cancel.check()?;
    let mut doc = docutil::load_document(input, None)?;
    let reporter = ProgressReporter::new(progress);
    reporter.emit_step("sanitize.scan", 0, 1);
    let report = sanitize_document(&mut doc, options, cancel)?;
    reporter.emit_step("sanitize.clean", 1, 1);
    cancel.check()?;
    docutil::save_document(&mut doc, output, true)?;
    reporter.emit_step("sanitize.save", 1, 1);
    Ok(report)
}
