//! Storage for the DeepSeek API key, OAuth tokens and the WebDAV password.
//!
//! On Windows the value is encrypted with DPAPI (CryptProtectData), so the
//! stored blob can only be decrypted by the same Windows user on the same
//! machine. On Android it is encrypted with an AES-256-GCM key that lives in
//! the Android Keystore (see `android_keystore.rs`). On other platforms it
//! falls back to plain text and the UI is told about it, so nothing pretends
//! to be safer than it is.
//!
//! The file holds one `<prefix>:<base64>` line: `dpapi:` and `keystore:` carry
//! the platform ciphertext, `plain:` carries the raw bytes. The platform
//! crypto sits behind [`SecretCipher`], so the prefix handling, the migration
//! of old `plain:` values and the failure paths are unit-tested on the host
//! with a fake cipher.

use pdfcore::error::PdfError;
use std::path::Path;
use std::sync::Mutex;

const DPAPI_PREFIX: &str = "dpapi:";
const PLAIN_PREFIX: &str = "plain:";
pub(crate) const KEYSTORE_PREFIX: &str = "keystore:";

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

/// Why a platform cipher could not protect or unprotect a value.
// Only the Android cipher (and the tests) builds `KeyLost`; Windows DPAPI
// reports every failure as `Failed`, so the variant is dead code there.
#[cfg_attr(not(any(target_os = "android", test)), allow(dead_code))]
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum CipherError {
    /// The key that sealed the value is gone or unusable (Android: backup
    /// restore, re-install, wiped Keystore). The value can never be read again.
    KeyLost,
    /// Any other failure; the value itself may still be fine.
    Failed(String),
}

/// Platform encryption for stored secrets.
pub(crate) trait SecretCipher {
    /// File prefix of values sealed by this cipher, e.g. `keystore:`.
    fn prefix(&self) -> &'static str;
    fn protect(&self, plaintext: &[u8]) -> Result<Vec<u8>, CipherError>;
    fn unprotect(&self, blob: &[u8]) -> Result<Vec<u8>, CipherError>;
    /// Whether a legacy `plain:` value is re-saved encrypted when it is read.
    fn migrates_plain(&self) -> bool {
        false
    }
}

#[cfg(windows)]
struct Dpapi;

#[cfg(windows)]
impl SecretCipher for Dpapi {
    fn prefix(&self) -> &'static str {
        DPAPI_PREFIX
    }
    fn protect(&self, plaintext: &[u8]) -> Result<Vec<u8>, CipherError> {
        protect(plaintext).map_err(CipherError::Failed)
    }
    fn unprotect(&self, blob: &[u8]) -> Result<Vec<u8>, CipherError> {
        unprotect(blob).map_err(CipherError::Failed)
    }
}

#[cfg(windows)]
fn platform_cipher() -> Option<&'static dyn SecretCipher> {
    Some(&Dpapi)
}

#[cfg(target_os = "android")]
fn platform_cipher() -> Option<&'static dyn SecretCipher> {
    Some(&crate::android_keystore::Keystore)
}

#[cfg(not(any(windows, target_os = "android")))]
fn platform_cipher() -> Option<&'static dyn SecretCipher> {
    None
}

const KEY_LOST_MESSAGE: &str = "The stored key can no longer be decrypted, please enter it again.";

/// Serializes writes to the secret files within this process, so a background
/// migration can never overwrite a value the user saved in the meantime.
static FILE_LOCK: Mutex<()> = Mutex::new(());

fn file_lock() -> std::sync::MutexGuard<'static, ()> {
    FILE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Builds the file payload: encrypted when the cipher works, `plain:` when it
/// is missing or fails (the user's value is never dropped).
fn seal(cipher: Option<&dyn SecretCipher>, value: &str) -> String {
    if let Some(cipher) = cipher {
        if let Ok(blob) = cipher.protect(value.as_bytes()) {
            return format!("{}{}", cipher.prefix(), base64_encode(&blob));
        }
    }
    format!("{PLAIN_PREFIX}{}", base64_encode(value.as_bytes()))
}

struct Opened {
    value: String,
    /// A `plain:` value that the cipher wants re-saved encrypted.
    migrate: bool,
}

fn utf8(bytes: Vec<u8>) -> Result<String, PdfError> {
    String::from_utf8(bytes).map_err(|_| PdfError::Internal("invalid key encoding".to_string()))
}

/// Decodes a stored payload. Unknown or empty content reads as "no value".
fn open(cipher: Option<&dyn SecretCipher>, content: &str) -> Result<Opened, PdfError> {
    let content = content.trim();
    if let Some(encoded) = content.strip_prefix(PLAIN_PREFIX) {
        let bytes = base64_decode(encoded).ok_or_else(|| PdfError::Internal("invalid key blob".to_string()))?;
        return Ok(Opened { value: utf8(bytes)?, migrate: cipher.is_some_and(|cipher| cipher.migrates_plain()) });
    }
    for prefix in [DPAPI_PREFIX, KEYSTORE_PREFIX] {
        let Some(encoded) = content.strip_prefix(prefix) else { continue };
        let cipher = cipher.filter(|cipher| cipher.prefix() == prefix).ok_or_else(|| {
            PdfError::Internal(format!("this value was stored with {prefix} protection, which is not available here"))
        })?;
        let blob = base64_decode(encoded).ok_or_else(|| PdfError::Internal("invalid key blob".to_string()))?;
        let plain = cipher.unprotect(&blob).map_err(|error| match error {
            CipherError::KeyLost => PdfError::Internal(KEY_LOST_MESSAGE.to_string()),
            CipherError::Failed(message) => PdfError::Internal(message),
        })?;
        return Ok(Opened { value: utf8(plain)?, migrate: false });
    }
    Ok(Opened { value: String::new(), migrate: false })
}

/// Re-saves a legacy `plain:` value encrypted. Best effort: it only replaces
/// the exact bytes that were read, only when the new payload decrypts back to
/// the same value, and any failure leaves the plain file untouched.
fn migrate_plain(cipher: &dyn SecretCipher, path: &Path, original: &str, value: &str) {
    if value.is_empty() {
        return;
    }
    let payload = seal(Some(cipher), value);
    if payload.starts_with(PLAIN_PREFIX) {
        return;
    }
    match open(Some(cipher), &payload) {
        Ok(check) if check.value == value => {}
        _ => return,
    }
    let _guard = file_lock();
    if std::fs::read_to_string(path).map(|now| now != original).unwrap_or(true) {
        return;
    }
    let _ = crate::commands::write_atomic(path, payload.as_bytes());
}

fn save_with(cipher: Option<&dyn SecretCipher>, path: &Path, key: &str) -> Result<bool, PdfError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(PdfError::from_io)?;
    }
    let payload = seal(cipher, key.trim());
    let _guard = file_lock();
    // A torn write here would make the key unrecoverable, so it must never
    // truncate the previous blob in place.
    crate::commands::write_atomic(path, payload.as_bytes())?;
    Ok(!payload.starts_with(PLAIN_PREFIX))
}

fn load_with(cipher: Option<&dyn SecretCipher>, path: &Path) -> Result<String, PdfError> {
    if !path.exists() {
        return Ok(String::new());
    }
    let content = std::fs::read_to_string(path).map_err(PdfError::from_io)?;
    let opened = open(cipher, &content)?;
    if opened.migrate {
        if let Some(cipher) = cipher {
            migrate_plain(cipher, path, &content, &opened.value);
        }
    }
    Ok(opened.value)
}

/// Writes the API key to `path`, encrypted when possible.
/// Returns `true` when platform encryption was used.
pub fn save_api_key(path: &Path, key: &str) -> Result<bool, PdfError> {
    save_with(platform_cipher(), path, key)
}

/// Reads the API key, decrypting it when it was stored encrypted. A `plain:`
/// value is migrated to encrypted storage on platforms that support it.
pub fn load_api_key(path: &Path) -> Result<String, PdfError> {
    load_with(platform_cipher(), path)
}

pub fn delete_api_key(path: &Path) -> Result<(), PdfError> {
    let _guard = file_lock();
    if path.exists() {
        std::fs::remove_file(path).map_err(PdfError::from_io)?;
    }
    Ok(())
}

fn kind_of(content: &str) -> &'static str {
    let content = content.trim_start();
    if content.starts_with(DPAPI_PREFIX) {
        "dpapi"
    } else if content.starts_with(KEYSTORE_PREFIX) {
        "keystore"
    } else {
        "plain"
    }
}

/// How the value at `path` is stored, for the UI: `dpapi`, `keystore`,
/// `plain` or `none` (no file).
pub fn storage_kind(path: &Path) -> &'static str {
    if !path.exists() {
        return "none";
    }
    std::fs::read_to_string(path).map(|content| kind_of(&content)).unwrap_or("plain")
}

pub(crate) fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

pub(crate) fn base64_decode(value: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(value.trim()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// Reversible stand-in for the platform crypto: reverses the bytes.
    struct FakeCipher {
        prefix: &'static str,
        migrates: bool,
        fail_protect: bool,
        key_lost: bool,
        protect_calls: Cell<u32>,
    }

    impl FakeCipher {
        fn keystore() -> Self {
            Self {
                prefix: KEYSTORE_PREFIX,
                migrates: true,
                fail_protect: false,
                key_lost: false,
                protect_calls: Cell::new(0),
            }
        }
    }

    impl SecretCipher for FakeCipher {
        fn prefix(&self) -> &'static str {
            self.prefix
        }
        fn protect(&self, plaintext: &[u8]) -> Result<Vec<u8>, CipherError> {
            self.protect_calls.set(self.protect_calls.get() + 1);
            if self.fail_protect {
                return Err(CipherError::Failed("encryption unavailable".into()));
            }
            Ok(plaintext.iter().rev().copied().collect())
        }
        fn unprotect(&self, blob: &[u8]) -> Result<Vec<u8>, CipherError> {
            if self.key_lost {
                return Err(CipherError::KeyLost);
            }
            Ok(blob.iter().rev().copied().collect())
        }
        fn migrates_plain(&self) -> bool {
            self.migrates
        }
    }

    fn plain_payload(value: &str) -> String {
        format!("{PLAIN_PREFIX}{}", base64_encode(value.as_bytes()))
    }

    fn secret_file(content: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secret.key");
        std::fs::write(&path, content).unwrap();
        (dir, path)
    }

    #[test]
    fn seal_uses_the_cipher_prefix_and_round_trips() {
        let cipher = FakeCipher::keystore();
        let payload = seal(Some(&cipher), "sk-abc");
        assert!(payload.starts_with("keystore:"));
        assert!(!payload.contains("sk-abc"));
        assert_eq!(cipher.protect_calls.get(), 1);
        let opened = open(Some(&cipher), &payload).unwrap();
        assert_eq!(opened.value, "sk-abc");
        assert!(!opened.migrate);
    }

    #[test]
    fn seal_falls_back_to_plain_without_a_working_cipher() {
        assert_eq!(seal(None, "k"), plain_payload("k"));
        let mut cipher = FakeCipher::keystore();
        cipher.fail_protect = true;
        assert_eq!(seal(Some(&cipher), "k"), plain_payload("k"));
    }

    #[test]
    fn open_plain_asks_for_migration_only_when_the_cipher_wants_it() {
        let payload = plain_payload("old-key");
        let cipher = FakeCipher::keystore();
        let opened = open(Some(&cipher), &payload).unwrap();
        assert_eq!((opened.value.as_str(), opened.migrate), ("old-key", true));

        let mut no_migrate = FakeCipher::keystore();
        no_migrate.migrates = false;
        assert!(!open(Some(&no_migrate), &payload).unwrap().migrate);
        assert!(!open(None, &payload).unwrap().migrate);
    }

    #[test]
    fn open_reports_a_lost_key_clearly() {
        let mut cipher = FakeCipher::keystore();
        let payload = seal(Some(&cipher), "sk-abc");
        cipher.key_lost = true;
        let message = open(Some(&cipher), &payload).err().unwrap().to_string();
        assert!(message.contains("can no longer be decrypted"), "{message}");
        assert!(message.contains("enter it again"), "{message}");
    }

    #[test]
    fn open_rejects_values_from_another_platform() {
        let payload = format!("{KEYSTORE_PREFIX}{}", base64_encode(b"xyz"));
        assert!(open(None, &payload).is_err());
        let mut dpapi_like = FakeCipher::keystore();
        dpapi_like.prefix = DPAPI_PREFIX;
        assert!(open(Some(&dpapi_like), &payload).is_err());
    }

    #[test]
    fn open_treats_empty_unknown_and_corrupt_content() {
        let cipher = FakeCipher::keystore();
        assert_eq!(open(Some(&cipher), "").unwrap().value, "");
        assert_eq!(open(Some(&cipher), "something-else").unwrap().value, "");
        assert!(open(Some(&cipher), "plain:***not base64***").is_err());
        assert!(open(Some(&cipher), "keystore:***not base64***").is_err());
        assert_eq!(open(Some(&cipher), &format!("{}\n", plain_payload("k"))).unwrap().value, "k");
    }

    #[test]
    fn kind_of_names_every_prefix() {
        assert_eq!(kind_of("dpapi:AAAA"), "dpapi");
        assert_eq!(kind_of("keystore:AAAA"), "keystore");
        assert_eq!(kind_of("plain:AAAA"), "plain");
        assert_eq!(kind_of(""), "plain");
    }

    #[test]
    fn storage_kind_reads_the_file() {
        let (dir, path) = secret_file("keystore:AAAA");
        assert_eq!(storage_kind(&path), "keystore");
        assert_eq!(storage_kind(&dir.path().join("missing.key")), "none");
    }

    #[test]
    fn save_then_load_round_trips_encrypted() {
        let cipher = FakeCipher::keystore();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("secret.key");
        assert!(save_with(Some(&cipher), &path, "  sk-abc \n").unwrap());
        assert_eq!(storage_kind(&path), "keystore");
        assert_eq!(load_with(Some(&cipher), &path).unwrap(), "sk-abc");
    }

    #[test]
    fn save_reports_plain_when_encryption_fails() {
        let mut cipher = FakeCipher::keystore();
        cipher.fail_protect = true;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secret.key");
        assert!(!save_with(Some(&cipher), &path, "sk-abc").unwrap());
        assert_eq!(storage_kind(&path), "plain");
        assert_eq!(load_with(Some(&cipher), &path).unwrap(), "sk-abc");
    }

    #[test]
    fn load_migrates_a_plain_value_to_encrypted_storage() {
        let cipher = FakeCipher::keystore();
        let (_dir, path) = secret_file(&plain_payload("old-key"));
        assert_eq!(load_with(Some(&cipher), &path).unwrap(), "old-key");
        assert_eq!(storage_kind(&path), "keystore");
        assert_eq!(load_with(Some(&cipher), &path).unwrap(), "old-key");
    }

    #[test]
    fn load_keeps_the_plain_value_when_migration_cannot_encrypt() {
        let mut cipher = FakeCipher::keystore();
        cipher.fail_protect = true;
        let payload = plain_payload("old-key");
        let (_dir, path) = secret_file(&payload);
        assert_eq!(load_with(Some(&cipher), &path).unwrap(), "old-key");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), payload);
    }

    #[test]
    fn load_does_not_migrate_without_a_migrating_cipher() {
        let payload = plain_payload("old-key");
        let (_dir, path) = secret_file(&payload);
        assert_eq!(load_with(None, &path).unwrap(), "old-key");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), payload);
    }

    #[test]
    fn load_surfaces_a_lost_key_and_leaves_the_file_alone() {
        let mut cipher = FakeCipher::keystore();
        let (_dir, path) = secret_file(&seal(Some(&cipher), "sk-abc"));
        let before = std::fs::read_to_string(&path).unwrap();
        cipher.key_lost = true;
        let error = load_with(Some(&cipher), &path).err().unwrap();
        assert!(error.to_string().contains("enter it again"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
    }

    #[test]
    fn load_of_a_missing_file_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load_with(None, &dir.path().join("none.key")).unwrap(), "");
    }

    #[test]
    fn migration_never_overwrites_a_concurrently_saved_value() {
        let cipher = FakeCipher::keystore();
        let (_dir, path) = secret_file(&plain_payload("newer"));
        // The migration read an older value, but the file has changed since.
        migrate_plain(&cipher, &path, &plain_payload("old"), "old");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), plain_payload("newer"));
    }
}
