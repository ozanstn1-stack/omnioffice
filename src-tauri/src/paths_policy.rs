//! Raw-path policy for commands that accept capability tokens.

use pdfcore::error::PdfError;

/// Whether a raw path from the webview is accepted. Desktop release builds
/// require a capability token; Android's SAF bridge returns real cache paths,
/// and debug/E2E builds still pass fixture paths to commands.
pub(crate) fn raw_paths_allowed() -> bool {
    cfg!(any(debug_assertions, target_os = "android"))
}

pub(crate) const RAW_PATH_ERROR: &str = "the renderer must pass a file token instead of a path";

pub(crate) fn ensure_raw_allowed() -> Result<(), PdfError> {
    if raw_paths_allowed() {
        Ok(())
    } else {
        Err(PdfError::InvalidInput(RAW_PATH_ERROR.to_string()))
    }
}
