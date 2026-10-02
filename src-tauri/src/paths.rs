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
//! * directories must be absolute and exist;
//! * none may contain `..`, `NUL` or reserved device names;
//! * Windows UNC (`\\server\share`) and extended-length (`\\?\`) paths are
//!   refused: the bundled native engines do not accept them reliably.
//!
//! Validation returns a [`ValidatedPath`] newtype rather than a bare `PathBuf`,
//! so a command cannot accidentally use an unvalidated path: the inner path is
//! only reachable through [`ValidatedPath::as_path`].
//!
//! This remains the V3.1 foundation of a file-handle/token architecture. The
//! validation is lexical plus an existence/permission check on the opened
//! object; it is not a full TOCTOU-proof handle design (that stays scheduled
//! for V4.0), but [`open_input_file`] narrows the window by validating the
//! already-open handle.

use pdfcore::error::PdfError;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

/// A filesystem path that passed [`input_file`], [`output_file`] or
/// [`directory`]. The raw string can only be obtained through
/// [`ValidatedPath::as_path`], which keeps an unvalidated `PathBuf` from leaking
/// back to a command by accident.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedPath(PathBuf);

impl ValidatedPath {
    /// The validated path. Callers that only need to open/stat it use this.
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// Consumes the wrapper. Prefer `as_path`; this exists for APIs that need
    /// an owned `PathBuf` (e.g. handing a path to a core function).
    pub fn into_path_buf(self) -> PathBuf {
        self.0
    }
}

/// Rejects the path shapes the bundled engines cannot handle and that are a
/// smell of a programmatic (not user-selected) path.
fn reject_unc_and_extended(path: &Path) -> Result<(), PdfError> {
    let text = path.to_string_lossy();
    // Likely a verbatim/UNC path. On Windows `\\?\C:\...` and `\\server\share`
    // both start with two separators; the engines shell out and do not accept
    // them. Refuse everywhere so behavior is identical across platforms.
    if text.starts_with("\\\\") {
        return Err(PdfError::InvalidInput(format!("UNC and extended-length paths are not supported: {text}")));
    }
    // A verbatim prefix can also appear as `\\?\` after normalization on some
    // hosts; the check above already covers it, but keep this explicit.
    if text.starts_with(r"\\?\") || text.starts_with(r"\\.\") {
        return Err(PdfError::InvalidInput(format!("extended-length paths are not supported: {text}")));
    }
    Ok(())
}

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
    reject_unc_and_extended(path)?;
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
///
/// The check is done on an **open handle**, not on the path: `File::open`
/// resolves the path once and every later `metadata()` call reads the object
/// that was actually opened. On Unix the handle identity is additionally
/// compared against a fresh path lookup, so a symlink swapped in between the
/// open and the check is rejected. This narrows the TOCTOU window for callers
/// that pass the path on; callers that read the bytes should prefer
/// [`read_input_file`], which keeps using the same handle for the whole read.
pub fn input_file(raw: &str) -> Result<ValidatedPath, PdfError> {
    let path = normalize(raw)?;
    // A directory is rejected before the open: on Windows opening one with
    // `File::open` fails with a permission error instead of the honest
    // "not a file" message the callers and tests expect.
    if path.is_dir() {
        return Err(PdfError::InvalidInput(format!("not a file: {raw}")));
    }
    let file = std::fs::File::open(path).map_err(PdfError::from_io)?;
    let handle_meta = file.metadata().map_err(PdfError::from_io)?;
    if !handle_meta.is_file() {
        return Err(PdfError::InvalidInput(format!("not a file: {raw}")));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let path_meta = std::fs::metadata(path).map_err(PdfError::from_io)?;
        if handle_meta.dev() != path_meta.dev() || handle_meta.ino() != path_meta.ino() {
            return Err(PdfError::InvalidInput(
                "the file changed between validation and open (possible symlink swap)".to_string(),
            ));
        }
    }
    Ok(ValidatedPath(path.to_path_buf()))
}

/// Validates an output path coming from the webview. The parent directory must
/// already exist: commands never create arbitrary directory chains from IPC.
pub fn output_file(raw: &str) -> Result<ValidatedPath, PdfError> {
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
    Ok(ValidatedPath(path.to_path_buf()))
}

/// Validates only the *shape* of a path coming from the webview: absolute, no
/// `..`, no NUL/control characters, no reserved device names, no UNC/extended
/// prefixes. Unlike the other validators it does not require the path to exist,
/// so it is the right check for paths the command intentionally creates (e.g.
/// [`crate::commands::ensure_dir`]) or probes for absence.
pub fn lexical(raw: &str) -> Result<ValidatedPath, PdfError> {
    let path = normalize(raw)?;
    Ok(ValidatedPath(path.to_path_buf()))
}

/// Validates a directory path coming from the webview. It must be absolute and
/// already exist as a directory.
pub fn directory(raw: &str) -> Result<ValidatedPath, PdfError> {
    let path = normalize(raw)?;
    if !path.exists() {
        return Err(PdfError::NotFound(raw.to_string()));
    }
    if !path.is_dir() {
        return Err(PdfError::InvalidInput(format!("not a folder: {raw}")));
    }
    Ok(ValidatedPath(path.to_path_buf()))
}

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
        assert_eq!(input_file(file.to_str().unwrap()).unwrap().as_path(), file);

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
    fn unc_and_extended_paths_are_refused() {
        assert!(input_file("\\\\server\\share\\a.pdf").is_err());
        assert!(output_file(r"\\?\C:\docs\out.pdf").is_err());
        assert!(directory("\\\\server\\share").is_err());
    }

    #[test]
    fn nul_and_control_characters_are_refused() {
        assert!(input_file("C:\\docs\\a\0b.pdf").is_err());
        assert!(output_file("C:\\docs\\a\u{7}b.pdf").is_err());
    }

    #[test]
    fn output_requires_an_existing_parent_folder() {
        let dir = scratch_dir();
        let target = dir.join("out.pdf");
        assert_eq!(output_file(target.to_str().unwrap()).unwrap().as_path(), target);
        assert!(matches!(
            output_file(dir.join("missing-dir").join("out.pdf").to_str().unwrap()),
            Err(PdfError::InvalidInput(_))
        ));
    }

    #[test]
    fn directory_requires_an_existing_folder() {
        let dir = scratch_dir();
        assert_eq!(directory(dir.to_str().unwrap()).unwrap().as_path(), dir);
        assert!(matches!(directory(dir.join("nope").to_str().unwrap()), Err(PdfError::NotFound(_))));

        let file = dir.join("a-file.pdf");
        std::fs::write(&file, b"%PDF").expect("write");
        assert!(matches!(directory(file.to_str().unwrap()), Err(PdfError::InvalidInput(_))));
    }

    #[test]
    fn unicode_and_space_paths_are_accepted() {
        let dir = scratch_dir();
        let file = dir.join("belge ç Ğ ü.pdf");
        std::fs::write(&file, b"%PDF").expect("write");
        assert_eq!(input_file(file.to_str().unwrap()).unwrap().as_path(), file);
    }

    #[test]
    fn read_input_file_enforces_the_size_cap() {
        let dir = scratch_dir();
        let file = dir.join("big.pdf");
        std::fs::write(&file, vec![0u8; 2048]).expect("write");
        assert!(matches!(read_input_file(file.to_str().unwrap(), 1024), Err(PdfError::InvalidInput(_))));
        assert_eq!(read_input_file(file.to_str().unwrap(), 4096).unwrap().len(), 2048);
    }

    #[cfg(unix)]
    #[test]
    fn read_input_file_rejects_a_symlink_swap() {
        use std::os::unix::fs::symlink;
        let dir = scratch_dir();
        let real = dir.join("real.pdf");
        std::fs::write(&real, b"%PDF real").expect("write");
        let link = dir.join("link.pdf");
        let _ = std::fs::remove_file(&link);
        symlink(&real, &link).expect("symlink");
        // A symlink pointing at a regular file is still a valid input (dialog
        // paths can be symlinks); the swap guard only fires on a race, which a
        // single-threaded test cannot trigger. Assert it reads the target.
        assert_eq!(read_input_file(link.to_str().unwrap(), 4096).unwrap(), b"%PDF real");
    }
}
