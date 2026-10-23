//! Byte reads after path validation, kept apart from the validators so
//! `paths.rs` stays under its line budget.

use crate::paths::input_file;
use pdfcore::PdfError;
use std::io::Read;

/// Reads an input file after validating the *opened* handle.
///
/// The path is validated lexically first, then the file is opened and its
/// metadata is read from the handle. On Unix a symlink swap between the check
/// and the open is closed by comparing the handle metadata against the path
/// metadata; the remaining Windows limitation (no `O_NOFOLLOW`) is documented
/// and covered by the lexical rules above.
pub fn read_input_file(raw: &str, max_bytes: u64) -> Result<Vec<u8>, PdfError> {
    let validated = input_file(raw)?;
    let mut file = std::fs::File::open(validated.as_path()).map_err(PdfError::from_io)?;
    let handle_meta = file.metadata().map_err(PdfError::from_io)?;
    if !handle_meta.is_file() {
        return Err(PdfError::InvalidInput(format!("not a file: {raw}")));
    }
    if handle_meta.len() > max_bytes {
        return Err(PdfError::InvalidInput(format!(
            "the file is larger than the {max_bytes} byte safety limit: {raw}"
        )));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let path_meta = std::fs::metadata(validated.as_path()).map_err(PdfError::from_io)?;
        if handle_meta.dev() != path_meta.dev() || handle_meta.ino() != path_meta.ino() {
            return Err(PdfError::InvalidInput(
                "the file changed between validation and open (possible symlink swap)".to_string(),
            ));
        }
    }
    let mut bytes = Vec::with_capacity(handle_meta.len().min(max_bytes) as usize);
    file.read_to_end(&mut bytes).map_err(PdfError::from_io)?;
    if bytes.len() as u64 > max_bytes {
        return Err(PdfError::InvalidInput(format!(
            "the file grew past the {max_bytes} byte safety limit while reading: {raw}"
        )));
    }
    Ok(bytes)
}
