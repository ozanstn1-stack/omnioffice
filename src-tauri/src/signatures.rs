//! Local signature / paraphe library.
//!
//! A signature or a paraphe the user draws once is stored under
//! `<app config dir>/signatures/<name>.sig`, so it can be reused without
//! redrawing. The files never leave the machine; the name is sanitised so a
//! save can never escape the directory, and the image is size-capped.
//!
//! The PNG is sealed like the API keys (`secret::seal_bytes`): DPAPI on
//! Windows, the Android Keystore on Android, `plain:` elsewhere. Up to 6.3.2
//! the PNG was stored as `<name>.png` in clear, so anything that could read the
//! app folder (a backup, another program running as the user) could copy a
//! handwritten signature; a legacy PNG is sealed and removed the first time
//! the library is listed.

use crate::secret::{self, SecretCipher};
use base64::Engine;
use pdfcore::error::PdfError;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

const SEALED: &str = "sig";
const LEGACY: &str = "png";

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

/// Seals `data` into `<dir>/<name>.sig` and drops a legacy plain PNG.
fn save_in(dir: &Path, cipher: Option<&dyn SecretCipher>, name: &str, data: &[u8]) -> Result<(), PdfError> {
    let payload = secret::seal_bytes(cipher, data);
    crate::commands::write_atomic(&dir.join(format!("{name}.{SEALED}")), payload.as_bytes())?;
    remove_if_present(&dir.join(format!("{name}.{LEGACY}")))
}

/// Every readable signature in `dir`, sorted by name. A legacy PNG is sealed
/// and removed once the sealed copy reads back; a value that cannot be
/// decrypted (Android: the Keystore key went with a re-install) is skipped and
/// left on disk.
fn list_in(dir: &Path, cipher: Option<&dyn SecretCipher>) -> Vec<SignatureEntry> {
    let mut sealed = Vec::new();
    let mut legacy = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        let Some(name) = path.file_stem().and_then(|stem| stem.to_str()).map(str::to_string) else { continue };
        match path.extension().and_then(|ext| ext.to_str()) {
            Some(SEALED) => sealed.push((name, path)),
            Some(LEGACY) => legacy.push((name, path)),
            _ => {}
        }
    }
    let mut entries = Vec::new();
    for (name, path) in &sealed {
        let Ok(content) = std::fs::read_to_string(path) else { continue };
        match secret::open_bytes(cipher, &content) {
            Ok(bytes) => entries.push(entry(name, &bytes)),
            Err(error) => eprintln!("signature {name} could not be read: {error}"),
        }
    }
    for (name, path) in legacy {
        if sealed.iter().any(|(sealed_name, _)| *sealed_name == name) {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else { continue };
        migrate_legacy(dir, cipher, &name, &bytes, &path);
        entries.push(entry(&name, &bytes));
    }
    entries.sort_by_key(|entry| entry.name.to_lowercase());
    entries
}

/// Seals a legacy PNG; the plain file is removed only after the sealed copy
/// decrypts back to the same bytes. Best effort: on failure the PNG stays.
fn migrate_legacy(dir: &Path, cipher: Option<&dyn SecretCipher>, name: &str, bytes: &[u8], legacy: &Path) {
    let sealed = dir.join(format!("{name}.{SEALED}"));
    let payload = secret::seal_bytes(cipher, bytes);
    if crate::commands::write_atomic(&sealed, payload.as_bytes()).is_err() {
        return;
    }
    let verified = std::fs::read_to_string(&sealed)
        .ok()
        .and_then(|content| secret::open_bytes(cipher, &content).ok())
        .is_some_and(|read| read == bytes);
    if verified {
        let _ = std::fs::remove_file(legacy);
    } else {
        let _ = std::fs::remove_file(&sealed);
    }
}

fn entry(name: &str, bytes: &[u8]) -> SignatureEntry {
    SignatureEntry { name: name.to_string(), image_base64: base64::engine::general_purpose::STANDARD.encode(bytes) }
}

fn delete_in(dir: &Path, name: &str) -> Result<(), PdfError> {
    remove_if_present(&dir.join(format!("{name}.{SEALED}")))?;
    remove_if_present(&dir.join(format!("{name}.{LEGACY}")))
}

fn remove_if_present(path: &Path) -> Result<(), PdfError> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(PdfError::from_io(error)),
        _ => Ok(()),
    }
}

/// Runs a signature operation off the main thread: the Android Keystore
/// bridge waits for the main thread, so a synchronous command would block it.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, PdfError> + Send + 'static,
) -> Result<T, PdfError> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| PdfError::Internal(format!("worker thread failed: {error}")))?
}

#[tauri::command]
pub async fn signature_save(app: AppHandle, name: String, image_base64: String) -> Result<(), PdfError> {
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
    let dir = signatures_dir(&app)?;
    blocking(move || save_in(&dir, secret::platform_cipher(), &name, &data)).await
}

#[tauri::command]
pub async fn signature_list(app: AppHandle) -> Result<Vec<SignatureEntry>, PdfError> {
    let dir = signatures_dir(&app)?;
    blocking(move || Ok(list_in(&dir, secret::platform_cipher()))).await
}

#[tauri::command]
pub async fn signature_delete(app: AppHandle, name: String) -> Result<(), PdfError> {
    let name = sanitize_name(&name);
    if name.is_empty() {
        return Ok(());
    }
    let dir = signatures_dir(&app)?;
    blocking(move || delete_in(&dir, &name)).await
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

    use crate::secret::{CipherError, SecretCipher};

    /// Stand-in for DPAPI / the Android Keystore: XORs the bytes.
    struct FakeCipher {
        key_lost: bool,
    }

    impl SecretCipher for FakeCipher {
        fn prefix(&self) -> &'static str {
            "keystore:"
        }
        fn protect(&self, plaintext: &[u8]) -> Result<Vec<u8>, CipherError> {
            Ok(plaintext.iter().map(|byte| byte ^ 0x5A).collect())
        }
        fn unprotect(&self, blob: &[u8]) -> Result<Vec<u8>, CipherError> {
            if self.key_lost {
                return Err(CipherError::KeyLost);
            }
            Ok(blob.iter().map(|byte| byte ^ 0x5A).collect())
        }
    }

    const CIPHER: FakeCipher = FakeCipher { key_lost: false };
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\nsignature-pixels";

    fn names(entries: &[SignatureEntry]) -> Vec<&str> {
        entries.iter().map(|entry| entry.name.as_str()).collect()
    }

    #[test]
    fn a_saved_signature_is_sealed_on_disk_and_lists_back() {
        let dir = tempfile::tempdir().unwrap();
        save_in(dir.path(), Some(&CIPHER), "İmza", PNG).unwrap();
        let stored = std::fs::read_to_string(dir.path().join("İmza.sig")).unwrap();
        assert!(stored.starts_with("keystore:"), "sealed: {stored}");
        let plain = base64::engine::general_purpose::STANDARD.encode(PNG);
        assert!(!stored.contains(&plain), "the PNG must not be readable from the file");
        assert!(!dir.path().join("İmza.png").exists());

        let entries = list_in(dir.path(), Some(&CIPHER));
        assert_eq!(names(&entries), ["İmza"]);
        assert_eq!(entries[0].image_base64, plain);
    }

    #[test]
    fn a_legacy_png_is_listed_and_moved_to_sealed_storage() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Paraf.png"), PNG).unwrap();
        let entries = list_in(dir.path(), Some(&CIPHER));
        assert_eq!(names(&entries), ["Paraf"]);
        assert_eq!(entries[0].image_base64, base64::engine::general_purpose::STANDARD.encode(PNG));
        assert!(!dir.path().join("Paraf.png").exists(), "the plain PNG is removed after sealing");
        assert!(std::fs::read_to_string(dir.path().join("Paraf.sig")).unwrap().starts_with("keystore:"));
        // A second read finds only the sealed copy.
        assert_eq!(names(&list_in(dir.path(), Some(&CIPHER))), ["Paraf"]);
    }

    #[test]
    fn without_platform_encryption_the_signature_is_still_stored_and_listed() {
        let dir = tempfile::tempdir().unwrap();
        save_in(dir.path(), None, "Linux", PNG).unwrap();
        assert!(std::fs::read_to_string(dir.path().join("Linux.sig")).unwrap().starts_with("plain:"));
        assert_eq!(names(&list_in(dir.path(), None)), ["Linux"]);
    }

    #[test]
    fn an_unreadable_signature_is_skipped_and_kept() {
        // Android: a re-install wipes the Keystore key; the other entries must
        // still load and the file is not deleted behind the user's back.
        let dir = tempfile::tempdir().unwrap();
        save_in(dir.path(), Some(&CIPHER), "Eski", PNG).unwrap();
        std::fs::write(dir.path().join("Yeni.png"), PNG).unwrap();
        let lost = FakeCipher { key_lost: true };
        assert_eq!(names(&list_in(dir.path(), Some(&lost))), ["Yeni"]);
        assert!(dir.path().join("Eski.sig").exists());
    }

    #[cfg(windows)]
    #[test]
    fn windows_seals_signatures_with_dpapi() {
        let dir = tempfile::tempdir().unwrap();
        save_in(dir.path(), secret::platform_cipher(), "Imza", PNG).unwrap();
        let stored = std::fs::read_to_string(dir.path().join("Imza.sig")).unwrap();
        assert!(stored.starts_with("dpapi:"), "stored as {stored:.12}");
        let entries = list_in(dir.path(), secret::platform_cipher());
        assert_eq!(entries[0].image_base64, base64::engine::general_purpose::STANDARD.encode(PNG));
    }

    #[test]
    fn delete_removes_the_sealed_and_the_legacy_file() {
        let dir = tempfile::tempdir().unwrap();
        save_in(dir.path(), Some(&CIPHER), "Imza", PNG).unwrap();
        std::fs::write(dir.path().join("Imza.png"), PNG).unwrap();
        delete_in(dir.path(), "Imza").unwrap();
        assert!(list_in(dir.path(), Some(&CIPHER)).is_empty());
        assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none());
    }
}
