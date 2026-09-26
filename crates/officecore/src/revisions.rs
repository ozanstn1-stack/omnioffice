//! Accept / reject helpers for Writer tracked changes.
//!
//! The model keeps deleted text in the document (marked with a `delete`
//! revision) until the user accepts or rejects it, which is what makes
//! accept/reject lossless: nothing is thrown away before the decision.
//!
//! Reviewing rules:
//! * accept insert: keep the run, clear the mark
//! * reject insert: remove the run
//! * accept delete: remove the run
//! * reject delete: keep the run, clear the mark
//! * accept format: keep the current formatting, clear the mark
//! * reject format: restore the captured original formatting, clear the mark
//!
//! Removing runs never removes the paragraph itself, so the paragraph
//! structure (and any following section break) stays stable.

use crate::model::{Block, RevisionMark, Run, TextDocument};

/// One pending revision, as shown in the review pane.
#[derive(Debug, Clone, PartialEq)]
pub struct RevisionSummary {
    pub id: String,
    pub kind: String,
    pub author: String,
    pub date: String,
    pub text: String,
    pub block_index: usize,
}

impl RevisionSummary {
    pub fn is_insert(&self) -> bool {
        self.kind == "insert"
    }

    pub fn is_delete(&self) -> bool {
        self.kind == "delete"
    }

    pub fn is_format(&self) -> bool {
        self.kind == "format"
    }
}

/// Every pending revision in document order.
pub fn revision_list(document: &TextDocument) -> Vec<RevisionSummary> {
    let mut out = Vec::new();
    for (block_index, block) in document.blocks.iter().enumerate() {
        collect_block(block, block_index, &mut out);
    }
    out
}

fn collect_block(block: &Block, block_index: usize, out: &mut Vec<RevisionSummary>) {
    match block {
        Block::Paragraph { runs, .. } => {
            for run in runs {
                if let Some(revision) = &run.revision {
                    if out.iter().any(|summary| summary.id == revision.id) {
                        continue;
                    }
                    out.push(summary_for(revision, &run.text, block_index));
                }
            }
        }
        Block::Table { table } => {
            for row in &table.rows {
                for cell in &row.cells {
                    for block in &cell.blocks {
                        collect_block(block, block_index, out);
                    }
                }
            }
        }
        _ => {}
    }
}

fn summary_for(revision: &RevisionMark, text: &str, block_index: usize) -> RevisionSummary {
    RevisionSummary {
        id: revision.id.clone(),
        kind: revision.kind.clone(),
        author: revision.author.clone(),
        date: revision.date.clone(),
        text: text.chars().take(120).collect(),
        block_index,
    }
}

pub fn revision_count(document: &TextDocument) -> usize {
    revision_list(document).len()
}

/// The next (or previous) revision id after `after`, wrapping around.
pub fn next_revision(document: &TextDocument, after: Option<&str>, forward: bool) -> Option<String> {
    let revisions = revision_list(document);
    if revisions.is_empty() {
        return None;
    }
    let position = after.and_then(|id| revisions.iter().position(|summary| summary.id == id));
    let index = match (position, forward) {
        (Some(index), true) => (index + 1) % revisions.len(),
        (Some(index), false) => (index + revisions.len() - 1) % revisions.len(),
        (None, true) => 0,
        (None, false) => revisions.len() - 1,
    };
    Some(revisions[index].id.clone())
}

/// Applies one revision: `accept` keeps the change, otherwise it is rolled back.
pub fn resolve_revision(document: &mut TextDocument, id: &str, accept: bool) -> bool {
    let mut resolved = false;
    for block in document.blocks.iter_mut() {
        resolved |= resolve_block(block, id, accept);
    }
    resolved
}

pub fn accept_revision(document: &mut TextDocument, id: &str) -> bool {
    resolve_revision(document, id, true)
}

pub fn reject_revision(document: &mut TextDocument, id: &str) -> bool {
    resolve_revision(document, id, false)
}

fn resolve_block(block: &mut Block, id: &str, accept: bool) -> bool {
    let mut resolved = false;
    match block {
        Block::Paragraph { runs, .. } => {
            resolved |= resolve_runs(runs, id, accept);
        }
        Block::Table { table } => {
            for row in table.rows.iter_mut() {
                for cell in row.cells.iter_mut() {
                    for block in cell.blocks.iter_mut() {
                        resolved |= resolve_block(block, id, accept);
                    }
                }
            }
        }
        _ => {}
    }
    resolved
}

fn resolve_runs(runs: &mut Vec<Run>, id: &str, accept: bool) -> bool {
    let mut resolved = false;
    let mut index = 0;
    while index < runs.len() {
        let remove = if let Some(revision) = runs[index].revision.clone() {
            if revision.id != id {
                index += 1;
                continue;
            }
            resolved = true;
            match (revision.kind.as_str(), accept) {
                ("insert", true) => false,
                ("insert", false) => true,
                ("delete", true) => true,
                ("delete", false) => false,
                ("format", true) => false,
                ("format", false) => {
                    if let Some(original) = &revision.original {
                        runs[index].apply_format(original);
                    }
                    false
                }
                _ => false,
            }
        } else {
            false
        };
        if remove {
            runs.remove(index);
        } else {
            runs[index].revision = None;
            index += 1;
        }
    }
    resolved
}

/// Accepts every pending revision.
pub fn accept_all(document: &mut TextDocument) -> usize {
    resolve_all(document, true)
}

/// Rejects every pending revision.
pub fn reject_all(document: &mut TextDocument) -> usize {
    resolve_all(document, false)
}

fn resolve_all(document: &mut TextDocument, accept: bool) -> usize {
    let mut count = 0;
    loop {
        let next = revision_list(document).first().map(|summary| summary.id.clone());
        match next {
            Some(id) => {
                if resolve_revision(document, &id, accept) {
                    count += 1;
                } else {
                    // Should not happen; break to avoid a loop on malformed data.
                    break;
                }
            }
            None => break,
        }
    }
    document.track_changes = false;
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ParaProps, RevisionMark};

    fn mark(id: &str, kind: &str) -> RevisionMark {
        RevisionMark {
            id: id.into(),
            kind: kind.into(),
            author: "Ada".into(),
            date: "2026-01-01T00:00:00Z".into(),
            original: None,
        }
    }

    fn document_with_revisions() -> TextDocument {
        let mut document = TextDocument::new_blank("Review");
        document.blocks = vec![
            Block::Paragraph {
                props: ParaProps::default(),
                runs: vec![
                    Run { text: "kept ".into(), ..Default::default() },
                    Run { text: "added".into(), revision: Some(mark("i1", "insert")), ..Default::default() },
                ],
            },
            Block::Paragraph {
                props: ParaProps::default(),
                runs: vec![
                    Run { text: "gone".into(), revision: Some(mark("d1", "delete")), ..Default::default() },
                    Run { text: " stays".into(), ..Default::default() },
                ],
            },
        ];
        document
    }

    #[test]
    fn lists_revisions_in_order() {
        let document = document_with_revisions();
        let list = revision_list(&document);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, "i1");
        assert_eq!(list[0].kind, "insert");
        assert_eq!(list[1].kind, "delete");
        assert_eq!(revision_count(&document), 2);
    }

    #[test]
    fn accept_insert_keeps_text_and_reject_insert_removes_it() {
        let mut accepted = document_with_revisions();
        assert!(accept_revision(&mut accepted, "i1"));
        assert!(accepted.plain_text().contains("added"));
        let mut rejected = document_with_revisions();
        assert!(reject_revision(&mut rejected, "i1"));
        assert!(!rejected.plain_text().contains("added"));
    }

    #[test]
    fn accept_delete_removes_text_and_reject_delete_keeps_it() {
        let mut accepted = document_with_revisions();
        assert!(accept_revision(&mut accepted, "d1"));
        assert!(!accepted.plain_text().contains("gone"));
        let mut rejected = document_with_revisions();
        assert!(reject_revision(&mut rejected, "d1"));
        assert!(rejected.plain_text().contains("gone"));
    }

    #[test]
    fn format_revision_accepts_and_restores() {
        let mut document = TextDocument::new_blank("Format");
        let mut run = Run { text: "styled".into(), bold: true, ..Default::default() };
        let mut revision = mark("f1", "format");
        revision.original = Some(Run { bold: false, ..Default::default() }.format_snapshot());
        run.revision = Some(revision);
        document.blocks = vec![Block::Paragraph { props: ParaProps::default(), runs: vec![run] }];

        let mut accept = document.clone();
        accept_revision(&mut accept, "f1");
        assert!(accept.plain_text().contains("styled"));
        let Block::Paragraph { runs, .. } = &accept.blocks[0] else { panic!() };
        assert!(runs[0].bold);

        let mut reject = document;
        reject_revision(&mut reject, "f1");
        let Block::Paragraph { runs, .. } = &reject.blocks[0] else { panic!() };
        assert!(!runs[0].bold);
    }

    #[test]
    fn accept_all_and_reject_all_clear_everything() {
        let mut document = document_with_revisions();
        assert_eq!(accept_all(&mut document), 2);
        assert_eq!(revision_count(&document), 0);
        assert!(!document.plain_text().contains("gone"));
        assert!(document.plain_text().contains("added"));

        let mut document = document_with_revisions();
        assert_eq!(reject_all(&mut document), 2);
        assert_eq!(revision_count(&document), 0);
        assert!(document.plain_text().contains("gone"));
        assert!(!document.plain_text().contains("added"));
    }

    #[test]
    fn next_revision_walks_both_directions() {
        let document = document_with_revisions();
        assert_eq!(next_revision(&document, None, true).as_deref(), Some("i1"));
        assert_eq!(next_revision(&document, Some("i1"), true).as_deref(), Some("d1"));
        assert_eq!(next_revision(&document, Some("d1"), true).as_deref(), Some("i1"));
        assert_eq!(next_revision(&document, Some("i1"), false).as_deref(), Some("d1"));
        assert_eq!(next_revision(&document, None, false).as_deref(), Some("d1"));
    }
}
