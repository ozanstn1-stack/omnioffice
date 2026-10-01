//! Storage for the DeepSeek API key.
//!
//! On Windows the key is encrypted with DPAPI (CryptProtectData), so the
//! stored blob can only be decrypted by the same Windows user on the same
//! machine. On other platforms (future ports) it falls back to plain text and
//! the UI is told about it, so nothing pretends to be safer than it is.

use pdfcore::error::PdfError;
use std::path::Path;

const DPAPI_PREFIX: &str = "dpapi:";
const PLAIN_PREFIX: &str = "plain:";

#[cfg(windows)]
fn protect(plaintext: &[u8]) -> Result<Vec<u8>, String> {
    use windows::Win32::Foundation::LocalFree;
    use windows::Win32::Security::Cryptography::{CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB};
    unsafe {
        let input = CRYPT_INTEGER_BLOB { cbData: plaintext.len() as u32, pbData: plaintext.as_ptr() as *mut u8 };
        let mut output = CRYPT_INTEGER_BLOB::default();
        CryptProtectData(
            &input,
            windows::core::PCWSTR::null(),
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
        .map_err(|error| format!("DPAPI protect failed: {error}"))?;
        let slice = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(Some(windows::Win32::Foundation::HLOCAL(output.pbData as *mut _)));
        Ok(slice)
    }
}

#[cfg(windows)]
fn unprotect(blob: &[u8]) -> Result<Vec<u8>, String> {
    use windows::Win32::Foundation::LocalFree;
    use windows::Win32::Security::Cryptography::{CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB};
    unsafe {
        let input = CRYPT_INTEGER_BLOB { cbData: blob.len() as u32, pbData: blob.as_ptr() as *mut u8 };
        let mut output = CRYPT_INTEGER_BLOB::default();
        CryptUnprotectData(&input, None, None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut output)
            .map_err(|error| format!("DPAPI unprotect failed: {error}"))?;
        let slice = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(Some(windows::Win32::Foundation::HLOCAL(output.pbData as *mut _)));
        Ok(slice)
    }
}

#[cfg(not(windows))]
fn protect(plaintext: &[u8]) -> Result<Vec<u8>, String> {
    let _ = plaintext;
    Err("secure storage is only implemented for Windows".into())
}

#[cfg(not(windows))]
fn unprotect(blob: &[u8]) -> Result<Vec<u8>, String> {
    let _ = blob;
    Err("secure storage is only implemented for Windows".into())
}

/// Writes the API key to `path`, encrypted when possible.
/// Returns `true` when DPAPI encryption was used.
pub fn save_api_key(path: &Path, key: &str) -> Result<bool, PdfError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(PdfError::from_io)?;
    }
    let trimmed = key.trim();
    let payload = match protect(trimmed.as_bytes()) {
        Ok(blob) => format!("{DPAPI_PREFIX}{}", base64_encode(&blob)),
        Err(_) => format!("{PLAIN_PREFIX}{}", base64_encode(trimmed.as_bytes())),
    };
    // A torn write here would make the key unrecoverable, so it must never
    // truncate the previous blob in place.
    crate::commands::write_atomic(path, payload.as_bytes())?;
    Ok(std::fs::read_to_string(path).map(|content| content.starts_with(DPAPI_PREFIX)).unwrap_or(false))
}

/// Reads the API key, decrypting it when it was stored with DPAPI.
pub fn load_api_key(path: &Path) -> Result<String, PdfError> {
    if !path.exists() {
        return Ok(String::new());
    }
    let content = std::fs::read_to_string(path).map_err(PdfError::from_io)?;
    let content = content.trim();
    if let Some(encoded) = content.strip_prefix(DPAPI_PREFIX) {
        let blob = base64_decode(encoded).ok_or_else(|| PdfError::Internal("invalid key blob".to_string()))?;
        let plain = unprotect(&blob).map_err(PdfError::Internal)?;
        return String::from_utf8(plain).map_err(|_| PdfError::Internal("invalid key encoding".to_string()));
    }
    if let Some(encoded) = content.strip_prefix(PLAIN_PREFIX) {
        let bytes = base64_decode(encoded).ok_or_else(|| PdfError::Internal("invalid key blob".to_string()))?;
        return String::from_utf8(bytes).map_err(|_| PdfError::Internal("invalid key encoding".to_string()));
    }
    Ok(String::new())
}

pub fn delete_api_key(path: &Path) -> Result<(), PdfError> {
    if path.exists() {
        std::fs::remove_file(path).map_err(PdfError::from_io)?;
    }
    Ok(())
}

fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn base64_decode(value: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(value.trim()).ok()
}
