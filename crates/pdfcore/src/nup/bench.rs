//! Imposition with a chosen font-cache budget.
//!
//! The cache is a per-document optimisation whose absence is invisible in the
//! output, so the way to prove it does something - and that it changes nothing -
//! is to run the same document with the budget set to zero. That is what the
//! benchmark and the byte-identity test do; nothing else should need this knob.

use lopdf::Document;

use super::{build_nup, CancelToken, NupOptions, PdfError, PdfResult, ProgressEvent};

/// Imposes `input` with `budget` bytes of room for parsed fonts. Zero reads every
/// font on every note, which is how this code behaved before the cache.
pub fn nup_pdf_bytes_with_font_cache(input: &[u8], options: &NupOptions, budget: usize) -> PdfResult<Vec<u8>> {
    impose_bytes(input, options, budget)
}

/// The one body both entry points use.
pub(super) fn impose_bytes(input: &[u8], options: &NupOptions, budget: usize) -> PdfResult<Vec<u8>> {
    let mut doc = Document::load_mem(input).map_err(|error| PdfError::from_lopdf(error, None))?;
    if doc.is_encrypted() {
        return Err(PdfError::PasswordRequired);
    }
    let cancel = CancelToken::new();
    let no_progress = |_event: ProgressEvent| {};
    build_nup(&mut doc, options, &no_progress, &cancel, budget)?;
    doc.compress();
    doc.prune_objects();
    doc.renumber_objects();
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).map_err(PdfError::from_io)?;
    Ok(bytes)
}
