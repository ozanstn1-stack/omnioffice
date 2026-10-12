//! Local signature / paraphe library.
//!
//! A signature or a paraphe the user draws once is stored as a PNG under
//! `<app config dir>/signatures/<name>.png`, so it can be reused without
//! redrawing. The files never leave the machine; the name is sanitised so a
//! save can never escape the directory, and the image is size-capped.

use base64::Engine;
use pdfcore::error::PdfError;
use serde::Serialize;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

/// Largest stored signature image (2 MB).
const MAX_SIGNATURE_BYTES: usize = 2 * 1024 * 1024;

/// Keeps letters, digits, spaces, `-` and `_`, capped at 48 characters. A name
/// that reduces to nothing is rejected by the caller.
pub fn sanitize_name(name: &str) -> String {
    name.chars()
        .filter(|ch| ch.is_alphanumeric() || *ch == '-' || *ch == '_' || *ch == ' ')
        .take(48)
        .collect::<String>()
        .trim()
        .to_string()
}

/// Strips an optional `data:` URL prefix and decodes the base64 payload.
fn decode_image(value: &str) -> Result<Vec<u8>, PdfError> {
    let payload = value.split(',').next_back().unwrap_or(value).trim();
    base64::engine::general_purpose::STANDARD
        .decode(payload.as_bytes())
        .map_err(|_| PdfError::InvalidInput("The signature image is not valid base64.".into()))
}

fn signatures_dir(app: &AppHandle) -> Result<PathBuf, PdfError> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|error| PdfError::Internal(format!("Could not locate the app data directory: {error}")))?
        .join("signatures");
    std::fs::create_dir_all(&dir).map_err(PdfError::from_io)?;
    Ok(dir)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureEntry {
    pub name: String,
    pub image_base64: String,
}

#[tauri::command]
pub fn signature_save(app: AppHandle, name: String, image_base64: String) -> Result<(), PdfError> {
    let name = sanitize_name(&name);
    if name.is_empty() {
        return Err(PdfError::InvalidInput("Give the signature a name.".into()));
    }
    let data = decode_image(&image_base64)?;
    if data.is_empty() {
        return Err(PdfError::InvalidInput("The signature image is empty.".into()));
    }
    if data.len() > MAX_SIGNATURE_BYTES {
        return Err(PdfError::InvalidInput("The signature image is larger than the 2 MB limit.".into()));
    }
    let path = signatures_dir(&app)?.join(format!("{name}.png"));
    crate::commands::write_atomic(&path, &data)?;
    Ok(())
}

#[tauri::command]
pub fn signature_list(app: AppHandle) -> Result<Vec<SignatureEntry>, PdfError> {
    let dir = signatures_dir(&app)?;
    let mut entries = Vec::new();
    if let Ok(read) = std::fs::read_dir(&dir) {
        for entry in read.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("png") {
                continue;
            }
            let Some(name) = path.file_stem().and_then(|stem| stem.to_str()) else { continue };
            if let Ok(bytes) = std::fs::read(&path) {
                entries.push(SignatureEntry {
                    name: name.to_string(),
                    image_base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
                });
            }
        }
    }
    entries.sort_by_key(|entry| entry.name.to_lowercase());
    Ok(entries)
}

#[tauri::command]
pub fn signature_delete(app: AppHandle, name: String) -> Result<(), PdfError> {
    let name = sanitize_name(&name);
    if name.is_empty() {
        return Ok(());
    }
    let path = signatures_dir(&app)?.join(format!("{name}.png"));
    if path.exists() {
        std::fs::remove_file(&path).map_err(PdfError::from_io)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_sanitized_and_bounded() {
        assert_eq!(sanitize_name("İmza 1"), "İmza 1");
        assert_eq!(sanitize_name("../../etc/passwd"), "etcpasswd");
        assert_eq!(sanitize_name("a/b\\c:d"), "abcd");
        assert_eq!(sanitize_name("   "), "");
        assert_eq!(sanitize_name(&"x".repeat(100)).chars().count(), 48);
    }

    #[test]
    fn data_urls_and_plain_base64_both_decode() {
        let raw = base64::engine::general_purpose::STANDARD.encode(b"png-bytes");
        assert_eq!(decode_image(&raw).unwrap(), b"png-bytes");
        assert_eq!(decode_image(&format!("data:image/png;base64,{raw}")).unwrap(), b"png-bytes");
        assert!(decode_image("not base64!!").is_err());
    }
}
