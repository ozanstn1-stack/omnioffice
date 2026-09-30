//! Canonical path policy for the IPC surface.
//!
//! The webview is the only untrusted boundary in the app: every Rust command
//! receives paths from it. This module centralizes the baseline rules so a
//! malformed or hostile invocation cannot smuggle NUL bytes, control
//! characters, traversal components or Windows device names into the file
//! system code:
//!
//! * inputs must be absolute, exist and be regular files;
//! * outputs must be absolute and live in an existing directory;
//! * neither may contain `..`, `NUL` or reserved device names.
//!
//! This is the V3.1 foundation of the file-handle/token architecture: the
//! complete opaque-token design (no raw paths over IPC at all) is scheduled
//! for V4.0.

use pdfcore::error::PdfError;
use std::path::{Component, Path, PathBuf};

fn normalize(raw: &str) -> Result<&Path, PdfError> {
    if raw.trim().is_empty() {
        return Err(PdfError::InvalidInput("the path is empty".to_string()));
    }
    if raw.contains('\0') {
        return Err(PdfError::InvalidInput("the path contains a NUL byte".to_string()));
    }
    let path = Path::new(raw);
    if !path.is_absolute() {
        return Err(PdfError::InvalidInput(format!("the path must be absolute: {raw}")));
    }
    for component in path.components() {
        match component {
            // Dialog and recents always hand out normalized absolute paths;
            // a `..` component can only come from a malformed/hostile caller.
            Component::ParentDir => {
                return Err(PdfError::InvalidInput(format!("the path must not contain '..': {raw}")))
            }
            Component::Normal(name) => {
                let text = name.to_string_lossy();
                if text.chars().any(char::is_control) {
                    return Err(PdfError::InvalidInput(format!("the path contains control characters: {raw}")));
                }
                if is_windows_reserved(&text) {
                    return Err(PdfError::InvalidInput(format!("the path uses a reserved device name: {raw}")));
                }
            }
            _ => {}
        }
    }
    Ok(path)
}

/// Windows reserved device names (`CON`, `NUL`, `COM1`…) resolve to devices
/// instead of files; refuse them on every platform so behavior is identical.
fn is_windows_reserved(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).trim_end_matches(' ').to_ascii_lowercase();
    matches!(
        stem.as_str(),
        "con"
            | "prn"
            | "aux"
            | "nul"
            | "com1"
            | "com2"
            | "com3"
            | "com4"
            | "com5"
            | "com6"
            | "com7"
            | "com8"
            | "com9"
            | "lpt1"
            | "lpt2"
            | "lpt3"
            | "lpt4"
            | "lpt5"
            | "lpt6"
            | "lpt7"
            | "lpt8"
            | "lpt9"
    )
}

/// Validates an input path coming from the webview. The file must exist and be
/// a regular file; nothing is rewritten (no `\\?\` prefix surprises).
pub fn input_file(raw: &str) -> Result<PathBuf, PdfError> {
    let path = normalize(raw)?;
    if !path.exists() {
        return Err(PdfError::NotFound(raw.to_string()));
    }
    if !path.is_file() {
        return Err(PdfError::InvalidInput(format!("not a file: {raw}")));
    }
    Ok(path.to_path_buf())
}

/// Validates an output path coming from the webview. The parent directory must
/// already exist: commands never create arbitrary directory chains from IPC.
pub fn output_file(raw: &str) -> Result<PathBuf, PdfError> {
    let path = normalize(raw)?;
    if path.is_dir() {
        return Err(PdfError::InvalidInput(format!("expected a file path, got a folder: {raw}")));
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| PdfError::InvalidInput(format!("the output path has no folder: {raw}")))?;
    if !parent.is_dir() {
        return Err(PdfError::InvalidInput(format!("the output folder does not exist: {}", parent.display())));
    }
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pdfsak-paths-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn input_requires_an_existing_regular_file() {
        let dir = scratch_dir();
        let file = dir.join("input.pdf");
        std::fs::write(&file, b"%PDF").expect("write fixture");
        assert_eq!(input_file(file.to_str().unwrap()).unwrap(), file);

        assert!(matches!(input_file(dir.to_str().unwrap()), Err(PdfError::InvalidInput(_))));
        assert!(matches!(input_file(dir.join("missing.pdf").to_str().unwrap()), Err(PdfError::NotFound(_))));
    }

    #[test]
    fn traversal_and_device_names_are_refused() {
        assert!(input_file("C:\\docs\\..\\secret.pdf").is_err());
        assert!(input_file("relative.pdf").is_err());
        assert!(input_file("").is_err());
        assert!(input_file("C:\\docs\\nul.pdf").is_err());
        assert!(input_file("C:\\docs\\con").is_err());
        assert!(output_file("C:\\docs\\..\\out.pdf").is_err());
        assert!(output_file("out.pdf").is_err());
        assert!(output_file("").is_err());
    }

    #[test]
    fn output_requires_an_existing_parent_folder() {
        let dir = scratch_dir();
        let target = dir.join("out.pdf");
        assert_eq!(output_file(target.to_str().unwrap()).unwrap(), target);
        assert!(matches!(
            output_file(dir.join("missing-dir").join("out.pdf").to_str().unwrap()),
            Err(PdfError::InvalidInput(_))
        ));
    }
}
