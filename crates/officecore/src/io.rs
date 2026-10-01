//! Small file helpers shared by the document engines.

use crate::error::{ErrorCode, OfficeError, OfficeResult};
use std::path::Path;

/// Largest file we open for office formats (protects against accidental huge inputs).
pub const MAX_INPUT_BYTES: u64 = 512 * 1024 * 1024;

pub fn read_bytes(path: &Path) -> OfficeResult<Vec<u8>> {
    let metadata = std::fs::metadata(path).map_err(|error| OfficeError::from_io(error, path))?;
    if metadata.len() > MAX_INPUT_BYTES {
        return Err(OfficeError::new(ErrorCode::TooLarge, format!("{} is larger than the supported limit.", path.display())));
    }
    std::fs::read(path).map_err(|error| OfficeError::from_io(error, path))
}

/// Writes atomically: a temp sibling is written, flushed to stable storage and
/// then renamed over the target, so a crash mid-write never leaves a
/// half-written document behind.
///
/// The rename intentionally does **not** remove the target first: `fs::rename`
/// replaces an existing file atomically on both Unix and Windows
/// (`MoveFileExW` with `MOVEFILE_REPLACE_EXISTING`). The previous
/// remove-then-rename sequence opened a window where the target did not exist
/// at all and a crash there lost the old file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> OfficeResult<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|error| OfficeError::from_io(error, parent))?;
        }
    }
    let temp = temp_sibling(path);
    {
        use std::io::Write;
        let mut file = std::fs::File::create(&temp).map_err(|error| OfficeError::from_io(error, &temp))?;
        file.write_all(bytes).map_err(|error| OfficeError::from_io(error, &temp))?;
        // Durability barrier: without this a power failure after the rename can
        // leave a zero-length or partially written file.
        file.sync_all().map_err(|error| OfficeError::from_io(error, &temp))?;
    }
    std::fs::rename(&temp, path).map_err(|error| {
        let _ = std::fs::remove_file(&temp);
        OfficeError::from_io(error, path)
    })?;
    sync_parent_dir(path);
    Ok(())
}

/// Best-effort directory fsync so the rename itself is durable. Windows cannot
/// open directories for sync; there the rename is already ordered by the
/// filesystem journal, so the call is a no-op.
fn sync_parent_dir(path: &Path) {
    #[cfg(unix)]
    {
        if let Some(parent) = path.parent() {
            if let Ok(dir) = std::fs::File::open(parent) {
                let _ = dir.sync_all();
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

fn temp_sibling(path: &Path) -> std::path::PathBuf {
    let name = path.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_else(|| "document".into());
    let temp_name = format!(".{name}.{}.tmp", uuid::Uuid::new_v4().simple());
    path.with_file_name(temp_name)
}

/// Lowercase extension without the dot ("DOCX" -> "docx"), or "docx" if absent.
pub fn extension_of(path: &Path) -> String {
    path.extension().map(|value| value.to_string_lossy().to_ascii_lowercase()).unwrap_or_default()
}

/// Human readable document title from a path.
pub fn file_stem(path: &Path) -> String {
    path.file_stem().map(|value| value.to_string_lossy().to_string()).unwrap_or_else(|| "Document".into())
}

/// Normalizes a hex color to "#RRGGBB" or returns None.
pub fn normalize_hex(value: &str) -> Option<String> {
    let trimmed = value.trim().trim_start_matches('#');
    if trimmed.len() == 3 && trimmed.chars().all(|ch| ch.is_ascii_hexdigit()) {
        let mut out = String::from("#");
        for ch in trimmed.chars() {
            out.push(ch);
            out.push(ch);
        }
        return Some(out.to_ascii_uppercase());
    }
    if trimmed.len() == 6 && trimmed.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Some(format!("#{}", trimmed.to_ascii_uppercase()));
    }
    None
}

/// Word/OOXML expects "RRGGBB" without the hash.
pub fn bare_hex(value: &str) -> Option<String> {
    normalize_hex(value).map(|value| value.trim_start_matches('#').to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_normalization() {
        assert_eq!(normalize_hex("#abc").as_deref(), Some("#AABBCC"));
        assert_eq!(bare_hex("1f2937").as_deref(), Some("1F2937"));
        assert_eq!(normalize_hex("zzz"), None);
    }

    #[test]
    fn atomic_write_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.bin");
        write_atomic(&path, b"one").unwrap();
        write_atomic(&path, b"two").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"two");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
