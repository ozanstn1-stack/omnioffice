//! Storage for the DeepSeek API key, OAuth tokens and the WebDAV password,
//! and (through [`seal_bytes`] / [`open_bytes`]) the saved signature images.
//!
//! On Windows the value is encrypted with DPAPI (CryptProtectData), so the
//! stored blob can only be decrypted by the same Windows user on the same
//! machine. On Android it is encrypted with an AES-256-GCM key that lives in
//! the Android Keystore (see `android_keystore.rs`). Those two are the first
//! tier. The second is the OS credential vault ([`vault`]): when
//! the platform has no cipher, the value goes into the Windows Credential
//! Manager, the macOS Keychain or the Linux Secret Service and the file keeps a
//! `keyring:<slot>` marker. Only when the vault does not answer either does the
//! value land in a `plain:` file - and that file is forced to `0600`, so the
//! fallback is not readable by other local users. The UI reports which tier is
//! in use, so nothing pretends to be safer than it is.
//!
//! The file holds one `<prefix>...` line: `dpapi:` and `keystore:` carry the
//! platform ciphertext, `keyring:` a vault slot and `plain:` the raw bytes. The
//! platform crypto sits behind [`SecretCipher`], so the prefix handling, the
//! migration of old `plain:` values and the failure paths are unit-tested on
//! the host with a fake cipher and a fake vault.

use pdfcore::error::PdfError;
use std::path::Path;
use std::sync::Mutex;

mod vault;

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
pub(crate) fn platform_cipher() -> Option<&'static dyn SecretCipher> {
    Some(&Dpapi)
}

#[cfg(target_os = "android")]
pub(crate) fn platform_cipher() -> Option<&'static dyn SecretCipher> {
    Some(&crate::android_keystore::Keystore)
}

#[cfg(not(any(windows, target_os = "android")))]
pub(crate) fn platform_cipher() -> Option<&'static dyn SecretCipher> {
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
    seal_bytes(cipher, value.as_bytes())
}

/// Builds the payload for a *secret* (not a binary blob): the platform cipher
/// first, then the OS vault, then a `plain:` file the caller restricts to
/// `0600`. The order matters: DPAPI and the Keystore need no daemon, the vault
/// does, and plain text is the last resort - never the first.
fn seal_for(cipher: Option<&dyn SecretCipher>, vault: Option<&dyn vault::Vault>, path: &Path, value: &str) -> String {
    if let Some(payload) = cipher.map(|cipher| seal(Some(cipher), value)) {
        if !payload.starts_with(PLAIN_PREFIX) {
            return payload;
        }
    }
    if let Some(vault) = vault {
        if let Some(marker) = vault::seal(vault, path, value) {
            return marker;
        }
    }
    format!("{PLAIN_PREFIX}{}", base64_encode(value.as_bytes()))
}

/// [`seal`] for arbitrary bytes (the signature images).
pub(crate) fn seal_bytes(cipher: Option<&dyn SecretCipher>, bytes: &[u8]) -> String {
    if let Some(cipher) = cipher {
        if let Ok(blob) = cipher.protect(bytes) {
            return format!("{}{}", cipher.prefix(), base64_encode(&blob));
        }
    }
    format!("{PLAIN_PREFIX}{}", base64_encode(bytes))
}

/// Decodes a payload written by [`seal_bytes`]. Unlike a key, unknown content
/// is an error: a signature file is never empty.
pub(crate) fn open_bytes(cipher: Option<&dyn SecretCipher>, content: &str) -> Result<Vec<u8>, PdfError> {
    let content = content.trim();
    if let Some(encoded) = content.strip_prefix(PLAIN_PREFIX) {
        return base64_decode(encoded).ok_or_else(|| PdfError::Internal("invalid stored blob".to_string()));
    }
    for prefix in [DPAPI_PREFIX, KEYSTORE_PREFIX] {
        let Some(encoded) = content.strip_prefix(prefix) else { continue };
        let cipher = cipher.filter(|cipher| cipher.prefix() == prefix).ok_or_else(|| {
            PdfError::Internal(format!("this value was stored with {prefix} protection, which is not available here"))
        })?;
        let blob = base64_decode(encoded).ok_or_else(|| PdfError::Internal("invalid stored blob".to_string()))?;
        return cipher.unprotect(&blob).map_err(|error| match error {
            CipherError::KeyLost => PdfError::Internal("the stored value can no longer be decrypted".to_string()),
            CipherError::Failed(message) => PdfError::Internal(message),
        });
    }
    Err(PdfError::Internal("unknown stored format".to_string()))
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
fn open_via(
    cipher: Option<&dyn SecretCipher>,
    vault: Option<&dyn vault::Vault>,
    content: &str,
) -> Result<Opened, PdfError> {
    let content = content.trim();
    if content.starts_with(vault::KEYRING_PREFIX) {
        let vault = vault.ok_or_else(|| PdfError::Internal(vault::VAULT_MISSING_MESSAGE.to_string()))?;
        return Ok(Opened { value: vault::open(vault, content)?, migrate: false });
    }
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

/// Replaces a `plain:` file with `payload`, but only when the file still holds
/// exactly the bytes the caller read. A concurrent save therefore wins over a
/// migration, and a failed write leaves the plain value in place.
fn replace_plain(path: &Path, original: &str, payload: &str) {
    let _guard = file_lock();
    if std::fs::read_to_string(path).map(|now| now != original).unwrap_or(true) {
        return;
    }
    let _ = crate::commands::write_atomic(path, payload.as_bytes());
    vault::restrict(path);
}

/// Re-saves a legacy `plain:` value encrypted. Best effort: it only replaces
/// the exact bytes that were read, and only when the new payload decrypts back
/// to the same value.
fn migrate_plain(cipher: &dyn SecretCipher, path: &Path, original: &str, value: &str) {
    if value.is_empty() {
        return;
    }
    let payload = seal(Some(cipher), value);
    if payload.starts_with(PLAIN_PREFIX) {
        return;
    }
    match open_via(Some(cipher), None, &payload) {
        Ok(check) if check.value == value => {}
        _ => return,
    }
    replace_plain(path, original, &payload);
}

/// Moves a legacy `plain:` value into the OS vault, the only encryption a
/// platform without a cipher has.
fn migrate_to_vault(vault: &dyn vault::Vault, path: &Path, original: &str, value: &str) {
    if value.is_empty() {
        return;
    }
    let Some(marker) = vault::seal(vault, path, value) else { return };
    // A vault that answered the write but not the read would lose the secret if
    // the plain file were replaced, so the read-back is part of the migration.
    if vault::open(vault, &marker).ok().as_deref() != Some(value) {
        vault::remove(vault, &marker);
        return;
    }
    replace_plain(path, original, &marker);
}

/// Saves one secret, reporting whether it is protected (`true`) or landed in a
/// `plain:` file (`false`).
/// Writes a sealed payload atomically and restricts the file to its owner. The
/// cipher may be missing (a platform with neither a cipher nor a vault), and a
/// sealed store - the LAN device key, a person card, a signature - must still
/// not be readable by other local users.
pub(crate) fn write_sealed(path: &Path, payload: &[u8]) -> Result<(), PdfError> {
    crate::commands::write_atomic(path, payload)?;
    vault::restrict(path);
    Ok(())
}

fn save_via(
    cipher: Option<&dyn SecretCipher>,
    vault: Option<&dyn vault::Vault>,
    path: &Path,
    key: &str,
) -> Result<bool, PdfError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(PdfError::from_io)?;
    }
    let payload = seal_for(cipher, vault, path, key.trim());
    let _guard = file_lock();
    // A torn write here would make the key unrecoverable, so it must never
    // truncate the previous blob in place.
    crate::commands::write_atomic(path, payload.as_bytes())?;
    vault::restrict(path);
    Ok(!payload.starts_with(PLAIN_PREFIX))
}

fn load_via(
    cipher: Option<&dyn SecretCipher>,
    vault: Option<&dyn vault::Vault>,
    path: &Path,
) -> Result<String, PdfError> {
    if !path.exists() {
        return Ok(String::new());
    }
    let content = std::fs::read_to_string(path).map_err(PdfError::from_io)?;
    let plain = content.trim().starts_with(PLAIN_PREFIX);
    if plain {
        // An old file may have been written with the umask's mode.
        vault::restrict(path);
    }
    let opened = open_via(cipher, vault, &content)?;
    if opened.migrate {
        if let Some(cipher) = cipher {
            migrate_plain(cipher, path, &content, &opened.value);
        }
    } else if plain && cipher.is_none() {
        if let Some(vault) = vault {
            migrate_to_vault(vault, path, &content, &opened.value);
        }
    }
    Ok(opened.value)
}

/// Writes the API key to `path`, encrypted when possible.
/// Returns `true` when the platform cipher or the OS vault took it.
pub fn save_api_key(path: &Path, key: &str) -> Result<bool, PdfError> {
    save_via(platform_cipher(), vault::system(), path, key)
}

/// Reads the API key, decrypting it when it was stored encrypted. A `plain:`
/// value is migrated to encrypted or vault storage when the platform offers it.
pub fn load_api_key(path: &Path) -> Result<String, PdfError> {
    load_via(platform_cipher(), vault::system(), path)
}

/// Deletes the stored value: the vault entry (the file then only holds a
/// marker) and the file itself.
pub fn delete_api_key(path: &Path) -> Result<(), PdfError> {
    delete_via(vault::system(), path)
}

fn delete_via(vault: Option<&dyn vault::Vault>, path: &Path) -> Result<(), PdfError> {
    let _guard = file_lock();
    if let Some(vault) = vault {
        match std::fs::read_to_string(path) {
            Ok(content) if content.trim().starts_with(vault::KEYRING_PREFIX) => vault::remove(vault, &content),
            _ => vault::remove_for(vault, path),
        }
    }
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
    } else if content.starts_with(vault::KEYRING_PREFIX) {
        "keyring"
    } else {
        "plain"
    }
}

/// How the value at `path` is stored, for the UI: `dpapi`, `keystore`,
/// `keyring`, `plain` or `none` (no file).
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
#[path = "secret_tests.rs"]
mod tests;
