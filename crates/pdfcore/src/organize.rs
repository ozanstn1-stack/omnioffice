//! Page organization: apply a page plan (order, deletion, duplication,
//! rotation), extract pages, delete pages, rotate pages and split documents.

use crate::docutil::*;
use crate::error::{PdfError, PdfResult};
use crate::pages::{format_groups, groups_to_pages, plan_split, SplitMode};
use crate::progress::{CancelToken, ProgressCallback, ProgressEvent};
use std::path::{Path, PathBuf};

/// Applies a full page plan to a document. `plan` entries reference 1-based
/// source page numbers of the *input* file, in the desired output order.
pub fn apply_page_plan(
    input: &Path,
    plan: &[PagePlanItem],
    output: &Path,
    policy: OverwritePolicy,
    password: Option<&str>,
) -> PdfResult<PathBuf> {
    if plan.is_empty() {
        return Err(PdfError::InvalidInput("the plan contains no pages".into()));
    }
    let mut doc = load_document(input, password)?;
    let total = doc.get_pages().len() as u32;
    if total == 0 {
        return Err(PdfError::InvalidPdf("document has no pages".into()));
    }
    for item in plan {
        if item.blank {
            continue;
        }
        if item.source_page == 0 || item.source_page > total {
            return Err(PdfError::RangeOutOfBounds);
        }
    }
    materialize_all_pages(&mut doc)?;
    rebuild_page_tree(&mut doc, plan)?;
    let final_path = resolve_output_path(output, policy)?;
    save_document(&mut doc, &final_path, true)?;
    Ok(final_path)
}

pub fn extract_pages(
    input: &Path,
    pages: &[u32],
    output: &Path,
    policy: OverwritePolicy,
    password: Option<&str>,
) -> PdfResult<PathBuf> {
    let plan: Vec<PagePlanItem> =
        pages.iter().map(|p| PagePlanItem { source_page: *p, ..Default::default() }).collect();
    apply_page_plan(input, &plan, output, policy, password)
}

pub fn delete_pages(
    input: &Path,
    pages: &[u32],
    output: &Path,
    policy: OverwritePolicy,
    password: Option<&str>,
) -> PdfResult<PathBuf> {
    let mut doc = load_document(input, password)?;
    let total = doc.get_pages().len() as u32;
    if total == 0 {
        return Err(PdfError::InvalidPdf("document has no pages".into()));
    }
    let delete: std::collections::HashSet<u32> = pages.iter().copied().collect();
    if delete.len() as u32 >= total {
        return Err(PdfError::InvalidInput("cannot delete every page of the document".into()));
    }
    let plan: Vec<PagePlanItem> = (1..=total)
        .filter(|p| !delete.contains(p))
        .map(|p| PagePlanItem { source_page: p, ..Default::default() })
        .collect();
    materialize_all_pages(&mut doc)?;
    rebuild_page_tree(&mut doc, &plan)?;
    let final_path = resolve_output_path(output, policy)?;
    save_document(&mut doc, &final_path, true)?;
    Ok(final_path)
}

pub fn rotate_pages(
    input: &Path,
    pages: &[u32],
    degrees: i32,
    output: &Path,
    policy: OverwritePolicy,
    password: Option<&str>,
) -> PdfResult<PathBuf> {
    if !matches!(degrees, 90 | 180 | 270 | -90 | -180 | -270) {
        return Err(PdfError::InvalidInput("rotation must be 90, 180 or 270 degrees".into()));
    }
    let mut doc = load_document(input, password)?;
    let total = doc.get_pages().len() as u32;
    let target: std::collections::HashSet<u32> =
        if pages.is_empty() { (1..=total).collect() } else { pages.iter().copied().collect() };
    for p in &target {
        if *p == 0 || *p > total {
            return Err(PdfError::RangeOutOfBounds);
        }
    }
    let plan: Vec<PagePlanItem> = (1..=total)
        .map(|p| PagePlanItem {
            source_page: p,
            rotation_delta: if target.contains(&p) { degrees } else { 0 },
            ..Default::default()
        })
        .collect();
    materialize_all_pages(&mut doc)?;
    rebuild_page_tree(&mut doc, &plan)?;
    let final_path = resolve_output_path(output, policy)?;
    save_document(&mut doc, &final_path, true)?;
    Ok(final_path)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SplitPart {
    pub path: String,
    pub first_page: u32,
    pub last_page: u32,
}

/// Splits a document into several files. `output_dir` is created if missing.
/// File names follow `<stem>_<range>.pdf`.
pub fn split_pdf(
    input: &Path,
    mode: &SplitMode,
    output_dir: &Path,
    policy: OverwritePolicy,
    password: Option<&str>,
    progress: &ProgressCallback,
    cancel: &CancelToken,
) -> PdfResult<Vec<SplitPart>> {
    let doc = load_document(input, password)?;
    let total = doc.get_pages().len() as u32;
    let groups = plan_split(mode, total)?;
    if !output_dir.exists() {
        std::fs::create_dir_all(output_dir).map_err(PdfError::from_io)?;
    }
    let stem = input.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "document".to_string());
    let mut parts = Vec::new();
    for (index, (a, b)) in groups.iter().enumerate() {
        cancel.check()?;
        progress(ProgressEvent::new("split.part", (index + 1) as u64, groups.len() as u64));
        let mut working = doc.clone();
        materialize_all_pages(&mut working)?;
        let plan: Vec<PagePlanItem> =
            (*a..=*b).map(|p| PagePlanItem { source_page: p, ..Default::default() }).collect();
        rebuild_page_tree(&mut working, &plan)?;
        let name = if groups.len() == 1 || a == b && groups.len() > 50 {
            format!("{stem}_page_{a:04}.pdf")
        } else {
            format!("{stem}_{a:04}-{b:04}.pdf")
        };
        let target = resolve_output_path(&output_dir.join(name), policy)?;
        save_document(&mut working, &target, true)?;
        parts.push(SplitPart { path: target.display().to_string(), first_page: *a, last_page: *b });
    }
    Ok(parts)
}

/// Splits into individual pages (convenience used by the UI).
pub fn split_individual(
    input: &Path,
    output_dir: &Path,
    policy: OverwritePolicy,
    progress: &ProgressCallback,
    cancel: &CancelToken,
) -> PdfResult<Vec<SplitPart>> {
    split_pdf(input, &SplitMode::Individual, output_dir, policy, None, progress, cancel)
}

pub fn split_group_summary(groups: &[(u32, u32)]) -> String {
    format_groups(groups)
}

/// Expands groups to page numbers (re-export for command layer).
pub fn expand_groups(groups: &[(u32, u32)]) -> Vec<u32> {
    groups_to_pages(groups)
}
